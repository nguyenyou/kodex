use super::{s, sym};
use crate::hash::contains_ignore_ascii_case;
use crate::model::{ArchivedEdgeList, ArchivedKodexIndex, ArchivedSymbol, ArchivedSymbolKind};
use crate::query::file_entry;
use rustc_hash::FxHashMap;
use std::fmt::Write;

// ── Ranking ─────────────────────────────────────────────────────────────────

/// Build a symbol_id → reference_count map for ranking search results.
fn build_ref_counts(index: &ArchivedKodexIndex) -> FxHashMap<u32, usize> {
    let mut counts = FxHashMap::default();
    for rl in index.references.iter() {
        let sid: u32 = rl.symbol_id.into();
        counts.insert(sid, rl.refs.len());
    }
    counts
}

/// Composite ranking score for a symbol. Lower is better.
///
/// Combines four signals:
/// - **Kind**: class/trait/object rank above methods, fields, parameters
/// - **Source type**: source files > test files > generated files
/// - **Popularity**: reference count (log-dampened to avoid utility symbols dominating)
/// - **Name length**: shorter names rank higher (more likely to be the "primary" definition)
fn ranking_score(sym: &ArchivedSymbol, index: &ArchivedKodexIndex, ref_counts: &FxHashMap<u32, usize>) -> u32 {
    // Kind boost: type-level definitions surface first
    let kind_penalty = match sym.kind {
        ArchivedSymbolKind::Class | ArchivedSymbolKind::Trait | ArchivedSymbolKind::Interface => 0,
        ArchivedSymbolKind::Object => 10,
        ArchivedSymbolKind::Type => 20,
        ArchivedSymbolKind::Method | ArchivedSymbolKind::Constructor => 30,
        ArchivedSymbolKind::Field => 40,
        ArchivedSymbolKind::Macro => 50,
        _ => 100, // Parameter, TypeParameter, Local, SelfParameter, etc.
    };

    // Source type: prefer source > test > generated
    let file_id: u32 = sym.file_id.into();
    let file = file_entry(index, file_id);
    let source_penalty = if file.is_generated { 40 } else if file.is_test { 20 } else { 0 };

    // Popularity: log-dampened ref count (inverted — more refs = lower score)
    let refs = ref_counts.get(&u32::from(sym.id)).copied().unwrap_or(0);
    // log2(refs+1) gives 0 for 0 refs, ~10 for 1000 refs, ~14 for 16000 refs
    let popularity_bonus = if refs > 0 { ((refs as f64).log2() * 3.0) as u32 } else { 0 };

    // Name length: shorter names are more likely the "primary" symbol
    let name_len = s(index, sym.name).len() as u32;
    let length_penalty = name_len.min(30); // cap at 30 to avoid dominating

    kind_penalty + source_penalty + length_penalty - popularity_bonus.min(40)
}

/// Smart ranking: sort symbols by composite score (kind + source type + popularity + name length).
fn smart_rank<'a>(results: &mut [&'a ArchivedSymbol], index: &ArchivedKodexIndex) {
    if results.len() <= 1 {
        return;
    }
    let ref_counts = build_ref_counts(index);
    results.sort_by_key(|sym| ranking_score(sym, index, &ref_counts));
}

// ── Resolution cascade ──────────────────────────────────────────────────────

