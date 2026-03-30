use super::CommandResult;
use crate::model::{ArchivedKodexIndex, ArchivedSymbolKind, NONE_ID};
use crate::query::filter;
use crate::query::format::{
    count_refs, format_access, format_file_location, format_properties_plain, module_tag, owner_name,
};
use crate::query::graph::{filtered_callers, filtered_callees};
use crate::query::s;
use crate::query::symbol::{edges_from, find_by_fqn, kind_str};
use crate::query::{file_entry, sym as sym_at};
use std::fmt::Write;

/// Composite command: complete picture of a type or method in one call.
/// Requires a fully-qualified name (FQN) from search results.
pub fn cmd_info(
    index: &ArchivedKodexIndex,
    fqn: &str,
    exclude: &[String],
) -> CommandResult {
    let Some(sym) = find_by_fqn(index, fqn) else {
        return CommandResult::symbol_not_found(index, fqn);
    };

    let mut out = String::new();
    let name = s(index, sym.name);
    let kind = kind_str(&sym.kind);
    let sig: &str = s(index, sym.type_signature);
    let file_id: u32 = sym.file_id.into();
    let fe = file_entry(index, file_id);
    let file = s(index, fe.path);
    let line: u32 = sym.line.into();
    let end_line: u32 = sym.end_line.into();
    let sym_id: u32 = sym.id.into();
    let mod_id: u32 = fe.module_id.into();
    let props: u32 = sym.properties.into();

    // ── Header ─────────────────────────────────────────────────────────────
    let mod_part = module_tag(index, mod_id);
    let loc = format_file_location(index, sym);
    writeln!(out, "{kind} {name}{mod_part} — {loc}").unwrap();

    // ── FQN ────────────────────────────────────────────────────────────────
    let fqn_str = s(index, sym.fqn);
    writeln!(out, "  fqn: {fqn_str}").unwrap();

    // ── Reference stats ────────────────────────────────────────────────────
    let (ref_count, ref_module_count) = count_refs(index, sym_id);
    if ref_count > 0 {
        writeln!(out, "  referenced: {ref_count} sites across {ref_module_count} modules").unwrap();
    }

    // ── Flags: test / generated ────────────────────────────────────────────
    let is_test: bool = fe.is_test.into();
    let is_generated: bool = fe.is_generated.into();
    if is_test || is_generated {
        let mut flags = Vec::new();
        if is_test {
            flags.push("test");
        }
        if is_generated {
            flags.push("generated");
        }
        writeln!(out, "  source: {}", flags.join(", ")).unwrap();
    }

    // ── Access ─────────────────────────────────────────────────────────────
    let access_str = format_access(&sym.access);
    if !access_str.is_empty() {
        writeln!(out, "  access: {access_str}").unwrap();
    }

    // ── Properties ─────────────────────────────────────────────────────────
    let props_str = format_properties_plain(props);
    if !props_str.is_empty() {
        writeln!(out, "  properties: {props_str}").unwrap();
    }

    writeln!(out).unwrap();

    // ── Signature ──────────────────────────────────────────────────────────
    if !sig.is_empty() {
        writeln!(out, "  Signature: {sig}").unwrap();
        writeln!(out).unwrap();
    }

    // ── Owner ──────────────────────────────────────────────────────────────
    let owner_id: u32 = sym.owner.into();
    if owner_id != NONE_ID && (owner_id as usize) < index.symbols.len() {
        let owner = sym_at(index, owner_id);
        let ok = kind_str(&owner.kind);
        let on = s(index, owner.name);
        let ofqn = s(index, owner.fqn);
        writeln!(out, "  Owner: {ok} {on}").unwrap();
        writeln!(out, "    fqn: {ofqn}").unwrap();
        writeln!(out).unwrap();
    }

    // ── Overrides (what this symbol overrides) ─────────────────────────────
    if !sym.overridden_symbols.is_empty() {
        let overrides: Vec<(&str, &str)> = sym
            .overridden_symbols
            .iter()
            .map(|oid| {
                let ofqn = s(index, *oid);
                let oname = find_by_fqn(index, ofqn).map_or_else(
                    || crate::symbol::symbol_display_name(ofqn),
                    |os| s(index, os.name),
                );
                (oname, ofqn)
            })
            .collect();
        writeln!(out, "  Overrides ({}):", overrides.len()).unwrap();
        for (oname, ofqn) in &overrides {
            writeln!(out, "    {oname}").unwrap();
            writeln!(out, "      fqn: {ofqn}").unwrap();
        }
        writeln!(out).unwrap();
    }

    // ── Overridden by (who overrides this symbol) ──────────────────────────
    let overriders = edges_from(&index.overrides, sym_id);
    if !overriders.is_empty() {
        writeln!(out, "  Overridden by ({}):", overriders.len()).unwrap();
        for oid in overriders {
            let o = sym_at(index, u32::from(*oid));
            let ok = kind_str(&o.kind);
            let on = s(index, o.name);
            let ofqn = s(index, o.fqn);
            let of_id: u32 = o.file_id.into();
            let of = s(index, file_entry(index, of_id).path);
            writeln!(out, "    {ok} {on} — {of}").unwrap();
            writeln!(out, "      fqn: {ofqn}").unwrap();
        }
        writeln!(out).unwrap();
    }

    // ── Parents / Extends ──────────────────────────────────────────────────
    if !sym.parents.is_empty() {
        let parents: Vec<(&str, &str)> = sym
            .parents
            .iter()
            .filter_map(|pid| {
                let pfqn = s(index, *pid);
                if matches!(
                    pfqn,
                    "java/lang/Object#"
                        | "scala/Product#"
                        | "scala/Serializable#"
                        | "java/io/Serializable#"
                        | "scala/AnyRef#"
                        | "scala/Any#"
                        | "scala/Equals#"
                ) {
                    return None;
                }
                let name = find_by_fqn(index, pfqn).map_or_else(
                    || crate::symbol::symbol_display_name(pfqn),
                    |ps| s(index, ps.name),
                );
                Some((name, pfqn))
            })
            .collect();
        if !parents.is_empty() {
            let names: Vec<&str> = parents.iter().map(|(name, _)| *name).collect();
            writeln!(out, "  Extends: {}", names.join(", ")).unwrap();
            for (_, pfqn) in &parents {
                writeln!(out, "    fqn: {pfqn}").unwrap();
            }
            writeln!(out).unwrap();
        }
    }

    // ── Members (for classes/traits/objects) ────────────────────────────────
    if matches!(
        sym.kind,
        ArchivedSymbolKind::Class
            | ArchivedSymbolKind::Trait
            | ArchivedSymbolKind::Object
            | ArchivedSymbolKind::Interface
    ) {
        let members = edges_from(&index.members, sym_id);
        let mut filtered: Vec<u32> = members
            .iter()
            .map(|v| u32::from(*v))
            .filter(|&mid| {
                let m = sym_at(index, mid);
                !matches!(
                    m.kind,
                    ArchivedSymbolKind::Parameter
                        | ArchivedSymbolKind::TypeParameter
                        | ArchivedSymbolKind::SelfParameter
                        | ArchivedSymbolKind::Constructor
                ) && !filter::is_synthetic_name(s(index, m.name))
            })
            .collect();
        // Sort: types first, then methods, then vals/fields (DI injections sink to bottom)
        filtered.sort_by_key(|&mid| {
            let m = sym_at(index, mid);
            let mprops: u32 = m.properties.into();
            let is_val =
                mprops & crate::model::PROP_VAL != 0 || mprops & crate::model::PROP_VAR != 0;
            match m.kind {
                ArchivedSymbolKind::Class
                | ArchivedSymbolKind::Trait
                | ArchivedSymbolKind::Object
                | ArchivedSymbolKind::Interface => 0u8,
                ArchivedSymbolKind::Type => 1,
                ArchivedSymbolKind::Method if !is_val => 2,
                ArchivedSymbolKind::Method => 3, // val methods (DI fields)
                ArchivedSymbolKind::Field => 3,
                _ => 4,
            }
        });
        if !filtered.is_empty() {
            writeln!(out, "  Members ({}):", filtered.len()).unwrap();
            for &mid in &filtered {
                let m = sym_at(index, mid);
                let mk = kind_str(&m.kind);
                let mn = s(index, m.name);
                let mfqn = s(index, m.fqn);
                let msig = s(index, m.type_signature);
                if msig.is_empty() {
                    writeln!(out, "    {mk} {mn}").unwrap();
                } else {
                    writeln!(out, "    {msig}").unwrap();
                }
                writeln!(out, "      fqn: {mfqn}").unwrap();
            }
            writeln!(out).unwrap();
        }
    }

    // ── Implementations (for traits/classes) ───────────────────────────────
    if matches!(
        sym.kind,
        ArchivedSymbolKind::Trait | ArchivedSymbolKind::Class | ArchivedSymbolKind::Interface
    ) {
        let subtypes = edges_from(&index.inheritance_forward, sym_id);
        let filtered: Vec<u32> = subtypes
            .iter()
            .map(|v| u32::from(*v))
            .filter(|&sid| {
                let st = sym_at(index, sid);
                !matches!(
                    st.kind,
                    ArchivedSymbolKind::Local | ArchivedSymbolKind::Parameter
                ) && !filter::is_noise(index, st)
            })
            .collect();
        if !filtered.is_empty() {
            writeln!(out, "  Implementations ({}):", filtered.len()).unwrap();
            for &sid in &filtered {
                let st = sym_at(index, sid);
                let sk = kind_str(&st.kind);
                let sn = s(index, st.name);
                let sfqn = s(index, st.fqn);
                let sf_id: u32 = st.file_id.into();
                let sf = s(index, file_entry(index, sf_id).path);
                writeln!(out, "    {sk} {sn} — {sf}").unwrap();
                writeln!(out, "      fqn: {sfqn}").unwrap();
            }
            writeln!(out).unwrap();
        }
    }

    // ── Call graph (depth 1, capped) for methods/fields ─────────────────────
    const CALL_GRAPH_CAP: usize = 50;
    if matches!(
        sym.kind,
        ArchivedSymbolKind::Method | ArchivedSymbolKind::Constructor | ArchivedSymbolKind::Field
    ) {
        let callers = filtered_callers(index, sym, exclude);
        let callees = filtered_callees(index, sym_id, exclude);

        if !callers.is_empty() || !callees.is_empty() {
            writeln!(out, "  Call graph (depth 1):").unwrap();
        }

        if !callers.is_empty() {
            writeln!(out).unwrap();
            writeln!(out, "    Callers — who calls this ({}):", callers.len()).unwrap();
            for &cid in callers.iter().take(CALL_GRAPH_CAP) {
                let c = sym_at(index, cid);
                let cn = s(index, c.name);
                let cfqn = s(index, c.fqn);
                let cf_id: u32 = c.file_id.into();
                let cf = s(index, file_entry(index, cf_id).path);
                let cmod_id: u32 = file_entry(index, cf_id).module_id.into();
                writeln!(out, "      {cn}{} — {cf}", module_tag(index, cmod_id)).unwrap();
                writeln!(out, "        fqn: {cfqn}").unwrap();
            }
            if callers.len() > CALL_GRAPH_CAP {
                writeln!(out, "      ... and {} more (use `calls '{fqn_str}' -r` for full list)", callers.len() - CALL_GRAPH_CAP).unwrap();
            }
        }

        if !callees.is_empty() {
            writeln!(out).unwrap();
            writeln!(out, "    Callees — what this calls ({}):", callees.len()).unwrap();
            let caller_mod: u32 = fe.module_id.into();
            for (i, &cid) in callees.iter().take(CALL_GRAPH_CAP).enumerate() {
                let c = sym_at(index, cid);
                let cn = s(index, c.name);
                let cf_id: u32 = c.file_id.into();
                let callee_mod: u32 = file_entry(index, cf_id).module_id.into();
                let cross =
                    if callee_mod != caller_mod && callee_mod != NONE_ID && caller_mod != NONE_ID {
                        module_tag(index, callee_mod)
                    } else {
                        String::new()
                    };
                let on = owner_name(index, c);
                let cfqn = s(index, c.fqn);
                if on.is_empty() {
                    writeln!(out, "      {}. {cn}{cross}", i + 1).unwrap();
                } else {
                    writeln!(out, "      {}. {on}.{cn}{cross}", i + 1).unwrap();
                }
                writeln!(out, "         fqn: {cfqn}").unwrap();
            }
            if callees.len() > CALL_GRAPH_CAP {
                writeln!(out, "      ... and {} more (use `calls '{fqn_str}'` for full list)", callees.len() - CALL_GRAPH_CAP).unwrap();
            }
        }
    }

    // ── Source body ──────────────────────────────────────────────────────────
    let abs_path = std::path::Path::new(index.workspace_root.as_str()).join(file);
    if let Ok(contents) = std::fs::read_to_string(&abs_path) {
        let lines: Vec<&str> = contents.lines().collect();
        let start = line as usize; // 0-based from SemanticDB
        let end = if end_line != NONE_ID && end_line > line {
            (end_line as usize + 1).min(lines.len()) // end_line is inclusive
        } else {
            (start + 30).min(lines.len())
        };
        if start < lines.len() {
            writeln!(out).unwrap();
            writeln!(out, "  Source:").unwrap();
            for (i, &ln) in lines[start..end].iter().enumerate() {
                writeln!(out, "    {:>4} | {}", start + i + 1, ln).unwrap();
            }
            if end < lines.len() && (end_line == NONE_ID || end_line <= line) {
                writeln!(out, "    ... ({} total lines in file)", lines.len()).unwrap();
            }
        }
    }

    CommandResult::Found(out)
}
