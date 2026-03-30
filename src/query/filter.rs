use super::{file_entry, s, sym as sym_at};
use crate::model::{ArchivedKodexIndex, ArchivedSymbol, ArchivedSymbolKind, NONE_ID};
use rustc_hash::FxHashSet;
use std::sync::LazyLock;

/// Standard library / runtime prefixes excluded from call graphs and output.
/// Note: "scala/" is intentionally NOT a prefix — user code often lives in `scala.*`
/// (e.g. scala-cli's `scala/cli/`, `scala/build/`).
const STDLIB_PREFIXES: &[&str] = &[
    "scala/Predef",
    "scala/Option",
    "scala/Some",
    "scala/None",
    "scala/collection/",
    "scala/runtime/",
    "scala/reflect/",
    "scala/math/",
    "scala/util/",
    "scala/io/",
    "scala/sys/",
    "scala/concurrent/",
    "scala/jdk/",
    "scala/compiletime/",
    "scala/deriving/",
    "scala/quoted/",
    "scala/Any",
    "scala/AnyRef",
    "scala/AnyVal",
    "scala/Nothing",
    "scala/Null",
    "scala/Unit",
    "scala/Boolean",
    "scala/Byte",
    "scala/Short",
    "scala/Int",
    "scala/Long",
    "scala/Float",
    "scala/Double",
    "scala/Char",
    "scala/String",
    "scala/Array",
    "scala/Tuple",
    "scala/Product",
    "scala/Serializable",
    "scala/Function",
    "scala/PartialFunction",
    "scala/Enumeration",
    "scala/StringContext",
    "java/lang/",
    "java/util/",
    "java/io/",
    "java/net/",
];

/// Effect plumbing method names excluded from call-graph/callees output (O(1) lookup).
static PLUMBING_METHODS: LazyLock<FxHashSet<&'static str>> = LazyLock::new(|| {
    [
        "apply",
        "unapply",
        "toString",
        "hashCode",
        "equals",
        "copy",
        "map",
        "flatMap",
        "filter",
        "foreach",
        "collect",
        "foldLeft",
        "foldRight",
        "get",
        "getOrElse",
        "orElse",
        "isEmpty",
        "nonEmpty",
        "isDefined",
        "mkString",
        "productElement",
        "productPrefix",
        "canEqual",
        "productArity",
        "productIterator",
        "productElementName",
        "succeed",
        "pure",
        "attempt",
        "fromOption",
        "when",
        "unless",
        "traverse",
        "traverseOption",
        "traverseOptionUnit",
        "foreachDiscard",
        "validate",
        "parTraverseN",
    ]
    .into_iter()
    .collect()
});

use crate::model::{PROP_VAL, PROP_VAR};

/// Check if a symbol FQN is from the standard library / runtime.
pub fn is_stdlib(fqn: &str) -> bool {
    STDLIB_PREFIXES.iter().any(|p| fqn.starts_with(p))
}

/// Check if a symbol is an effect plumbing method (apply, map, flatMap, etc.)
pub fn is_plumbing(name: &str) -> bool {
    PLUMBING_METHODS.contains(name)
}

/// Check if a symbol is a val/var field accessor (reading a dependency, not a real call).
pub fn is_val_accessor(sym: &ArchivedSymbol) -> bool {
    let props: u32 = sym.properties.into();
    // val or var fields on classes/traits — these are dependency reads, not service calls
    (props & PROP_VAL != 0 || props & PROP_VAR != 0)
        && matches!(
            sym.kind,
            ArchivedSymbolKind::Method | ArchivedSymbolKind::Field
        )
}

/// Check if a file is a test file (using pre-classified flag from index).
pub fn is_test_file(index: &ArchivedKodexIndex, file_id: u32) -> bool {
    index.files.get(file_id as usize).is_some_and(|f| f.is_test)
}

/// Check if a file is generated code (using pre-classified flag from index).
pub fn is_generated_file(index: &ArchivedKodexIndex, file_id: u32) -> bool {
    index
        .files
        .get(file_id as usize)
        .is_some_and(|f| f.is_generated)
}