/// Find symbols matching a query string.
///
/// # Resolution cascade
///
/// Tries these strategies in order, returning on first hit:
///
/// 1. **Exact FQN** — trigram-narrowed, O(k) where k = trigram posting list size
/// 2. **FQN suffix** — e.g. `com/example/Order#` matches full FQN ending
/// 3. **Owner.member** — dotted notation: `OrderService.createOrder` (nesting up to depth 5)
/// 4. **Exact display name** — O(1) hash index lookup, case-insensitive
/// 5. **Substring** — trigram-accelerated substring on display name
/// 6. **Substring fallback** — linear scan for short queries where trigrams are unavailable
/// 7. **CamelCase matching** — two strategies:
///    - **Segment matching**: IntelliJ CamelHumps — abbreviation (`hcf` → `HttpClientFactory`)
///      and segment subsequence (`UserService` → `UserProfileService`)
///    - **Character subsequence**: scalex-style char-level matching with segment-boundary skipping,
///      handles all-lowercase abbreviations (`lpfuse` → `linkProfileForUser`)
/// 8. **Fuzzy** — Damerau-Levenshtein with adaptive threshold, last resort
///
/// # Ranking
///
/// Results within each step are ranked by a composite score:
/// - **Kind**: class/trait/object > method > field > parameter
/// - **Source type**: source > test > generated
/// - **Popularity**: reference count (log-dampened)
/// - **Name length**: shorter names rank higher
///
/// For scored steps (CamelCase, fuzzy), match quality is the primary sort key
/// and the composite score is the tiebreaker.
#[must_use]
pub fn resolve_symbols<'a>(index: &'a ArchivedKodexIndex, query: &str) -> Vec<&'a ArchivedSymbol> {
    // 1. Exact FQN match — use trigram index to narrow candidates
    let candidates = trigram_candidates(index, query);
    if let Some(ref cands) = candidates {
        let exact: Vec<_> = cands
            .iter()
            .filter(|&&sid| s(index, sym(index, sid).fqn) == query)
            .map(|&sid| sym(index, sid))
            .collect();
        if !exact.is_empty() {
            return exact;
        }
    }
    if candidates.is_none() || query.contains('/') {
        let exact: Vec<_> = index
            .symbols
            .iter()
            .filter(|sym| s(index, sym.fqn) == query)
            .collect();
        if !exact.is_empty() {
            return exact;
        }
    }

    // 2. Suffix match on FQN
    // SemanticDB uses different suffixes for different symbol kinds:
    //   val/object: `Owner.name.`   (ends with `name.`)
    //   def method: `Owner#name().` (ends with `name().`)
    //   type/class: `Owner#name#`   (ends with `name#`)
    // We check all three patterns so a query like "createOrder" finds both
    // val-style endpoints and def-style methods.
    let suffix_dot = format!("{query}.");
    let suffix_hash = format!("{query}#");
    let suffix_paren = format!("{query}().");
    let is_fqn_suffix = |fqn: &str| {
        fqn.ends_with(query)
            || fqn.ends_with(&suffix_dot)
            || fqn.ends_with(&suffix_hash)
            || fqn.ends_with(&suffix_paren)
    };
    if let Some(ref cands) = candidates {
        let suffix: Vec<_> = cands
            .iter()
            .filter(|&&sid| is_fqn_suffix(s(index, sym(index, sid).fqn)))
            .map(|&sid| sym(index, sid))
            .collect();
        if !suffix.is_empty() {
            return suffix;
        }
    }
    if candidates.is_none() || query.contains('/') {
        let suffix: Vec<_> = index
            .symbols
            .iter()
            .filter(|sym| is_fqn_suffix(s(index, sym.fqn)))
            .collect();
        if !suffix.is_empty() {
            return suffix;
        }
    }

    // 3. Owner.member qualified lookup (e.g. "Build.build" or "Build#build")
    if let Some(results) = resolve_owner_member(index, query) {
        if !results.is_empty() {
            return results;
        }
    }

    // query_lower deferred until here — steps 1-3 (FQN + owner.member) don't need it
    let query_lower = query.to_ascii_lowercase();

    // 4. Exact display name match — use hash index for O(1)
    let mut by_hash = hash_lookup(index, query, &query_lower);
    if !by_hash.is_empty() {
        smart_rank(&mut by_hash, index);
        return by_hash;
    }

    // 5. Substring match on display name via trigram intersection
    if let Some(ref cands) = candidates {
        let substr: Vec<_> = cands
            .iter()
            .filter(|&&sid| {
                contains_ignore_ascii_case(s(index, sym(index, sid).name), &query_lower)
            })
            .map(|&sid| sym(index, sid))
            .collect();
        if !substr.is_empty() {
            let mut result = substr;
            smart_rank(&mut result, index);
            return result;
        }
    }

    // 6. Linear substring fallback (for short queries where trigrams aren't available)
    if candidates.is_none() {
        let substr: Vec<_> = index
            .symbols
            .iter()
            .filter(|sym| contains_ignore_ascii_case(s(index, sym.name), &query_lower))
            .collect();
        if !substr.is_empty() {
            let mut result = substr;
            smart_rank(&mut result, index);
            return result;
        }
    }

    // 7. CamelCase segment matching (IntelliJ CamelHumps)
    let query_segs = split_camel_case(query);
    let has_meaningful_segs = query_segs.len() >= 2
        || (query_segs.len() == 1 && query_segs[0].len() >= 2);
    if has_meaningful_segs {
        let hash_size: u32 = index.name_hash_size.into();
        if hash_size > 0 {
            let mut camel_matches: Vec<(&ArchivedSymbol, u32)> = Vec::new();
            let mut seen_names = rustc_hash::FxHashSet::default();
            let mut seg_dl_buf = vec![0usize; 30];
            for bucket in index.name_hash_buckets.iter() {
                for sid in bucket.symbol_ids.iter() {
                    let sid_val: u32 = (*sid).into();
                    let name = s(index, sym(index, sid_val).name);
                    if seen_names.contains(name) {
                        continue;
                    }
                    let cand_segs = split_camel_case(name);
                    if let Some(score) = camel_match_score(&query_segs, &cand_segs, &mut seg_dl_buf) {
                        seen_names.insert(name);
                        camel_matches.push((sym(index, sid_val), score));
                    } else if let Some(score) = char_subsequence_score(query, name) {
                        seen_names.insert(name);
                        camel_matches.push((sym(index, sid_val), score));
                    }
                }
            }
            if !camel_matches.is_empty() {
                // Dedup by symbol ID first (must be adjacent for dedup_by_key)
                camel_matches.sort_by_key(|&(sym, _)| u32::from(sym.id));
                camel_matches.dedup_by_key(|&mut (sym, _)| u32::from(sym.id));
                // Sort by match score (primary), then composite ranking (tiebreaker)
                let ref_counts = build_ref_counts(index);
                camel_matches.sort_by(|a, b| {
                    a.1.cmp(&b.1).then_with(|| {
                        ranking_score(a.0, index, &ref_counts)
                            .cmp(&ranking_score(b.0, index, &ref_counts))
                    })
                });
                return camel_matches.into_iter().map(|(sym, _)| sym).collect();
            }
        }
    }

    // 8. Fuzzy match (Damerau-Levenshtein) — last resort for typos
    let max_dist = fuzzy_threshold(query.len());
    let mut fuzzy: Vec<(&ArchivedSymbol, usize)> = Vec::new();
    let hash_size: u32 = index.name_hash_size.into();
    if max_dist > 0 && hash_size > 0 {
        let mut dl_buf = vec![0usize; 3 * (query_lower.len() + 1)];
        let mut name_buf = String::new();
        for bucket in index.name_hash_buckets.iter() {
            for sid in bucket.symbol_ids.iter() {
                let sid_val: u32 = (*sid).into();
                let name = s(index, sym(index, sid_val).name);
                name_buf.clear();
                name_buf.extend(name.chars().map(|c| c.to_ascii_lowercase()));
                // Early rejection: length difference alone exceeds threshold
                let len_diff = query_lower.len().abs_diff(name_buf.len());
                if len_diff > max_dist {
                    continue;
                }
                let dist = damerau_levenshtein_buffered(&query_lower, &name_buf, &mut dl_buf);
                if dist > 0 && dist <= max_dist {
                    // Meaningfulness guard: suppress if more than half the chars are wrong
                    if dist * 2 > query_lower.len().max(name_buf.len()) {
                        continue;
                    }
                    fuzzy.push((sym(index, sid_val), dist));
                }
            }
        }
    }
    if !fuzzy.is_empty() {
        fuzzy.sort_by_key(|&(sym, _)| u32::from(sym.id));
        fuzzy.dedup_by_key(|&mut (sym, _)| u32::from(sym.id));
        let ref_counts = build_ref_counts(index);
        fuzzy.sort_by(|a, b| {
            a.1.cmp(&b.1).then_with(|| {
                ranking_score(a.0, index, &ref_counts)
                    .cmp(&ranking_score(b.0, index, &ref_counts))
            })
        });
        return fuzzy.into_iter().map(|(sym, _)| sym).collect();
    }

    vec![]
}

