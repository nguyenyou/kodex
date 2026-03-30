use super::CommandResult;
use crate::model::{ArchivedKodexIndex, ArchivedReferenceRole};
use crate::query::format::module_display_name;
use crate::query::s;
use crate::query::symbol::find_by_fqn;
use crate::query::file_entry;
use std::collections::BTreeMap;
use std::fmt::Write;

/// Show all reference locations for a symbol, grouped by module and file.
/// Requires a fully-qualified name (FQN) from search results.
pub fn cmd_refs(
    index: &ArchivedKodexIndex,
    fqn: &str,
    limit: usize,
) -> CommandResult {
    let Some(sym) = find_by_fqn(index, fqn) else {
        return CommandResult::symbol_not_found(index, fqn);
    };

    let name = s(index, sym.name);
    let sym_id: u32 = sym.id.into();

    // Find the ReferenceList for this symbol (linear scan — not sorted by symbol_id).
    let refs_opt = index
        .references
        .iter()
        .find(|rl| u32::from(rl.symbol_id) == sym_id);

    let Some(ref_list) = refs_opt else {
        return CommandResult::Found(format!("{name} — 0 references\n"));
    };

    // Collect refs grouped by module_id → file_id → [lines].
    // Only include Reference role (skip Definition — the user already knows where it's defined).
    // BTreeMap for deterministic sorted output.
    let mut by_module: BTreeMap<u32, BTreeMap<u32, Vec<u32>>> = BTreeMap::new();

    for r in ref_list.refs.iter() {
        if !matches!(r.role, ArchivedReferenceRole::Reference) {
            continue;
        }

        let fid: u32 = r.file_id.into();
        if fid as usize >= index.files.len() {
            continue;
        }

        let fe = file_entry(index, fid);
        let mid: u32 = fe.module_id.into();

        let line: u32 = r.line.into();
        by_module
            .entry(mid)
            .or_default()
            .entry(fid)
            .or_default()
            .push(line + 1); // 1-based display
    }

    // Dedup lines per file before counting totals, so header matches displayed output.
    for files in by_module.values_mut() {
        for lines in files.values_mut() {
            lines.sort_unstable();
            lines.dedup();
        }
    }

    let total_refs: usize = by_module.values().flat_map(|files| files.values()).map(|lines| lines.len()).sum();
    let total_files: usize = by_module.values().map(|files| files.len()).sum();
    let total_modules = by_module.len();

    if total_refs == 0 {
        return CommandResult::Found(format!("{name} — 0 references\n"));
    }

    let mut out = String::new();

    // Header
    writeln!(out, "{name} — {total_refs} references across {total_files} files, {total_modules} modules").unwrap();

    // By module summary
    writeln!(out).unwrap();
    writeln!(out, "By module:").unwrap();
    for (&mid, files) in &by_module {
        let mod_name = module_display_name(index, mid);
        let mod_label = if mod_name.is_empty() { "(unknown)" } else { mod_name };
        let ref_count: usize = files.values().map(|lines| lines.len()).sum();
        let file_count = files.len();
        writeln!(out, "  {mod_label:<40} {ref_count} refs in {file_count} files").unwrap();
    }

    // Locations grouped by module, then file (capped by limit)
    let effective_limit = if limit == 0 { usize::MAX } else { limit };
    let mut shown = 0usize;
    writeln!(out).unwrap();
    writeln!(out, "Locations:").unwrap();
    'outer: for (&mid, files) in &by_module {
        let mod_name = module_display_name(index, mid);
        let mod_label = if mod_name.is_empty() { "(unknown)" } else { mod_name };
        writeln!(out, "  [{mod_label}]").unwrap();
        for (&fid, lines) in files {
            if shown >= effective_limit {
                break 'outer;
            }
            let file_path = s(index, file_entry(index, fid).path);
            let line_strs: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
            writeln!(out, "    {file_path}:{}", line_strs.join(",")).unwrap();
            shown += 1;
        }
    }
    if shown < total_files {
        writeln!(out, "  ... and {} more files (use --limit 0 for all)", total_files - shown).unwrap();
    }

    CommandResult::Found(out)
}
