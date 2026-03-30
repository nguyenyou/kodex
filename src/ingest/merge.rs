use rustc_hash::FxHashMap;

use crate::ingest::classify::{
    classify_generated, classify_test, generated_prefixes, test_modules,
    register_module,
};
use crate::ingest::interner::StringInterner;
use crate::ingest::provider::BuildMetadata;
use crate::ingest::types::IntermediateDoc;
use crate::model::*;

/// Build a `KodexIndex` from intermediate documents and optional build metadata.
#[must_use]
pub fn build_index(
    docs: &[IntermediateDoc],
    metadata: Option<&BuildMetadata>,
    workspace_root: &str,
) -> KodexIndex {
    use std::time::Instant;
    let t0 = Instant::now();
    let mut last = t0;
    let trace = std::env::var("KODEX_TRACE").is_ok();
    macro_rules! phase {
        ($name:expr) => {
            if trace {
                let now = Instant::now();
                eprintln!(
                    "  {:>6.1}ms  {}",
                    (now - last).as_secs_f64() * 1000.0,
                    $name
                );
                #[allow(unused_assignments)]
                {
                    last = now;
                }
            }
        };
    }

    let total_syms: usize = docs.iter().map(|d| d.symbols.len()).sum();
    let mut interner = StringInterner::with_capacity(total_syms * 3);

    let test_mods = test_modules(metadata);
    let gen_prefixes = generated_prefixes(metadata, workspace_root);

    // Phase 1: Collect files, classify, detect modules
    let mut file_map: FxHashMap<String, u32> = FxHashMap::default();
    let mut files: Vec<FileEntry> = Vec::new();
    let mut module_map: FxHashMap<String, u32> = FxHashMap::default();
    let mut modules: Vec<Module> = Vec::new();

    for doc in docs {
        if !file_map.contains_key(&doc.uri) {
            let fid = files.len() as u32;
            file_map.insert(doc.uri.clone(), fid);
            let uri = &doc.uri;
            let module_id = register_module(
                &doc.module_segments,
                &mut module_map,
                &mut modules,
                &mut interner,
            );
            files.push(FileEntry {
                path: interner.intern(uri),
                module_id,
                is_test: classify_test(&doc.module_segments, &test_mods, uri),
                is_generated: classify_generated(&gen_prefixes, uri),
            });
        }
    }
    phase!("phase 1: files + modules");

    // Phase 2: Collect symbols, assign IDs, intern strings
    let SymbolCollection {
        mut symbols,
        sym_map,
        parent_fqns,
        override_fqns,
    } = collect_symbols(docs, &file_map, &mut interner, total_syms);
    phase!("phase 2: symbols + intern");

    // Phase 3: Resolve owner symbol IDs
    resolve_owners(&mut symbols, &sym_map);
    phase!("phase 3: owner resolution");

    // Phase 4: Build references index
    let references = build_references(docs, &file_map, &sym_map);
    phase!("phase 4: references");

    // Phase 5: Build inheritance indexes
    let (inh_fwd, inh_rev) = build_inheritance(&parent_fqns, &sym_map);
    phase!("phase 5: inheritance");

    // Phase 6: Build members index (owner → members)
    let members_map = build_members(&symbols);
    phase!("phase 6: members");

    // Phase 7: Build overrides index
    let overrides_map = build_overrides(&override_fqns, &sym_map);
    phase!("phase 7: overrides");

    // Phase 8: Build call graph + compute end_line
    let (call_fwd, call_rev) =
        build_call_graph(docs, &sym_map, &mut symbols);
    phase!("phase 8: call graph + end_line");

    // Phase 9: Merge build metadata into modules
    let (module_deps_map, ivy_deps) = merge_build_metadata(
        metadata,
        &mut modules,
        &mut module_map,
        &mut interner,
    );
    phase!("phase 9: build metadata");

    // Phase 10: Compute symbol_count per module
    for sym in &symbols {
        let fid = sym.file_id;
        if (fid as usize) < files.len() {
            let mid = files[fid as usize].module_id;
            if mid != NONE_ID && (mid as usize) < modules.len() {
                modules[mid as usize].symbol_count += 1;
            }
        }
    }
    phase!("phase 10: symbol_count");

    // Consume interner — no more interning after this point
    let strings_vec = interner.into_vec();

    // Phase 11: Build trigram + hash indexes (parallel)
    let ((name_trigrams, name_hash_buckets, name_hash_size), (fqn_hash_buckets, fqn_hash_size)) =
        rayon::join(
            || build_name_indexes(&symbols, &strings_vec),
            || build_fqn_hash_index(&symbols, &strings_vec),
        );
    phase!("phase 11: trigram + hash + fqn indexes");

    // Phase 12: Reverse module dependency graph
    let module_deps_rev = reverse_edges(&module_deps_map);
    phase!("phase 12: module_deps_reverse");

    let index = KodexIndex {
        version: KODEX_INDEX_VERSION,
        workspace_root: std::path::Path::new(workspace_root)
            .canonicalize()
            .map_or_else(|_| workspace_root.to_string(), |p| p.to_string_lossy().into_owned()),
        strings: strings_vec,
        files,
        symbols,
        references,
        call_graph_forward: to_edge_lists(call_fwd),
        call_graph_reverse: to_edge_lists(call_rev),
        inheritance_forward: to_edge_lists(inh_fwd),
        inheritance_reverse: to_edge_lists(inh_rev),
        members: to_edge_lists(members_map),
        overrides: to_edge_lists(overrides_map),
        modules,
        module_deps: to_edge_lists(module_deps_map),
        module_deps_reverse: to_edge_lists(module_deps_rev),
        ivy_deps,
        name_trigrams,
        name_hash_buckets,
        name_hash_size,
        fqn_hash_buckets,
        fqn_hash_size,
    };
    phase!("assemble + edge list sort");
    if trace {
        eprintln!(
            "  {:>6.1}ms  total build_index",
            t0.elapsed().as_secs_f64() * 1000.0
        );
    }

    debug_assert!({
        validate_index(&index);
        true
    });
    index
}