/// Use the hash index for O(1) exact display name lookup.
fn hash_lookup<'a>(
    index: &'a ArchivedKodexIndex,
    query: &str,
    query_lower: &str,
) -> Vec<&'a ArchivedSymbol> {
    let hash_size: u32 = index.name_hash_size.into();
    if hash_size == 0 {
        return vec![];
    }
    let bucket_idx = name_hash_query(query, hash_size);
    if bucket_idx as usize >= index.name_hash_buckets.len() {
        return vec![];
    }
    let bucket = &index.name_hash_buckets[bucket_idx as usize];
    bucket
        .symbol_ids
        .iter()
        .filter(|sid| {
            let sid_val: u32 = (**sid).into();
            s(index, sym(index, sid_val).name).eq_ignore_ascii_case(query_lower)
        })
        .map(|sid| {
            let sid_val: u32 = (*sid).into();
            sym(index, sid_val)
        })
        .collect()
}

/// Resolve "Owner.member" or "Owner#member" by finding the owner, then filtering its members.
/// Supports nested owners like "Outer.Inner.method" via recursion (max depth 5).
fn resolve_owner_member<'a>(
    index: &'a ArchivedKodexIndex,
    query: &str,
) -> Option<Vec<&'a ArchivedSymbol>> {
    resolve_owner_member_impl(index, query, 0)
}

fn resolve_owner_member_impl<'a>(
    index: &'a ArchivedKodexIndex,
    query: &str,
    depth: u8,
) -> Option<Vec<&'a ArchivedSymbol>> {
    if depth > 5 {
        return None;
    }
    // Split on last '.' or '#'
    let split_pos = query.rfind(['.', '#'])?;
    let (owner_query, member_query) = query.split_at(split_pos);
    let member_query = &member_query[1..]; // skip the delimiter
    if owner_query.is_empty() || member_query.is_empty() {
        return None;
    }

    // Resolve the owner by name (hash lookup for speed)
    let owner_lower = owner_query.to_ascii_lowercase();
    let mut owners = hash_lookup(index, owner_query, &owner_lower);

    // If hash lookup fails and owner itself contains '.' or '#', recurse
    if owners.is_empty() && owner_query.contains(['.', '#']) {
        if let Some(nested) = resolve_owner_member_impl(index, owner_query, depth + 1) {
            owners = nested;
        }
    }

    if owners.is_empty() {
        return None;
    }

    let member_lower = member_query.to_ascii_lowercase();
    let mut results: Vec<&ArchivedSymbol> = Vec::new();
    for owner in &owners {
        let owner_id: u32 = owner.id.into();
        find_member_recursive(index, owner_id, &member_lower, &mut results, 0);
    }
    Some(results)
}

/// Recursively search for a member by name within an owner's members.
/// First checks direct members, then recurses into nested type members
/// (classes, traits, objects) up to depth 3. This handles the common Scala.js
/// pattern where methods live inside a `Backend` inner class:
///   `Component.method` finds `Component.Backend.method`
fn find_member_recursive<'a>(
    index: &'a ArchivedKodexIndex,
    owner_id: u32,
    member_lower: &str,
    results: &mut Vec<&'a ArchivedSymbol>,
    depth: u8,
) {
    let member_ids = edges_from(&index.members, owner_id);

    // First pass: look for direct matches
    for mid in member_ids {
        let mid_val: u32 = (*mid).into();
        let member = sym(index, mid_val);
        if s(index, member.name).eq_ignore_ascii_case(member_lower) {
            results.push(member);
        }
    }

    // If found directly, don't recurse — direct matches take priority
    if !results.is_empty() || depth >= 3 {
        return;
    }

    // Second pass: recurse into nested type members (class, trait, object)
    for mid in edges_from(&index.members, owner_id) {
        let mid_val: u32 = (*mid).into();
        let member = sym(index, mid_val);
        match member.kind {
            ArchivedSymbolKind::Class
            | ArchivedSymbolKind::Trait
            | ArchivedSymbolKind::Object
            | ArchivedSymbolKind::Interface => {
                find_member_recursive(index, mid_val, member_lower, results, depth + 1);
            }
            _ => {}
        }
    }
}

/// Use trigram index to find candidate symbol IDs matching a query.
/// Returns None if query is too short for trigrams (< 3 chars).
fn trigram_candidates(index: &ArchivedKodexIndex, query: &str) -> Option<Vec<u32>> {
    let bytes = query.as_bytes();
    if bytes.len() < 3 {
        return None;
    }

    // Extract trigrams from query
    let mut query_trigrams: Vec<u32> = Vec::new();
    for i in 0..bytes.len() - 2 {
        query_trigrams.push(trigram_key_query(bytes[i], bytes[i + 1], bytes[i + 2]));
    }
    query_trigrams.sort_unstable();
    query_trigrams.dedup();

    // Intersect posting lists
    let mut result: Option<Vec<u32>> = None;
    for &tri_key in &query_trigrams {
        // Binary search in sorted trigram entries
        let posting = match index
            .name_trigrams
            .binary_search_by_key(&tri_key, |entry| entry.key.into())
        {
            Ok(idx) => index.name_trigrams[idx]
                .symbol_ids
                .iter()
                .map(|v| (*v).into())
                .collect::<Vec<u32>>(),
            Err(_) => return Some(vec![]), // trigram not in index → no matches possible
        };
        result = Some(match result {
            None => posting,
            Some(prev) => intersect_sorted(&prev, &posting),
        });
        // Early exit if intersection is empty
        if result.as_ref().is_some_and(std::vec::Vec::is_empty) {
            return Some(vec![]);
        }
    }
    result
}

