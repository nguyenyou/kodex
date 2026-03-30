//! Mill build-tool provider.
//!
//! Discovers SemanticDB files from Mill's `out/` directory structure
//! and reads module metadata from Mill's cached JSON files.

use anyhow::{Context, Result};
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use super::provider::{
    collect_semanticdb_files, BuildMetadata, BuildProvider, DiscoveredFile, DiscoveryResult,
    ModuleInfo,
};

// ── Provider ───────────────────────────────────────────────────────────────

pub struct MillProvider;

impl BuildProvider for MillProvider {
    fn discover(&self, root: &Path) -> Result<DiscoveryResult> {
        let root = root
            .canonicalize()
            .context("Failed to resolve workspace root")?;
        let out_dir = root.join("out");
        if !out_dir.exists() {
            anyhow::bail!("No out/ directory found at {}", root.display());
        }

        // Phase 1: Find all semanticDbDataDetailed.dest directories + extract module segments
        let sdb_dirs: Vec<(PathBuf, String, PathBuf)> = WalkDir::new(&out_dir)
            .max_depth(8)
            .into_iter()
            .filter_map(std::result::Result::ok)
            .filter(|e| e.file_type().is_dir())
            .filter(|e| e.file_name() == "semanticDbDataDetailed.dest")
            .filter_map(|e| {
                let dest_dir = e.into_path();
                let sdb_dir = dest_dir.join("data/META-INF/semanticdb");
                if sdb_dir.exists() {
                    let segments = extract_module_segments(&out_dir, &dest_dir);
                    let module_dir = dest_dir.parent()?.to_path_buf();
                    Some((sdb_dir, segments, module_dir))
                } else {
                    None
                }
            })
            .collect();

        let module_out_dirs: HashMap<String, PathBuf> = sdb_dirs
            .iter()
            .map(|(_, segments, module_dir)| (segments.clone(), module_dir.clone()))
            .collect();

        // Phase 2: Walk each semanticdb directory for .semanticdb files (parallel)
        let files: Vec<DiscoveredFile> = sdb_dirs
            .par_iter()
            .flat_map(|(dir, module_segments, _)| {
                collect_semanticdb_files(dir, module_segments)
            })
            .collect();

        // Fallback: check out/META-INF/semanticdb/ directly (scalac with -semanticdb-target)
        let fallback_dir = out_dir.join("META-INF/semanticdb");
        let mut all_files = files;
        if fallback_dir.exists() {
            all_files.extend(collect_semanticdb_files(&fallback_dir, ""));
        }

        Ok(DiscoveryResult {
            files: all_files,
            module_out_dirs,
        })
    }

    fn metadata(&self, root: &Path, discovery: &DiscoveryResult) -> Result<Option<BuildMetadata>> {
        match read_mill_metadata_from_out(root, &discovery.module_out_dirs) {
            Ok(m) => Ok(Some(m)),
            Err(e) => {
                eprintln!("Warning: Mill metadata unavailable ({e}), continuing without it");
                Ok(None)
            }
        }
    }

}

// ── Discovery helpers ──────────────────────────────────────────────────────

/// Extract the module segment path from an `out/` directory path.
///
/// Given `out_dir` = `/project/out` and `sdb_dest_dir` = `/project/out/modules/billing/billing/jvm/semanticDbDataDetailed.dest`,
/// returns `"modules.billing.billing.jvm"`.
fn extract_module_segments(out_dir: &Path, sdb_dest_dir: &Path) -> String {
    let Ok(rel) = sdb_dest_dir.strip_prefix(out_dir) else {
        return String::new();
    };
    // rel = "modules/billing/billing/jvm/semanticDbDataDetailed.dest"
    // We want everything except the last component (the task .dest dir)
    let Some(parent) = rel.parent() else {
        return String::new();
    };
    // parent = "modules/billing/billing/jvm"
    parent
        .iter()
        .map(|c| c.to_string_lossy())
        .collect::<Vec<_>>()
        .join(".")
}

// ── Read metadata from out/ JSON cache ──────────────────────────────────────

