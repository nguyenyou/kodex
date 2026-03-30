//! File classification (metadata-aware with path heuristic fallback) and module registration.

use crate::ingest::interner::StringInterner;
use crate::ingest::provider::BuildMetadata;
use crate::model::*;
use rustc_hash::{FxHashMap, FxHashSet};

// ── File classification ─────────────────────────────────────────────────────

/// Collect module segments that have a test framework (= are test modules).
pub fn test_modules(metadata: Option<&BuildMetadata>) -> FxHashSet<String> {
    let mut set = FxHashSet::default();
    if let Some(m) = metadata {
        for minfo in &m.modules {
            if !minfo.test_framework.is_empty() {
                set.insert(minfo.segments.clone());
            }
        }
    }
    set
}

/// Collect generated source directory prefixes, made relative to workspace root.
///
/// Excludes shared-source paths (`jsSharedSources.dest`, `jvmSharedSources.dest`)
/// which are actually cross-compiled copies of hand-written source files, not codegen output.
pub fn generated_prefixes(metadata: Option<&BuildMetadata>, workspace_root: &str) -> Vec<String> {
    let mut prefixes = Vec::new();
    if let Some(m) = metadata {
        let root_prefix = if workspace_root.ends_with('/') {
            workspace_root.to_string()
        } else {
            format!("{workspace_root}/")
        };
        for minfo in &m.modules {
            for gp in &minfo.generated_source_paths {
                let lower = gp.to_ascii_lowercase();
                if lower.contains("sharedsources.dest") {
                    continue; // cross-compiled copy, not codegen
                }
                let relative = gp.strip_prefix(&root_prefix).unwrap_or(gp);
                prefixes.push(relative.to_string());
            }
        }
    }
    prefixes
}

/// Classify a file as test using build metadata (module has `test_framework`),
/// module segment patterns, or path heuristic fallback.
#[allow(
    clippy::case_sensitive_file_extension_comparisons,
    clippy::implicit_hasher
)]
pub fn classify_test(
    module_segments: &str,
    test_modules: &FxHashSet<String>,
    uri: &str,
) -> bool {
    if !module_segments.is_empty() && test_modules.contains(module_segments) {
        return true;
    }
    if !module_segments.is_empty() {
        let segs = module_segments;
        if segs.ends_with(".test")
            || segs.ends_with(".it")
            || segs.contains(".test.")
            || segs.contains(".it.")
            || segs.ends_with(".multiregionit")
        {
            return true;
        }
    }
    let lower = uri.to_ascii_lowercase();
    lower.contains("/test/")
        || lower.contains("/tests/")
        || lower.contains("/it/")
        || lower.contains("/spec/")
        || lower.ends_with("test.scala")
        || lower.ends_with("spec.scala")
        || lower.ends_with("suite.scala")
        || lower.ends_with("integ.scala")
}

/// Classify a file as generated using build metadata `generatedSources` paths,
/// or path heuristic fallback.
pub fn classify_generated(generated_prefixes: &[String], uri: &str) -> bool {
    for prefix in generated_prefixes {
        if uri.starts_with(prefix.as_str()) {
            return true;
        }
    }
    let lower = uri.to_ascii_lowercase();
    lower.contains("compilescalapb.dest")
        || lower.contains("compilepb.dest")
        || lower.contains("/generated/")
        || lower.contains("/src_managed/")
        || lower.contains("generatedsources")
        || lower.ends_with(".pb.scala")
        || lower.ends_with("grpc.scala")
        || lower.contains("buildinfo.scala")
}

// ── Module registration ─────────────────────────────────────────────────────