/// Check if a symbol should be excluded from default output.
/// Excludes: stdlib, test, generated, plumbing, case class synthetics.
pub fn is_noise(index: &ArchivedKodexIndex, sym: &ArchivedSymbol) -> bool {
    let fqn = s(index, sym.fqn);
    let name = s(index, sym.name);
    let file_id: u32 = sym.file_id.into();

    is_stdlib(fqn)
        || is_test_file(index, file_id)
        || is_generated_file(index, file_id)
        || is_plumbing(name)
}

/// Check if a symbol is noise for call graph output (callers/callees/call-graph).
/// Filters: universal noise + val accessors + synthetics + user --exclude patterns.
pub fn is_callgraph_noise(index: &ArchivedKodexIndex, sym: &ArchivedSymbol) -> bool {
    if is_noise(index, sym) {
        return true;
    }
    let name = s(index, sym.name);
    // Default parameter accessors
    if name.contains("$default$") {
        return true;
    }
    // Tuple field accessors (_1, _2, ...)
    if name.len() >= 2 && name.starts_with('_') && name[1..].chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    // Val/var field reads — dependency wiring, not real calls
    if is_val_accessor(sym) {
        return true;
    }
    false
}

/// Check if a symbol matches any user-provided --exclude patterns.
/// Patterns match against both FQN and owner name (substring match).
pub fn matches_exclude(
    index: &ArchivedKodexIndex,
    sym: &ArchivedSymbol,
    exclude: &[String],
) -> bool {
    if exclude.is_empty() {
        return false;
    }
    let fqn = s(index, sym.fqn);
    let name = s(index, sym.name);
    let owner_id: u32 = sym.owner.into();
    let owner_name = if owner_id != NONE_ID && (owner_id as usize) < index.symbols.len() {
        s(index, sym_at(index, owner_id).name)
    } else {
        ""
    };

    exclude.iter().any(|pattern| {
        fqn.contains(pattern.as_str())
            || name.contains(pattern.as_str())
            || owner_name.contains(pattern.as_str())
    })
}

/// Check if a module name matches a pattern.
///
/// If the pattern has no dots, uses simple case-insensitive substring matching
/// (backward compatible). If the pattern contains dots, splits both on `.` and
/// checks that all pattern segments appear in order as substrings of name segments.
///
/// Example: `"billing.js"` matches `"modules.billing.billing.js"` because
/// `"billing"` ⊂ segment `"billing"` and `"js"` ⊂ segment `"js"` (in order).
pub fn module_name_matches(mod_name: &str, pattern: &str) -> bool {
    let pattern_lower = pattern.to_ascii_lowercase();
    // Fast path: no dots → simple substring match (backward compat)
    if !pattern.contains('.') {
        return crate::hash::contains_ignore_ascii_case(mod_name, &pattern_lower);
    }
    let name_lower = mod_name.to_ascii_lowercase();
    let pattern_segs: Vec<&str> = pattern_lower.split('.').filter(|s| !s.is_empty()).collect();
    if pattern_segs.is_empty() {
        return true;
    }
    let name_segs: Vec<&str> = name_lower.split('.').collect();
    let mut ni = 0;
    for pseg in &pattern_segs {
        let mut found = false;
        while ni < name_segs.len() {
            if name_segs[ni].contains(pseg) {
                ni += 1;
                found = true;
                break;
            }
            ni += 1;
        }
        if !found {
            return false;
        }
    }
    true
}

/// Filter symbols to those belonging to a module matching the pattern.
pub fn filter_by_module<'a>(
    index: &'a ArchivedKodexIndex,
    symbols: &[&'a ArchivedSymbol],
    module_pattern: &str,
) -> Vec<&'a ArchivedSymbol> {
    symbols
        .iter()
        .filter(|sym| {
            let file_id: u32 = sym.file_id.into();
            if file_id as usize >= index.files.len() {
                return false;
            }
            let module_id: u32 = file_entry(index, file_id).module_id.into();
            if module_id == NONE_ID || module_id as usize >= index.modules.len() {
                return false;
            }
            let m = &index.modules[module_id as usize];
            let mod_name = s(index, m.name);
            let artifact = s(index, m.artifact_name);
            module_name_matches(mod_name, module_pattern)
                || (!artifact.is_empty() && module_name_matches(artifact, module_pattern))
        })
        .copied()
        .collect()
}

