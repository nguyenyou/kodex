use super::CommandResult;
use crate::model::ArchivedKodexIndex;
use crate::query::format::{format_symbol_detail, format_symbol_line};
use crate::query::symbol::{resolve_filtered, list_module_symbols};
use std::fmt::Write;

/// Search for symbol definitions.
///
/// # Resolution (9-step cascade, returns on first hit)
///
/// 1. Exact FQN → 2. FQN suffix → 3. Owner.member (dotted, nested) →
/// 4. Exact name (O(1) hash) → 5. Prefix (<5 chars) → 6. Substring (trigram) →
/// 7. Substring (linear fallback) → 8. CamelCase (segment + char subsequence) →
/// 9. Fuzzy (Damerau-Levenshtein)
///
/// Step 8 uses two complementary matchers:
/// - **Segment matching**: splits query and candidate on CamelCase boundaries,
///   matches segments in order (abbreviation: `hcf` → `HttpClientFactory`,
///   subsequence: `UserService` → `UserProfileService`)
/// - **Character subsequence**: matches each query char greedily, skipping to
///   the next CamelCase boundary on mismatch (`lpfuse` → `linkProfileForUser`).
///   Handles all-lowercase abbreviations that segment matching misses.
///
/// # Ranking
///
/// Results are ranked by a composite score combining:
/// - **Kind**: class/trait > object > type > method > field > parameter
/// - **Source type**: source > test > generated
/// - **Popularity**: reference count (log-dampened to avoid utility symbols dominating)
/// - **Name length**: shorter names rank higher
///
/// For scored steps (CamelCase, fuzzy), match quality is primary, composite is tiebreaker.
pub fn cmd_search(
    index: &ArchivedKodexIndex,
    query: Option<&str>,
    limit: usize,
    kind_filter: Option<&str>,
    module_filter: Option<&str>,
    exclude: &[String],
    include_noise: bool,
) -> CommandResult {
    // Module-only mode: list all symbols in the module
    let mut candidates = if let Some(q) = query {
        resolve_filtered(index, q, kind_filter, module_filter)
    } else {
        // query is None — module_filter is guaranteed by caller
        let module_pattern = module_filter.expect("--module required when query is omitted");
        list_module_symbols(index, module_pattern, kind_filter)
    };

    if candidates.is_empty() {
        return not_found_message(index, query, kind_filter, module_filter);
    }

    // Baseline noise filter: exclude generated, test, stdlib, plumbing symbols
    if !include_noise {
        candidates.retain(|sym| !crate::query::filter::is_noise(index, sym));
        if candidates.is_empty() {
            return not_found_message(index, query, kind_filter, module_filter);
        }
    }

    if !exclude.is_empty() {
        candidates.retain(|sym| !crate::query::filter::matches_exclude(index, sym, exclude));
        if candidates.is_empty() {
            return not_found_message(index, query, kind_filter, module_filter);
        }
    }

    let mut out = String::new();
    if candidates.len() == 1 {
        write!(
            out,
            "{}",
            format_symbol_detail(index, candidates[0], false)
        )
        .unwrap();
    } else {
        let label = match (query, module_filter) {
            (Some(q), Some(m)) => format!("symbols matching '{q}' in module '{m}'"),
            (Some(q), None) => format!("symbols matching '{q}'"),
            (None, Some(m)) => format!("symbols in module '{m}'"),
            (None, None) => "symbols".to_string(),
        };
        writeln!(out, "{} {label}", candidates.len()).unwrap();
        let effective_limit = if limit == 0 { candidates.len() } else { limit };
        for s in candidates.iter().take(effective_limit) {
            writeln!(out, "{}", format_symbol_line(index, s)).unwrap();
        }
        if candidates.len() > effective_limit {
            writeln!(
                out,
                "... and {} more (use --limit 0 for all)",
                candidates.len() - effective_limit
            )
            .unwrap();
        }
    }
    CommandResult::Found(out)
}

/// Build an appropriate not-found message, with kind-aware suggestions when applicable.
fn not_found_message(
    index: &ArchivedKodexIndex,
    query: Option<&str>,
    kind_filter: Option<&str>,
    module_filter: Option<&str>,
) -> CommandResult {
    // Feature #5: kind-aware suggestions
    // When --kind was specified but yielded no results, check if the query matches
    // symbols of other kinds and suggest those.
    if let (Some(q), Some(kind)) = (query, kind_filter) {
        let without_kind = resolve_filtered(index, q, None, module_filter);
        // Filter noise out of suggestions too
        let suggestions: Vec<_> = without_kind
            .into_iter()
            .filter(|sym| !crate::query::filter::is_noise(index, sym))
            .collect();
        if !suggestions.is_empty() {
            let mut out = format!("Not found: No {kind} found matching '{q}'\n");
            out.push_str("Found under other kinds:\n");
            let show_limit = suggestions.len().min(10);
            for s in suggestions.iter().take(show_limit) {
                writeln!(out, "{}", format_symbol_line(index, s)).unwrap();
            }
            if suggestions.len() > show_limit {
                writeln!(out, "  ... and {} more", suggestions.len() - show_limit).unwrap();
            }
            return CommandResult::NotFound(out);
        }
    }

    // Module-only mode not-found
    if query.is_none() {
        if let Some(m) = module_filter {
            let msg = if let Some(kind) = kind_filter {
                format!("Not found: No {kind} symbols in module '{m}'\n")
            } else {
                format!("Not found: No symbols in module '{m}'\n")
            };
            return CommandResult::NotFound(msg);
        }
    }

    // Default: standard not-found with fuzzy suggestions
    CommandResult::symbol_not_found(index, query.unwrap_or(""))
}