// ── Phase 2: Collect symbols ────────────────────────────────────────────────

/// Collected symbols and lookup tables produced by phase 2.
struct SymbolCollection {
    symbols: Vec<Symbol>,
    sym_map: FxHashMap<String, u32>,
    parent_fqns: Vec<Vec<String>>,
    override_fqns: Vec<Vec<String>>,
}

fn collect_symbols(
    docs: &[IntermediateDoc],
    file_map: &FxHashMap<String, u32>,
    interner: &mut StringInterner,
    total_syms: usize,
) -> SymbolCollection {
    let mut sym_map: FxHashMap<String, u32> = FxHashMap::default();
    sym_map.reserve(total_syms);
    let mut symbols: Vec<Symbol> = Vec::with_capacity(total_syms);
    let mut parent_fqns: Vec<Vec<String>> = Vec::with_capacity(total_syms);
    let mut override_fqns: Vec<Vec<String>> = Vec::with_capacity(total_syms);

    for doc in docs {
        let file_id = file_map[&doc.uri];

        // Pre-index definition locations: fqn → (line, col)
        let mut def_locs: FxHashMap<&str, (u32, u32)> = FxHashMap::default();
        for occ in &doc.occurrences {
            if matches!(occ.role, ReferenceRole::Definition) {
                def_locs
                    .entry(&occ.symbol)
                    .or_insert((occ.start_line, occ.start_col));
            }
        }

        for isym in &doc.symbols {
            if sym_map.contains_key(&isym.fqn) {
                continue;
            }
            let sid = symbols.len() as u32;
            sym_map.insert(isym.fqn.clone(), sid);

            let &(line, col) = def_locs.get(isym.fqn.as_str()).unwrap_or(&(0, 0));
            let parent_string_ids: Vec<u32> =
                isym.parents.iter().map(|p| interner.intern(p)).collect();
            let override_string_ids: Vec<u32> = isym
                .overridden_symbols
                .iter()
                .map(|o| interner.intern(o))
                .collect();
            symbols.push(Symbol {
                id: sid,
                name: interner.intern(&isym.display_name),
                fqn: interner.intern(&isym.fqn),
                kind: isym.kind,
                file_id,
                line,
                col,
                end_line: NONE_ID,
                type_signature: interner.intern(&isym.signature),
                owner: NONE_ID,
                properties: isym.properties,
                access: isym.access,
                parents: parent_string_ids,
                overridden_symbols: override_string_ids,
            });
            parent_fqns.push(isym.parents.clone());
            override_fqns.push(isym.overridden_symbols.clone());
        }
    }
    SymbolCollection {
        symbols,
        sym_map,
        parent_fqns,
        override_fqns,
    }
}

