//! scala-cli build-tool provider.
//!
//! Discovers SemanticDB files from scala-cli's `.scala-build/` directory structure.
//!
//! scala-cli SemanticDB path patterns:
//!   <source-dir>/.scala-build/<hash>/classes/main/META-INF/semanticdb/...
//!   <source-dir>/.scala-build/<hash>/classes/test/META-INF/semanticdb/...
//!
//! Duplicates under `.scala-build/.bloop/` are skipped — only the primary
//! `classes/` copies are indexed.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use super::provider::{BuildMetadata, BuildProvider, DiscoveredFile, DiscoveryResult, ModuleInfo};

pub struct ScalaCliProvider;

impl BuildProvider for ScalaCliProvider {
    fn discover(&self, root: &Path) -> Result<DiscoveryResult> {
        let root = root
            .canonicalize()
            .context("Failed to resolve workspace root")?;

        // Find all .scala-build directories, then look for classes/main or classes/test
        // under the hash subdirs. Skip .bloop/ to avoid duplicates.
        let sdb_dirs: Vec<(PathBuf, String, bool)> = WalkDir::new(&root)
            .into_iter()
            .filter_map(std::result::Result::ok)
            .filter(|e| e.file_type().is_dir() && e.file_name() == "semanticdb")
            .filter(|e| {
                let p = e.path();
                // Must be under META-INF/semanticdb
                p.parent().is_some_and(|par| par.file_name().is_some_and(|n| n == "META-INF"))
                    // Must be under .scala-build
                    && path_contains_component(p, ".scala-build")
                    // Skip .bloop duplicates
                    && !path_contains_component(p, ".bloop")
            })
            .filter_map(|e| {
                let sdb_dir = e.into_path();
                let (source_dir_name, is_test) =
                    extract_scala_cli_info(&root, &sdb_dir)?;
                Some((sdb_dir, source_dir_name, is_test))
            })
            .collect();

        // Collect .semanticdb files
        let files: Vec<DiscoveredFile> = sdb_dirs
            .iter()
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

        // Build module_out_dirs from the .scala-build dirs
        let mut module_out_dirs: HashMap<String, PathBuf> = HashMap::new();
        for (sdb_dir, segments, _) in &sdb_dirs {
            if !module_out_dirs.contains_key(segments) {
                if let Some(scala_build_dir) = find_ancestor_named(sdb_dir, ".scala-build") {
                    module_out_dirs.insert(segments.clone(), scala_build_dir);
                }
            }
        }

        Ok(DiscoveryResult {
            files,
            module_out_dirs,
        })
    }

