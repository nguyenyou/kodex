use anyhow::{Context, Result};
use rkyv::to_bytes;
use std::fs;
use std::io::Write;
use std::path::Path;

use crate::model::KodexIndex;

/// Magic bytes at the start of every kodex.idx file.
/// Allows fast rejection of non-kodex files before rkyv deserialization.
pub const MAGIC: &[u8; 8] = b"KODEX\x00\x00\x00";

/// Serialize a KodexIndex to disk using rkyv, prefixed with magic bytes.
pub fn write_index(index: &KodexIndex, path: &Path) -> Result<()> {
    let bytes = to_bytes::<rkyv::rancor::Error>(index)
        .map_err(|e| anyhow::anyhow!("rkyv serialization failed: {e}"))?;

    // Atomic write: write to temp file, then rename
    let tmp = path.with_extension("idx.tmp");
    {
        let mut f = fs::File::create(&tmp)
            .with_context(|| format!("Failed to create {}", tmp.display()))?;
        f.write_all(MAGIC)
            .with_context(|| format!("Failed to write magic to {}", tmp.display()))?;
        f.write_all(&bytes)
            .with_context(|| format!("Failed to write index to {}", tmp.display()))?;
    }
    fs::rename(&tmp, path).with_context(|| format!("Failed to rename to {}", path.display()))?;

    let total = MAGIC.len() + bytes.len();
    eprintln!(
        "Index size: {total} bytes ({:.1} MB)",
        total as f64 / 1_048_576.0
    );
    Ok(())
}