/// Read Mill metadata from cached JSON files in `out/` directories.
///
/// Zero Mill CLI calls — all metadata is read from `out/` JSON caches:
/// - 8 task outputs read directly from `<module>/<task>.json`
/// - Module dependencies derived from `upstreamCompileOutput.json` (transitive deps)
fn read_mill_metadata_from_out(
    root: &Path,
    module_out_dirs: &HashMap<String, PathBuf>,
) -> Result<BuildMetadata> {
    if module_out_dirs.is_empty() {
        return Ok(BuildMetadata {
            modules: vec![],
            uri_rewrites: vec![],
        });
    }

    let out_dir = root
        .canonicalize()
        .context("Failed to resolve root")?
        .join("out");

    eprintln!(
        "Reading Mill metadata from out/ cache ({} modules)...",
        module_out_dirs.len()
    );

    // Build path->segments reverse lookup for upstreamCompileOutput parsing
    // Maps "out/core/3.3.7/compile.dest" -> "core.3.3.7"
    let path_to_segments: HashMap<String, String> = module_out_dirs
        .iter()
        .filter_map(|(segments, dir)| {
            let rel = dir.strip_prefix(&out_dir).ok()?;
            // upstreamCompileOutput references "out/<module>/compile.dest/..."
            // so the key is the relative module path (e.g. "core/3.3.7")
            Some((rel.to_string_lossy().to_string(), segments.clone()))
        })
        .collect();

    // Derive module deps from upstreamCompileOutput.json (transitive compile deps)
    let module_deps: HashMap<String, Vec<String>> = module_out_dirs
        .iter()
        .filter_map(|(name, dir)| {
            let deps = read_upstream_module_deps(dir, &out_dir, &path_to_segments);
            if deps.is_empty() {
                None
            } else {
                Some((name.clone(), deps))
            }
        })
        .collect();

    let modules: Vec<ModuleInfo> = module_out_dirs
        .iter()
        .map(|(name, dir)| ModuleInfo {
            segments: name.clone(),
            artifact_name: read_json_string(dir, "artifactName"),
            source_paths: read_json_pathref_list(dir, "sources"),
            generated_source_paths: read_json_pathref_list(dir, "generatedSources"),
            scala_version: read_json_string(dir, "scalaVersion"),
            scalac_options: read_json_string_list(dir, "scalacOptions"),
            main_class: read_json_string(dir, "mainClass"),
            test_framework: read_json_string(dir, "testFramework"),
            ivy_deps: read_ivy_deps(dir),
            module_deps: module_deps.get(name).cloned().unwrap_or_default(),
        })
        .collect();

    let uri_rewrites = build_shared_source_rewrites(&modules, &out_dir);
    if !uri_rewrites.is_empty() {
        eprintln!("  {} shared-source URI rewrite(s) detected", uri_rewrites.len());
    }

    eprintln!("  {} modules loaded", modules.len());
    Ok(BuildMetadata {
        modules,
        uri_rewrites,
    })
}

/// Derive module dependencies from `upstreamCompileOutput.json`.
///
/// Each entry's `analysisFile` path contains the upstream module's `out/` path,
/// e.g. `".../out/core/3.3.7/compile.dest/zinc"` -> module `core.3.3.7`.
fn read_upstream_module_deps(
    dir: &Path,
    out_dir: &Path,
    path_to_segments: &HashMap<String, String>,
) -> Vec<String> {
    let path = dir.join("upstreamCompileOutput.json");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return vec![];
    };
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) else {
        return vec![];
    };
    let Some(arr) = parsed.get("value").and_then(|v| v.as_array()) else {
        return vec![];
    };

    arr.iter()
        .filter_map(|entry| {
            let analysis = entry.get("analysisFile")?.as_str()?;
            // Extract module path: ".../out/core/3.3.7/compile.dest/zinc" -> "core/3.3.7"
            let analysis_path = Path::new(analysis);
            let rel = analysis_path.strip_prefix(out_dir).ok()?;
            // Walk up from "core/3.3.7/compile.dest/zinc" to "core/3.3.7"
            let module_rel = rel.parent()?.parent()?; // strip "zinc" then "compile.dest"
            path_to_segments
                .get(&module_rel.to_string_lossy().to_string())
                .cloned()
        })
        .collect()
}

// ── Shared-source URI rewriting ────────────────────────────────────────────

