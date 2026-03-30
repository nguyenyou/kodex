use super::CommandResult;
use crate::model::ArchivedKodexIndex;
use crate::query::format::{format_symbol_detail, format_symbol_line};
use crate::query::symbol::{resolve_filtered, list_module_symbols};
use std::fmt::Write;

/// Search for symbol definitions.
///
/// # Modes
///
/// - **Query mode** (`query` is `Some`): resolves symbols via a 9-step cascade
///   (exact FQN → suffix → owner.member → exact name → prefix → substring →
///   CamelCase → fuzzy), then applies kind/module/noise/exclude filters.
/// - **Module-only mode** (`query` is `None`, `module_filter` is `Some`): lists
///   all symbols in the matching module, filtered by kind/noise/exclude.
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
        return not_found_message(index, query, kind_filter, module_filter, exclude, include_noise);
    }

    // Baseline noise filter: exclude generated, test, stdlib, plumbing symbols
    if !include_noise {
        candidates.retain(|sym| !crate::query::filter::is_noise(index, sym));
        if candidates.is_empty() {
            return not_found_message(index, query, kind_filter, module_filter, exclude, include_noise);
        }
    }

    if !exclude.is_empty() {
        candidates.retain(|sym| !crate::query::filter::matches_exclude(index, sym, exclude));
        if candidates.is_empty() {
            return not_found_message(index, query, kind_filter, module_filter, exclude, include_noise);
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
///
/// Propagates `exclude` and `include_noise` so suggestions respect the same filters
/// as the primary search.
fn not_found_message(
    index: &ArchivedKodexIndex,
    query: Option<&str>,
    kind_filter: Option<&str>,
    module_filter: Option<&str>,
    exclude: &[String],
    include_noise: bool,
) -> CommandResult {
    // Feature #5: kind-aware suggestions
    // When --kind was specified but yielded no results, check if the query matches
    // symbols of other kinds and suggest those.
    if let (Some(q), Some(kind)) = (query, kind_filter) {
        // Use resolve_symbols directly to avoid spurious stderr warning from resolve_filtered
        // when module_filter matches nothing in the re-query path.
        let mut without_kind = crate::query::symbol::resolve_symbols(index, q);
        if let Some(mp) = module_filter {
            without_kind = crate::query::filter::filter_by_module(index, &without_kind, mp);
        }
        // Apply same noise/exclude filters as the primary search
        if !include_noise {
            without_kind.retain(|sym| !crate::query::filter::is_noise(index, sym));
        }
        if !exclude.is_empty() {
            without_kind.retain(|sym| !crate::query::filter::matches_exclude(index, sym, exclude));
        }
        if !without_kind.is_empty() {
            let mut out = String::new();
            writeln!(out, "Not found: No {kind} found matching '{q}'").unwrap();
            writeln!(out, "Found under other kinds:").unwrap();
            let show_limit = without_kind.len().min(10);
            for s in without_kind.iter().take(show_limit) {
                writeln!(out, "{}", format_symbol_line(index, s)).unwrap();
            }
            if without_kind.len() > show_limit {
                writeln!(out, "  ... and {} more", without_kind.len() - show_limit).unwrap();
            }
            return CommandResult::NotFound(out);
        }
    }

    // Module-only mode not-found
    if query.is_none() {
        if let Some(m) = module_filter {
            let mut out = String::new();
            if let Some(kind) = kind_filter {
                writeln!(out, "Not found: No {kind} symbols in module '{m}'").unwrap();
            } else {
                writeln!(out, "Not found: No symbols in module '{m}'").unwrap();
            }
            return CommandResult::NotFound(out);
        }
    }

    // Default: standard not-found with fuzzy suggestions
    CommandResult::symbol_not_found(index, query.unwrap_or(""))
}