// ── Phase 3: Resolve owners ─────────────────────────────────────────────────

fn resolve_owners(symbols: &mut [Symbol], sym_map: &FxHashMap<String, u32>) {
    let id_to_fqn: FxHashMap<u32, &str> = sym_map
        .iter()
        .map(|(fqn, &id)| (id, fqn.as_str()))
        .collect();
    for sym in symbols.iter_mut() {
        if let Some(fqn_str) = id_to_fqn.get(&sym.id) {
            let owner_fqn = crate::symbol::symbol_owner(fqn_str);
            if let Some(&owner_id) = sym_map.get(owner_fqn) {
                sym.owner = owner_id;
            }
        }
    }
}

// ── Multi-symbol splitting ───────────────────────────────────────────────────

/// Iterate over the individual symbols in an occurrence's symbol field.
/// Normal symbols are yielded as-is. Multi-symbols (`;sym1;sym2`) are split.
fn iter_occurrence_symbols(symbol: &str) -> impl Iterator<Item = &str> {
    // Multi-symbols start with ';'. Splitting on ';' and filtering empties
    // works for both cases: normal "sym" yields ["sym"], and ";sym1;sym2"
    // yields ["sym1", "sym2"] (the leading empty segment is filtered out).
    symbol.split(';').filter(|s| !s.is_empty())
}

// ── Phase 4: References ─────────────────────────────────────────────────────

fn build_references(
    docs: &[IntermediateDoc],
    file_map: &FxHashMap<String, u32>,
    sym_map: &FxHashMap<String, u32>,
) -> Vec<ReferenceList> {
    let mut refs_by_sym: FxHashMap<u32, Vec<Reference>> = FxHashMap::default();
    for doc in docs {
        let file_id = file_map[&doc.uri];
        for occ in &doc.occurrences {
            for sym in iter_occurrence_symbols(&occ.symbol) {
                if let Some(&sid) = sym_map.get(sym) {
                    refs_by_sym.entry(sid).or_default().push(Reference {
                        file_id,
                        line: occ.start_line,
                        col: occ.start_col,
                        role: occ.role,
                    });
                }
            }
        }
    }
    refs_by_sym
        .into_iter()
        .map(|(symbol_id, refs)| ReferenceList { symbol_id, refs })
        .collect()
}

// ── Phase 5: Inheritance ────────────────────────────────────────────────────

fn build_inheritance(
    parent_fqns: &[Vec<String>],
    sym_map: &FxHashMap<String, u32>,
) -> (FxHashMap<u32, Vec<u32>>, FxHashMap<u32, Vec<u32>>) {
    let mut fwd: FxHashMap<u32, Vec<u32>> = FxHashMap::default();
    let mut rev: FxHashMap<u32, Vec<u32>> = FxHashMap::default();
    for (i, parents) in parent_fqns.iter().enumerate() {
        let child_id = i as u32;
        for parent_fqn in parents {
            if let Some(&parent_id) = sym_map.get(parent_fqn) {
                fwd.entry(parent_id).or_default().push(child_id);
                rev.entry(child_id).or_default().push(parent_id);
            }
        }
    }
    (fwd, rev)
}

// ── Phase 6: Members ────────────────────────────────────────────────────────

fn build_members(symbols: &[Symbol]) -> FxHashMap<u32, Vec<u32>> {
    let mut map: FxHashMap<u32, Vec<u32>> = FxHashMap::default();
    for sym in symbols {
        if sym.owner != NONE_ID {
            map.entry(sym.owner).or_default().push(sym.id);
        }
    }
    map
}

// ── Phase 7: Overrides ──────────────────────────────────────────────────────

fn build_overrides(
    override_fqns: &[Vec<String>],
    sym_map: &FxHashMap<String, u32>,
) -> FxHashMap<u32, Vec<u32>> {
    let mut map: FxHashMap<u32, Vec<u32>> = FxHashMap::default();
    for (i, overrides) in override_fqns.iter().enumerate() {
        let overrider_id = i as u32;
        for base_fqn in overrides {
            if let Some(&base_id) = sym_map.get(base_fqn) {
                map.entry(base_id).or_default().push(overrider_id);
            }
        }
    }
    map
}