/// Check if a symbol is synthetic boilerplate that should be hidden in file listings.
/// Checks kind, name patterns, and val constructor params.
pub fn is_synthetic_symbol(index: &ArchivedKodexIndex, sym: &ArchivedSymbol) -> bool {
    if matches!(
        sym.kind,
        ArchivedSymbolKind::Parameter
            | ArchivedSymbolKind::TypeParameter
            | ArchivedSymbolKind::SelfParameter
            | ArchivedSymbolKind::Local
    ) {
        return true;
    }
    let name = s(index, sym.name);
    if is_synthetic_name(name) {
        return true;
    }
    // Constructor val params shown as methods — filter if owner is a case class constructor
    let props: u32 = sym.properties.into();
    if props & PROP_VAL != 0 && matches!(sym.kind, ArchivedSymbolKind::Method) {
        let fqn = s(index, sym.fqn);
        // val methods whose FQN contains `<init>` parent are constructor params
        if fqn.contains("#`<init>`") {
            return true;
        }
    }
    false
}

/// Check if a symbol name is synthetic boilerplate (case class copy, tuple accessors, etc.)
///
/// Uses first-byte dispatch to avoid sequential `starts_with` checks on the common
/// path (most names don't start with `_`, `$`, `c`, `d`, or `g`).
pub fn is_synthetic_name(name: &str) -> bool {
    if is_plumbing(name) {
        return true;
    }
    // Default parameter accessors: foo$default$1, $default$2, copy$default$3, etc.
    if name.contains("$default$") {
        return true;
    }
    // Constructor
    if name == "<init>" {
        return true;
    }
    match name.as_bytes().first() {
        Some(b'_') => {
            // Tuple field accessors (_1, _2, ...)
            name.len() >= 2 && name[1..].bytes().all(|b| b.is_ascii_digit())
        }
        Some(b'$') => {
            name.starts_with("$anon")
        }
        Some(b'd') => name.starts_with("derived$"),
        Some(b'g') => {
            name.starts_with("given_")
                || name == "getMessage"
        }
        Some(b'w') => name == "writeReplace",
        Some(b'o') => name == "ordinal",
        _ => false,
    }
}

/// Minimum reference count to consider a symbol an infrastructure hub.
const INFRA_HUB_MIN_REFS: usize = 200;