/// Build URI rewrite rules for cross-compiled shared sources.
///
/// In Mill cross-platform builds, shared sources are added differently:
///   - JVM: `sources += shared/src/` (canonical path)
///   - JS:  `generatedSources += out/.../jsSharedSources.dest/` (copy)
///
/// This causes SemanticDB to record different URIs for the same source file.
/// We detect these copies by finding `generatedSources` paths inside `out/`
/// whose file contents match a `sources` path from a sibling module.
///
/// Returns pairs of (out_prefix, canonical_prefix) as workspace-relative paths.
fn build_shared_source_rewrites(modules: &[ModuleInfo], out_dir: &Path) -> Vec<(String, String)> {
    let Some(root) = out_dir.parent() else {
        return vec![];
    };

    // Collect all real source paths that are NOT inside out/ across all modules.
    // Use starts_with(out_dir) instead of substring match to avoid false positives
    // on workspace paths containing "out" (e.g., /checkout-out/project/).
    let canonical_source_dirs: Vec<&str> = modules
        .iter()
        .flat_map(|m| m.source_paths.iter())
        .filter(|sp| !Path::new(sp.as_str()).starts_with(out_dir))
        .map(String::as_str)
        .collect();

    let mut rewrites: Vec<(String, String)> = Vec::new();

    for m in modules {
        for gen_path in &m.generated_source_paths {
            // Only process generated sources that live inside out/.
            // Canonicalize to handle macOS symlinks (/private/var vs /var).
            let Ok(gen_abs) = Path::new(gen_path).canonicalize() else {
                continue;
            };
            if !gen_abs.starts_with(out_dir) {
                continue;
            }
            if !gen_abs.is_dir() {
                continue;
            }
            for &canonical in &canonical_source_dirs {
                let Ok(canonical_abs) = Path::new(canonical).canonicalize() else {
                    continue;
                };
                if !canonical_abs.is_dir() {
                    continue;
                }
                if dirs_have_matching_files(&gen_abs, &canonical_abs) {
                    // Convert to workspace-relative paths for URI matching.
                    // Append "/" for path-boundary-safe prefix matching
                    // (prevents "dest2" matching "dest").
                    let gen_rel = format!(
                        "{}/",
                        gen_abs
                            .strip_prefix(root)
                            .unwrap_or(&gen_abs)
                            .to_string_lossy()
                    );
                    let canonical_rel = format!(
                        "{}/",
                        canonical_abs
                            .strip_prefix(root)
                            .unwrap_or(&canonical_abs)
                            .to_string_lossy()
                    );
                    rewrites.push((gen_rel, canonical_rel));
                }
            }
        }
    }

    rewrites.sort();
    rewrites.dedup();
    rewrites
}

/// Check if two directories contain matching files (same relative paths).
/// Returns true if ALL files in the generated dir also exist in the canonical dir.
fn dirs_have_matching_files(generated: &Path, canonical: &Path) -> bool {
    let gen_files: Vec<PathBuf> = WalkDir::new(generated)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.path().strip_prefix(generated).ok().map(PathBuf::from))
        .collect();

    if gen_files.is_empty() {
        return false;
    }
    // All generated files must exist in the canonical dir (true subset check)
    gen_files
        .iter()
        .all(|rel| canonical.join(rel).exists())
}

// ── Ivy dependency readers ──────────────────────────────────────────────────

/// Read ivy dependencies: prefer `mvnDeps.json` (direct, clean format),
/// fall back to `resolvedMvnDeps.json` (transitive, parsed from Coursier cache paths).
fn read_ivy_deps(dir: &Path) -> Vec<String> {
    let direct = read_json_string_list(dir, "mvnDeps");
    if !direct.is_empty() {
        return direct;
    }
    // Fallback: parse resolved deps from Coursier cache paths (qref format)
    read_resolved_mvn_deps(dir)
}

/// Parse `resolvedMvnDeps.json` entries.
///
/// Each entry is a `qref:v1:hash:/path/to/coursier/cache/.../group/artifact/version/artifact-version.jar`.
/// The Maven path convention gives us: `.../<group-as-dirs>/<artifact>/<version>/<filename>.jar`
fn read_resolved_mvn_deps(dir: &Path) -> Vec<String> {
    let entries = read_json_string_list(dir, "resolvedMvnDeps");
    let mut deps: Vec<String> = entries
        .iter()
        .filter_map(|entry| parse_qref_to_coordinate(entry))
        .collect();
    deps.sort();
    deps.dedup();
    deps
}

/// Extract `group:artifact:version` from a qref Coursier cache path.
///
/// Path layout: `.../maven2/<group-dirs>/<artifact>/<version>/<artifact>-<version>.jar`
/// or:          `.../artifactory/<repo>/<group-dirs>/<artifact>/<version>/<artifact>-<version>.jar`
fn parse_qref_to_coordinate(entry: &str) -> Option<String> {
    // Strip "qref:v1:hash:" prefix to get the path
    let path = entry.split(':').next_back()?;
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() < 4 {
        return None;
    }
    let version = parts[parts.len() - 2];
    let artifact = parts[parts.len() - 3];

    // Find repo root: "maven2" or the directory after "artifactory"
    let repo_end = parts.iter().enumerate().find_map(|(i, &p)| {
        if p == "maven2" {
            Some(i + 1)
        } else if p == "artifactory" {
            // artifactory/repo-name/group/... -> skip artifactory + repo name
            Some(i + 2)
        } else {
            None
        }
    })?;

    if repo_end >= parts.len() - 3 {
        return None;
    }

    let group = parts[repo_end..parts.len() - 3].join(".");
    Some(format!("{group}:{artifact}:{version}"))
}

