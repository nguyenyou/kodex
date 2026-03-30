use super::CommandResult;
use crate::model::{ArchivedKodexIndex, NONE_ID};
use crate::query::format::{format_file_location, module_tag, owner_name};
use crate::query::graph::filtered_neighbors;
use crate::query::symbol::{find_by_fqn, kind_str};
use crate::query::{file_entry, s, sym as sym_at};
use std::fmt::Write;

/// Call tree with info-level detail (signature + source) at each node.
/// Like `calls` but shows the full picture at every level.
pub fn cmd_trace(
    index: &ArchivedKodexIndex,
    fqn: &str,
    depth: usize,
    exclude: &[String],
    reverse: bool,
    cross_module_only: bool,
) -> CommandResult {
    let Some(sym) = find_by_fqn(index, fqn) else {
        return CommandResult::symbol_not_found(index, fqn);
    };

    let mut out = String::new();
    let sym_id: u32 = sym.id.into();
    let file_id: u32 = sym.file_id.into();
    let root_mod: u32 = file_entry(index, file_id).module_id.into();

    // Render root node
    render_node(&mut out, index, sym_id, root_mod, "", "");

    let mut visited = rustc_hash::FxHashSet::default();
    visited.insert(sym_id);

    let ctx = TraceCtx {
        index,
        root_mod,
        max_depth: depth,
        exclude,
        reverse,
        cross_module_only,
    };
    print_trace_tree(&mut out, &ctx, sym_id, 1, &mut visited);

    // Empty tree hint
    if visited.len() == 1 {
        let label = if cross_module_only {
            if reverse { "cross-module callers" } else { "cross-module callees" }
        } else if reverse {
            "callers"
        } else {
            "callees"
        };
        writeln!(out, "(no {label} found)").unwrap();
    }

    CommandResult::Found(out)
}

struct TraceCtx<'a> {
    index: &'a ArchivedKodexIndex,
    root_mod: u32,
    max_depth: usize,
    exclude: &'a [String],
    reverse: bool,
    cross_module_only: bool,
}

fn print_trace_tree(
    out: &mut String,
    ctx: &TraceCtx<'_>,
    sym_id: u32,
    depth: usize,
    visited: &mut rustc_hash::FxHashSet<u32>,
) {
    if depth > ctx.max_depth {
        return;
    }

    let edge_list = if ctx.reverse {
        &ctx.index.call_graph_reverse
    } else {
        &ctx.index.call_graph_forward
    };
    let mut filtered = filtered_neighbors(ctx.index, edge_list, sym_id, ctx.exclude);

    if ctx.cross_module_only {
        filtered.retain(|&cid| {
            let c = sym_at(ctx.index, cid);
            let cf_id: u32 = c.file_id.into();
            let neighbor_mod: u32 = file_entry(ctx.index, cf_id).module_id.into();
            neighbor_mod != ctx.root_mod && neighbor_mod != NONE_ID && ctx.root_mod != NONE_ID
        });
    }

    for (i, &cid) in filtered.iter().enumerate() {
        let is_last = i == filtered.len() - 1;
        let branch = if is_last { "└── " } else { "├── " };
        let cont = if is_last { "    " } else { "│   " };
        let prefix = "│   ".repeat(depth.saturating_sub(1));

        let tree_prefix = format!("{prefix}{branch}");
        let cont_prefix = format!("{prefix}{cont}");

        render_node(out, ctx.index, cid, ctx.root_mod, &tree_prefix, &cont_prefix);

        if visited.insert(cid) {
            print_trace_tree(out, ctx, cid, depth + 1, visited);
        }
    }
}

/// Render a single node with info-level detail.
/// `tree_prefix` is for the first line (e.g., "├── "), `cont_prefix` for continuation lines.
fn render_node(
    out: &mut String,
    index: &ArchivedKodexIndex,
    sym_id: u32,
    root_mod: u32,
    tree_prefix: &str,
    cont_prefix: &str,
) {
    let sym = sym_at(index, sym_id);
    let name = s(index, sym.name);
    let kind = kind_str(&sym.kind);
    let sig = s(index, sym.type_signature);
    let file_id: u32 = sym.file_id.into();
    let fe = file_entry(index, file_id);
    let mod_id: u32 = fe.module_id.into();
    let loc = format_file_location(index, sym);

    let on = owner_name(index, sym);
    let owner = if on.is_empty() { String::new() } else { format!("{on}.") };

    let cross = if mod_id != root_mod && mod_id != NONE_ID && root_mod != NONE_ID {
        format!("{} — cross-module", module_tag(index, mod_id))
    } else {
        module_tag(index, mod_id)
    };

    // Header line
    writeln!(out, "{tree_prefix}{kind} {owner}{name}{cross} — {loc}").unwrap();
    // FQN
    writeln!(out, "{cont_prefix}  fqn: {}", s(index, sym.fqn)).unwrap();
    // Signature
    if !sig.is_empty() {
        writeln!(out, "{cont_prefix}  sig: {sig}").unwrap();
    }
    // Source (max 10 lines)
    let file = s(index, fe.path);
    let abs_path = std::path::Path::new(index.workspace_root.as_str()).join(file);
    if let Ok(contents) = std::fs::read_to_string(&abs_path) {
        let lines: Vec<&str> = contents.lines().collect();
        let line: u32 = sym.line.into();
        let start = line as usize;
        let end_line: u32 = sym.end_line.into();
        let end = if end_line != NONE_ID && end_line > line {
            (end_line as usize + 1).min(lines.len()).min(start + 10)
        } else {
            (start + 10).min(lines.len())
        };
        if start < lines.len() {
            for (i, &ln) in lines[start..end].iter().enumerate() {
                writeln!(out, "{cont_prefix}  {:>4} | {}", start + i + 1, ln).unwrap();
            }
            let total = if end_line != NONE_ID && end_line > line {
                end_line as usize + 1 - start
            } else {
                0
            };
            if total > 10 {
                writeln!(out, "{cont_prefix}  ... ({total} lines total, showing first 10)").unwrap();
            }
        }
    }
    writeln!(out).unwrap();
}