// ── Phase 8: Call graph + end_line ──────────────────────────────────────────

/// A definition occurrence in a file, used for call graph and end_line computation.
struct DefInfo {
    sid: u32,
    owner: u32,
    start_line: u32,
    end_col: u32,
    body_end: u32,
    is_callable: bool,
}

/// Entry for computing body boundaries between sibling definitions.
struct SiblingBound {
    owner: u32,
    start_line: u32,
    body_end: u32,
}

fn build_call_graph(
    docs: &[IntermediateDoc],
    sym_map: &FxHashMap<String, u32>,
    symbols: &mut [Symbol],
) -> (FxHashMap<u32, Vec<u32>>, FxHashMap<u32, Vec<u32>>) {
    let mut call_fwd: FxHashMap<u32, Vec<u32>> = FxHashMap::default();
    let mut call_rev: FxHashMap<u32, Vec<u32>> = FxHashMap::default();

    // Group doc indices by URI (multiple .semanticdb files can reference the same source)
    let mut docs_by_uri: FxHashMap<&str, Vec<usize>> = FxHashMap::default();
    for (i, doc) in docs.iter().enumerate() {
        docs_by_uri.entry(&doc.uri).or_default().push(i);
    }

    for doc_indices in docs_by_uri.values() {
        let mut defs: Vec<DefInfo> = Vec::new();
        for &di in doc_indices {
            for occ in &docs[di].occurrences {
                if !matches!(occ.role, ReferenceRole::Definition) {
                    continue;
                }
                if occ.symbol.starts_with("local") {
                    continue;
                }
                let Some(&sid) = sym_map.get(&occ.symbol) else {
                    continue;
                };
                if sid >= symbols.len() as u32 {
                    continue;
                }
                let kind = symbols[sid as usize].kind;
                defs.push(DefInfo {
                    sid,
                    owner: symbols[sid as usize].owner,
                    start_line: occ.start_line,
                    end_col: occ.end_col,
                    body_end: NONE_ID,
                    is_callable: matches!(
                        kind,
                        SymbolKind::Method
                            | SymbolKind::Constructor
                            | SymbolKind::Field
                            | SymbolKind::Object
                            | SymbolKind::Class
                    ),
                });
            }
        }
        defs.sort_by_key(|d| d.start_line);

        // Compute body_end for each def
        let mut bounds: Vec<SiblingBound> = defs
            .iter()
            .map(|d| SiblingBound {
                owner: d.owner,
                start_line: d.start_line,
                body_end: NONE_ID,
            })
            .collect();
        compute_sibling_bounds(&mut bounds);

        // Write back body_end to defs and set end_line on symbols
        for (i, bound) in bounds.iter().enumerate() {
            defs[i].body_end = bound.body_end;
            if bound.body_end != NONE_ID {
                symbols[defs[i].sid as usize].end_line = bound.body_end.saturating_sub(1);
            }
        }

        // For each reference, find enclosing callable def
        let callable_defs: Vec<&DefInfo> = defs.iter().filter(|d| d.is_callable).collect();
        for &di in doc_indices {
            for occ in &docs[di].occurrences {
                if !matches!(occ.role, ReferenceRole::Reference) {
                    continue;
                }
                for sym in iter_occurrence_symbols(&occ.symbol) {
                    let Some(&callee_id) = sym_map.get(sym) else {
                        continue;
                    };
                    if callee_id >= symbols.len() as u32 {
                        continue;
                    }
                    let callee_kind = symbols[callee_id as usize].kind;
                    if !matches!(
                        callee_kind,
                        SymbolKind::Method | SymbolKind::Constructor | SymbolKind::Field
                    ) {
                        continue;
                    }

                    let line = occ.start_line;
                    let col = occ.start_col;
                    let pos = callable_defs.partition_point(|d| d.start_line <= line);
                    let enclosing = callable_defs[..pos].iter().rev().find(|d| {
                        d.start_line <= line
                            && line < d.body_end
                            && (line > d.start_line || col > d.end_col)
                    });
                    if let Some(def) = enclosing {
                        if def.sid != callee_id {
                            call_fwd.entry(def.sid).or_default().push(callee_id);
                            call_rev.entry(callee_id).or_default().push(def.sid);
                        }
                    }
                }
            }
        }
    }

    (call_fwd, call_rev)
}

