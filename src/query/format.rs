use super::{file_entry, s, sym as sym_at};
use crate::model::{
    ArchivedAccess, ArchivedKodexIndex, ArchivedReferenceRole, ArchivedSymbol, NONE_ID,
};
use crate::query::symbol::display_kind;
use rustc_hash::FxHashSet;
use std::fmt::Write;

/// Format a symbol as a one-line summary.
pub fn format_symbol_line(index: &ArchivedKodexIndex, sym: &ArchivedSymbol) -> String {
    let kind = display_kind(sym);
    let name = s(index, sym.name);
    let fqn = s(index, sym.fqn);
    let props = format_properties(sym.properties.into());
    let file = s(index, file_entry(index, sym.file_id).path);
    let loc = format_location(sym);
    format!("  {kind} {name}{props} ({file}{loc})\n    fqn: {fqn}")
}

/// Format a symbol with full detail (multi-line).
pub fn format_symbol_detail(
    index: &ArchivedKodexIndex,
    sym: &ArchivedSymbol,
    verbose: bool,
) -> String {
    let kind = display_kind(sym);
    let name = s(index, sym.name);
    let fqn = s(index, sym.fqn);
    let file = s(index, file_entry(index, sym.file_id).path);
    let sig = s(index, sym.type_signature);
    let props = format_properties(sym.properties.into());
    let loc = format_location(sym);

    let mut out = String::new();
    writeln!(out, "{kind} {name}{props} — {file}{loc}").unwrap();
    writeln!(out, "  fqn: {fqn}").unwrap();
    if !sig.is_empty() {
        writeln!(out, "  signature: {sig}").unwrap();
    }
    if !sym.parents.is_empty() {
        let parent_fqns: Vec<&str> = sym.parents.iter().map(|pid| s(index, *pid)).collect();
        writeln!(out, "  parents: {}", parent_fqns.join(", ")).unwrap();
    }
    if verbose {
        let owner: u32 = sym.owner.into();
        if owner != NONE_ID {
            let owner_fqn = s(index, sym_at(index, owner).fqn);
            writeln!(out, "  owner: {owner_fqn}").unwrap();
        }
    }
    out
}

/// Property bitmask → display name mapping. Single source of truth for both formatters.
const PROPERTY_FLAGS: &[(u32, &str)] = &[
    (crate::model::PROP_ABSTRACT, "abstract"),
    (crate::model::PROP_FINAL, "final"),
    (crate::model::PROP_SEALED, "sealed"),
    (crate::model::PROP_IMPLICIT, "implicit"),
    (crate::model::PROP_LAZY, "lazy"),
    (crate::model::PROP_CASE, "case"),
    (crate::model::PROP_VAL, "val"),
    (crate::model::PROP_VAR, "var"),
    (crate::model::PROP_STATIC, "static"),
    (crate::model::PROP_PRIMARY, "primary"),
    (crate::model::PROP_ENUM, "enum"),
    (crate::model::PROP_DEFAULT, "default"),
    (crate::model::PROP_GIVEN, "given"),
    (crate::model::PROP_INLINE, "inline"),
    (crate::model::PROP_OPEN, "open"),
    (crate::model::PROP_TRANSPARENT, "transparent"),
    (crate::model::PROP_INFIX, "infix"),
    (crate::model::PROP_OPAQUE, "opaque"),
    (crate::model::PROP_OVERRIDE, "override"),
];

/// Format properties bitmask as ` [abstract, sealed, ...]` for one-line display,
/// or empty string if no flags set.
pub fn format_properties(props: u32) -> String {
    if props == 0 {
        return String::new();
    }
    let mut out = String::from(" [");
    let mut first = true;
    for &(mask, name) in PROPERTY_FLAGS {
        if props & mask != 0 {
            if !first {
                out.push_str(", ");
            }
            out.push_str(name);
            first = false;
        }
    }
    if first {
        return String::new();
    } // no flags matched
    out.push(']');
    out
}

/// Format properties bitmask as a plain comma-separated string (no brackets).
/// Returns empty string if no flags set.
pub fn format_properties_plain(props: u32) -> String {
    let parts: Vec<&str> = PROPERTY_FLAGS
        .iter()
        .filter(|(mask, _)| props & mask != 0)
        .map(|(_, name)| *name)
        .collect();
    parts.join(", ")
}

/// Format `:line` or `:line-end_line` location suffix.
pub fn format_location(sym: &ArchivedSymbol) -> String {
    let line: u32 = sym.line.into();
    let end_line: u32 = sym.end_line.into();
    // Storage is 0-based (from SemanticDB); display is 1-based.
    let display_line = line + 1;
    if end_line != NONE_ID && end_line > line {
        format!(":{display_line}-{}", end_line + 1)
    } else {
        format!(":{display_line}")
    }
}

