use super::CommandResult;
use crate::model::ArchivedKodexIndex;
use crate::query::format::{format_symbol_detail, format_symbol_line};
use crate::query::symbol::resolve_filtered;
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
    query: &str,
    limit: usize,
    kind_filter: Option<&str>,
    module_filter: Option<&str>,
    exclude: &[String],
) -> CommandResult {
    let mut candidates = resolve_filtered(index, query, kind_filter, module_filter);
    if candidates.is_empty() {
        return CommandResult::symbol_not_found(index, query);
    }

    if !exclude.is_empty() {
        candidates.retain(|sym| !crate::query::filter::matches_exclude(index, sym, exclude));
        if candidates.is_empty() {
            return CommandResult::symbol_not_found(index, query);
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
        writeln!(out, "{} symbols matching '{query}'", candidates.len()).unwrap();
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