    fn metadata(&self, root: &Path, discovery: &DiscoveryResult) -> Result<Option<BuildMetadata>> {
        if discovery.module_out_dirs.is_empty() {
            return Ok(None);
        }

        let root = root.canonicalize().context("Failed to resolve root")?;

        // Collect info from discovered dirs.
        // Detect test sources by checking for `classes/test/` in the path
        // (not just any "test" component, which would false-positive on dirs named "test").
        let mut module_info: HashMap<String, bool> = HashMap::new();
        for file in &discovery.files {
            let is_test = has_classes_test_segment(&file.path);
            let entry = module_info
                .entry(file.module_segments.clone())
                .or_insert(false);
            if is_test {
                *entry = true;
            }
        }

        // Try to extract Scala version from .scala-build directory names or class files
        let scala_version = discovery
            .module_out_dirs
            .values()
            .find_map(|scala_build_dir| find_scala_version_in_scala_build(scala_build_dir));

        let modules: Vec<ModuleInfo> = module_info
            .into_iter()
            .filter(|(segments, _)| !segments.is_empty())
            .map(|(segments, is_test)| {
                // Derive artifact name: use the source dir name
                let artifact_name = segments
                    .rsplit('.')
                    .next()
                    .unwrap_or(&segments)
                    .to_string();

                // Derive source path from root + segments
                let source_path = segments
                    .replace('.', "/");
                let abs_source = root.join(&source_path);
                let source_paths = if abs_source.exists() {
                    vec![abs_source.to_string_lossy().to_string()]
                } else {
                    vec![]
                };

                ModuleInfo {
                    segments,
                    artifact_name,
                    source_paths,
                    generated_source_paths: vec![],
                    scala_version: scala_version.clone().unwrap_or_default(),
                    scalac_options: vec![],
                    module_deps: vec![],
                    ivy_deps: vec![],
                    main_class: String::new(),
                    test_framework: if is_test {
                        "scala-cli-test".to_string()
                    } else {
                        String::new()
                    },
                }
            })
            .collect();

        eprintln!("scala-cli metadata: {} modules", modules.len());
        Ok(Some(BuildMetadata { modules }))
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Check if a path contains the `classes/test/` pattern (not just any "test" component).
fn has_classes_test_segment(path: &Path) -> bool {
    let components: Vec<_> = path.iter().collect();
    components.windows(2).any(|w| w[0] == "classes" && w[1] == "test")
}

/// Extract the source directory name (as module segments) and test status.
///
/// Path pattern: `<root>/<source-dir>/.scala-build/<hash>/classes/{main,test}/META-INF/semanticdb`
///
/// Returns `("src", false)` for a main source, or `("src", true)` for test.
fn extract_scala_cli_info(root: &Path, sdb_dir: &Path) -> Option<(String, bool)> {
    let rel = sdb_dir.strip_prefix(root).ok()?;
    let components: Vec<&str> = rel
        .iter()
        .map(|c| c.to_str().unwrap_or(""))
        .collect();

    // Find ".scala-build" component
    let sb_idx = components.iter().position(|&c| c == ".scala-build")?;

    // Module segments = path components before .scala-build, joined with "."
    let module_segments = if sb_idx == 0 {
        String::new()
    } else {
        components[..sb_idx].join(".")
    };

    // Check for test vs main: look for "classes" followed by "test"
    let is_test = components
        .windows(2)
        .any(|w| w[0] == "classes" && w[1] == "test");

    Some((module_segments, is_test))
}

/// Check if any path component equals the given name.
fn path_contains_component(path: &Path, name: &str) -> bool {
    path.iter().any(|c| c == name)
}

/// Walk up from `path` to find the nearest ancestor with the given name.
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

/// Try to find a Scala version from scala-cli build artifacts.
/// Looks for directory names like `scala-3.6.4` or parses from bloop JSON.
fn find_scala_version_in_scala_build(scala_build_dir: &Path) -> Option<String> {
    // Check subdirectories for names matching hash patterns that might
    // contain version info in nearby bloop config
    let bloop_dir = scala_build_dir.join(".bloop");
    if bloop_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&bloop_dir) {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "json") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        // Quick scan for "scalaVersion" in bloop JSON
                        if let Some(ver) = extract_scala_version_from_json(&content) {
                            return Some(ver);
                        }
                    }
                }
            }
        }
    }
    None
}

/// Extract Scala version from bloop JSON content.
fn extract_scala_version_from_json(content: &str) -> Option<String> {
    // Look for "version":"3.x.y" inside a "scala" block
    let parsed: serde_json::Value = serde_json::from_str(content).ok()?;
    parsed
        .get("project")?
        .get("scala")?
        .get("version")?
        .as_str()
        .map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_scala_cli_info_main() {
        let root = Path::new("/project");
        let sdb_dir = Path::new(
            "/project/src/.scala-build/src_abc123/classes/main/META-INF/semanticdb",
        );
        let (segments, is_test) = extract_scala_cli_info(root, sdb_dir).unwrap();
        assert_eq!(segments, "src");
        assert!(!is_test);
    }

    #[test]
    fn test_extract_scala_cli_info_test() {
        let root = Path::new("/project");
        let sdb_dir = Path::new(
            "/project/src/.scala-build/src_abc123/classes/test/META-INF/semanticdb",
        );
        let (segments, is_test) = extract_scala_cli_info(root, sdb_dir).unwrap();
        assert_eq!(segments, "src");
        assert!(is_test);
    }

    #[test]
    fn test_extract_scala_cli_info_nested_source() {
        let root = Path::new("/project");
        let sdb_dir = Path::new(
            "/project/modules/core/.scala-build/core_abc123/classes/main/META-INF/semanticdb",
        );
        let (segments, is_test) = extract_scala_cli_info(root, sdb_dir).unwrap();
        assert_eq!(segments, "modules.core");
        assert!(!is_test);
    }

    #[test]
    fn test_extract_scala_cli_info_root_level() {
        let root = Path::new("/project");
        let sdb_dir = Path::new(
            "/project/.scala-build/project_abc123/classes/main/META-INF/semanticdb",
        );
        let (segments, is_test) = extract_scala_cli_info(root, sdb_dir).unwrap();
        assert_eq!(segments, "");
        assert!(!is_test);
    }

    #[test]
    fn test_extract_scala_version_from_json() {
        let json = r#"{"project":{"scala":{"version":"3.6.4","organization":"org.scala-lang"}}}"#;
        assert_eq!(
            extract_scala_version_from_json(json),
            Some("3.6.4".to_string())
        );
    }

    #[test]
    fn test_extract_scala_version_from_json_missing() {
        let json = r#"{"project":{"name":"foo"}}"#;
        assert_eq!(extract_scala_version_from_json(json), None);
    }
}