// ── JSON cache readers ─────────────────────────────────────────────────────

/// Read a string value from `<dir>/<task>.json`.
fn read_json_string(dir: &Path, task: &str) -> String {
    let path = dir.join(format!("{task}.json"));
    let Ok(content) = std::fs::read_to_string(&path) else {
        return String::new();
    };
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) else {
        return String::new();
    };
    parsed
        .get("value")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// Read a string-list value from `<dir>/<task>.json`.
fn read_json_string_list(dir: &Path, task: &str) -> Vec<String> {
    let path = dir.join(format!("{task}.json"));
    let Ok(content) = std::fs::read_to_string(&path) else {
        return vec![];
    };
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) else {
        return vec![];
    };
    parsed
        .get("value")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(std::string::ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Read a `PathRef`-list value from `<dir>/<task>.json`, stripping `ref:` prefixes.
fn read_json_pathref_list(dir: &Path, task: &str) -> Vec<String> {
    let path = dir.join(format!("{task}.json"));
    let Ok(content) = std::fs::read_to_string(&path) else {
        return vec![];
    };
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) else {
        return vec![];
    };
    parsed
        .get("value")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| strip_ref_prefix(s).to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Strip "ref:v0:hash:" prefix from Mill PathRef strings.
fn strip_ref_prefix(s: &str) -> &str {
    if let Some(rest) = s.strip_prefix("ref:") {
        let mut colons = 0;
        for (i, c) in rest.char_indices() {
            if c == ':' {
                colons += 1;
                if colons == 2 {
                    return &rest[i + 1..];
                }
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_module_segments() {
        let out_dir = PathBuf::from("/project/out");

        assert_eq!(
            extract_module_segments(
                &out_dir,
                &PathBuf::from(
                    "/project/out/modules/billing/billing/jvm/semanticDbDataDetailed.dest"
                )
            ),
            "modules.billing.billing.jvm"
        );

        assert_eq!(
            extract_module_segments(
                &out_dir,
                &PathBuf::from("/project/out/platform/database/semanticDbDataDetailed.dest")
            ),
            "platform.database"
        );

        assert_eq!(
            extract_module_segments(
                &out_dir,
                &PathBuf::from("/project/out/webapp/webapp/jvm/semanticDbDataDetailed.dest")
            ),
            "webapp.webapp.jvm"
        );

        // Cross-compilation value as path segment
        assert_eq!(
            extract_module_segments(
                &out_dir,
                &PathBuf::from("/project/out/modules/billing/2.12/jvm/semanticDbDataDetailed.dest")
            ),
            "modules.billing.2.12.jvm"
        );

        // Single segment module
        assert_eq!(
            extract_module_segments(
                &out_dir,
                &PathBuf::from("/project/out/core/semanticDbDataDetailed.dest")
            ),
            "core"
        );
    }

    #[test]
    fn test_strip_ref_prefix() {
        assert_eq!(
            strip_ref_prefix("ref:v0:559c103b:/Users/foo/src"),
            "/Users/foo/src"
        );
        assert_eq!(strip_ref_prefix("/plain/path"), "/plain/path");
    }

    #[test]
    fn test_parse_qref_maven_central() {
        let entry = "qref:v1:aabbccdd:/home/user/.cache/coursier/v1/https/repo1.maven.org/maven2/org/example/my-lib_3/1.2.3/my-lib_3-1.2.3.jar";
        assert_eq!(
            parse_qref_to_coordinate(entry),
            Some("org.example:my-lib_3:1.2.3".to_string())
        );
    }

    #[test]
    fn test_parse_qref_artifactory() {
        let entry = "qref:v1:aabbccdd:/home/user/.cache/coursier/v1/https/build%40artifactory.example.com/artifactory/libs-release/com/example/my-runtime_sjs1_3/0.1.0/my-runtime_sjs1_3-0.1.0.jar";
        assert_eq!(
            parse_qref_to_coordinate(entry),
            Some("com.example:my-runtime_sjs1_3:0.1.0".to_string())
        );
    }

    #[test]
    fn test_parse_qref_deep_group() {
        let entry = "qref:v1:aabbccdd:/home/user/.cache/coursier/v1/https/repo1.maven.org/maven2/com/github/acme/json-tools/json-tools-core_3/2.0.0/json-tools-core_3-2.0.0.jar";
        assert_eq!(
            parse_qref_to_coordinate(entry),
            Some("com.github.acme.json-tools:json-tools-core_3:2.0.0".to_string())
        );
    }
}