/// Format `file:line` or `file:line-end_line`.
pub fn format_file_location(index: &ArchivedKodexIndex, sym: &ArchivedSymbol) -> String {
    let file = s(index, file_entry(index, sym.file_id).path);
    format!("{file}{}", format_location(sym))
}

/// Format a module tag like ` [module-name]`, or empty if no module.
pub fn module_tag(index: &ArchivedKodexIndex, mod_id: u32) -> String {
    let dn = module_display_name(index, mod_id);
    if dn.is_empty() {
        String::new()
    } else {
        format!(" [{dn}]")
    }
}

/// Get a display name for a module: artifact_name if available, otherwise segment path.
pub fn module_display_name(index: &ArchivedKodexIndex, module_id: u32) -> &str {
    if module_id == NONE_ID || module_id as usize >= index.modules.len() {
        return "";
    }
    let m = &index.modules[module_id as usize];
    let artifact = s(index, m.artifact_name);
    if artifact.is_empty() {
        s(index, m.name)
    } else {
        artifact
    }
}

/// Get the display name of a symbol's owner (e.g., "OrderService"), or empty string.
pub fn owner_name<'a>(index: &'a ArchivedKodexIndex, sym: &ArchivedSymbol) -> &'a str {
    let owner_id: u32 = sym.owner.into();
    if owner_id != NONE_ID && (owner_id as usize) < index.symbols.len() {
        s(index, sym_at(index, owner_id).name)
    } else {
        ""
    }
}

/// Format access level as a display string. Returns empty for public (the default).
pub fn format_access(access: &ArchivedAccess) -> &'static str {
    match *access {
        ArchivedAccess::Public => "",
        ArchivedAccess::Private => "private",
        ArchivedAccess::PrivateThis => "private[this]",
        ArchivedAccess::PrivateWithin => "private[scope]",
        ArchivedAccess::Protected => "protected",
        ArchivedAccess::ProtectedThis => "protected[this]",
        ArchivedAccess::ProtectedWithin => "protected[scope]",
    }
}

/// Count references and distinct modules for a symbol.
/// Returns `(total_ref_count, distinct_module_count)`.
pub fn count_refs(index: &ArchivedKodexIndex, sym_id: u32) -> (usize, usize) {
    for rl in index.references.iter() {
        let sid: u32 = rl.symbol_id.into();
        if sid == sym_id {
            let count = rl.refs.len();
            let mut modules = FxHashSet::default();
            for r in rl.refs.iter() {
                if matches!(r.role, ArchivedReferenceRole::Reference) {
                    let fid: u32 = r.file_id.into();
                    if (fid as usize) < index.files.len() {
                        let mid: u32 = file_entry(index, fid).module_id.into();
                        if mid != NONE_ID {
                            modules.insert(mid);
                        }
                    }
                }
            }
            return (count, modules.len());
        }
    }
    (0, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_properties_empty() {
        assert_eq!(format_properties(0), "");
    }

    #[test]
    fn test_format_properties_abstract() {
        assert_eq!(format_properties(0x4), " [abstract]");
    }

    #[test]
    fn test_format_properties_multiple() {
        assert_eq!(format_properties(0x4 | 0x10), " [abstract, sealed]");
    }

    #[test]
    fn test_format_properties_all_known() {
        let all = 0x4
            | 0x8
            | 0x10
            | 0x20
            | 0x40
            | 0x80
            | 0x400
            | 0x800
            | 0x4000
            | 0x10000
            | 0x20000
            | 0x40000
            | 0x200000
            | 0x400000;
        let result = format_properties(all);
        assert!(result.contains("abstract"));
        assert!(result.contains("final"));
        assert!(result.contains("sealed"));
        assert!(result.contains("implicit"));
        assert!(result.contains("lazy"));
        assert!(result.contains("case"));
        assert!(result.contains("val"));
        assert!(result.contains("var"));
        assert!(result.contains("enum"));
        assert!(result.contains("given"));
        assert!(result.contains("inline"));
        assert!(result.contains("open"));
        assert!(result.contains("opaque"));
        assert!(result.contains("override"));
    }

    #[test]
    fn test_format_properties_plain() {
        assert_eq!(format_properties_plain(0), "");
        assert_eq!(format_properties_plain(0x4), "abstract");
        assert_eq!(format_properties_plain(0x4 | 0x10), "abstract, sealed");
    }

    #[test]
    fn test_format_access() {
        assert_eq!(format_access(&ArchivedAccess::Public), "");
        assert_eq!(format_access(&ArchivedAccess::Private), "private");
        assert_eq!(format_access(&ArchivedAccess::Protected), "protected");
    }
}
