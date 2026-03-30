use super::CommandResult;
use crate::model::{ArchivedKodexIndex, ArchivedSymbolKind, NONE_ID};
use crate::query::filter;
use crate::query::format::owner_name;
use crate::query::symbol::kind_str;
use crate::query::{file_entry, s, sym as sym_at};
use rustc_hash::{FxHashMap, FxHashSet};
use std::fmt::Write;

/// Minimum reverse edge count (call sites) to consider a method as effect plumbing.
const PLUMBING_MIN_CALL_SITES: usize = 5;
/// Maximum forward edge count (callees) for a method to be considered a leaf.
const PLUMBING_MAX_CALLEES: usize = 1;

/// Minimum reference count to consider a type as a hub utility.
const HUB_MIN_REFS: usize = 100;
/// Minimum module spread for hub utilities.
const HUB_MIN_MODULES: usize = 3;

/// Infrastructure plumbing: method call sites threshold.
const INFRA_MIN_CALL_SITES: usize = 10;
/// Infrastructure plumbing: owner ref count threshold.
const INFRA_OWNER_MIN_REFS: usize = 50;
/// Infrastructure plumbing: owner module spread threshold.
const INFRA_OWNER_MIN_MODULES: usize = 5;

/// Factory method name patterns — pure ID generation, no business logic.
const FACTORY_METHOD_PATTERNS: &[&str] = &[
    "unsafeRandomId",
    "unsafeNode",
    "unsafeValue",
    "randomId",
    "generateId",
    "newId",
    "uuid",
    "nextId",
    "randomUUID",
    "freshId",
];

/// Owner name suffixes that indicate a store/repository.
const STORE_OWNER_SUFFIXES: &[&str] = &[
    "StoreOperations",
    "Store",
    "Repository",
    "Repo",
    "DAO",
    "Dao",
];

/// CRUD method names on stores — low-level boilerplate.
const CRUD_METHOD_NAMES: &[&str] = &[
    "upsert", "getOpt", "create", "delete", "update", "insert", "findBy", "findAll", "save",
    "get", "put", "remove", "list", "count", "exists", "deleteAll", "updateAll", "getAll",
    "exist", "getOptByTuple",
];

/// Minimum names sharing a prefix for it to be collapsed.
const PREFIX_MIN_GROUP: usize = 3;
/// Minimum prefix length (chars) to consider collapsing.
const PREFIX_MIN_LEN: usize = 6;

struct NoiseCandidate {
    /// Owner or type name — used for building --exclude patterns.
    exclude_name: String,
    /// Display string for the output line.
    display: String,
    /// Evidence string (e.g., "47 call sites, 0 callees").
    evidence: String,
}

/// Compute the noise exclude pattern as a comma-separated string.
/// Returns empty string if no noise detected. Used by `--noise-filter`.
pub fn compute_noise_exclude(index: &ArchivedKodexIndex) -> String {
    let limit = 15; // same default as cmd_noise
    let mut forward_counts: FxHashMap<u32, usize> = FxHashMap::default();
    for el in index.call_graph_forward.iter() {
        forward_counts.insert(el.from.into(), el.to.len());
    }
    let mut reverse_counts: FxHashMap<u32, usize> = FxHashMap::default();
    for el in index.call_graph_reverse.iter() {
        reverse_counts.insert(el.from.into(), el.to.len());
    }
    let mut ref_counts: FxHashMap<u32, usize> = FxHashMap::default();
    for rl in index.references.iter() {
        ref_counts.insert(rl.symbol_id.into(), rl.refs.len());
    }
    let module_spreads = build_module_spreads(index);
    let mut emitted: FxHashSet<u32> = FxHashSet::default();

    let plumbing = detect_effect_plumbing(index, &forward_counts, &reverse_counts, &mut emitted, limit);
    let hubs = detect_hub_utilities(index, &ref_counts, &module_spreads, &mut emitted, limit);
    let factories = detect_id_factories(index, &reverse_counts, &mut emitted, limit);
    let store_ops = detect_store_ops(index, &reverse_counts, &mut emitted, limit);
    let infra = detect_infra_plumbing(index, &reverse_counts, &ref_counts, &module_spreads, &mut emitted, limit);

    let all: Vec<&NoiseCandidate> = plumbing.iter()
        .chain(hubs.iter())
        .chain(factories.iter())
        .chain(store_ops.iter())
        .chain(infra.iter())
        .collect();

    if all.is_empty() {
        return String::new();
    }

    let mut seen: FxHashSet<&str> = FxHashSet::default();
    let mut raw_terms: Vec<&str> = Vec::new();
    for c in &all {
        if seen.insert(&c.exclude_name) {
            raw_terms.push(&c.exclude_name);
        }
    }
    collapse_prefixes(&raw_terms).join(",")
}

