use super::CommandResult;
use crate::model::{ArchivedKodexIndex, ArchivedSymbolKind, NONE_ID};
use crate::query::filter;
use crate::query::format::owner_name;
use crate::query::symbol::display_kind;
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

impl NoiseCandidate {
    /// Build a candidate for a method symbol, using its owner name for display/exclude.
    fn method(index: &ArchivedKodexIndex, sid: u32, evidence: String) -> Self {
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
        Self {
            exclude_name,
            display,
            evidence,
        }
    }
}

/// Pre-computed metric maps shared by `cmd_noise` and `compute_noise_exclude`.
struct NoiseMetrics {
    forward_counts: FxHashMap<u32, usize>,
    reverse_counts: FxHashMap<u32, usize>,
    ref_counts: FxHashMap<u32, usize>,
    module_spreads: FxHashMap<u32, usize>,
}

impl NoiseMetrics {
    fn build(index: &ArchivedKodexIndex) -> Self {
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
        Self {
            forward_counts,
            reverse_counts,
            ref_counts,
            module_spreads,
        }
    }
}

/// A category of noise patterns with a label and collapsed exclude terms.
pub struct NoiseCategory {
    pub label: &'static str,
    pub patterns: Vec<String>,
}

/// Compute noise patterns grouped by category.
/// Collapse is done across all categories (matching `cmd_noise` output).
pub fn compute_noise_patterns(index: &ArchivedKodexIndex, limit: usize) -> Vec<NoiseCategory> {
    let m = NoiseMetrics::build(index);
    let mut emitted: FxHashSet<u32> = FxHashSet::default();

    let categories: Vec<(&str, Vec<NoiseCandidate>)> = vec![
        ("Effect plumbing", detect_effect_plumbing(index, &m.forward_counts, &m.reverse_counts, &mut emitted, limit)),
        ("Hub utilities", detect_hub_utilities(index, &m.ref_counts, &m.module_spreads, &mut emitted, limit)),
        ("ID factories", detect_id_factories(index, &m.reverse_counts, &mut emitted, limit)),
        ("Store ops", detect_store_ops(index, &m.reverse_counts, &mut emitted, limit)),
        ("Infrastructure plumbing", detect_infra_plumbing(index, &m.reverse_counts, &m.ref_counts, &m.module_spreads, &mut emitted, limit)),
    ];

    // Deduplicate across categories, then collapse prefixes globally (matching cmd_noise)
    let mut seen: FxHashSet<String> = FxHashSet::default();
    let mut per_category: Vec<(&str, Vec<String>)> = Vec::new();
    let mut all_terms: Vec<String> = Vec::new();

    for (label, candidates) in &categories {
        let mut cat_terms = Vec::new();
        for c in candidates {
            if seen.insert(c.exclude_name.clone()) {
                cat_terms.push(c.exclude_name.clone());
                all_terms.push(c.exclude_name.clone());
            }
        }
        if !cat_terms.is_empty() {
            per_category.push((label, cat_terms));
        }
    }

    if all_terms.is_empty() {
        return Vec::new();
    }

    // Collapse globally, then map each original term to its collapsed form
    let term_refs: Vec<&str> = all_terms.iter().map(String::as_str).collect();
    let collapsed = collapse_prefixes(&term_refs);
    let mut term_to_collapsed: FxHashMap<&str, &str> = FxHashMap::default();
    // Build reverse mapping by walking the collapse logic
    for orig in &all_terms {
        for col in &collapsed {
            if orig.starts_with(col.as_str()) || orig == col {
                term_to_collapsed.insert(orig.as_str(), col.as_str());
                break;
            }
        }
        // If no prefix matched, the term itself is in collapsed
        term_to_collapsed.entry(orig.as_str()).or_insert(orig.as_str());
    }

    // Build categories with collapsed terms, deduplicating within each
    per_category
        .into_iter()
        .filter_map(|(label, terms)| {
            let mut cat_seen: FxHashSet<&str> = FxHashSet::default();
            let patterns: Vec<String> = terms
                .iter()
                .filter_map(|t| {
                    let fallback = t.as_str();
                    let collapsed_term = term_to_collapsed.get(t.as_str()).unwrap_or(&fallback);
                    if cat_seen.insert(collapsed_term) {
                        Some((*collapsed_term).to_string())
                    } else {
                        None
                    }
                })
                .collect();
            if patterns.is_empty() {
                None
            } else {
                Some(NoiseCategory { label, patterns })
            }
        })
        .collect()
}

/// Compute the noise exclude pattern as a comma-separated string.
/// Returns empty string if no noise detected.
pub fn compute_noise_exclude(index: &ArchivedKodexIndex) -> String {
    let categories = compute_noise_patterns(index, 15);
    let all: Vec<String> = categories
        .into_iter()
        .flat_map(|c| c.patterns)
        .collect();
    all.join(",")
}

// ── Noise config file I/O ──────────────────────────────────────────────────

const NOISE_CONF_NAME: &str = "noise.conf";

const NOISE_CONF_HEADER: &str = "\
# kodex noise config — auto-generated, safe to edit
# One exclude pattern per line. Matches FQN, name, or owner (substring).
# Delete lines to stop filtering those symbols. Add lines to filter more.
# Regenerate with: kodex noise --init
";

