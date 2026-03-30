use anyhow::{Context, Result};
use memmap2::Mmap;
use rkyv::access;
use std::path::Path;

use crate::index::writer::MAGIC;
use crate::model::{ArchivedKodexIndex, KODEX_INDEX_VERSION};

/// Memory-mapped, zero-copy index reader.
pub struct IndexReader {
    _mmap: Mmap,
    index: &'static ArchivedKodexIndex,
}

impl IndexReader {
    /// Open and mmap a kodex.idx file.
    ///
    /// Validates the magic header and version before returning.
    pub fn open(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path)
            .with_context(|| format!("Failed to open {}", path.display()))?;
        let mmap = unsafe { Mmap::map(&file) }
            .with_context(|| format!("Failed to mmap {}", path.display()))?;

        // Check magic header
        if mmap.len() < MAGIC.len() || &mmap[..MAGIC.len()] != MAGIC {
            anyhow::bail!(
                "Not a kodex index file: {}. Re-run `kodex index`.",
                path.display()
            );
        }

        // Validate the rkyv archive (after magic prefix)
        let data = &mmap[MAGIC.len()..];
        let index: &ArchivedKodexIndex = access::<ArchivedKodexIndex, rkyv::rancor::Error>(data)
            .map_err(|e| anyhow::anyhow!("Corrupt index file {}: {e}", path.display()))?;

        // SAFETY: The mmap is owned by IndexReader and lives as long as this struct.
        // The ArchivedKodexIndex borrows from the mmap's memory, so extending to 'static
        // is safe because _mmap is never dropped before index (struct drop order is
        // declaration order, but we never hand out &'static references that outlive self).
        let index: &'static ArchivedKodexIndex = unsafe { &*std::ptr::from_ref(index) };

        if index.version != KODEX_INDEX_VERSION {
            anyhow::bail!(
                "Index version mismatch: expected {}, got {}. Re-run `kodex index`.",
                KODEX_INDEX_VERSION,
                index.version
            );
        }

        Ok(Self { _mmap: mmap, index })
    }

    pub fn index(&self) -> &ArchivedKodexIndex {
        self.index
    }
}