/// Noise analysis: detect symbols that clutter call-graph/info output.
pub fn cmd_noise(index: &ArchivedKodexIndex, limit: usize) -> CommandResult {
    let mut out = String::new();
    writeln!(out, "Noise analysis").unwrap();

    // ── Phase 1: build metric maps ──────────────────────────────────────────

    // Forward edge counts: symbol_id → number of callees
    let mut forward_counts: FxHashMap<u32, usize> = FxHashMap::default();
    for el in index.call_graph_forward.iter() {
        let from: u32 = el.from.into();
        forward_counts.insert(from, el.to.len());
    }

    // Reverse edge counts: symbol_id → number of callers
    let mut reverse_counts: FxHashMap<u32, usize> = FxHashMap::default();
    for el in index.call_graph_reverse.iter() {
        let from: u32 = el.from.into();
        reverse_counts.insert(from, el.to.len());
    }

    // Reference counts: symbol_id → number of references
    let mut ref_counts: FxHashMap<u32, usize> = FxHashMap::default();
    for rl in index.references.iter() {
        let sid: u32 = rl.symbol_id.into();
        ref_counts.insert(sid, rl.refs.len());
    }

    // Module spread: symbol_id → number of distinct modules referencing it
    let module_spreads = build_module_spreads(index);

    // Track emitted symbols to deduplicate across categories
    let mut emitted: FxHashSet<u32> = FxHashSet::default();

    // ── Phase 2: detect candidates per category ─────────────────────────────

    // Category 1: Effect plumbing
    let plumbing = detect_effect_plumbing(
        index,
        &forward_counts,
        &reverse_counts,
        &mut emitted,
        limit,
    );
    if !plumbing.is_empty() {
        writeln!(out).unwrap();
        writeln!(out, "Effect plumbing (high fan-in, no callees):").unwrap();
        for c in &plumbing {
            writeln!(out, "  {:<45} — {}", c.display, c.evidence).unwrap();
        }
    }

    // Category 2: Hub utilities
    let hubs = detect_hub_utilities(index, &ref_counts, &module_spreads, &mut emitted, limit);
    if !hubs.is_empty() {
        writeln!(out).unwrap();
        writeln!(out, "Hub utilities (high ref count, wide module spread):").unwrap();
        for c in &hubs {
            writeln!(out, "  {:<45} — {}", c.display, c.evidence).unwrap();
        }
    }

    // Category 3: ID factories
    let factories = detect_id_factories(index, &reverse_counts, &mut emitted, limit);
    if !factories.is_empty() {
        writeln!(out).unwrap();
        writeln!(out, "ID factories (pure generation, no business logic):").unwrap();
        for c in &factories {
            writeln!(out, "  {:<45} — {}", c.display, c.evidence).unwrap();
        }
    }

    // Category 4: Boilerplate store ops
    let store_ops = detect_store_ops(index, &reverse_counts, &mut emitted, limit);
    if !store_ops.is_empty() {
        writeln!(out).unwrap();
        writeln!(out, "Boilerplate store ops (low-level CRUD):").unwrap();
        for c in &store_ops {
            writeln!(out, "  {:<45} — {}", c.display, c.evidence).unwrap();
        }
    }

    // Category 5: Infrastructure plumbing
    let infra = detect_infra_plumbing(
        index,
        &reverse_counts,
        &ref_counts,
        &module_spreads,
        &mut emitted,
        limit,
    );
    if !infra.is_empty() {
        writeln!(out).unwrap();
        writeln!(
            out,
            "Infrastructure plumbing (high fan-in, owner is cross-cutting):"
        )
        .unwrap();
        for c in &infra {
            writeln!(out, "  {:<45} — {}", c.display, c.evidence).unwrap();
        }
    }

    // ── Phase 3: build suggested --exclude ──────────────────────────────────

    let all_candidates: Vec<&NoiseCandidate> = plumbing
        .iter()
        .chain(hubs.iter())
        .chain(factories.iter())
        .chain(store_ops.iter())
        .chain(infra.iter())
        .collect();

    if !all_candidates.is_empty() {
        // Collect unique exclude names, preserving order of first appearance
        let mut seen_names: FxHashSet<&str> = FxHashSet::default();
        let mut raw_terms: Vec<&str> = Vec::new();
        for c in &all_candidates {
            if seen_names.insert(&c.exclude_name) {
                raw_terms.push(&c.exclude_name);
            }
        }

        // Collapse terms that share a common prefix
        let collapsed = collapse_prefixes(&raw_terms);

        writeln!(out).unwrap();
        writeln!(out, "Suggested --exclude:").unwrap();
        writeln!(out, "  --exclude \"{}\"", collapsed.join(",")).unwrap();
    }

    CommandResult::Found(out)
}

