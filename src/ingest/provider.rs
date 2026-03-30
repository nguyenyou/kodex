//! Build-tool provider trait and shared types.
//!
//! Each build tool (Mill, sbt, etc.) implements `BuildProvider` to supply
//! SemanticDB discovery and module metadata.

use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use super::fallback::FallbackProvider;
use super::mill::MillProvider;
use super::sbt::SbtProvider;
use super::scala_cli::ScalaCliProvider;

// ── Shared types returned by all providers ─────────────────────────────────

/// A discovered .semanticdb file tagged with its module.
pub struct DiscoveredFile {
    pub path: PathBuf,
    /// Module segment path, e.g. "modules.billing.billing.jvm".
    /// Empty string for files without a module assignment.
    pub module_segments: String,
}

/// Result of the discovery phase.
pub struct DiscoveryResult {
    pub files: Vec<DiscoveredFile>,
    /// Module segments -> module's output directory (used for metadata reading).
    pub module_out_dirs: HashMap<String, PathBuf>,
}

/// Build-tool-agnostic module metadata.
pub struct BuildMetadata {
    pub modules: Vec<ModuleInfo>,
    /// URI prefix rewrites for cross-compiled shared sources.
    ///
    /// When a build tool copies shared sources to a build output directory
    /// (e.g., Mill's `generatedSources` copying `shared/src/` to
    /// `out/.../jsSharedSources.dest/`), SemanticDB records the copy's path
    /// as the URI. This map rewrites those URIs back to the canonical source path.
    ///
    /// Key: `out/` prefix (relative to workspace root, with trailing slash)
    /// Value: canonical source prefix (relative to workspace root, with trailing slash)
    pub uri_rewrites: Vec<(String, String)>,
}

/// Per-module metadata returned by a build provider.
pub struct ModuleInfo {
    /// Module segment path, e.g. "modules.billing.billing.jvm"
    pub segments: String,
    /// Human-readable artifact name, e.g. "my-app-server"
    pub artifact_name: String,
    /// Source directories (absolute paths)
    pub source_paths: Vec<String>,
    /// Generated source directories (absolute paths)
    pub generated_source_paths: Vec<String>,
    /// Scala version, e.g. "3.8.2"
    pub scala_version: String,
    /// Scalac options, e.g. `["-deprecation", "-feature"]`
    pub scalac_options: Vec<String>,
    /// Module dependencies (segment paths)
    pub module_deps: Vec<String>,
    /// External dependencies, e.g. "org.typelevel::cats-effect:3.5.4"
    pub ivy_deps: Vec<String>,
    /// Configured main class (FQN), e.g. "com.example.MyApp". Empty if not set.
    pub main_class: String,
    /// Test framework (only for test modules), e.g. "org.scalatest.tools.Framework"
    pub test_framework: String,
}

/// Walk `dir` for `.semanticdb` files and tag each with `module_segments`.
pub fn collect_semanticdb_files(dir: &Path, module_segments: &str) -> Vec<DiscoveredFile> {
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
            module_segments: module_segments.to_string(),
        })
        .collect()
}

/// Check if any path component equals the given name.
pub(crate) fn path_contains_component(path: &Path, name: &str) -> bool {
    path.iter().any(|c| c == name)
}

/// Walk up from `path` to find the nearest ancestor directory with the given name.
pub(crate) fn find_ancestor_named(path: &Path, name: &str) -> Option<PathBuf> {
    let mut current = path;
    while let Some(parent) = current.parent() {
        if parent.file_name().is_some_and(|n| n == name) {
            return Some(parent.to_path_buf());
        }
        current = parent;
    }
    None
}

// ── Provider trait ─────────────────────────────────────────────────────────

/// Trait that build-tool adapters implement.
///
/// Mill, sbt, etc. each get their own implementation.
pub trait BuildProvider {
    /// Discover .semanticdb files and assign them to modules.
    fn discover(&self, root: &Path) -> Result<DiscoveryResult>;

    /// Read module metadata (artifact names, deps, scala version, etc.)
    /// Returns `None` if metadata is unavailable.
    fn metadata(&self, root: &Path, discovery: &DiscoveryResult) -> Result<Option<BuildMetadata>>;
}

// ── Auto-detection ─────────────────────────────────────────────────────────

/// Detect the appropriate build provider for a workspace.
pub fn detect_provider(root: &Path) -> Box<dyn BuildProvider> {
    if root.join("build.mill").exists()
        || root.join("build.mill.scala").exists()
        || root.join("build.sc").exists()
    {
        Box::new(MillProvider)
    } else if root.join("build.sbt").exists()
        || root.join("project").join("build.properties").exists()
    {
        Box::new(SbtProvider)
    } else if has_scala_build_dir(root) {
        Box::new(ScalaCliProvider)
    } else {
        Box::new(FallbackProvider)
    }
}

/// Check if this looks like a scala-cli project by finding `.scala-build/`
/// in the root or any direct child directory.
fn has_scala_build_dir(root: &Path) -> bool {
    if root.join(".scala-build").exists() {
        return true;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return false;
    };
    entries.filter_map(Result::ok).any(|entry| {
        entry.file_type().is_ok_and(|ft| ft.is_dir()) && entry.path().join(".scala-build").exists()
    })
}
