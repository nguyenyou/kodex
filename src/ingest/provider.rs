//! Build-tool provider trait and shared types.
//!
//! Each build tool (Mill, sbt, etc.) implements `BuildProvider` to supply
//! SemanticDB discovery and module metadata.

use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
