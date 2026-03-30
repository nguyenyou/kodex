use super::CommandResult;
use crate::model::ArchivedKodexIndex;
use crate::query::format::module_display_name;
use std::fmt::Write;

/// Codebase overview: total stats and module list sorted by symbol count.
pub fn cmd_overview(index: &ArchivedKodexIndex) -> CommandResult {
    let mut out = String::new();

    let total_modules = index.modules.len();
    let total_symbols = index.symbols.len();
    let total_files = index.files.len();

    writeln!(out, "{total_modules} modules, {total_symbols} symbols, {total_files} files").unwrap();

    if total_modules == 0 {
        return CommandResult::Found(out);
    }

    // Collect modules with their display names and sort by symbol count (descending).
    let mut modules: Vec<(usize, &str, u32, u32)> = (0..total_modules)
        .map(|i| {
            let name = module_display_name(index, i as u32);
            let sym_count: u32 = index.modules[i].symbol_count.into();
            let file_count: u32 = index.modules[i].file_count.into();
            (i, name, sym_count, file_count)
        })
        .collect();
    modules.sort_by(|a, b| b.2.cmp(&a.2));

    writeln!(out).unwrap();
    writeln!(out, "Modules:").unwrap();
    for (_, name, sym_count, file_count) in &modules {
        let label = if name.is_empty() { "(unknown)" } else { name };
        writeln!(out, "  {label:<50} {sym_count:>6} symbols  {file_count:>4} files").unwrap();
    }

    CommandResult::Found(out)
}