// ── Category detectors ──────────────────────────────────────────────────────

/// Category 1: Methods called from many sites but with 0-1 callees (leaf plumbing).
fn detect_effect_plumbing(
    index: &ArchivedKodexIndex,
    forward_counts: &FxHashMap<u32, usize>,
    reverse_counts: &FxHashMap<u32, usize>,
    emitted: &mut FxHashSet<u32>,
    limit: usize,
) -> Vec<NoiseCandidate> {
    let mut candidates: Vec<(u32, usize, usize)> = Vec::new(); // (sym_id, call_sites, callees)

    for (&sid, &call_sites) in reverse_counts {
        if call_sites < PLUMBING_MIN_CALL_SITES {
            continue;
        }
        let sym = sym_at(index, sid);
        if !matches!(sym.kind, ArchivedSymbolKind::Method) {
            continue;
        }
        if skip_symbol(index, sym) {
            continue;
        }
        let callees = forward_counts.get(&sid).copied().unwrap_or(0);
        if callees > PLUMBING_MAX_CALLEES {
            continue;
        }
        candidates.push((sid, call_sites, callees));
    }

    candidates.sort_by(|a, b| b.1.cmp(&a.1));
    candidates.truncate(limit);

    candidates
        .into_iter()
        .filter(|(sid, _, _)| emitted.insert(*sid))
        .map(|(sid, call_sites, callees)| {
            let sym = sym_at(index, sid);
            let name = s(index, sym.name);
            let oname = owner_name(index, sym);
            let display = if oname.is_empty() {
                format!("method {name}")
            } else {
                format!("method {oname}.{name}")
            };
            let exclude_name = if oname.is_empty() {
                name.to_string()
            } else {
                oname.to_string()
            };
            NoiseCandidate {
                exclude_name,
                display,
                evidence: format!("{call_sites} call sites, {callees} callees"),
            }
        })
        .collect()
}

/// Category 2: Types with very high reference counts spread across many modules.
fn detect_hub_utilities(
    index: &ArchivedKodexIndex,
    ref_counts: &FxHashMap<u32, usize>,
    module_spreads: &FxHashMap<u32, usize>,
    emitted: &mut FxHashSet<u32>,
    limit: usize,
) -> Vec<NoiseCandidate> {
    let mut candidates: Vec<(u32, usize, usize)> = Vec::new(); // (sym_id, refs, modules)

    for (&sid, &refs) in ref_counts {
        if refs < HUB_MIN_REFS {
            continue;
        }
        let sym = sym_at(index, sid);
        if !matches!(
            sym.kind,
            ArchivedSymbolKind::Class | ArchivedSymbolKind::Trait | ArchivedSymbolKind::Object
        ) {
            continue;
        }
        if skip_symbol(index, sym) {
            continue;
        }
        let modules = module_spreads.get(&sid).copied().unwrap_or(0);
        if modules < HUB_MIN_MODULES {
            continue;
        }
        candidates.push((sid, refs, modules));
    }

    candidates.sort_by(|a, b| b.1.cmp(&a.1));
    candidates.truncate(limit);

    candidates
        .into_iter()
        .filter(|(sid, _, _)| emitted.insert(*sid))
        .map(|(sid, refs, modules)| {
            let sym = sym_at(index, sid);
            let name = s(index, sym.name);
            let kind = kind_str(&sym.kind);
            NoiseCandidate {
                exclude_name: name.to_string(),
                display: format!("{kind:<6} {name}"),
                evidence: format!("{refs} refs, {modules} modules"),
            }
        })
        .collect()
}