/// Detect infrastructure "hub" symbols — high ref count, utility nature.
/// Returns (name, ref_count) pairs for the top N infrastructure symbols.
/// Used by `noise` to suggest --exclude patterns to the agent.
pub fn detect_infra_hubs(index: &ArchivedKodexIndex, top_n: usize) -> Vec<(String, usize)> {
    use rustc_hash::FxHashMap;

    // Count refs per symbol
    let mut ref_counts: FxHashMap<u32, usize> = FxHashMap::default();
    for rl in index.references.iter() {
        let sid: u32 = rl.symbol_id.into();
        ref_counts.insert(sid, rl.refs.len());
    }

    // Find objects/classes with very high ref counts that look like infrastructure
    let mut candidates: Vec<(String, usize)> = Vec::new();
    for (&sid, &count) in &ref_counts {
        if count < INFRA_HUB_MIN_REFS {
            continue;
        }
        let sym = sym_at(index, sid);
        if !matches!(
            sym.kind,
            ArchivedSymbolKind::Object | ArchivedSymbolKind::Class
        ) {
            continue;
        }
        let file_id: u32 = sym.file_id.into();
        if is_test_file(index, file_id) || is_generated_file(index, file_id) {
            continue;
        }

        let name = s(index, sym.name);
        // Heuristic: infrastructure symbols have utility-like names
        let looks_infra = name.contains("Utils")
            || name.contains("Ops")
            || name.contains("Operations")
            || name.contains("Helper")
            || name.contains("IO")
            || name.contains("Factory")
            || name.ends_with("Store")
            || name.ends_with("Database")
            || name.ends_with("Mapping")
            || name.ends_with("Converter")
            || name.ends_with("Provider")
            || name.ends_with("Enum")
            || name.ends_with("Companion")
            || name.ends_with("Defaults");
        if looks_infra {
            candidates.push((name.to_string(), count));
        }
    }

    candidates.sort_by(|a, b| b.1.cmp(&a.1));
    candidates.truncate(top_n);
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_stdlib() {
        assert!(is_stdlib("scala/Option#"));
        assert!(is_stdlib("scala/Some#"));
        assert!(is_stdlib("scala/None."));
        assert!(is_stdlib("java/lang/String#"));
        assert!(is_stdlib("java/util/List#"));
        assert!(is_stdlib("scala/collection/immutable/List#"));
        assert!(is_stdlib("scala/runtime/BoxedUnit#"));
        assert!(is_stdlib("scala/Predef.println()."));
        assert!(is_stdlib("scala/Int#"));
        assert!(!is_stdlib("com/example/Foo#"));
        assert!(!is_stdlib("org/apache/Bar#"));
        // User code in scala.* namespace must NOT be filtered
        assert!(!is_stdlib("scala/cli/ScalaCli.main()."));
        assert!(!is_stdlib("scala/build/Build.build()."));
        assert!(!is_stdlib(""));
    }

    #[test]
    fn test_module_name_matches() {
        // No-dot pattern: simple substring (backward compat)
        assert!(module_name_matches("modules.billing", "billing"));
        assert!(module_name_matches("modules.billing", "BILLING"));
        assert!(module_name_matches("modules.billing", "bill"));
        assert!(!module_name_matches("modules.billing", "shipping"));

        // Dotted pattern: ordered segment subsequence
        assert!(module_name_matches("modules.billing.billing.js", "billing.js"));
        assert!(module_name_matches("modules.core.core.jvm", "core.jvm"));
        assert!(module_name_matches("modules.api.api.js", "api.js"));

        // Order matters
        assert!(!module_name_matches("modules.app.core", "core.app"));

        // Multiple segments
        assert!(module_name_matches("platform.core.api.jvm", "core.api.jvm"));
        assert!(module_name_matches("platform.core.api.jvm", "core.jvm"));

        // Empty / edge cases
        assert!(module_name_matches("modules.billing", ""));
        assert!(!module_name_matches("modules.billing", "nonexistent.thing"));

        // Trailing dot should not over-match (empty segments filtered)
        assert!(module_name_matches("modules.billing.billing.js", "billing."));
        // ^ "billing." splits to ["billing"] after filtering empty, same as "billing"
    }

    // Regression: empty segments from trailing/leading dots should be ignored.
    // Without fix, "js." splits to ["js", ""] — the empty segment consumes an
    // extra name segment, causing mismatches when the name has fewer segments.
    #[test]
    fn test_module_trailing_dot_ignored() {
        // "js." should behave like "js" — trailing empty segment must not consume a slot
        assert!(module_name_matches("modules.billing.js", "js."));
        // "billing." should still work
        assert!(module_name_matches("modules.billing.billing.js", "billing."));
        assert!(!module_name_matches("modules.shipping.shipping.js", "billing."));
    }

    // Regression: all-dot pattern "." had no real segments after filtering.
    // Without fix, "." splits to ["", ""] which tries to match 2 empty segments
    // and fails when the name doesn't have enough segments.
    #[test]
    fn test_module_all_dot_pattern() {
        assert!(module_name_matches("modules.billing", "."));
        assert!(module_name_matches("modules.billing", ".."));
        assert!(module_name_matches("a", "."));
    }

    #[test]
    fn test_is_plumbing() {
        assert!(is_plumbing("apply"));
        assert!(is_plumbing("flatMap"));
        assert!(is_plumbing("traverse"));
        assert!(is_plumbing("productPrefix"));
        assert!(is_plumbing("succeed"));
        assert!(!is_plumbing("process"));
        assert!(!is_plumbing("createUser"));
        assert!(!is_plumbing(""));
    }
}