/// Compute body_end for each def = next sibling (same owner) start line.
/// Groups by owner for amortized O(n) instead of O(n²).
fn compute_sibling_bounds(bounds: &mut [SiblingBound]) {
    let mut by_owner: FxHashMap<u32, Vec<usize>> = FxHashMap::default();
    for (i, b) in bounds.iter().enumerate() {
        by_owner.entry(b.owner).or_default().push(i);
    }
    for indices in by_owner.values() {
        for w in indices.windows(2) {
            let (i, j) = (w[0], w[1]);
            if bounds[j].start_line > bounds[i].start_line {
                bounds[i].body_end = bounds[j].start_line;
            }
        }
    }
}

// ── Phase 9: Build metadata ─────────────────────────────────────────────────

fn merge_build_metadata(
    metadata: Option<&BuildMetadata>,
    modules: &mut Vec<Module>,
    module_map: &mut FxHashMap<String, u32>,
    interner: &mut StringInterner,
) -> (FxHashMap<u32, Vec<u32>>, Vec<IvyDep>) {
    let mut module_deps_map: FxHashMap<u32, Vec<u32>> = FxHashMap::default();
    let mut ivy_deps: Vec<IvyDep> = Vec::new();

    let Some(meta) = metadata else {
        return (module_deps_map, ivy_deps);
    };

    for minfo in &meta.modules {
        let empty_str = interner.intern("");
        let mod_id = if let Some(&id) = module_map.get(&minfo.segments) {
            id
        } else {
            let id = modules.len() as u32;
            module_map.insert(minfo.segments.clone(), id);
            modules.push(Module {
                name: interner.intern(&minfo.segments),
                artifact_name: empty_str,
                source_paths: vec![],
                generated_source_paths: vec![],
                scala_version: empty_str,
                scalac_options: vec![],
                main_class: empty_str,
                test_framework: empty_str,
                file_count: 0,
                symbol_count: 0,
            });
            id
        };

        let m = &mut modules[mod_id as usize];
        if !minfo.artifact_name.is_empty() {
            m.artifact_name = interner.intern(&minfo.artifact_name);
        }
        if m.source_paths.is_empty() && !minfo.source_paths.is_empty() {
            m.source_paths = minfo.source_paths.iter().map(|p| interner.intern(p)).collect();
        }
        if m.generated_source_paths.is_empty() && !minfo.generated_source_paths.is_empty() {
            m.generated_source_paths = minfo
                .generated_source_paths
                .iter()
                .map(|p| interner.intern(p))
                .collect();
        }
        if !minfo.scala_version.is_empty() {
            m.scala_version = interner.intern(&minfo.scala_version);
        }
        if m.scalac_options.is_empty() && !minfo.scalac_options.is_empty() {
            m.scalac_options = minfo.scalac_options.iter().map(|o| interner.intern(o)).collect();
        }
        if !minfo.main_class.is_empty() {
            m.main_class = interner.intern(&minfo.main_class);
        }
        if !minfo.test_framework.is_empty() {
            m.test_framework = interner.intern(&minfo.test_framework);
        }

        let dep_ids: Vec<u32> = minfo
            .module_deps
            .iter()
            .filter_map(|dep_name| module_map.get(dep_name).copied())
            .collect();
        if !dep_ids.is_empty() {
            module_deps_map.insert(mod_id, dep_ids);
        }

        for dep in &minfo.ivy_deps {
            ivy_deps.push(IvyDep {
                module_id: mod_id,
                dep: interner.intern(dep),
            });
        }
    }

    (module_deps_map, ivy_deps)
}

// ── Edge list helpers ────────────────────────────────────────────────────────

fn dedup_edges(map: FxHashMap<u32, Vec<u32>>) -> Vec<EdgeList> {
    map.into_iter()
        .map(|(from, mut to)| {
            to.sort_unstable();
            to.dedup();
            EdgeList { from, to }
        })
        .collect()
}

fn to_edge_lists(map: FxHashMap<u32, Vec<u32>>) -> Vec<EdgeList> {
    let mut edges = dedup_edges(map);
    edges.sort_by_key(|e| e.from);
    edges
}