/// Register a module by its segment path (from discovery).
/// Returns `module_id`, or `NONE_ID` if segment path is empty.
#[allow(clippy::implicit_hasher)]
pub fn register_module(
    segments: &str,
    module_map: &mut FxHashMap<String, u32>,
    modules: &mut Vec<Module>,
    interner: &mut StringInterner,
) -> u32 {
    if segments.is_empty() {
        return NONE_ID;
    }
    if let Some(&id) = module_map.get(segments) {
        modules[id as usize].file_count += 1;
        return id;
    }
    let id = modules.len() as u32;
    module_map.insert(segments.to_string(), id);
    let empty = interner.intern("");
    modules.push(Module {
        name: interner.intern(segments),
        artifact_name: empty,
        source_paths: vec![],
        generated_source_paths: vec![],
        scala_version: empty,
        scalac_options: vec![],
        main_class: empty,
        test_framework: empty,
        file_count: 1,
        symbol_count: 0,
    });
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── classify_test ─────────────────────────────────────────────────

    #[test]
    fn test_classify_test_from_framework() {
        let mut test_modules = FxHashSet::default();
        test_modules.insert("webapp.webapp.jvm.test".to_string());
        assert!(classify_test(
            "webapp.webapp.jvm.test",
            &test_modules,
            "webapp/webapp/jvm/test/src/Foo.scala"
        ));
    }

    #[test]
    fn test_classify_test_from_segments() {
        let test_modules = FxHashSet::default();
        assert!(classify_test(
            "webapp.webapp.jvm.test",
            &test_modules,
            "src/Foo.scala"
        ));
        assert!(classify_test(
            "platform.database.it",
            &test_modules,
            "src/Foo.scala"
        ));
        assert!(classify_test(
            "webapp.webapp.jvm.multiregionit",
            &test_modules,
            "src/Foo.scala"
        ));
        assert!(classify_test(
            "modules.chatbot.chatbot.jvm.it.something",
            &test_modules,
            "src/Foo.scala"
        ));
    }

    #[test]
    fn test_classify_test_fallback() {
        let test_modules = FxHashSet::default();
        assert!(classify_test(
            "",
            &test_modules,
            "modules/billing/billing/jvm/test/src/Foo.scala"
        ));
        assert!(classify_test("", &test_modules, "FooTest.scala"));
        assert!(classify_test("", &test_modules, "FooSpec.scala"));
    }

    #[test]
    fn test_classify_test_negative() {
        let test_modules = FxHashSet::default();
        assert!(!classify_test(
            "webapp.webapp.jvm",
            &test_modules,
            "webapp/webapp/jvm/src/Foo.scala"
        ));
        assert!(!classify_test(
            "",
            &test_modules,
            "modules/billing/billing/jvm/src/Foo.scala"
        ));
    }

    // ── classify_generated ───────────────────────────────────────────

    #[test]
    fn test_classify_generated_from_prefix() {
        let prefixes = vec![
            "out/webapp/webapp/js/jsSharedSources.dest".to_string(),
            "out/platform/cue/js/jsSharedSources.dest".to_string(),
        ];
        assert!(classify_generated(
            &prefixes,
            "out/webapp/webapp/js/jsSharedSources.dest/com/Foo.scala"
        ));
        assert!(!classify_generated(
            &prefixes,
            "webapp/webapp/jvm/src/Foo.scala"
        ));
    }

    #[test]
    fn test_classify_generated_fallback() {
        let prefixes: Vec<String> = vec![];
        assert!(classify_generated(
            &prefixes,
            "modules/billing/compilescalapb.dest/Foo.scala"
        ));
        assert!(classify_generated(&prefixes, "Foo.pb.scala"));
        assert!(classify_generated(
            &prefixes,
            "modules/billing/src/buildinfo.scala"
        ));
    }

    #[test]
    fn test_classify_generated_negative() {
        let prefixes: Vec<String> = vec![];
        assert!(!classify_generated(
            &prefixes,
            "modules/billing/billing/jvm/src/Foo.scala"
        ));
    }

    // Regression: jsSharedSources.dest and jvmSharedSources.dest are cross-compiled
    // copies of hand-written source, not codegen. They should NOT be classified as
    // generated even though Mill reports them in generatedSources.
    #[test]
    fn test_classify_shared_sources_not_generated() {
        // Simulate what Mill reports: jsSharedSources.dest in generatedSources
        let prefixes = generated_prefixes(
            Some(&crate::ingest::provider::BuildMetadata {
                modules: vec![crate::ingest::provider::ModuleInfo {
                    segments: "modules.app.js".to_string(),
                    artifact_name: String::new(),
                    source_paths: vec![],
                    generated_source_paths: vec![
                        "/workspace/out/modules/app/js/jsSharedSources.dest".to_string(),
                        "/workspace/out/modules/app/js/compileScalaPB.dest".to_string(),
                    ],
                    scala_version: String::new(),
                    scalac_options: vec![],
                    module_deps: vec![],
                    ivy_deps: vec![],
                    main_class: String::new(),
                    test_framework: String::new(),
                }],
                uri_rewrites: vec![],
            }),
            "/workspace",
        );
        // Shared source should be excluded from prefixes
        assert!(
            !prefixes.iter().any(|p| p.contains("jsSharedSources")),
            "jsSharedSources.dest should NOT be in generated prefixes, got: {:?}",
            prefixes
        );
        // But compileScalaPB should remain
        assert!(
            prefixes.iter().any(|p| p.contains("compileScalaPB")),
            "compileScalaPB.dest should still be in generated prefixes"
        );
    }

    // ── metadata helpers ──────────────────────────────────────────────

    #[test]
    fn test_test_modules_empty() {
        let result = test_modules(None);
        assert!(result.is_empty());
    }

    #[test]
    fn test_generated_prefixes_empty() {
        let result = generated_prefixes(None, "/workspace");
        assert!(result.is_empty());
    }
}