/// Category 3: Factory methods that generate IDs — no business logic.
fn detect_id_factories(
    index: &ArchivedKodexIndex,
    reverse_counts: &FxHashMap<u32, usize>,
    emitted: &mut FxHashSet<u32>,
    limit: usize,
) -> Vec<NoiseCandidate> {
    let mut candidates: Vec<(u32, usize)> = Vec::new(); // (sym_id, call_sites)

    for (sid_usize, sym) in index.symbols.iter().enumerate() {
        let sid = sid_usize as u32;
        if !matches!(sym.kind, ArchivedSymbolKind::Method) {
            continue;
        }
        if skip_symbol(index, sym) {
            continue;
        }
        let name = s(index, sym.name);
        let oname = owner_name(index, sym);

        let is_factory = FACTORY_METHOD_PATTERNS.iter().any(|p| name == *p)
            || (oname.contains("Factory") || oname.contains("Generator"));

        if !is_factory {
            continue;
        }

        let call_sites = reverse_counts.get(&sid).copied().unwrap_or(0);
        if call_sites == 0 {
            continue;
        }
        candidates.push((sid, call_sites));
    }

    candidates.sort_by(|a, b| b.1.cmp(&a.1));
    candidates.truncate(limit);

    candidates
        .into_iter()
        .filter(|(sid, _)| emitted.insert(*sid))
        .map(|(sid, call_sites)| {
            let sym = sym_at(index, sid);
            let name = s(index, sym.name);
            let oname = owner_name(index, sym);
            let display = if oname.is_empty() {
                format!("method {name}")
            } else {
                format!("method {oname}.{name}")
            };
            let exclude_name = if oname.is_empty() {
                name.to_string()
            } else {
                oname.to_string()
            };
            NoiseCandidate {
                exclude_name,
                display,
                evidence: format!("{call_sites} call sites"),
            }
        })
        .collect()
}

/// Category 4: CRUD methods on Store/Repository owners.
fn detect_store_ops(
    index: &ArchivedKodexIndex,
    reverse_counts: &FxHashMap<u32, usize>,
    emitted: &mut FxHashSet<u32>,
    limit: usize,
) -> Vec<NoiseCandidate> {
    let mut candidates: Vec<(u32, usize)> = Vec::new(); // (sym_id, call_sites)

    for (sid_usize, sym) in index.symbols.iter().enumerate() {
        let sid = sid_usize as u32;
        if !matches!(sym.kind, ArchivedSymbolKind::Method) {
            continue;
        }
        if skip_symbol(index, sym) {
            continue;
        }
        let name = s(index, sym.name);
        let oname = owner_name(index, sym);

        let is_store_owner = STORE_OWNER_SUFFIXES
            .iter()
            .any(|suffix| oname.ends_with(suffix));
        let is_crud = CRUD_METHOD_NAMES.iter().any(|m| name == *m);

        if !is_store_owner || !is_crud {
            continue;
        }

        let call_sites = reverse_counts.get(&sid).copied().unwrap_or(0);
        if call_sites == 0 {
            continue;
        }
        candidates.push((sid, call_sites));
    }

    candidates.sort_by(|a, b| b.1.cmp(&a.1));
    candidates.truncate(limit);

    candidates
        .into_iter()
        .filter(|(sid, _)| emitted.insert(*sid))
        .map(|(sid, call_sites)| {
            let sym = sym_at(index, sid);
            let name = s(index, sym.name);
            let oname = owner_name(index, sym);
            let display = if oname.is_empty() {
                format!("method {name}")
            } else {
                format!("method {oname}.{name}")
            };
            let exclude_name = if oname.is_empty() {
                name.to_string()
            } else {
                oname.to_string()
            };
            NoiseCandidate {
                exclude_name,
                display,
                evidence: format!("{call_sites} call sites"),
            }
        })
        .collect()
}