/// Intersect two sorted Vec<u32>.
fn intersect_sorted(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut result = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                result.push(a[i]);
                i += 1;
                j += 1;
            }
        }
    }
    result
}

use crate::hash::{
    case_insensitive_hash as name_hash_query, case_sensitive_hash as fqn_hash_query,
    trigram_key as trigram_key_query,
};

/// Resolve symbols and apply kind + module filters.
/// Returns empty vec if nothing matches. Prints warnings for module filter misses.
///
/// Note: generated-file filtering is NOT applied here — it must be applied at the
/// command level on output lists. Filtering during resolution would make symbols in
/// generated files (including shared cross-compiled sources) completely invisible.
#[must_use]
pub fn resolve_filtered<'a>(
    index: &'a ArchivedKodexIndex,
    query: &str,
    kind_filter: Option<&str>,
    module_filter: Option<&str>,
) -> Vec<&'a ArchivedSymbol> {
    let mut candidates = resolve_symbols(index, query);
    if candidates.is_empty() {
        return vec![];
    }
    // Apply kind filter strictly — return empty if no symbols match the requested kind
    if let Some(k) = kind_filter {
        candidates.retain(|sym| kind_str(&sym.kind).eq_ignore_ascii_case(k));
        if candidates.is_empty() {
            return vec![];
        }
    }
    if let Some(mp) = module_filter {
        let by_mod = crate::query::filter::filter_by_module(index, &candidates, mp);
        if by_mod.is_empty() {
            eprintln!("Warning: --module '{mp}' matched no results, showing all matches");
        } else {
            candidates = by_mod;
        }
    }
    candidates
}

/// List all symbols belonging to a module (with optional kind filter).
/// Used for module-only search when no query is provided.
#[must_use]
pub fn list_module_symbols<'a>(
    index: &'a ArchivedKodexIndex,
    module_pattern: &str,
    kind_filter: Option<&str>,
) -> Vec<&'a ArchivedSymbol> {
    let all_symbols: Vec<&ArchivedSymbol> = index.symbols.iter().collect();
    let mut candidates = crate::query::filter::filter_by_module(index, &all_symbols, module_pattern);
    if let Some(k) = kind_filter {
        candidates.retain(|sym| kind_str(&sym.kind).eq_ignore_ascii_case(k));
    }
    // Filter out parameters, type parameters, locals, self parameters
    candidates.retain(|sym| {
        !matches!(
            sym.kind,
            ArchivedSymbolKind::Parameter
                | ArchivedSymbolKind::TypeParameter
                | ArchivedSymbolKind::SelfParameter
                | ArchivedSymbolKind::Local
        )
    });
    smart_rank(&mut candidates, index);
    candidates
}

/// Resolve to a single symbol, printing disambiguation to stderr if ambiguous.
/// When ambiguous, ranks candidates: exact name match > type-level symbols > others.
pub fn resolve_one<'a>(
    index: &'a ArchivedKodexIndex,
    query: &str,
    kind_filter: Option<&str>,
    module_filter: Option<&str>,
) -> Option<&'a ArchivedSymbol> {
    let mut candidates = resolve_filtered(index, query, kind_filter, module_filter);
    if candidates.is_empty() {
        return None;
    }

    if candidates.len() > 1 {
        let query_lower = query.to_ascii_lowercase();
        // Rank: exact name match first, then type-level kinds, then by file position
        candidates.sort_by(|a, b| {
            let a_exact = s(index, a.name).eq_ignore_ascii_case(&query_lower);
            let b_exact = s(index, b.name).eq_ignore_ascii_case(&query_lower);
            b_exact
                .cmp(&a_exact)
                .then_with(|| kind_rank(&b.kind).cmp(&kind_rank(&a.kind)))
        });
        eprintln!(
            "Ambiguous: {} symbols match '{}'. Using {}",
            candidates.len(),
            query,
            s(index, candidates[0].fqn)
        );
        eprintln!("  Disambiguate with FQN or --kind. Candidates:");
        for sym in candidates.iter().take(5) {
            eprintln!("    {} {}", kind_str(&sym.kind), s(index, sym.fqn));
        }
        if candidates.len() > 5 {
            eprintln!("    ... and {} more", candidates.len() - 5);
        }
    }
    Some(candidates[0])
}

/// Ranking weight for ambiguity resolution: higher = preferred.
fn kind_rank(kind: &ArchivedSymbolKind) -> u8 {
    match *kind {
        ArchivedSymbolKind::Class | ArchivedSymbolKind::Trait | ArchivedSymbolKind::Interface => 3,
        ArchivedSymbolKind::Object => 2,
        ArchivedSymbolKind::Method | ArchivedSymbolKind::Field | ArchivedSymbolKind::Type => 1,
        _ => 0,
    }
}

pub fn filter_by_kind<'a>(
    symbols: &[&'a ArchivedSymbol],
    kind_filter: Option<&str>,
) -> Vec<&'a ArchivedSymbol> {
    match kind_filter {
        None => symbols.to_vec(),
        Some(k) => symbols
            .iter()
            .filter(|sym| kind_str(&sym.kind).eq_ignore_ascii_case(k))
            .copied()
            .collect(),
    }
}

#[must_use]
#[allow(clippy::match_wildcard_for_single_variants)]
pub fn kind_str(kind: &ArchivedSymbolKind) -> &'static str {
    crate::model::kind_str_match!(*kind, ArchivedSymbolKind)
}

