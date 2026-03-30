//! sbt build-tool provider.
//!
//! Discovers SemanticDB files from sbt's `target/` directory structure
//! and extracts module names and Scala versions from the path layout.
//!
//! sbt SemanticDB path patterns:
//!   <module>/target/scala-<version>/meta/META-INF/semanticdb/...
//!   <module>/target/scala-<version>/test-meta/META-INF/semanticdb/...
//!   <module>/target/scala-<version>/sbt-<version>/meta/META-INF/semanticdb/...
//!   <module>/target/meta/META-INF/semanticdb/...  (Java-only, no scala version)

use anyhow::{Context, Result};
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use super::provider::{BuildMetadata, BuildProvider, DiscoveredFile, DiscoveryResult, ModuleInfo};

pub struct SbtProvider;

impl BuildProvider for SbtProvider {
    fn discover(&self, root: &Path) -> Result<DiscoveryResult> {
        let root = root
            .canonicalize()
            .context("Failed to resolve workspace root")?;

        // Find all META-INF/semanticdb directories under target/ dirs
        let sdb_dirs: Vec<(PathBuf, String, bool)> = WalkDir::new(&root)
            .into_iter()
            .filter_map(std::result::Result::ok)
            .filter(|e| e.file_type().is_dir() && e.file_name() == "semanticdb")
            .filter(|e| {
                // Must be under META-INF/semanticdb and within a target/ tree
                let p = e.path();
                p.parent().is_some_and(|par| par.file_name().is_some_and(|n| n == "META-INF"))
                    && path_contains_component(p, "target")
            })
            .filter_map(|e| {
                let sdb_dir = e.into_path();
                let (module_segments, is_test) = extract_sbt_module_info(&root, &sdb_dir)?;
                Some((sdb_dir, module_segments, is_test))
            })
            .collect();

        // Build module_out_dirs: module_segments → the target/ dir for that module
        let mut module_out_dirs: HashMap<String, PathBuf> = HashMap::new();
        for (sdb_dir, segments, _) in &sdb_dirs {
            if !segments.is_empty() && !module_out_dirs.contains_key(segments) {
                // Walk up from semanticdb dir to find the target/ dir
                if let Some(target_dir) = find_ancestor_named(sdb_dir, "target") {
                    module_out_dirs.insert(segments.clone(), target_dir);
                }
            }
        }

        // Collect .semanticdb files (parallel)
        let files: Vec<DiscoveredFile> = sdb_dirs
            .par_iter()
            .flat_map(|(dir, module_segments, _)| {
                WalkDir::new(dir)
                    .into_iter()
                    .filter_map(std::result::Result::ok)
                    .filter(|e| e.file_type().is_file())
                    .filter(|e| {
                        e.path()
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("semanticdb"))
                    })
                    .map(|e| DiscoveredFile {
                        path: e.into_path(),
                        module_segments: module_segments.clone(),
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        Ok(DiscoveryResult {
            files,
            module_out_dirs,
        })
    }

    fn metadata(&self, _root: &Path, discovery: &DiscoveryResult) -> Result<Option<BuildMetadata>> {
        if discovery.module_out_dirs.is_empty() {
            return Ok(None);
        }

        // Derive per-module info from the discovery result (no re-walk needed).
        // Extract scala version and test status from file paths.
        let mut module_info: HashMap<String, (Option<String>, bool)> = HashMap::new();
        for file in &discovery.files {
            let scala_ver = extract_scala_version(&file.path);
            let is_test = has_test_meta_segment(&file.path);
            let entry = module_info
                .entry(file.module_segments.clone())
                .or_insert_with(|| (None, false));
            if entry.0.is_none() {
                entry.0 = scala_ver;
            }
            if is_test {
                entry.1 = true;
            }
        }

        let modules: Vec<ModuleInfo> = module_info
            .into_iter()
            .filter(|(segments, _)| !segments.is_empty())
            .map(|(segments, (scala_ver, is_test))| {
                // Derive artifact name from the last segment
                let artifact_name = segments
                    .rsplit('.')
                    .next()
                    .unwrap_or(&segments)
                    .to_string();
                ModuleInfo {
                    segments,
                    artifact_name,
                    source_paths: vec![],
                    generated_source_paths: vec![],
                    scala_version: scala_ver.unwrap_or_default(),
                    scalac_options: vec![],
                    module_deps: vec![],
                    ivy_deps: vec![],
                    main_class: String::new(),
                    test_framework: if is_test {
                        "sbt-test".to_string()
                    } else {
                        String::new()
                    },
                }
            })
            .collect();

        eprintln!("sbt metadata: {} modules", modules.len());
        Ok(Some(BuildMetadata { modules }))
    }

}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Check if a file path contains `test-meta/` (sbt's convention for test SemanticDB output).
fn has_test_meta_segment(path: &Path) -> bool {
    path.iter().any(|c| c == "test-meta")
}

/// Extract module segments and test status from an sbt semanticdb directory path.
///
/// Given root `/project` and sdb_dir `/project/tests/unit/target/scala-2.13/meta/META-INF/semanticdb`,
/// returns `Some(("tests.unit", false))`.
///
/// For test-meta: `/project/foo/target/scala-2.13/test-meta/META-INF/semanticdb`
/// returns `Some(("foo", true))`.
fn extract_sbt_module_info(root: &Path, sdb_dir: &Path) -> Option<(String, bool)> {
    let rel = sdb_dir.strip_prefix(root).ok()?;
    let components: Vec<&str> = rel
        .iter()
        .map(|c| c.to_str().unwrap_or(""))
        .collect();

    // Find "target" component
    let target_idx = components.iter().position(|&c| c == "target")?;

    // Module segments = everything before "target", joined with "."
    let module_segments = if target_idx == 0 {
        String::new() // root-level target/
    } else {
        components[..target_idx].join(".")
    };

    // Check for test-meta vs meta
    let is_test = components.iter().any(|&c| c == "test-meta");

    Some((module_segments, is_test))
}

/// Extract Scala version from path like `.../scala-2.13.18/meta/...`
fn extract_scala_version(sdb_dir: &Path) -> Option<String> {
    for component in sdb_dir.iter() {
        let s = component.to_str()?;
        if let Some(ver) = s.strip_prefix("scala-") {
            return Some(ver.to_string());
        }
    }
    None
}

/// Check if any path component equals the given name.
fn path_contains_component(path: &Path, name: &str) -> bool {
    path.iter().any(|c| c == name)
}

/// Walk up from `path` to find the nearest ancestor directory with the given name.
fn find_ancestor_named(path: &Path, name: &str) -> Option<PathBuf> {
    let mut current = path;
    while let Some(parent) = current.parent() {
        if parent.file_name().is_some_and(|n| n == name) {
            return Some(parent.to_path_buf());
        }
        current = parent;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_sbt_module_info_basic() {
        let root = Path::new("/project");
        let sdb_dir = Path::new(
            "/project/metals/target/scala-2.13/meta/META-INF/semanticdb",
        );
        let (segments, is_test) = extract_sbt_module_info(root, sdb_dir).unwrap();
        assert_eq!(segments, "metals");
        assert!(!is_test);
    }

    #[test]
    fn test_extract_sbt_module_info_nested() {
        let root = Path::new("/project");
        let sdb_dir = Path::new(
            "/project/tests/unit/target/scala-2.13/meta/META-INF/semanticdb",
        );
        let (segments, is_test) = extract_sbt_module_info(root, sdb_dir).unwrap();
        assert_eq!(segments, "tests.unit");
        assert!(!is_test);
    }

    #[test]
    fn test_extract_sbt_module_info_test_meta() {
        let root = Path::new("/project");
        let sdb_dir = Path::new(
            "/project/tests/input/target/scala-2.13/test-meta/META-INF/semanticdb",
        );
        let (segments, is_test) = extract_sbt_module_info(root, sdb_dir).unwrap();
        assert_eq!(segments, "tests.input");
        assert!(is_test);
    }

    #[test]
    fn test_extract_sbt_module_info_java_no_scala() {
        let root = Path::new("/project");
        let sdb_dir = Path::new(
            "/project/mtags-interfaces/target/meta/META-INF/semanticdb",
        );
        let (segments, is_test) = extract_sbt_module_info(root, sdb_dir).unwrap();
        assert_eq!(segments, "mtags-interfaces");
        assert!(!is_test);
    }

    #[test]
    fn test_extract_sbt_module_info_sbt_plugin() {
        let root = Path::new("/project");
        let sdb_dir = Path::new(
            "/project/sbt-metals/target/scala-2.12/sbt-1.0/meta/META-INF/semanticdb",
        );
        let (segments, is_test) = extract_sbt_module_info(root, sdb_dir).unwrap();
        assert_eq!(segments, "sbt-metals");
        assert!(!is_test);
    }

    #[test]
    fn test_extract_sbt_module_info_root_target() {
        let root = Path::new("/project");
        let sdb_dir = Path::new(
            "/project/target/scala-2.13/meta/META-INF/semanticdb",
        );
        let (segments, is_test) = extract_sbt_module_info(root, sdb_dir).unwrap();
        assert_eq!(segments, "");
        assert!(!is_test);
    }

    #[test]
    fn test_extract_scala_version() {
        let path = Path::new("/project/metals/target/scala-2.13/meta/META-INF/semanticdb");
        assert_eq!(extract_scala_version(path), Some("2.13".to_string()));

        let path = Path::new("/project/mtags/target/scala-2.13.18/meta/META-INF/semanticdb");
        assert_eq!(extract_scala_version(path), Some("2.13.18".to_string()));

        let path = Path::new("/project/foo/target/meta/META-INF/semanticdb");
        assert_eq!(extract_scala_version(path), None);
    }
}