/// Category 5: Methods with high fan-in whose owner is a widely-referenced,
/// cross-cutting type. Catches infrastructure methods that have callees
/// (so they miss category 1) but are still plumbing — e.g., database
/// transaction methods, SQL effect wrappers, request context accessors.
fn detect_infra_plumbing(
    index: &ArchivedKodexIndex,
    reverse_counts: &FxHashMap<u32, usize>,
    ref_counts: &FxHashMap<u32, usize>,
    module_spreads: &FxHashMap<u32, usize>,
    emitted: &mut FxHashSet<u32>,
    limit: usize,
) -> Vec<NoiseCandidate> {
    let mut candidates: Vec<(u32, usize, String)> = Vec::new(); // (sym_id, call_sites, oname)

    for (&sid, &call_sites) in reverse_counts {
        if call_sites < INFRA_MIN_CALL_SITES {
            continue;
        }
        let sym = sym_at(index, sid);
        if !matches!(sym.kind, ArchivedSymbolKind::Method) {
            continue;
        }
        if skip_symbol(index, sym) {
            continue;
        }

        // Check if this method's owner is a widely-referenced, cross-cutting type
        let owner_id: u32 = sym.owner.into();
        if owner_id == NONE_ID || owner_id as usize >= index.symbols.len() {
            continue;
        }
        let owner_refs = ref_counts.get(&owner_id).copied().unwrap_or(0);
        if owner_refs < INFRA_OWNER_MIN_REFS {
            continue;
        }
        let owner_modules = module_spreads.get(&owner_id).copied().unwrap_or(0);
        if owner_modules < INFRA_OWNER_MIN_MODULES {
            continue;
        }

        candidates.push((sid, call_sites, owner_name(index, sym).to_string()));
    }

    candidates.sort_by(|a, b| b.1.cmp(&a.1));
    candidates.truncate(limit);

    candidates
        .into_iter()
        .filter(|(sid, _, _)| emitted.insert(*sid))
        .map(|(sid, call_sites, oname)| {
            let sym = sym_at(index, sid);
            let name = s(index, sym.name);
            let display = if oname.is_empty() {
                format!("method {name}")
            } else {
                format!("method {oname}.{name}")
            };
            let exclude_name = if oname.is_empty() {
                name.to_string()
            } else {
                oname
            };
            NoiseCandidate {
                exclude_name,
                display,
                evidence: format!("{call_sites} call sites"),
            }
        })
        .collect()
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Check if a symbol should be skipped (stdlib, test, generated, plumbing).
fn skip_symbol(index: &ArchivedKodexIndex, sym: &crate::model::ArchivedSymbol) -> bool {
    let file_id: u32 = sym.file_id.into();
    filter::is_stdlib(s(index, sym.fqn))
        || filter::is_test_file(index, file_id)
        || filter::is_generated_file(index, file_id)
}

/// Build a map of symbol_id → count of distinct modules that reference it.
/// Single pass over all references.
fn build_module_spreads(index: &ArchivedKodexIndex) -> FxHashMap<u32, usize> {
    let mut modules_per_sym: FxHashMap<u32, FxHashSet<u32>> = FxHashMap::default();

    for rl in index.references.iter() {
        let sid: u32 = rl.symbol_id.into();
        let set = modules_per_sym.entry(sid).or_default();
        for r in rl.refs.iter() {
            let file_id: u32 = r.file_id.into();
            if (file_id as usize) < index.files.len() {
                let module_id: u32 = file_entry(index, file_id).module_id.into();
                if module_id != NONE_ID {
                    set.insert(module_id);
                }
            }
        }
    }

    modules_per_sym
        .into_iter()
        .map(|(sid, set)| (sid, set.len()))
        .collect()
}

/// Collapse a list of exclude terms by finding common prefixes.
/// If 3+ terms share a prefix of 6+ chars, replace them with the prefix.
/// Preserves order of first appearance. Returns at most 20 terms.
fn collapse_prefixes(terms: &[&str]) -> Vec<String> {
    if terms.is_empty() {
        return Vec::new();
    }

    // Sort a copy to find prefix groups
    let mut sorted: Vec<&str> = terms.to_vec();
    sorted.sort_unstable();

    // Find prefix groups: scan sorted terms, greedily group by longest common prefix
    let mut prefix_map: FxHashMap<String, Vec<&str>> = FxHashMap::default();
    let mut i = 0;
    while i < sorted.len() {
        let mut best_prefix = String::new();
        let mut best_end = i + 1;

        // Try to extend a group starting from sorted[i]
        for j in (i + 1)..sorted.len() {
            let lcp = longest_common_prefix(sorted[i], sorted[j]);
            if lcp.len() >= PREFIX_MIN_LEN {
                // Check how many terms share this prefix
                let count = sorted[i..=j]
                    .iter()
                    .filter(|t| t.starts_with(&lcp))
                    .count();
                if count >= PREFIX_MIN_GROUP && lcp.len() > best_prefix.len() {
                    best_prefix = lcp;
                    best_end = j + 1;
                }
            }
        }

        if best_prefix.is_empty() {
            i += 1;
        } else {
            // Record which terms this prefix covers
            let covered: Vec<&str> = sorted[i..best_end]
                .iter()
                .filter(|t| t.starts_with(&best_prefix))
                .copied()
                .collect();
            prefix_map.insert(best_prefix, covered);
            i = best_end;
        }
    }

    // Build result: walk original order, replace covered terms with their prefix
    let mut result: Vec<String> = Vec::new();
    let mut seen: FxHashSet<String> = FxHashSet::default();
    // Build reverse map: term -> prefix
    let mut term_to_prefix: FxHashMap<&str, &str> = FxHashMap::default();
    for (prefix, members) in &prefix_map {
        for m in members {
            term_to_prefix.insert(m, prefix.as_str());
        }
    }

    for &term in terms {
        let output = if let Some(&prefix) = term_to_prefix.get(term) {
            prefix.to_string()
        } else {
            term.to_string()
        };
        if seen.insert(output.clone()) {
            result.push(output);
        }
    }

    result.truncate(20);
    result
}

/// Longest common prefix of two strings.
fn longest_common_prefix(a: &str, b: &str) -> String {
    a.chars()
        .zip(b.chars())
        .take_while(|(ca, cb)| ca == cb)
        .map(|(c, _)| c)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collapse_prefixes_basic() {
        let terms = vec![
            "HttpClientFactory",
            "HttpClientPool",
            "HttpClientConfig",
            "JsonParser",
            "EventBusUtils",
        ];
        let result = collapse_prefixes(&terms);
        assert!(result.contains(&"HttpClient".to_string()));
        assert!(result.contains(&"JsonParser".to_string()));
        assert!(result.contains(&"EventBusUtils".to_string()));
        assert!(!result.contains(&"HttpClientFactory".to_string()));
    }

    #[test]
    fn test_collapse_prefixes_no_collapse() {
        let terms = vec!["Alpha", "Beta", "Gamma"];
        let result = collapse_prefixes(&terms);
        assert_eq!(result, vec!["Alpha", "Beta", "Gamma"]);
    }

    #[test]
    fn test_collapse_prefixes_short_prefix_no_collapse() {
        // Prefix "AB" is too short (< 6 chars)
        let terms = vec!["ABfoo", "ABbar", "ABbaz"];
        let result = collapse_prefixes(&terms);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_collapse_prefixes_preserves_order() {
        let terms = vec![
            "JsonParser",
            "CacheNodeFactory",
            "CacheRootNodeFactory",
            "CacheChildNodeFactory",
            "Other",
        ];
        let result = collapse_prefixes(&terms);
        // JsonParser should come before CacheN* prefix
        let json_pos = result.iter().position(|s| s == "JsonParser").unwrap();
        let cache_pos = result.iter().position(|s| s.starts_with("Cache")).unwrap();
        assert!(json_pos < cache_pos);
    }

    #[test]
    fn test_collapse_prefixes_empty() {
        let result = collapse_prefixes(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_longest_common_prefix() {
        assert_eq!(longest_common_prefix("HttpClientFactory", "HttpClientPool"), "HttpClient");
        assert_eq!(longest_common_prefix("abc", "xyz"), "");
        assert_eq!(longest_common_prefix("same", "same"), "same");
    }
}
