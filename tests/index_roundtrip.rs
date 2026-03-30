//! Integration test: build index -> write to disk -> read back -> verify all fields.

mod common;

use kodex::model::*;

#[test]
fn test_index_roundtrip() {
    let reader = common::build_and_load_index(common::make_billing_test_docs());
    let archived = reader.index();

    // ── Version ──
    assert_eq!(u32::from(archived.version), KODEX_INDEX_VERSION);

    // ── Files ──
    assert_eq!(archived.files.len(), 2);

    // ── Modules ──
    assert_eq!(archived.modules.len(), 1);
    let mod_name: &str = &archived.strings[u32::from(archived.modules[0].name) as usize];
    assert_eq!(mod_name, "modules.billing");
    assert_eq!(u32::from(archived.modules[0].file_count), 2);

    // ── Symbols ──
    assert_eq!(archived.symbols.len(), 5); // Service, Service#process, ServiceImpl, ServiceImpl#process, ServiceImpl#save

    // Find symbols by FQN
    let find_sym = |fqn: &str| -> &kodex::model::ArchivedSymbol {
        archived
            .symbols
            .iter()
            .find(|s| archived.strings[u32::from(s.fqn) as usize] == fqn)
            .unwrap_or_else(|| panic!("Symbol not found: {fqn}"))
    };

    let service = find_sym("com/example/Service#");
    let service_process = find_sym("com/example/Service#process().");
    let impl_sym = find_sym("com/example/ServiceImpl#");
    let impl_process = find_sym("com/example/ServiceImpl#process().");
    let impl_save = find_sym("com/example/ServiceImpl#save().");

    // ── Owner resolution ──
    assert_eq!(
        u32::from(service_process.owner),
        u32::from(service.id),
        "Service#process owner should be Service"
    );
    assert_eq!(
        u32::from(impl_process.owner),
        u32::from(impl_sym.id),
        "ServiceImpl#process owner should be ServiceImpl"
    );
    assert_eq!(
        u32::from(impl_save.owner),
        u32::from(impl_sym.id),
        "ServiceImpl#save owner should be ServiceImpl"
    );

    // ── Inheritance ──
    let inh_fwd =
        kodex::query::symbol::edges_from(&archived.inheritance_forward, u32::from(service.id));
    assert!(
        inh_fwd
            .iter()
            .any(|v| u32::from(*v) == u32::from(impl_sym.id)),
        "Service should have ServiceImpl as child"
    );

    let inh_rev =
        kodex::query::symbol::edges_from(&archived.inheritance_reverse, u32::from(impl_sym.id));
    assert!(
        inh_rev
            .iter()
            .any(|v| u32::from(*v) == u32::from(service.id)),
        "ServiceImpl should have Service as parent"
    );

    // ── Call graph ──
    let callees =
        kodex::query::symbol::edges_from(&archived.call_graph_forward, u32::from(impl_process.id));
    assert!(
        callees
            .iter()
            .any(|v| u32::from(*v) == u32::from(impl_save.id)),
        "impl.process should call impl.save"
    );

    let callers =
        kodex::query::symbol::edges_from(&archived.call_graph_reverse, u32::from(impl_save.id));
    assert!(
        callers
            .iter()
            .any(|v| u32::from(*v) == u32::from(impl_process.id)),
        "impl.save should be called by impl.process"
    );

    // ── Members ──
    let service_members =
        kodex::query::symbol::edges_from(&archived.members, u32::from(service.id));
    assert!(
        service_members
            .iter()
            .any(|v| u32::from(*v) == u32::from(service_process.id)),
        "Service should have process as member"
    );

    let impl_members = kodex::query::symbol::edges_from(&archived.members, u32::from(impl_sym.id));
    assert!(
        impl_members
            .iter()
            .any(|v| u32::from(*v) == u32::from(impl_process.id))
    );
    assert!(
        impl_members
            .iter()
            .any(|v| u32::from(*v) == u32::from(impl_save.id))
    );

    // ── Overrides ──
    let overriders =
        kodex::query::symbol::edges_from(&archived.overrides, u32::from(service_process.id));
    assert!(
        overriders
            .iter()
            .any(|v| u32::from(*v) == u32::from(impl_process.id)),
        "Service#process should be overridden by ServiceImpl#process"
    );

    // ── References ──
    assert!(!archived.references.is_empty(), "should have references");

    // ── Trigram + hash indexes ──
    assert!(
        !archived.name_trigrams.is_empty(),
        "should have trigram index"
    );
    assert!(
        u32::from(archived.name_hash_size) > 0,
        "should have hash index"
    );

    // ── end_line ──
    let process_end: u32 = impl_process.end_line.into();
    let save_line: u32 = impl_save.line.into();
    assert!(
        process_end < save_line || process_end == u32::MAX,
        "impl.process.end_line ({process_end}) should be < impl.save.line ({save_line})"
    );
}
