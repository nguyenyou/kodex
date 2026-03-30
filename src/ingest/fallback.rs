//! Fallback provider for workspaces without a recognized build tool.
//!
//! Discovers SemanticDB files anywhere under the workspace root
//! (looking for `META-INF/semanticdb/` directories). No metadata enrichment.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;
use walkdir::WalkDir;

use super::provider::{BuildMetadata, BuildProvider, DiscoveredFile, DiscoveryResult};

pub struct FallbackProvider;

impl BuildProvider for FallbackProvider {
    fn discover(&self, root: &Path) -> Result<DiscoveryResult> {
        let root = root
            .canonicalize()
            .with_context(|| format!("Failed to resolve workspace root: {}", root.display()))?;

        let files: Vec<DiscoveredFile> = WalkDir::new(&root)
            .into_iter()
            .filter_map(std::result::Result::ok)
            .filter(|e| e.file_type().is_file())
            .filter(|e| {
                e.path()
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("semanticdb"))
            })
            .filter(|e| {
                // Only include files under META-INF/semanticdb/
                e.path()
                    .components()
                    .any(|c| c.as_os_str() == "semanticdb")
            })
            .map(|e| DiscoveredFile {
                path: e.into_path(),
                module_segments: String::new(),
            })
            .collect();

        Ok(DiscoveryResult {
            files,
            module_out_dirs: HashMap::new(),
        })
    }

    fn metadata(
        &self,
        _root: &Path,
        _discovery: &DiscoveryResult,
    ) -> Result<Option<BuildMetadata>> {
        Ok(None)
    }
}