/// Write `.scalex/noise.conf` with categorized exclude patterns.
/// Returns the path to the written file.
pub fn write_noise_conf(
    scalex_dir: &std::path::Path,
    index: &ArchivedKodexIndex,
    limit: usize,
) -> std::io::Result<std::path::PathBuf> {
    let categories = compute_noise_patterns(index, limit);
    let conf_path = scalex_dir.join(NOISE_CONF_NAME);

    let mut content = String::from(NOISE_CONF_HEADER);
    for cat in &categories {
        content.push_str(&format!("\n# {}\n", cat.label));
        for p in &cat.patterns {
            content.push_str(p);
            content.push('\n');
        }
    }

    std::fs::write(&conf_path, &content)?;
    Ok(conf_path)
}

/// Read `.scalex/noise.conf` if it exists.
/// Returns `None` if the file is missing (caller should fall back to auto-compute).
/// Returns `Some(vec![])` if the file exists but has no patterns (user cleared it).
pub fn read_noise_conf(scalex_dir: &std::path::Path) -> Option<Vec<String>> {
    let conf_path = scalex_dir.join(NOISE_CONF_NAME);
    let content = std::fs::read_to_string(&conf_path).ok()?;
    let patterns = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(String::from)
        .collect();
    Some(patterns)
}

/// Noise analysis: detect symbols that clutter call-graph/info output.
pub fn cmd_noise(index: &ArchivedKodexIndex, limit: usize) -> CommandResult {
    let mut out = String::new();
    writeln!(out, "Noise analysis").unwrap();

    // ── Phase 1: build metric maps ──────────────────────────────────────────
    let m = NoiseMetrics::build(index);

    // Track emitted symbols to deduplicate across categories
    let mut emitted: FxHashSet<u32> = FxHashSet::default();

    // ── Phase 2: detect candidates per category ─────────────────────────────

    // Category 1: Effect plumbing
    let plumbing = detect_effect_plumbing(
        index,
        &m.forward_counts,
        &m.reverse_counts,
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
    let hubs = detect_hub_utilities(index, &m.ref_counts, &m.module_spreads, &mut emitted, limit);
    if !hubs.is_empty() {
        writeln!(out).unwrap();
        writeln!(out, "Hub utilities (high ref count, wide module spread):").unwrap();
        for c in &hubs {
            writeln!(out, "  {:<45} — {}", c.display, c.evidence).unwrap();
        }
    }

    // Category 3: ID factories
    let factories = detect_id_factories(index, &m.reverse_counts, &mut emitted, limit);
    if !factories.is_empty() {
        writeln!(out).unwrap();
        writeln!(out, "ID factories (pure generation, no business logic):").unwrap();
        for c in &factories {
            writeln!(out, "  {:<45} — {}", c.display, c.evidence).unwrap();
        }
    }

    // Category 4: Boilerplate store ops
    let store_ops = detect_store_ops(index, &m.reverse_counts, &mut emitted, limit);
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
        &m.reverse_counts,
        &m.ref_counts,
        &m.module_spreads,
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

    writeln!(out).unwrap();
    writeln!(out, "Config: .scalex/noise.conf (edit to customize, `kodex noise --init` to regenerate)").unwrap();

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
            NoiseCandidate::method(index, sid, format!("{call_sites} call sites, {callees} callees"))
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
            let kind = display_kind(sym);
            NoiseCandidate {
                exclude_name: name.to_string(),
                display: format!("{kind:<11} {name}"),
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

        let is_factory = FACTORY_METHOD_PATTERNS.contains(&name)
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
            NoiseCandidate::method(index, sid, format!("{call_sites} call sites"))
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
        let is_crud = CRUD_METHOD_NAMES.contains(&name);

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
            NoiseCandidate::method(index, sid, format!("{call_sites} call sites"))
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
        .map(|(sid, call_sites, _oname)| {
            NoiseCandidate::method(index, sid, format!("{call_sites} call sites"))
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

    #[test]
    fn test_read_noise_conf_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_noise_conf(dir.path()).is_none());
    }

    #[test]
    fn test_read_noise_conf_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("noise.conf"), "# only comments\n\n").unwrap();
        let result = read_noise_conf(dir.path());
        assert_eq!(result, Some(vec![]));
    }

    #[test]
    fn test_read_noise_conf_with_patterns() {
        let dir = tempfile::tempdir().unwrap();
        let content = "# header\nFooBar\n  BazQux  \n# comment\n\nHello\n";
        std::fs::write(dir.path().join("noise.conf"), content).unwrap();
        let result = read_noise_conf(dir.path()).unwrap();
        assert_eq!(result, vec!["FooBar", "BazQux", "Hello"]);
    }

    #[test]
    fn test_write_and_read_noise_conf_roundtrip() {
        // Build a minimal index — won't have noise candidates, but tests the I/O path
        use crate::index::writer::write_index;
        use crate::ingest::merge::build_index;
        use crate::ingest::types::*;

        let docs: Vec<IntermediateDoc> = vec![];
        let built = build_index(&docs, None, ".");
        let dir = tempfile::tempdir().unwrap();
        let idx_path = dir.path().join("kodex.idx");
        write_index(&built, &idx_path).unwrap();

        let reader = crate::index::reader::IndexReader::open(&idx_path).unwrap();
        let conf_path = write_noise_conf(dir.path(), reader.index(), 15).unwrap();

        assert!(conf_path.exists());
        let content = std::fs::read_to_string(&conf_path).unwrap();
        assert!(content.starts_with("# kodex noise config"));

        // Round-trip: read back what we wrote
        let patterns = read_noise_conf(dir.path()).unwrap();
        // Empty index → no noise patterns, but file exists
        assert!(patterns.is_empty());
    }
}