/// Restricted Damerau-Levenshtein with reusable buffer (avoids allocation per call).
/// Supports insertion, deletion, substitution, and transposition of adjacent characters.
/// "Restricted" means already-transposed characters cannot be further modified.
/// This matches the algorithm used by rustc and GCC for "did you mean?" suggestions.
/// `buf` is grown as needed — callers don't need to pre-size it precisely.
fn damerau_levenshtein_buffered(a: &str, b: &str, buf: &mut Vec<usize>) -> usize {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let (m, n) = (a.len(), b.len());
    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }
    // Three rows of length (n+1): prev_prev, prev, current
    let row_len = n + 1;
    let needed = 3 * row_len;
    if buf.len() < needed {
        buf.resize(needed, 0);
    }
    // Work with explicit index ranges to avoid borrow conflicts
    let pp = 0;
    let pv = row_len;
    let cr = 2 * row_len;
    // Initialize prev row (row 0)
    for j in 0..row_len {
        buf[pv + j] = j;
    }
    for i in 1..=m {
        buf[cr] = i;
        for j in 1..=n {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            buf[cr + j] = (buf[pv + j] + 1) // deletion
                .min(buf[cr + j - 1] + 1) // insertion
                .min(buf[pv + j - 1] + cost); // substitution
            // Transposition: swap of adjacent characters
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                buf[cr + j] = buf[cr + j].min(buf[pp + j - 2] + 1);
            }
        }
        // Rotate rows: prev_prev <- prev, prev <- current
        buf.copy_within(pv..pv + row_len, pp);
        buf.copy_within(cr..cr + row_len, pv);
    }
    buf[pv + n]
}

/// Max fuzzy distance threshold, scaled to identifier length.
/// Matches the industry consensus (rustc, Clang, GCC): ~one-third of name length.
/// Capped at 5 to prevent garbage suggestions on very long names.
/// Returns 0 for queries ≤ 2 chars (too many false positives).
fn fuzzy_threshold(query_len: usize) -> usize {
    if query_len <= 2 {
        return 0;
    }
    std::cmp::min(std::cmp::max(query_len, 3) / 3, 5)
}

/// Split an identifier on CamelCase boundaries, returning lowercased segments.
///
/// Rules (matching IntelliJ CamelHumps):
/// - Uppercase after lowercase starts a new segment: `UserProfile` → `["user", "profile"]`
/// - Consecutive uppercase: last uppercase before a lowercase starts a new segment:
///   `HTMLParser` → `["html", "parser"]`, `getHTTPResponse` → `["get", "http", "response"]`
/// - Digits start a new segment: `Room2Go` → `["room", "2", "go"]`
/// - Underscores are separators (dropped): `my_Func` → `["my", "func"]`
fn split_camel_case(name: &str) -> Vec<String> {
    let mut segments: Vec<String> = Vec::new();
    let mut current = String::new();
    let bytes = name.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        let c = bytes[i] as char;
        if c == '_' {
            // Underscore: flush and skip
            if !current.is_empty() {
                segments.push(current.to_ascii_lowercase());
                current.clear();
            }
            i += 1;
            continue;
        }
        if c.is_ascii_digit() && !current.is_empty() && !current.bytes().all(|b| (b as char).is_ascii_digit()) {
            // Digit after non-digit: start new segment
            segments.push(current.to_ascii_lowercase());
            current.clear();
        }
        if c.is_ascii_uppercase() {
            if !current.is_empty() {
                // Check if this is a transition from lowercase→uppercase (new segment)
                let prev = bytes[i - 1] as char;
                if prev.is_ascii_lowercase() || prev.is_ascii_digit() {
                    segments.push(current.to_ascii_lowercase());
                    current.clear();
                } else if prev.is_ascii_uppercase() {
                    // Consecutive uppercase: check if next char is lowercase (split before current)
                    // e.g., "HTMLParser" at 'P': split "HTM" | "LParser"... actually "HTML" | "Parser"
                    if i + 1 < len && (bytes[i + 1] as char).is_ascii_lowercase() {
                        segments.push(current.to_ascii_lowercase());
                        current.clear();
                    }
                }
            }
        }
        current.push(c);
        i += 1;
    }
    if !current.is_empty() {
        segments.push(current.to_ascii_lowercase());
    }
    segments
}

/// Score how well query segments match candidate segments (CamelCase matching).
/// Returns `Some(score)` if matched (lower is better), `None` if no match.
///
/// Supports three strategies:
/// 1. **Abbreviation**: query is short lowercase chars matching first char of each candidate segment
///    e.g., `"hcf"` matches `["http","client","factory"]`
/// 2. **Segment subsequence**: query segments match candidate segments in order (with skips)
///    Each segment matches by exact, prefix, or fuzzy (DL ≤ 1)
///    e.g., `["user","service"]` matches `["user","profile","service"]`
fn camel_match_score(query_segs: &[String], cand_segs: &[String], dl_buf: &mut Vec<usize>) -> Option<u32> {
    if query_segs.is_empty() || cand_segs.is_empty() {
        return None;
    }

    // Strategy 1: Abbreviation — query is a single segment of lowercase initials
    // e.g., query "hcf" (one segment) against candidate ["http","client","factory"]
    if query_segs.len() == 1 {
        let q = &query_segs[0];
        let q_bytes = q.as_bytes();
        if q_bytes.len() >= 2
            && q_bytes.len() <= cand_segs.len()
            && q_bytes.len() * 2 > cand_segs.len() // must cover more than half the segments
            && q_bytes.iter().all(|b| b.is_ascii_lowercase())
        {
            let mut ci = 0;
            let mut matched = true;
            for &qb in q_bytes {
                let found = cand_segs[ci..].iter().position(|seg| {
                    seg.as_bytes().first().copied() == Some(qb)
                });
                match found {
                    Some(offset) => ci += offset + 1,
                    None => { matched = false; break; }
                }
            }
            if matched {
                // Score: 100 base (abbreviation), penalty for skipped segments
                let skips = cand_segs.len() - q_bytes.len();
                return Some(100 + skips as u32);
            }
        }
    }

    // Strategy 2: Segment subsequence — match query segments against candidate segments in order
    if query_segs.len() > cand_segs.len() {
        return None;
    }
    let mut ci = 0;
    let mut score: u32 = 0;
    let mut skips: u32 = 0;
    for qs in query_segs {
        let mut found = false;
        while ci < cand_segs.len() {
            let cs = &cand_segs[ci];
            ci += 1;
            // Exact match
            if qs == cs {
                found = true;
                break;
            }
            // Prefix match: query segment is a prefix of candidate segment
            if cs.starts_with(qs.as_str()) && qs.len() >= 2 {
                score += 10;
                found = true;
                break;
            }
            // Fuzzy match: DL distance ≤ 1 on the segment (only for segments ≥ 3 chars)
            if qs.len() >= 3 && cs.len() >= 3 {
                let seg_dist = damerau_levenshtein_buffered(qs, cs, dl_buf);
                if seg_dist <= 1 {
                    score += 20;
                    found = true;
                    break;
                }
            }
            skips += 1;
        }
        if !found {
            return None;
        }
    }
    // Penalize skipped segments and total candidate length
    score += skips * 50;
    Some(score)
}

