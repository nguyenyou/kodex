use super::{filter, s, sym as sym_at};
use crate::model::{ArchivedKodexIndex, ArchivedSymbol};
use crate::query::symbol::{edges_from, find_by_fqn};

/// Collect all callers of a symbol, including callers of overridden base methods.
/// Returns deduplicated caller symbol IDs, excluding the target itself.
pub fn trait_aware_callers(index: &ArchivedKodexIndex, sym: &ArchivedSymbol) -> Vec<u32> {
    let sym_id: u32 = sym.id.into();
    let mut caller_ids: Vec<u32> = edges_from(&index.call_graph_reverse, sym_id)
        .iter()
        .map(|v| u32::from(*v))
        .collect();
    for base_fqn_id in sym.overridden_symbols.iter() {
        let base_fqn = s(index, *base_fqn_id);
        if let Some(base_sym) = find_by_fqn(index, base_fqn) {
            let base_id: u32 = base_sym.id.into();
            caller_ids.extend(
                edges_from(&index.call_graph_reverse, base_id)
                    .iter()
                    .map(|v| u32::from(*v)),
            );
        }
    }
    caller_ids.sort_unstable();
    caller_ids.dedup();
    caller_ids.retain(|&cid| cid != sym_id);
    caller_ids
}

/// Return filtered callers (trait-aware) for a symbol.
/// Filters out callgraph noise and user-specified exclude patterns.
pub fn filtered_callers(
    index: &ArchivedKodexIndex,
    sym: &ArchivedSymbol,
    exclude: &[String],
) -> Vec<u32> {
    trait_aware_callers(index, sym)
        .into_iter()
        .filter(|&cid| {
            let caller = sym_at(index, cid);
            !filter::is_callgraph_noise(index, caller)
                && !filter::matches_exclude(index, caller, exclude)
        })
        .collect()
}

/// Return filtered callees for a symbol.
/// Filters out callgraph noise and user-specified exclude patterns.
pub fn filtered_callees(
    index: &ArchivedKodexIndex,
    sym_id: u32,
    exclude: &[String],
) -> Vec<u32> {
    edges_from(&index.call_graph_forward, sym_id)
        .iter()
        .map(|v| u32::from(*v))
        .filter(|&cid| {
            let callee = sym_at(index, cid);
            !filter::is_callgraph_noise(index, callee)
                && !filter::matches_exclude(index, callee, exclude)
        })
        .collect()
}

/// Return filtered neighbors (callers or callees) for a symbol from a given edge list.
/// Filters out callgraph noise and user-specified exclude patterns.
pub fn filtered_neighbors(
    index: &ArchivedKodexIndex,
    edge_list: &[crate::model::ArchivedEdgeList],
    sym_id: u32,
    exclude: &[String],
) -> Vec<u32> {
    edges_from(edge_list, sym_id)
        .iter()
        .map(|v| u32::from(*v))
        .filter(|&cid| {
            let sym = sym_at(index, cid);
            !filter::is_callgraph_noise(index, sym)
                && !filter::matches_exclude(index, sym, exclude)
        })
        .collect()
}