/// Build the reverse of an edge map.
fn reverse_edges(forward: &FxHashMap<u32, Vec<u32>>) -> FxHashMap<u32, Vec<u32>> {
    let mut rev: FxHashMap<u32, Vec<u32>> = FxHashMap::default();
    for (&from, deps) in forward {
        for &to in deps {
            rev.entry(to).or_default().push(from);
        }
    }
    rev
}

// ── Trigram + hash index building ───────────────────────────────────────────

use crate::hash::{case_insensitive_hash, case_sensitive_hash, trigram_key};

/// Extract all trigram keys from a string (lowercased) into a reusable buffer.
fn extract_trigrams_into(s: &str, buf: &mut Vec<u32>) {
    buf.clear();
    let bytes = s.as_bytes();
    if bytes.len() < 3 {
        return;
    }
    buf.reserve(bytes.len() - 2);
    for i in 0..bytes.len() - 2 {
        buf.push(trigram_key(bytes[i], bytes[i + 1], bytes[i + 2]));
    }
    buf.sort_unstable();
    buf.dedup();
}

fn build_name_indexes(
    symbols: &[Symbol],
    strings: &[String],
) -> (Vec<TrigramEntry>, Vec<HashBucket>, u32) {
    let mut tri_map: FxHashMap<u32, Vec<u32>> = FxHashMap::default();
    let bucket_count = ((symbols.len() * 4 / 3).max(1024)) as u32;
    let mut buckets = vec![HashBucket { symbol_ids: vec![] }; bucket_count as usize];
    let mut tri_buf: Vec<u32> = Vec::new();

    for sym in symbols {
        let name = &strings[sym.name as usize];
        let fqn = &strings[sym.fqn as usize];
        let sid = sym.id;

        extract_trigrams_into(name, &mut tri_buf);
        for &tri in &tri_buf {
            tri_map.entry(tri).or_default().push(sid);
        }
        if let Some(last_seg) = fqn.rsplit('/').next() {
            extract_trigrams_into(last_seg, &mut tri_buf);
            for &tri in &tri_buf {
                tri_map.entry(tri).or_default().push(sid);
            }
        }

        let bucket = case_insensitive_hash(name, bucket_count);
        buckets[bucket as usize].symbol_ids.push(sid);
    }

    let mut trigrams: Vec<TrigramEntry> = tri_map
        .into_iter()
        .map(|(key, mut ids)| {
            ids.sort_unstable();
            ids.dedup();
            TrigramEntry {
                key,
                symbol_ids: ids,
            }
        })
        .collect();
    trigrams.sort_by_key(|t| t.key);

    (trigrams, buckets, bucket_count)
}

fn build_fqn_hash_index(symbols: &[Symbol], strings: &[String]) -> (Vec<HashBucket>, u32) {
    let bucket_count = ((symbols.len() * 4 / 3).max(1024)) as u32;
    let mut buckets = vec![HashBucket { symbol_ids: vec![] }; bucket_count as usize];

    for sym in symbols {
        let fqn = &strings[sym.fqn as usize];
        let bucket = case_sensitive_hash(fqn, bucket_count);
        buckets[bucket as usize].symbol_ids.push(sym.id);
    }

    (buckets, bucket_count)
}

// ── Index validation ────────────────────────────────────────────────────────