/// Is position `i` the start of a CamelCase segment in `name`?
/// True at: start of string, uppercase letters, positions after `_`.
#[inline]
fn is_segment_start(name: &[u8], i: usize) -> bool {
    i == 0 || (name[i] as char).is_ascii_uppercase() || (i > 0 && name[i - 1] == b'_')
}

/// Character-level subsequence matching with CamelCase segment-boundary skipping.
/// Matches each query char greedily against the candidate name. On mismatch, skips
/// to the next segment boundary (uppercase letter or after `_`).
///
/// This handles lowercase abbreviations like `lpfuse` → `linkProfileForUser`:
/// l→l, p→P(rofile), f→F(or), u→U(ser), s→s(er), e→e(r).
fn char_subsequence_score(query: &str, candidate_name: &str) -> Option<u32> {
    if query.len() < 2 || candidate_name.len() < query.len() {
        return None;
    }
    let q_lower: Vec<u8> = query.bytes().map(|b| b.to_ascii_lowercase()).collect();
    let n_bytes = candidate_name.as_bytes();
    let n_lower: Vec<u8> = n_bytes.iter().map(|b| b.to_ascii_lowercase()).collect();

    let mut qi: usize = 0;
    let mut ni: usize = 0;
    let mut boundary_matches: u32 = 0;
    let mut total_skips: u32 = 0;

    while qi < q_lower.len() && ni < n_lower.len() {
        if q_lower[qi] == n_lower[ni] {
            if is_segment_start(n_bytes, ni) {
                boundary_matches += 1;
            }
            qi += 1;
            ni += 1;
        } else {
            // Skip to next segment boundary
            ni += 1;
            while ni < n_lower.len() && !is_segment_start(n_bytes, ni) {
                ni += 1;
            }
            total_skips += 1;
        }
    }

    if qi == q_lower.len() {
        // All query chars matched. Score: lower is better.
        // Base 50 (between abbreviation=100 and exact segment=0).
        // Penalize skips, reward boundary alignment.
        Some(50 + total_skips * 10 - boundary_matches.min(5) * 5)
    } else {
        None
    }
}

/// Find "Did you mean: X?" suggestions when a query finds nothing.
/// Uses Damerau-Levenshtein with a threshold scaled to query length.
/// Returns formatted suggestion text (empty string if no suggestions).
pub fn suggest_similar(index: &ArchivedKodexIndex, query: &str) -> String {
    let max_dist = fuzzy_threshold(query.len());
    let query_lower = query.to_ascii_lowercase();
    let mut suggestions: Vec<(&str, usize)> = Vec::new();
    let mut seen = rustc_hash::FxHashSet::default();

    let hash_size: u32 = index.name_hash_size.into();
    if hash_size == 0 {
        return String::new();
    }
    // DL fuzzy matching (only if threshold > 0, i.e., query > 2 chars)
    if max_dist > 0 {
        let mut dl_buf = vec![0usize; 3 * (query_lower.len() + 1)];
        let mut name_buf = String::new();
        for bucket in index.name_hash_buckets.iter() {
            for sid in bucket.symbol_ids.iter() {
                let sid_val: u32 = (*sid).into();
                let name = s(index, sym(index, sid_val).name);
                name_buf.clear();
                name_buf.extend(name.chars().map(|c| c.to_ascii_lowercase()));
                if seen.contains(name_buf.as_str()) {
                    continue;
                }
                // Early rejection: length difference alone exceeds threshold
                let len_diff = query_lower.len().abs_diff(name_buf.len());
                if len_diff > max_dist {
                    continue;
                }
                let dist = damerau_levenshtein_buffered(&query_lower, &name_buf, &mut dl_buf);
                if dist > 0 && dist <= max_dist {
                    // GCC's meaningfulness guard: suppress if more than half the chars are wrong
                    if dist * 2 > query_lower.len().max(name_buf.len()) {
                        continue;
                    }
                    seen.insert(name_buf.clone()); // only allocate on match (rare)
                    suggestions.push((name, dist));
                }
            }
        }
    }
    // If DL found nothing, try CamelCase segment matching as fallback
    if suggestions.is_empty() {
        let query_segs = split_camel_case(query);
        if !query_segs.is_empty() {
            let mut camel_suggestions: Vec<(&str, u32)> = Vec::new();
            let mut camel_seen = rustc_hash::FxHashSet::default();
            let mut seg_dl_buf = vec![0usize; 30];
            for bucket in index.name_hash_buckets.iter() {
                for sid in bucket.symbol_ids.iter() {
                    let sid_val: u32 = (*sid).into();
                    let name = s(index, sym(index, sid_val).name);
                    if camel_seen.contains(name) {
                        continue;
                    }
                    let cand_segs = split_camel_case(name);
                    if let Some(score) = camel_match_score(&query_segs, &cand_segs, &mut seg_dl_buf) {
                        camel_seen.insert(name);
                        camel_suggestions.push((name, score));
                    }
                }
            }
            if !camel_suggestions.is_empty() {
                camel_suggestions.sort_by_key(|&(_, score)| score);
                let mut out = String::new();
                writeln!(out, "Did you mean:").unwrap();
                for (name, _) in camel_suggestions.iter().take(5) {
                    writeln!(out, "  {name}").unwrap();
                }
                return out;
            }
        }
    }

    if suggestions.is_empty() {
        return String::new();
    }
    suggestions.sort_by_key(|&(_, dist)| dist);
    suggestions.dedup_by_key(|&mut (name, _)| name);
    let mut out = String::new();
    writeln!(out, "Did you mean:").unwrap();
    for (name, _) in suggestions.iter().take(5) {
        writeln!(out, "  {name}").unwrap();
    }
    out
}

