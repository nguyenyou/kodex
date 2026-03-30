//! Regression tests for bugs discovered via snapshot testing.
//!
//! Uses the billing test fixture:
//!   Service (trait) -> process() [abstract]
//!   ServiceImpl (class extends Service) -> process() [overrides, calls save()] -> save()

mod common;

use kodex::query::symbol::{edges_from, resolve_symbols};

// ── Bug 1: FQN lookup ──────────────────────────────────────────────────────

#[test]
fn fqn_lookup_exact_match() {
    let reader = common::build_and_load_index(common::make_billing_test_docs());
    let index = reader.index();

    let results = resolve_symbols(index, "com/example/Service#");
    assert_eq!(
        results.len(),
        1,
        "full FQN should resolve to exactly 1 symbol"
    );
    assert_eq!(
        &index.strings[u32::from(results[0].name) as usize],
        "Service"
    );
}

#[test]
fn fqn_lookup_class_with_hash() {
    let reader = common::build_and_load_index(common::make_billing_test_docs());
    let index = reader.index();

    let results = resolve_symbols(index, "com/example/ServiceImpl#");
    assert_eq!(results.len(), 1);
    assert_eq!(
        &index.strings[u32::from(results[0].name) as usize],
        "ServiceImpl"
    );
}

#[test]
fn fqn_lookup_method() {
    let reader = common::build_and_load_index(common::make_billing_test_docs());
    let index = reader.index();

    let results = resolve_symbols(index, "com/example/ServiceImpl#save().");
    assert_eq!(results.len(), 1);
    assert_eq!(&index.strings[u32::from(results[0].name) as usize], "save");
}

#[test]
fn fqn_lookup_still_works_by_display_name() {
    let reader = common::build_and_load_index(common::make_billing_test_docs());
    let index = reader.index();

    let results = resolve_symbols(index, "Service");
    assert!(!results.is_empty(), "display name 'Service' should resolve");
}

// ── Bug 3: Callees trait-aware ─────────────────────────────────────────────

#[test]
fn callees_of_abstract_trait_method_follows_overrides() {
    let reader = common::build_and_load_index(common::make_billing_test_docs());
    let index = reader.index();

    let trait_process = index
        .symbols
        .iter()
        .find(|s| &index.strings[u32::from(s.fqn) as usize] == "com/example/Service#process().")
        .expect("Service#process should exist");

    let trait_process_id: u32 = trait_process.id.into();

    let direct_callees = edges_from(&index.call_graph_forward, trait_process_id);
    assert!(
        direct_callees.is_empty(),
        "abstract trait method should have no direct callees"
    );

    let overriders = edges_from(&index.overrides, trait_process_id);
    assert!(
        !overriders.is_empty(),
        "Service#process should have overriders"
    );

    let mut all_callees: Vec<u32> = direct_callees.iter().map(|v| u32::from(*v)).collect();
    for overrider_id in overriders.iter() {
        all_callees.extend(
            edges_from(&index.call_graph_forward, u32::from(*overrider_id))
                .iter()
                .map(|v| u32::from(*v)),
        );
    }
    all_callees.sort_unstable();
    all_callees.dedup();

    assert!(
        !all_callees.is_empty(),
        "trait-aware callees should find callees via overriders"
    );

    let save_sym = index
        .symbols
        .iter()
        .find(|s| &index.strings[u32::from(s.fqn) as usize] == "com/example/ServiceImpl#save().")
        .expect("ServiceImpl#save should exist");
    let save_id: u32 = save_sym.id.into();
    assert!(
        all_callees.contains(&save_id),
        "callees of Service#process (via override) should include ServiceImpl#save"
    );
}

#[test]
fn callees_of_concrete_method_still_works() {
    let reader = common::build_and_load_index(common::make_billing_test_docs());
    let index = reader.index();

    let impl_process = index
        .symbols
        .iter()
        .find(|s| &index.strings[u32::from(s.fqn) as usize] == "com/example/ServiceImpl#process().")
        .expect("ServiceImpl#process should exist");

    let callees = edges_from(&index.call_graph_forward, impl_process.id.into());
    assert!(
        !callees.is_empty(),
        "concrete method should have direct callees"
    );

    let save_sym = index
        .symbols
        .iter()
        .find(|s| &index.strings[u32::from(s.fqn) as usize] == "com/example/ServiceImpl#save().")
        .expect("ServiceImpl#save should exist");
    assert!(
        callees
            .iter()
            .any(|v| u32::from(*v) == u32::from(save_sym.id))
    );
}