/// Validate structural invariants of a KodexIndex.
/// Panics with a descriptive message if any invariant is violated.
pub fn validate_index(index: &KodexIndex) {
    let n_strings = index.strings.len() as u32;
    let n_files = index.files.len() as u32;
    let n_symbols = index.symbols.len() as u32;
    let n_modules = index.modules.len() as u32;

    assert_eq!(index.version, KODEX_INDEX_VERSION, "version mismatch");

    for (i, sym) in index.symbols.iter().enumerate() {
        assert_eq!(sym.id, i as u32, "symbols[{i}].id mismatch");
        assert!(sym.name < n_strings, "symbols[{i}].name out of bounds");
        assert!(sym.fqn < n_strings, "symbols[{i}].fqn out of bounds");
        assert!(
            sym.type_signature < n_strings,
            "symbols[{i}].type_signature out of bounds"
        );
        assert!(sym.file_id < n_files, "symbols[{i}].file_id out of bounds");
        assert!(
            sym.owner < n_symbols || sym.owner == NONE_ID,
            "symbols[{i}].owner out of bounds"
        );
        assert!(
            sym.end_line >= sym.line || sym.end_line == NONE_ID,
            "symbols[{i}].end_line ({}) < line ({})",
            sym.end_line,
            sym.line
        );
        for pid in &sym.parents {
            assert!(*pid < n_strings, "symbols[{i}].parents ref out of bounds");
        }
        for oid in &sym.overridden_symbols {
            assert!(
                *oid < n_strings,
                "symbols[{i}].overridden_symbols ref out of bounds"
            );
        }
    }

    for (i, f) in index.files.iter().enumerate() {
        assert!(f.path < n_strings, "files[{i}].path out of bounds");
        assert!(
            f.module_id < n_modules || f.module_id == NONE_ID,
            "files[{i}].module_id out of bounds"
        );
    }

    for (i, m) in index.modules.iter().enumerate() {
        assert!(m.name < n_strings, "modules[{i}].name out of bounds");
        assert!(
            m.artifact_name < n_strings,
            "modules[{i}].artifact_name out of bounds"
        );
        assert!(
            m.scala_version < n_strings,
            "modules[{i}].scala_version out of bounds"
        );
        assert!(
            m.main_class < n_strings,
            "modules[{i}].main_class out of bounds"
        );
        assert!(
            m.test_framework < n_strings,
            "modules[{i}].test_framework out of bounds"
        );
    }

    fn check_edges(edges: &[EdgeList], n: u32, name: &str) {
        for i in 1..edges.len() {
            assert!(
                edges[i].from > edges[i - 1].from,
                "{name} not sorted at index {i}: from={} after from={}",
                edges[i].from,
                edges[i - 1].from
            );
        }
        for el in edges {
            assert!(el.from < n, "{name} from={} out of bounds", el.from);
            for &to in &el.to {
                assert!(to < n, "{name} to={to} out of bounds (from={})", el.from);
            }
        }
    }

    check_edges(&index.call_graph_forward, n_symbols, "call_graph_forward");
    check_edges(&index.call_graph_reverse, n_symbols, "call_graph_reverse");
    check_edges(&index.inheritance_forward, n_symbols, "inheritance_forward");
    check_edges(&index.inheritance_reverse, n_symbols, "inheritance_reverse");
    check_edges(&index.members, n_symbols, "members");
    check_edges(&index.overrides, n_symbols, "overrides");

    fn check_symmetry(forward: &[EdgeList], reverse: &[EdgeList], name: &str) {
        for el in forward {
            for &to in &el.to {
                let idx = reverse
                    .binary_search_by_key(&to, |r| r.from)
                    .unwrap_or_else(|_| {
                        panic!(
                            "{name} has {}→{to} but reverse has no entry for {to}",
                            el.from
                        )
                    });
                assert!(
                    reverse[idx].to.contains(&el.from),
                    "{name} has {}→{to} but reverse missing from list",
                    el.from
                );
            }
        }
    }

    check_symmetry(
        &index.call_graph_forward,
        &index.call_graph_reverse,
        "call_graph",
    );
    check_symmetry(
        &index.inheritance_forward,
        &index.inheritance_reverse,
        "inheritance",
    );

    for i in 1..index.name_trigrams.len() {
        assert!(
            index.name_trigrams[i].key > index.name_trigrams[i - 1].key,
            "name_trigrams not sorted at index {i}"
        );
    }

    assert_eq!(
        index.name_hash_buckets.len() as u32,
        index.name_hash_size,
        "name hash bucket count mismatch"
    );
    assert_eq!(
        index.fqn_hash_buckets.len() as u32,
        index.fqn_hash_size,
        "fqn hash bucket count mismatch"
    );

    check_edges(&index.module_deps, n_modules, "module_deps");
    check_edges(&index.module_deps_reverse, n_modules, "module_deps_reverse");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::types::{IntermediateOccurrence, IntermediateSymbol};

    #[test]
    fn test_extract_trigrams_short() {
        let mut buf = Vec::new();
        extract_trigrams_into("ab", &mut buf);
        assert!(buf.is_empty());
        extract_trigrams_into("a", &mut buf);
        assert!(buf.is_empty());
        extract_trigrams_into("", &mut buf);
        assert!(buf.is_empty());
    }

    #[test]
    fn test_extract_trigrams_basic() {
        let mut buf = Vec::new();
        extract_trigrams_into("abcd", &mut buf);
        assert_eq!(buf.len(), 2); // abc, bcd
    }

    #[test]
    fn test_extract_trigrams_dedup() {
        let mut buf = Vec::new();
        extract_trigrams_into("aaa", &mut buf);
        assert_eq!(buf.len(), 1);
    }

    #[test]
    fn test_build_index_minimal() {
        let docs = vec![IntermediateDoc {
            uri: "modules/billing/src/com/example/Billing.scala".to_string(),
            module_segments: "modules.billing".to_string(),
            symbols: vec![
                IntermediateSymbol {
                    fqn: "com/example/Billing#".to_string(),
                    display_name: "Billing".to_string(),
                    kind: SymbolKind::Class,
                    properties: 0,
                    signature: "class Billing".to_string(),
                    parents: vec![],
                    overridden_symbols: vec![],
                    access: Access::Public,
                },
                IntermediateSymbol {
                    fqn: "com/example/Billing#process().".to_string(),
                    display_name: "process".to_string(),
                    kind: SymbolKind::Method,
                    properties: 0,
                    signature: "def process(): Unit".to_string(),
                    parents: vec![],
                    overridden_symbols: vec![],
                    access: Access::Public,
                },
                IntermediateSymbol {
                    fqn: "com/example/Billing#save().".to_string(),
                    display_name: "save".to_string(),
                    kind: SymbolKind::Method,
                    properties: 0,
                    signature: "def save(): Unit".to_string(),
                    parents: vec![],
                    overridden_symbols: vec![],
                    access: Access::Public,
                },
            ],
            occurrences: vec![
                IntermediateOccurrence {
                    symbol: "com/example/Billing#".to_string(),
                    role: ReferenceRole::Definition,
                    start_line: 5,
                    start_col: 6,
                    end_col: 13,
                },
                IntermediateOccurrence {
                    symbol: "com/example/Billing#process().".to_string(),
                    role: ReferenceRole::Definition,
                    start_line: 7,
                    start_col: 6,
                    end_col: 13,
                },
                IntermediateOccurrence {
                    symbol: "com/example/Billing#save().".to_string(),
                    role: ReferenceRole::Definition,
                    start_line: 15,
                    start_col: 6,
                    end_col: 10,
                },
                IntermediateOccurrence {
                    symbol: "com/example/Billing#save().".to_string(),
                    role: ReferenceRole::Reference,
                    start_line: 10,
                    start_col: 4,
                    end_col: 8,
                },
            ],
        }];

        let index = build_index(&docs, None, ".");

        assert_eq!(index.symbols.len(), 3);
        assert_eq!(index.files.len(), 1);
        assert_eq!(index.modules.len(), 1);

        assert_eq!(
            index.strings[index.modules[0].name as usize],
            "modules.billing"
        );

        let billing_id = index
            .symbols
            .iter()
            .find(|s| index.strings[s.fqn as usize] == "com/example/Billing#")
            .unwrap()
            .id;
        let process = index
            .symbols
            .iter()
            .find(|s| index.strings[s.fqn as usize] == "com/example/Billing#process().")
            .unwrap();
        let save = index
            .symbols
            .iter()
            .find(|s| index.strings[s.fqn as usize] == "com/example/Billing#save().")
            .unwrap();
        assert_eq!(process.owner, billing_id);
        assert_eq!(save.owner, billing_id);

        let process_callees: Vec<u32> = index
            .call_graph_forward
            .iter()
            .find(|e| e.from == process.id)
            .map(|e| e.to.clone())
            .unwrap_or_default();
        assert!(
            process_callees.contains(&save.id),
            "process should call save"
        );

        let save_callers: Vec<u32> = index
            .call_graph_reverse
            .iter()
            .find(|e| e.from == save.id)
            .map(|e| e.to.clone())
            .unwrap_or_default();
        assert!(
            save_callers.contains(&process.id),
            "save should be called by process"
        );

        let billing_members: Vec<u32> = index
            .members
            .iter()
            .find(|e| e.from == billing_id)
            .map(|e| e.to.clone())
            .unwrap_or_default();
        assert!(billing_members.contains(&process.id));
        assert!(billing_members.contains(&save.id));

        assert!(
            process.end_line < save.line,
            "process.end_line ({}) should be < save.line ({})",
            process.end_line,
            save.line
        );
    }
}