/// Look up a symbol by exact FQN. Uses FQN hash index for O(1) lookup.
#[must_use]
pub fn find_by_fqn<'a>(index: &'a ArchivedKodexIndex, fqn: &str) -> Option<&'a ArchivedSymbol> {
    let hash_size: u32 = index.fqn_hash_size.into();
    if hash_size > 0 {
        let bucket_idx = fqn_hash_query(fqn, hash_size);
        if let Some(bucket) = index.fqn_hash_buckets.get(bucket_idx as usize) {
            for sid in bucket.symbol_ids.iter() {
                let sid_val: u32 = (*sid).into();
                if s(index, sym(index, sid_val).fqn) == fqn {
                    return Some(sym(index, sid_val));
                }
            }
        }
        return None;
    }
    // Fallback for indexes built before v6
    index.symbols.iter().find(|sym| s(index, sym.fqn) == fqn)
}

/// Find edges from a given node in an edge list (binary search — edge lists sorted by `from`).
/// Returns a borrowed slice — zero allocation.
pub fn edges_from(edge_lists: &[ArchivedEdgeList], from_id: u32) -> &[rkyv::Archived<u32>] {
    match edge_lists.binary_search_by_key(&from_id, |el| el.from.into()) {
        Ok(idx) => &edge_lists[idx].to,
        Err(_) => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dl(a: &str, b: &str) -> usize {
        let mut buf = vec![0usize; 3 * (a.len() + 1)];
        damerau_levenshtein_buffered(a, b, &mut buf)
    }

    #[test]
    fn test_levenshtein() {
        // Basic edit distance (insertion, deletion, substitution)
        assert_eq!(dl("kitten", "sitting"), 3);
        assert_eq!(dl("abc", "abc"), 0);
        assert_eq!(dl("ab", "abc"), 1);
        assert_eq!(dl("abc", "ab"), 1);
        assert_eq!(dl("service", "servce"), 1);
        assert_eq!(dl("Service", "Servce"), 1);
        assert_eq!(dl("a", "b"), 1);
    }

    #[test]
    fn test_levenshtein_empty() {
        assert_eq!(dl("", ""), 0);
        assert_eq!(dl("", "abc"), 3);
        assert_eq!(dl("abc", ""), 3);
    }

    #[test]
    fn test_damerau_transposition() {
        // Transpositions should cost 1, not 2
        assert_eq!(dl("ab", "ba"), 1);        // single transposition
        assert_eq!(dl("stauts", "status"), 1); // the classic git typo
        assert_eq!(dl("teh", "the"), 1);       // common typo
        assert_eq!(dl("abc", "bac"), 1);       // prefix transposition
        assert_eq!(dl("abcd", "abdc"), 1);     // suffix transposition
    }

    #[test]
    fn test_fuzzy_threshold() {
        assert_eq!(fuzzy_threshold(1), 0);  // too short, no suggestions
        assert_eq!(fuzzy_threshold(2), 0);  // too short
        assert_eq!(fuzzy_threshold(3), 1);  // 3/3 = 1
        assert_eq!(fuzzy_threshold(5), 1);  // 5/3 = 1
        assert_eq!(fuzzy_threshold(6), 2);  // 6/3 = 2
        assert_eq!(fuzzy_threshold(9), 3);  // 9/3 = 3
        assert_eq!(fuzzy_threshold(12), 4); // 12/3 = 4
        assert_eq!(fuzzy_threshold(15), 5); // 15/3 = 5
        assert_eq!(fuzzy_threshold(30), 5); // capped at 5
    }

    #[test]
    fn test_split_camel_case() {
        let s = |name: &str| split_camel_case(name);
        assert_eq!(s("HttpClientFactory"), vec!["http", "client", "factory"]);
        assert_eq!(s("HTMLParser"), vec!["html", "parser"]);
        assert_eq!(s("getHTTPResponse"), vec!["get", "http", "response"]);
        assert_eq!(s("ServiceImpl"), vec!["service", "impl"]);
        assert_eq!(s("PetStore"), vec!["pet", "store"]);
        assert_eq!(s("save"), vec!["save"]);
        assert_eq!(s("A"), vec!["a"]);
        assert_eq!(s(""), Vec::<String>::new());
        assert_eq!(s("XMLToJSON"), vec!["xml", "to", "json"]);
        assert_eq!(s("Room2Go"), vec!["room", "2", "go"]);
        assert_eq!(s("my_Func"), vec!["my", "func"]);
        assert_eq!(s("IOUtils"), vec!["io", "utils"]);
    }

    #[test]
    fn test_camel_match_abbreviation() {
        let q = |query: &str| split_camel_case(query);
        let c = |name: &str| split_camel_case(name);
        let mut buf = vec![0usize; 30];
        // "hcf" matches HttpClientFactory (abbreviation: h→http, c→client, f→factory)
        assert!(camel_match_score(&q("hcf"), &c("HttpClientFactory"), &mut buf).is_some());
        // "si" matches ServiceImpl (abbreviation: s→service, i→impl)
        assert!(camel_match_score(&q("si"), &c("ServiceImpl"), &mut buf).is_some());
        // "ps" matches PetStore
        assert!(camel_match_score(&q("ps"), &c("PetStore"), &mut buf).is_some());
        // "xyz" does NOT match HttpClientFactory
        assert!(camel_match_score(&q("xyz"), &c("HttpClientFactory"), &mut buf).is_none());
    }

    #[test]
    fn test_camel_match_segment_subsequence() {
        let q = |query: &str| split_camel_case(query);
        let c = |name: &str| split_camel_case(name);
        let mut buf = vec![0usize; 30];
        // Exact segments: "HttpClient" matches HttpClientFactory (subset: skips "Factory")
        assert!(camel_match_score(&q("HttpClient"), &c("HttpClientFactory"), &mut buf).is_some());
        // Prefix per segment: "HttpCliFact" → ["http","cli","fact"] matches ["http","client","factory"]
        assert!(camel_match_score(&q("HttpCliFact"), &c("HttpClientFactory"), &mut buf).is_some());
        // Fuzzy per segment: "HttpClinetFactory" → ["http","clinet","factory"]
        // "clinet" vs "client" → DL distance 1 (transposition)
        assert!(camel_match_score(&q("HttpClinetFactory"), &c("HttpClientFactory"), &mut buf).is_some());
        // Skip middle segment: "HttpFactory" → ["http","factory"] matches ["http","client","factory"]
        assert!(camel_match_score(&q("HttpFactory"), &c("HttpClientFactory"), &mut buf).is_some());
        // Too many query segments: doesn't match
        assert!(camel_match_score(&q("HttpClientFactoryBuilder"), &c("HttpClientFactory"), &mut buf).is_none());
    }

    #[test]
    fn test_intersect_sorted_basic() {
        assert_eq!(intersect_sorted(&[1, 3, 5], &[2, 3, 4, 5]), vec![3, 5]);
    }

    #[test]
    fn test_intersect_sorted_empty() {
        assert_eq!(intersect_sorted(&[], &[1, 2, 3]), Vec::<u32>::new());
        assert_eq!(intersect_sorted(&[1, 2], &[]), Vec::<u32>::new());
    }

    #[test]
    fn test_intersect_sorted_disjoint() {
        assert_eq!(intersect_sorted(&[1, 3, 5], &[2, 4, 6]), Vec::<u32>::new());
    }

    #[test]
    fn test_intersect_sorted_identical() {
        assert_eq!(intersect_sorted(&[1, 2, 3], &[1, 2, 3]), vec![1, 2, 3]);
    }

    // trigram_key and case_insensitive_hash are tested in hash.rs

    #[test]
    fn test_kind_str_all_variants() {
        assert_eq!(kind_str(&ArchivedSymbolKind::Class), "class");
        assert_eq!(kind_str(&ArchivedSymbolKind::Trait), "trait");
        assert_eq!(kind_str(&ArchivedSymbolKind::Object), "object");
        assert_eq!(kind_str(&ArchivedSymbolKind::Method), "method");
        assert_eq!(kind_str(&ArchivedSymbolKind::Field), "field");
        assert_eq!(kind_str(&ArchivedSymbolKind::Type), "type");
        assert_eq!(kind_str(&ArchivedSymbolKind::Constructor), "constructor");
        assert_eq!(kind_str(&ArchivedSymbolKind::Parameter), "parameter");
        assert_eq!(
            kind_str(&ArchivedSymbolKind::TypeParameter),
            "typeparameter"
        );
        assert_eq!(kind_str(&ArchivedSymbolKind::Package), "package");
        assert_eq!(
            kind_str(&ArchivedSymbolKind::PackageObject),
            "packageobject"
        );
        assert_eq!(kind_str(&ArchivedSymbolKind::Macro), "macro");
        assert_eq!(kind_str(&ArchivedSymbolKind::Local), "local");
        assert_eq!(kind_str(&ArchivedSymbolKind::Interface), "interface");
        assert_eq!(
            kind_str(&ArchivedSymbolKind::SelfParameter),
            "selfparameter"
        );
    }

    #[test]
    fn test_is_segment_start() {
        let name = b"linkProfileForUser";
        assert!(is_segment_start(name, 0));    // 'l' — start of string
        assert!(!is_segment_start(name, 1));   // 'i' — middle
        assert!(is_segment_start(name, 4));    // 'P' — uppercase
        assert!(is_segment_start(name, 11));   // 'F' — uppercase
        assert!(is_segment_start(name, 14));   // 'U' — uppercase

        let underscored = b"my_Func";
        assert!(is_segment_start(underscored, 3)); // 'F' — after underscore
    }

    #[test]
    fn test_char_subsequence_score() {
        // lpfuse → linkProfileForUser: l→l, p→P, f→F, u→U, s→s, e→e
        assert!(char_subsequence_score("lpfuse", "linkProfileForUser").is_some());

        // hcf → HttpClientFactory: h→H, c→C, f→F
        assert!(char_subsequence_score("hcf", "HttpClientFactory").is_some());

        // ghr → getHTTPResponse: g→g, h→H, r→R
        assert!(char_subsequence_score("ghr", "getHTTPResponse").is_some());

        // ors → OrderService: o→O, r→r, s→S
        assert!(char_subsequence_score("ors", "OrderService").is_some());

        // xyz → no match
        assert!(char_subsequence_score("xyz", "HttpClientFactory").is_none());

        // single char — too short
        assert!(char_subsequence_score("h", "HttpClientFactory").is_none());

        // query longer than candidate — no match
        assert!(char_subsequence_score("httpservletrequest", "Http").is_none());

        // exact prefix still works
        assert!(char_subsequence_score("link", "linkProfileForUser").is_some());
    }
}
