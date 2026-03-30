use super::CommandResult;
use crate::model::{ArchivedKodexIndex, NONE_ID};
use crate::query::format::{module_tag, owner_name};
use crate::query::graph::filtered_neighbors;
use crate::query::{file_entry, s, sym as sym_at};
use crate::query::symbol::{edges_from, find_by_fqn};
use std::fmt::Write;

/// Which direction to walk the call graph.
#[derive(Clone, Copy)]
enum Direction {
    /// Follow callees (downstream).
    Forward,
    /// Follow callers (upstream).
    Reverse,
}

/// Call graph with module boundary annotations.
/// Requires a fully-qualified name (FQN) from search results.
/// `reverse = false` → downstream (callees), `reverse = true` → upstream (callers).
/// `cross_module_only = true` → only show edges that cross module boundaries.
pub fn cmd_calls(
    index: &ArchivedKodexIndex,
    fqn: &str,
    depth: usize,
    exclude: &[String],
    reverse: bool,
    cross_module_only: bool,
) -> CommandResult {
    let direction = if reverse {
        Direction::Reverse
    } else {
        Direction::Forward
    };
    cmd_tree(index, fqn, depth, exclude, direction, cross_module_only)
}

fn cmd_tree(
    index: &ArchivedKodexIndex,
    fqn: &str,
    depth: usize,
    exclude: &[String],
    direction: Direction,
    cross_module_only: bool,
) -> CommandResult {
    let Some(sym) = find_by_fqn(index, fqn) else {
        return CommandResult::symbol_not_found(index, fqn);
    };

    let mut out = String::new();
    let name = s(index, sym.name);
    let sym_id: u32 = sym.id.into();
    let file_id: u32 = sym.file_id.into();
    let root_mod: u32 = file_entry(index, file_id).module_id.into();

    writeln!(out, "{name}{}", module_tag(index, root_mod)).unwrap();
    let mut visited = rustc_hash::FxHashSet::default();
    visited.insert(sym_id);

    let ctx = TreeCtx {
        index,
        root_mod,
        max_depth: depth,
        exclude,
        direction,
        cross_module_only,
    };
    print_tree(&mut out, &ctx, sym_id, 1, &mut visited);

    // If the tree is empty (root only, no children), add a hint
    if visited.len() == 1 {
        let label = if cross_module_only {
            match direction {
                Direction::Forward => "cross-module callees",
                Direction::Reverse => "cross-module callers",
            }
        } else {
            match direction {
                Direction::Forward => "callees",
                Direction::Reverse => "callers",
            }
        };
        let fqn = s(index, sym.fqn);
        let file = s(index, file_entry(index, file_id).path);
        writeln!(out, "(no {label} found for {fqn})").unwrap();
        writeln!(out, "  resolved to: {file}").unwrap();
        // Check if other symbols with the same name exist in different modules
        let edge_list = match direction {
            Direction::Forward => &index.call_graph_forward,
            Direction::Reverse => &index.call_graph_reverse,
        };
        let others: Vec<_> = crate::query::symbol::resolve_filtered(index, name, None, None)
            .into_iter()
            .filter(|s| {
                let sid: u32 = s.id.into();
                sid != sym_id && !edges_from(edge_list, sid).is_empty()
            })
            .collect();
        if !others.is_empty() {
            writeln!(out, "  other variants with {label}:").unwrap();
            for other in others.iter().take(5) {
                let of_id: u32 = other.file_id.into();
                let of = s(index, file_entry(index, of_id).path);
                writeln!(out, "    {} — {of}", s(index, other.fqn)).unwrap();
            }
        }
    }

    CommandResult::Found(out)
}

struct TreeCtx<'a> {
    index: &'a ArchivedKodexIndex,
    root_mod: u32,
    max_depth: usize,
    exclude: &'a [String],
    direction: Direction,
    cross_module_only: bool,
}

fn print_tree(
    out: &mut String,
    ctx: &TreeCtx<'_>,
    sym_id: u32,
    indent: usize,
    visited: &mut rustc_hash::FxHashSet<u32>,
) {
    if indent > ctx.max_depth {
        return;
    }

    let edge_list = match ctx.direction {
        Direction::Forward => &ctx.index.call_graph_forward,
        Direction::Reverse => &ctx.index.call_graph_reverse,
    };
    let mut filtered = filtered_neighbors(ctx.index, edge_list, sym_id, ctx.exclude);

    // When --cross-module-only, keep only neighbors in a different module than the
    // current node (not just the root). This ensures edges between two non-root modules
    // are shown if they cross a boundary.
    if ctx.cross_module_only {
        let parent_file_id: u32 = sym_at(ctx.index, sym_id).file_id.into();
        let parent_mod: u32 = file_entry(ctx.index, parent_file_id).module_id.into();
        filtered.retain(|&cid| {
            let c = sym_at(ctx.index, cid);
            let cf_id: u32 = c.file_id.into();
            let neighbor_mod: u32 = file_entry(ctx.index, cf_id).module_id.into();
            neighbor_mod != parent_mod && neighbor_mod != NONE_ID && parent_mod != NONE_ID
        });
    }

    for (i, &cid) in filtered.iter().enumerate() {
        let c = sym_at(ctx.index, cid);
        let cn = s(ctx.index, c.name);
        let cf_id: u32 = c.file_id.into();
        let neighbor_mod: u32 = file_entry(ctx.index, cf_id).module_id.into();

        let cross =
            if neighbor_mod != ctx.root_mod && neighbor_mod != NONE_ID && ctx.root_mod != NONE_ID {
                format!("{} — cross-module", module_tag(ctx.index, neighbor_mod))
            } else {
                module_tag(ctx.index, neighbor_mod)
            };

        let on = owner_name(ctx.index, c);
        let owner = if on.is_empty() {
            String::new()
        } else {
            format!("{on}.")
        };

        let is_last = i == filtered.len() - 1;
        let prefix = "│   ".repeat(indent.saturating_sub(1));
        let branch = if is_last { "└── " } else { "├── " };
        writeln!(out, "{prefix}{branch}{owner}{cn}{cross}").unwrap();

        if visited.insert(cid) {
            print_tree(out, ctx, cid, indent + 1, visited);
        }
    }
}
