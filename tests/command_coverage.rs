//! Integration tests exercising all query command modules, symbol resolution,
//! and filter/format functions that require an ArchivedKodexIndex.

mod common;

use kodex::query::filter;
use kodex::query::format;
use kodex::query::symbol::{filter_by_kind, resolve_one, resolve_symbols};

fn build_test_index() -> common::TestIndex {
    common::build_and_load_index(common::make_billing_test_docs())
}

// ── symbol::resolve_symbols ─────────────────────────────────────────────────

#[test]
fn resolve_by_display_name() {
    let reader = build_test_index();
    let index = reader.index();
    let results = resolve_symbols(index, "Service");
    assert!(!results.is_empty());
}

#[test]
fn resolve_by_fqn() {
    let reader = build_test_index();
    let index = reader.index();
    let results = resolve_symbols(index, "com/example/Service#");
    assert_eq!(results.len(), 1);
}

#[test]
fn resolve_short_query() {
    let reader = build_test_index();
    let index = reader.index();
    let results = resolve_symbols(index, "Zq");
    assert!(
        results.is_empty(),
        "short query with no match should return empty"
    );
    let results2 = resolve_symbols(index, "Sa");
    assert!(
        !results2.is_empty(),
        "short query 'Sa' should match 'save' via substring"
    );
}

#[test]
fn resolve_no_match() {
    let reader = build_test_index();
    let index = reader.index();
    let results = resolve_symbols(index, "ZZZZZ");
    assert!(results.is_empty());
}

#[test]
fn resolve_fuzzy() {
    let reader = build_test_index();
    let index = reader.index();
    let results = resolve_symbols(index, "Servce");
    assert!(!results.is_empty(), "fuzzy match should find Service");
}

#[test]
fn resolve_one_not_found() {
    let reader = build_test_index();
    let index = reader.index();
    assert!(resolve_one(index, "NonExistent99999", None, None).is_none());
}

#[test]
fn resolve_one_ambiguous() {
    let reader = build_test_index();
    let index = reader.index();
    let result = resolve_one(index, "process", None, None);
    assert!(
        result.is_some(),
        "ambiguous should still return first match"
    );
}

#[test]
fn filter_by_kind_method() {
    let reader = build_test_index();
    let index = reader.index();
    let all = resolve_symbols(index, "process");
    let methods = filter_by_kind(&all, Some("method"));
    assert!(!methods.is_empty());
    assert!(methods.len() <= all.len());
}

#[test]
fn filter_by_kind_none_returns_all() {
    let reader = build_test_index();
    let index = reader.index();
    let all = resolve_symbols(index, "Service");
    let filtered = filter_by_kind(&all, None);
    assert_eq!(filtered.len(), all.len());
}

// ── filter functions needing index ──────────────────────────────────────────

#[test]
fn filter_is_noise_stdlib() {
    let reader = build_test_index();
    let index = reader.index();
    let service = resolve_one(index, "Service", Some("trait"), None).unwrap();
    assert!(
        !filter::is_noise(index, service),
        "Service should not be noise"
    );
}

#[test]
fn filter_is_callgraph_noise_normal() {
    let reader = build_test_index();
    let index = reader.index();
    let save = resolve_one(index, "save", None, None).unwrap();
    assert!(
        !filter::is_callgraph_noise(index, save),
        "save should not be callgraph noise"
    );
}

#[test]
fn filter_matches_exclude_basic() {
    let reader = build_test_index();
    let index = reader.index();
    let save = resolve_one(index, "save", None, None).unwrap();
    let exclude = vec!["save".to_string()];
    assert!(filter::matches_exclude(index, save, &exclude));
}

#[test]
fn filter_matches_exclude_empty() {
    let reader = build_test_index();
    let index = reader.index();
    let save = resolve_one(index, "save", None, None).unwrap();
    let exclude: Vec<String> = vec![];
    assert!(!filter::matches_exclude(index, save, &exclude));
}

#[test]
fn filter_detect_infra_hubs_small_index() {
    let reader = build_test_index();
    let index = reader.index();
    let hubs = filter::detect_infra_hubs(index, 10);
    assert!(hubs.is_empty());
}

// ── format functions needing index ──────────────────────────────────────────

#[test]
fn format_symbol_line_basic() {
    let reader = build_test_index();
    let index = reader.index();
    let sym = resolve_one(index, "ServiceImpl", Some("class"), None).unwrap();
    let line = format::format_symbol_line(index, sym);
    assert!(line.contains("class"));
    assert!(line.contains("ServiceImpl"));
}

#[test]
fn format_symbol_detail_basic() {
    let reader = build_test_index();
    let index = reader.index();
    let sym = resolve_one(index, "Service", Some("trait"), None).unwrap();
    let detail = format::format_symbol_detail(index, sym, false);
    assert!(detail.contains("trait Service"));
    assert!(detail.contains("fqn:"));
}

#[test]
fn format_symbol_detail_verbose() {
    let reader = build_test_index();
    let index = reader.index();
    let sym = resolve_one(index, "save", None, None).unwrap();
    let detail = format::format_symbol_detail(index, sym, true);
    assert!(detail.contains("owner:"));
}

#[test]
fn format_module_display_name() {
    let reader = build_test_index();
    let index = reader.index();
    let name = format::module_display_name(index, 0);
    assert!(!name.is_empty());
}

#[test]
fn format_module_display_name_invalid() {
    let reader = build_test_index();
    let index = reader.index();
    assert_eq!(format::module_display_name(index, u32::MAX), "");
}


// ── refs.rs ─────────────────────────────────────────────────────────────

#[test]
fn cmd_refs_found() {
    let reader = build_test_index();
    let index = reader.index();
    let out = kodex::query::commands::refs::cmd_refs(index, "com/example/Service#", 100);
    assert!(out.is_found());
    let text = out.output();
    assert!(text.contains("Service"), "should show symbol name");
    assert!(text.contains("references"), "should show reference count");
}

#[test]
fn cmd_refs_not_found() {
    let reader = build_test_index();
    let index = reader.index();
    let out = kodex::query::commands::refs::cmd_refs(index, "NonExistent99999", 100);
    assert!(!out.is_found());
}

// ── calls.rs ────────────────────────────────────────────────────────────

#[test]
fn cmd_flow_depth1() {
    let reader = build_test_index();
    let index = reader.index();
    let out =
        kodex::query::commands::calls::cmd_calls(index, "com/example/ServiceImpl#process().", 1, &[], false, false);
    assert!(out.is_found());
    insta::assert_snapshot!(out.output());
}

#[test]
fn cmd_flow_not_found() {
    let reader = build_test_index();
    let index = reader.index();
    let out = kodex::query::commands::calls::cmd_calls(index, "NonExistent99999", 2, &[], false, false);
    assert!(!out.is_found());
}

#[test]
fn cmd_flow_with_exclude() {
    let reader = build_test_index();
    let index = reader.index();
    let out = kodex::query::commands::calls::cmd_calls(
        index,
        "com/example/ServiceImpl#process().",
        2,
        &["save".to_string()],
        false,
        false,
    );
    assert!(out.is_found());
}

// ── info.rs ──────────────────────────────────────────────────────────────

#[test]
fn cmd_info_class() {
    let reader = build_test_index();
    let index = reader.index();
    let out = kodex::query::commands::info::cmd_info(
        index,
        "com/example/ServiceImpl#",
        &[],
    );
    assert!(out.is_found());
    insta::assert_snapshot!(out.output());
}

#[test]
fn cmd_info_trait() {
    let reader = build_test_index();
    let index = reader.index();
    let out = kodex::query::commands::info::cmd_info(
        index,
        "com/example/Service#",
        &[],
    );
    assert!(out.is_found());
    insta::assert_snapshot!(out.output());
}

#[test]
fn cmd_info_not_found() {
    let reader = build_test_index();
    let index = reader.index();
    let out = kodex::query::commands::info::cmd_info(
        index,
        "NonExistent99999",
        &[],
    );
    assert!(!out.is_found());
}

#[test]
fn cmd_info_with_exclude() {
    let reader = build_test_index();
    let index = reader.index();
    kodex::query::commands::info::cmd_info(
        index,
        "com/example/ServiceImpl#",
        &["save".to_string()],
    );
}

#[test]
fn cmd_info_method() {
    let reader = build_test_index();
    let index = reader.index();
    kodex::query::commands::info::cmd_info(index, "com/example/ServiceImpl#process().", &[]);
}

// ── New features: module filter, prefix ─────────────────────────────────────

#[test]
fn filter_by_module_match() {
    let reader = build_test_index();
    let index = reader.index();
    let all = resolve_symbols(index, "Service");
    let filtered = kodex::query::filter::filter_by_module(index, &all, "billing");
    assert!(
        !filtered.is_empty(),
        "billing module should contain Service symbols"
    );
}

#[test]
fn filter_by_module_no_match() {
    let reader = build_test_index();
    let index = reader.index();
    let all = resolve_symbols(index, "Service");
    let filtered = kodex::query::filter::filter_by_module(index, &all, "nonexistent_module");
    assert!(filtered.is_empty());
}

#[test]
fn resolve_one_with_module_filter() {
    let reader = build_test_index();
    let index = reader.index();
    let result = resolve_one(index, "process", Some("method"), Some("billing"));
    assert!(result.is_some());
}

#[test]
fn resolve_prefix_short_query() {
    let reader = build_test_index();
    let index = reader.index();
    let results = resolve_symbols(index, "Serv");
    assert!(
        !results.is_empty(),
        "prefix 'Serv' should match Service/ServiceImpl"
    );
}

// ── calls --reverse ─────────────────────────────────────────────────────────

#[test]
fn cmd_flow_reverse_depth1() {
    let reader = build_test_index();
    let index = reader.index();
    let out = kodex::query::commands::calls::cmd_calls(index, "com/example/ServiceImpl#save().", 1, &[], true, false);
    assert!(out.output().contains("save"));
}

#[test]
fn cmd_flow_reverse_not_found() {
    let reader = build_test_index();
    let index = reader.index();
    let out = kodex::query::commands::calls::cmd_calls(index, "NonExistent99999", 2, &[], true, false);
    assert!(!out.is_found());
}

#[test]
fn cmd_flow_reverse_with_exclude() {
    let reader = build_test_index();
    let index = reader.index();
    kodex::query::commands::calls::cmd_calls(index, "com/example/ServiceImpl#save().", 2, &["process".to_string()], true, false);
}

#[test]
fn test_is_synthetic_name() {
    assert!(kodex::query::filter::is_synthetic_name("copy"));
    assert!(kodex::query::filter::is_synthetic_name("copy$default$1"));
    assert!(kodex::query::filter::is_synthetic_name("_1"));
    assert!(kodex::query::filter::is_synthetic_name("_22"));
    assert!(kodex::query::filter::is_synthetic_name("apply"));
    assert!(kodex::query::filter::is_synthetic_name("unapply"));
    assert!(kodex::query::filter::is_synthetic_name("productPrefix"));
    assert!(!kodex::query::filter::is_synthetic_name("process"));
    assert!(!kodex::query::filter::is_synthetic_name("save"));
    assert!(!kodex::query::filter::is_synthetic_name("createUser"));
}

// ── cmd_search ──────────────────────────────────────────────────────────────

#[test]
fn cmd_search_found() {
    let reader = build_test_index();
    let index = reader.index();
    let out = kodex::query::commands::search::cmd_search(index, Some("Service"), 50, None, None, &[], true);
    assert!(out.is_found());
    assert!(out.output().contains("Service"));
}

#[test]
fn cmd_search_not_found() {
    let reader = build_test_index();
    let index = reader.index();
    let out = kodex::query::commands::search::cmd_search(index, Some("NonExistent"), 50, None, None, &[], true);
    assert!(!out.is_found());
}

#[test]
fn cmd_search_kind_filter() {
    let reader = build_test_index();
    let index = reader.index();
    let out = kodex::query::commands::search::cmd_search(index, Some("Service"), 50, Some("trait"), None, &[], true);
    assert!(out.is_found());
    assert!(out.output().contains("trait"));
}

#[test]
fn cmd_search_with_exclude() {
    let reader = build_test_index();
    let index = reader.index();
    // "process" matches methods in both Service and ServiceImpl
    let out = kodex::query::commands::search::cmd_search(index, Some("process"), 50, None, None, &[], true);
    assert!(out.is_found());
    // Exclude ServiceImpl — should only show Service's process
    let out = kodex::query::commands::search::cmd_search(
        index, Some("process"), 50, None, None, &["ServiceImpl".to_string()], true,
    );
    assert!(out.is_found());
    assert!(!out.output().contains("ServiceImpl"));
}

// ── search output format (snapshot) ─────────────────────────────────────────

#[test]
fn search_multi_match_shows_file_and_line() {
    let reader = build_rich_index();
    let index = reader.index();
    // "speak" matches methods in both Animal (line 4) and Dog (line 5)
    let out = kodex::query::commands::search::cmd_search(index, Some("speak"), 50, None, None, &[], true);
    assert!(out.is_found());
    insta::assert_snapshot!(out.output());
}

#[test]
fn search_single_match_detail_shows_file_and_line() {
    let reader = build_rich_index();
    let index = reader.index();
    // "Animal" with --kind trait → single match, detail view
    let out = kodex::query::commands::search::cmd_search(index, Some("Animal"), 50, Some("trait"), None, &[], true);
    assert!(out.is_found());
    insta::assert_snapshot!(out.output());
}

#[test]
fn search_single_match_method_shows_line_range() {
    let reader = build_rich_index();
    let index = reader.index();
    // "bark" → single method, should show line-end_line range
    let out = kodex::query::commands::search::cmd_search(index, Some("bark"), 50, None, None, &[], true);
    assert!(out.is_found());
    insta::assert_snapshot!(out.output());
}

// ── FQN suffix resolution: val-style vs def-style methods ──────────────────
// Regression test: searching for a method name must find BOTH val-style
// endpoints (FQN: `Owner.name.`) and def-style methods (FQN: `Owner#name().`).
// Before the fix, step 2 (FQN suffix) only matched `name.` and `name#`,
// missing `name().` — so def-style methods were invisible.

fn build_val_vs_def_index() -> common::TestIndex {
    use kodex::ingest::types::*;
    use kodex::model::*;
    common::build_and_load_index(vec![
        // Endpoint definition: val-style FQN ending in `.createOrder.`
        IntermediateDoc {
            uri: "api/OrderEndpoints.scala".to_string(),
            module_segments: "modules.api".to_string(),
            symbols: vec![IntermediateSymbol {
                fqn: "com/example/OrderEndpoints.createOrder.".to_string(),
                display_name: "createOrder".to_string(),
                kind: SymbolKind::Method,
                properties: 0x400, // val
                signature: "val createOrder: Endpoint[OrderParams, OrderResponse]".to_string(),
                parents: vec![],
                overridden_symbols: vec![],
                access: Access::Public,
            }],
            occurrences: vec![IntermediateOccurrence {
                symbol: "com/example/OrderEndpoints.createOrder.".to_string(),
                role: ReferenceRole::Definition,
                start_line: 10,
                start_col: 6,
                end_col: 17,
            }],
        },
        // Service implementation: def-style FQN ending in `.createOrder().`
        IntermediateDoc {
            uri: "service/OrderService.scala".to_string(),
            module_segments: "modules.service".to_string(),
            symbols: vec![
                IntermediateSymbol {
                    fqn: "com/example/OrderService#".to_string(),
                    display_name: "OrderService".to_string(),
                    kind: SymbolKind::Class,
                    properties: 0,
                    signature: "class OrderService".to_string(),
                    parents: vec![],
                    overridden_symbols: vec![],
                    access: Access::Public,
                },
                IntermediateSymbol {
                    fqn: "com/example/OrderService#createOrder().".to_string(),
                    display_name: "createOrder".to_string(),
                    kind: SymbolKind::Method,
                    properties: 0,
                    signature: "def createOrder(params: OrderParams): Task[OrderResponse]".to_string(),
                    parents: vec![],
                    overridden_symbols: vec![],
                    access: Access::Public,
                },
            ],
            occurrences: vec![
                IntermediateOccurrence {
                    symbol: "com/example/OrderService#".to_string(),
                    role: ReferenceRole::Definition,
                    start_line: 5,
                    start_col: 6,
                    end_col: 18,
                },
                IntermediateOccurrence {
                    symbol: "com/example/OrderService#createOrder().".to_string(),
                    role: ReferenceRole::Definition,
                    start_line: 12,
                    start_col: 6,
                    end_col: 17,
                },
            ],
        },
    ])
}

#[test]
fn search_finds_both_val_and_def_methods() {
    let reader = build_val_vs_def_index();
    let index = reader.index();
    let out = kodex::query::commands::search::cmd_search(index, Some("createOrder"), 50, None, None, &[], true);
    assert!(out.is_found());
    // Must find BOTH the val endpoint AND the def method
    assert!(
        out.output().contains("OrderEndpoints"),
        "should find val-style endpoint: {}",
        out.output()
    );
    assert!(
        out.output().contains("OrderService"),
        "should find def-style method: {}",
        out.output()
    );
    insta::assert_snapshot!(out.output());
}

#[test]
fn info_finds_symbol_by_fqn() {
    let reader = build_val_vs_def_index();
    let index = reader.index();
    // info with exact FQN finds the def-style method directly
    let out = kodex::query::commands::info::cmd_info(
        index, "com/example/OrderService#createOrder().", &[],
    );
    assert!(out.is_found());
    assert!(
        out.output().contains("OrderService"),
        "info should find def-style method by FQN: {}",
        out.output()
    );
}

#[test]
fn info_rejects_short_name() {
    let reader = build_val_vs_def_index();
    let index = reader.index();
    // info with a short name (not FQN) should return NotFound
    let out = kodex::query::commands::info::cmd_info(
        index, "createOrder", &[],
    );
    assert!(!out.is_found(), "info should reject non-FQN queries: {}", out.output());
}

// ── generated file handling ─────────────────────────────────────────────────

#[test]
fn cmd_search_excludes_generated_by_default() {
    let reader =
        common::build_and_load_index(common::make_billing_with_generated_docs());
    let index = reader.index();
    // Generated files are excluded from search results by default
    let out = kodex::query::commands::search::cmd_search(index, Some("ServiceProto"), 50, None, None, &[], false);
    assert!(!out.is_found());
    // But included when include_noise is true
    let out = kodex::query::commands::search::cmd_search(index, Some("ServiceProto"), 50, None, None, &[], true);
    assert!(out.is_found());
}

// Regression: generated-file filtering should not prevent finding a target symbol
// that lives in a generated file. The filter only applies to output lists.
#[test]
fn cmd_info_finds_target_in_generated_file() {
    let reader =
        common::build_and_load_index(common::make_billing_with_generated_docs());
    let index = reader.index();
    // ServiceProto is in a generated file. info should still FIND it —
    // generated filtering is command-level (search/callers/impls only),
    // not in resolve_one. Target resolution must always succeed.
    let out = kodex::query::commands::info::cmd_info(
        index,
        "com/example/ServiceProto#",
        &[],
    );
    assert!(
        out.is_found(),
        "info should find target even if it's in a generated file"
    );
}

// ── Owner.member resolution ─────────────────────────────────────────────────

#[test]
fn resolve_owner_dot_member() {
    let reader = build_test_index();
    let index = reader.index();
    let results = resolve_symbols(index, "ServiceImpl.process");
    assert!(!results.is_empty(), "Owner.member syntax should resolve");
    let fqn = kodex::query::s(index, results[0].fqn);
    assert!(
        fqn.contains("ServiceImpl"),
        "should resolve to ServiceImpl's process"
    );
}

#[test]
fn resolve_owner_hash_member() {
    let reader = build_test_index();
    let index = reader.index();
    let results = resolve_symbols(index, "ServiceImpl#save");
    assert!(!results.is_empty(), "Owner#member syntax should resolve");
}

#[test]
fn resolve_owner_dot_member_not_found() {
    let reader = build_test_index();
    let index = reader.index();
    let results = resolve_symbols(index, "Service.nonexistent");
    assert!(results.is_empty(), "nonexistent member should return empty");
}

#[test]
fn resolve_owner_dot_member_bad_owner() {
    let reader = build_test_index();
    let index = reader.index();
    let results = resolve_symbols(index, "FakeOwner.process");
    // Owner not found → falls through, but full query "FakeOwner.process" matches nothing
    assert!(
        results.is_empty(),
        "unknown owner with dot syntax should not match"
    );
}

// ── Nested owner.member resolution ──────────────────────────────────────────

#[test]
fn resolve_nested_owner_member() {
    let reader = common::build_and_load_index(common::make_nested_class_docs());
    let index = reader.index();
    // Two-level: Component.Backend → resolves Backend inner class
    let results = resolve_symbols(index, "Component.Backend");
    assert!(!results.is_empty(), "Component.Backend should resolve");
    let fqn = kodex::query::s(index, results[0].fqn);
    assert!(fqn.contains("Backend"), "should resolve to Backend");
}

#[test]
fn resolve_nested_owner_member_three_levels() {
    let reader = common::build_and_load_index(common::make_nested_class_docs());
    let index = reader.index();
    // Three-level: Component.Backend.render → resolves render method
    let results = resolve_symbols(index, "Component.Backend.render");
    assert!(
        !results.is_empty(),
        "Component.Backend.render should resolve via recursion"
    );
    let fqn = kodex::query::s(index, results[0].fqn);
    assert!(fqn.contains("render"), "should resolve to render method");
}

#[test]
fn resolve_nested_owner_member_bad_leaf() {
    let reader = common::build_and_load_index(common::make_nested_class_docs());
    let index = reader.index();
    let results = resolve_symbols(index, "Component.Backend.nonexistent");
    assert!(
        results.is_empty(),
        "nonexistent nested member should return empty"
    );
}

#[test]
fn resolve_owner_member_deep_search_skipping_inner_class() {
    // Component.render should find render inside Backend (a nested inner class)
    // without requiring the full path Component.Backend.render
    let reader = common::build_and_load_index(common::make_nested_class_docs());
    let index = reader.index();
    let results = resolve_symbols(index, "Component.render");
    assert!(
        !results.is_empty(),
        "Component.render should resolve by searching through nested Backend class"
    );
    let fqn = kodex::query::s(index, results[0].fqn);
    assert!(fqn.contains("render"), "should resolve to render method");
}

// ── Ambiguity ranking ───────────────────────────────────────────────────────

#[test]
fn resolve_one_prefers_exact_name() {
    let reader = build_test_index();
    let index = reader.index();
    // "Service" should pick the trait Service, not ServiceImpl (which contains "Service" as substring)
    let result = resolve_one(index, "Service", None, None);
    assert!(result.is_some());
    let name = kodex::query::s(index, result.unwrap().name);
    assert_eq!(name, "Service", "should prefer exact name match");
}

// ── cross-module tests (using rich fixture) ─────────────────────────────────

fn build_rich_index() -> common::TestIndex {
    common::build_and_load_index(common::make_rich_test_docs())
}

#[test]
fn cross_module_flow() {
    let reader = build_rich_index();
    let index = reader.index();
    let out = kodex::query::commands::calls::cmd_calls(index, "com/example/PetStore.adopt().", 3, &[], false, false);
    assert!(out.is_found());
    // adopt() calls bark() which is in a different module
    assert!(out.output().contains("bark"));
}

// ── calls --cross-module-only ─────────────────────────────────────────────

#[test]
fn cross_module_only_shows_cross_module_edges() {
    let reader = build_rich_index();
    let index = reader.index();
    // adopt() in modules.app calls bark() in modules.core — cross-module edge
    let out = kodex::query::commands::calls::cmd_calls(
        index, "com/example/PetStore.adopt().", 3, &[], false, true,
    );
    assert!(out.is_found());
    assert!(out.output().contains("bark"), "cross-module callee bark should appear");
    assert!(out.output().contains("cross-module"), "should have cross-module annotation");
}

#[test]
fn cross_module_only_hides_same_module_edges() {
    let reader = build_test_index();
    let index = reader.index();
    // Single-module fixture: all edges are within modules.billing
    let out = kodex::query::commands::calls::cmd_calls(
        index, "com/example/ServiceImpl#process().", 3, &[], false, true,
    );
    assert!(out.is_found());
    // save() is in the same module — should be hidden
    assert!(!out.output().contains("save"), "same-module callee save should be hidden");
    assert!(out.output().contains("no cross-module callees"), "should show empty hint");
}

#[test]
fn cross_module_only_reverse() {
    let reader = build_rich_index();
    let index = reader.index();
    // bark() in modules.core is called by adopt() in modules.app — cross-module caller
    let out = kodex::query::commands::calls::cmd_calls(
        index, "com/example/Dog#bark().", 3, &[], true, true,
    );
    assert!(out.is_found());
    assert!(out.output().contains("adopt"), "cross-module caller adopt should appear");
}

// ── trace ─────────────────────────────────────────────────────────────────

#[test]
fn cmd_trace_not_found() {
    let reader = build_test_index();
    let index = reader.index();
    let out = kodex::query::commands::trace::cmd_trace(
        index, "NonExistent99999", 2, &[], false, false,
    );
    assert!(!out.is_found());
}

#[test]
fn cmd_trace_single_module() {
    let reader = build_test_index();
    let index = reader.index();
    let out = kodex::query::commands::trace::cmd_trace(
        index, "com/example/ServiceImpl#process().", 2, &[], false, false,
    );
    assert!(out.is_found());
    let text = out.output();
    // Root node should have info-level detail
    assert!(text.contains("fqn: com/example/ServiceImpl#process()."), "should show FQN");
    assert!(text.contains("sig:") || text.contains("process"), "should show signature or name");
    // Callee save() should appear with info detail
    assert!(text.contains("save"), "callee save should appear");
    assert!(text.contains("fqn: com/example/ServiceImpl#save()."), "callee should have FQN");
}

#[test]
fn cmd_trace_cross_module() {
    let reader = build_rich_index();
    let index = reader.index();
    let out = kodex::query::commands::trace::cmd_trace(
        index, "com/example/PetStore.adopt().", 3, &[], false, false,
    );
    assert!(out.is_found());
    let text = out.output();
    // Root: adopt
    assert!(text.contains("fqn: com/example/PetStore.adopt()."), "root should show FQN");
    // Cross-module callee: bark
    assert!(text.contains("bark"), "cross-module callee bark should appear");
    assert!(text.contains("cross-module"), "should annotate cross-module edge");
    assert!(text.contains("fqn: com/example/Dog#bark()."), "callee should have FQN");
}

#[test]
fn cmd_trace_cross_module_only() {
    let reader = build_rich_index();
    let index = reader.index();
    let out = kodex::query::commands::trace::cmd_trace(
        index, "com/example/PetStore.adopt().", 3, &[], false, true,
    );
    assert!(out.is_found());
    let text = out.output();
    assert!(text.contains("bark"), "cross-module callee should appear");
    assert!(text.contains("fqn: com/example/Dog#bark()."), "should have info detail");
}

#[test]
fn cmd_trace_reverse() {
    let reader = build_rich_index();
    let index = reader.index();
    let out = kodex::query::commands::trace::cmd_trace(
        index, "com/example/Dog#bark().", 2, &[], true, false,
    );
    assert!(out.is_found());
    let text = out.output();
    // bark's caller: adopt (cross-module) and speak (same-module)
    assert!(text.contains("adopt") || text.contains("speak"), "should show callers");
}

#[test]
fn rich_info_trait_implementations() {
    let reader = build_rich_index();
    let index = reader.index();
    let out = kodex::query::commands::info::cmd_info(index, "com/example/Animal#", &[]);
    assert!(out.is_found());
    let text = out.output();
    assert!(text.contains("Animal"));
}

#[test]
fn rich_owner_dot_member_cross_module() {
    let reader = build_rich_index();
    let index = reader.index();
    let results = resolve_symbols(index, "PetStore.adopt");
    assert!(!results.is_empty());
    let fqn = kodex::query::s(index, results[0].fqn);
    assert!(fqn.contains("PetStore"), "should be PetStore's adopt method");
    assert!(fqn.contains("adopt"));
}

#[test]
fn rich_owner_dot_member_class() {
    let reader = build_rich_index();
    let index = reader.index();
    let results = resolve_symbols(index, "Dog.bark");
    assert!(!results.is_empty());
    let fqn = kodex::query::s(index, results[0].fqn);
    assert!(fqn.contains("Dog"));
    assert!(fqn.contains("bark"));
}

/// Reproduce: when the same source file is compiled by multiple Scala versions,
/// duplicate definition occurrences corrupt sibling body boundaries,
/// causing forward call graph edges to be lost.
#[test]
fn cross_version_duplicate_docs_preserve_call_edges() {
    use kodex::ingest::types::*;
    use kodex::model::*;

    // Same URI, two "versions" — simulates cross-compilation
    let docs = vec![
        // Version 1 of the file
        IntermediateDoc {
            uri: "src/MyApp.scala".to_string(),
            module_segments: "app.3.3.7".to_string(),
            symbols: vec![
                IntermediateSymbol {
                    fqn: "com/example/MyApp.".to_string(),
                    display_name: "MyApp".to_string(),
                    kind: SymbolKind::Object,
                    properties: 0x8,
                    signature: "object MyApp".to_string(),
                    parents: vec![],
                    overridden_symbols: vec![],
                    access: Access::Public,
                },
                IntermediateSymbol {
                    fqn: "com/example/MyApp.main().".to_string(),
                    display_name: "main".to_string(),
                    kind: SymbolKind::Method,
                    properties: 0,
                    signature: "def main(): Unit".to_string(),
                    parents: vec![],
                    overridden_symbols: vec![],
                    access: Access::Public,
                },
                IntermediateSymbol {
                    fqn: "com/example/MyApp.helper().".to_string(),
                    display_name: "helper".to_string(),
                    kind: SymbolKind::Method,
                    properties: 0,
                    signature: "def helper(): Unit".to_string(),
                    parents: vec![],
                    overridden_symbols: vec![],
                    access: Access::Public,
                },
            ],
            occurrences: vec![
                IntermediateOccurrence {
                    symbol: "com/example/MyApp.".to_string(),
                    role: ReferenceRole::Definition,
                    start_line: 1, start_col: 7, end_col: 12,
                },
                IntermediateOccurrence {
                    symbol: "com/example/MyApp.main().".to_string(),
                    role: ReferenceRole::Definition,
                    start_line: 3, start_col: 6, end_col: 10,
                },
                IntermediateOccurrence {
                    symbol: "com/example/MyApp.helper().".to_string(),
                    role: ReferenceRole::Definition,
                    start_line: 10, start_col: 6, end_col: 12,
                },
                // main() calls helper() at line 5
                IntermediateOccurrence {
                    symbol: "com/example/MyApp.helper().".to_string(),
                    role: ReferenceRole::Reference,
                    start_line: 5, start_col: 4, end_col: 10,
                },
            ],
        },
        // Version 2 of the same file (different module, same URI)
        IntermediateDoc {
            uri: "src/MyApp.scala".to_string(),
            module_segments: "app.3.8.2".to_string(),
            symbols: vec![
                IntermediateSymbol {
                    fqn: "com/example/MyApp.".to_string(),
                    display_name: "MyApp".to_string(),
                    kind: SymbolKind::Object,
                    properties: 0x8,
                    signature: "object MyApp".to_string(),
                    parents: vec![],
                    overridden_symbols: vec![],
                    access: Access::Public,
                },
                IntermediateSymbol {
                    fqn: "com/example/MyApp.main().".to_string(),
                    display_name: "main".to_string(),
                    kind: SymbolKind::Method,
                    properties: 0,
                    signature: "def main(): Unit".to_string(),
                    parents: vec![],
                    overridden_symbols: vec![],
                    access: Access::Public,
                },
                IntermediateSymbol {
                    fqn: "com/example/MyApp.helper().".to_string(),
                    display_name: "helper".to_string(),
                    kind: SymbolKind::Method,
                    properties: 0,
                    signature: "def helper(): Unit".to_string(),
                    parents: vec![],
                    overridden_symbols: vec![],
                    access: Access::Public,
                },
            ],
            occurrences: vec![
                IntermediateOccurrence {
                    symbol: "com/example/MyApp.".to_string(),
                    role: ReferenceRole::Definition,
                    start_line: 1, start_col: 7, end_col: 12,
                },
                IntermediateOccurrence {
                    symbol: "com/example/MyApp.main().".to_string(),
                    role: ReferenceRole::Definition,
                    start_line: 3, start_col: 6, end_col: 10,
                },
                IntermediateOccurrence {
                    symbol: "com/example/MyApp.helper().".to_string(),
                    role: ReferenceRole::Definition,
                    start_line: 10, start_col: 6, end_col: 12,
                },
                // same reference: main() calls helper() at line 5
                IntermediateOccurrence {
                    symbol: "com/example/MyApp.helper().".to_string(),
                    role: ReferenceRole::Reference,
                    start_line: 5, start_col: 4, end_col: 10,
                },
            ],
        },
    ];

    let ti = common::build_and_load_index(docs);
    let index = ti.index();

    // Forward: main should call helper
    let flow_out =
        kodex::query::commands::calls::cmd_calls(index, "com/example/MyApp.main().", 1, &[], false, false);
    assert!(flow_out.is_found());
    assert!(
        flow_out.output().contains("helper"),
        "main should have forward edge to helper even with duplicate docs.\nGot: {}",
        flow_out.output()
    );

    // Reverse: helper should be called by main
    let rflow_out =
        kodex::query::commands::calls::cmd_calls(index, "com/example/MyApp.helper().", 1, &[], true, false);
    assert!(rflow_out.is_found());
    assert!(
        rflow_out.output().contains("main"),
        "helper should show main as caller"
    );
}

// ── noise.rs ─────────────────────────────────────────────────────────────

#[test]
fn cmd_noise_small_index() {
    let reader = build_test_index();
    let index = reader.index();
    let out = kodex::query::commands::noise::cmd_noise(index, 15);
    assert!(out.is_found());
    assert!(out.output().contains("Noise analysis"));
    insta::assert_snapshot!(out.output());
}

#[test]
fn cmd_noise_rich_index() {
    let reader = build_rich_index();
    let index = reader.index();
    let out = kodex::query::commands::noise::cmd_noise(index, 15);
    assert!(out.is_found());
    assert!(out.output().contains("Noise analysis"));
    insta::assert_snapshot!(out.output());
}

#[test]
fn cmd_noise_custom_limit() {
    let reader = build_test_index();
    let index = reader.index();
    let out = kodex::query::commands::noise::cmd_noise(index, 5);
    assert!(out.is_found());
}

#[test]
fn rich_ambiguity_prefers_type_over_method() {
    let reader = build_rich_index();
    let index = reader.index();
    // "speak" matches both Animal#speak (abstract method) and Dog#speak (method)
    // Both are methods, so ranking doesn't change order, but resolve_one should still work
    let result = resolve_one(index, "speak", None, None);
    assert!(result.is_some());
}

// ── Feature #4: search with --module only (no query) ───────────────────────

#[test]
fn cmd_search_module_only_returns_all_symbols_in_module() {
    let reader = build_rich_index();
    let index = reader.index();
    // Search by module only, no query
    let out = kodex::query::commands::search::cmd_search(
        index, None, 50, None, Some("core"), &[], true,
    );
    assert!(out.is_found());
    // Should find symbols from modules.core: Animal, Dog, speak, bark
    assert!(out.output().contains("Animal"), "should contain Animal: {}", out.output());
    assert!(out.output().contains("Dog"), "should contain Dog: {}", out.output());
}

#[test]
fn cmd_search_module_only_with_kind_filter() {
    let reader = build_rich_index();
    let index = reader.index();
    // Search by module + kind, no query
    let out = kodex::query::commands::search::cmd_search(
        index, None, 50, Some("trait"), Some("core"), &[], true,
    );
    assert!(out.is_found());
    // Should find Animal trait from modules.core
    assert!(out.output().contains("Animal"), "should contain Animal trait: {}", out.output());
    // Should NOT contain Dog (it's a class, not a trait)
    assert!(!out.output().contains("class Dog"), "should not contain Dog class: {}", out.output());
}

#[test]
fn cmd_search_module_only_nonexistent_module() {
    let reader = build_rich_index();
    let index = reader.index();
    let out = kodex::query::commands::search::cmd_search(
        index, None, 50, None, Some("nonexistent_module_xyz"), &[], true,
    );
    assert!(!out.is_found());
    assert!(out.output().contains("No symbols in module"), "should report no symbols: {}", out.output());
}

#[test]
fn cmd_search_module_only_with_kind_no_match() {
    let reader = build_rich_index();
    let index = reader.index();
    // modules.app has PetStore (object) and adopt/name (methods), no traits
    let out = kodex::query::commands::search::cmd_search(
        index, None, 50, Some("trait"), Some("app"), &[], true,
    );
    assert!(!out.is_found());
    assert!(out.output().contains("No trait symbols in module"), "should report no trait: {}", out.output());
}

#[test]
fn cmd_search_module_only_respects_limit() {
    let reader = build_rich_index();
    let index = reader.index();
    // modules.core has multiple symbols; limit to 2
    let out = kodex::query::commands::search::cmd_search(
        index, None, 2, None, Some("core"), &[], true,
    );
    assert!(out.is_found());
    assert!(out.output().contains("... and"), "should show truncation message: {}", out.output());
}

// ── Feature #5: kind-aware suggestions ─────────────────────────────────────

#[test]
fn cmd_search_kind_mismatch_suggests_other_kinds() {
    let reader = build_rich_index();
    let index = reader.index();
    // "Dog" exists as a class, not a trait. Searching with --kind trait should suggest.
    let out = kodex::query::commands::search::cmd_search(
        index, Some("Dog"), 50, Some("trait"), None, &[], true,
    );
    assert!(!out.is_found());
    assert!(out.output().contains("No trait found matching 'Dog'"), "should say no trait found: {}", out.output());
    assert!(out.output().contains("Found under other kinds"), "should suggest other kinds: {}", out.output());
    assert!(out.output().contains("class Dog"), "should show Dog as class: {}", out.output());
}

#[test]
fn cmd_search_kind_mismatch_no_match_at_all() {
    let reader = build_rich_index();
    let index = reader.index();
    // "ZZZZZ" doesn't exist at all — should get standard not-found, no kind suggestions
    let out = kodex::query::commands::search::cmd_search(
        index, Some("ZZZZZ"), 50, Some("trait"), None, &[], true,
    );
    assert!(!out.is_found());
    assert!(out.output().contains("Not found"), "should be not found: {}", out.output());
    assert!(!out.output().contains("Found under other kinds"), "should not suggest other kinds: {}", out.output());
}

#[test]
fn cmd_search_kind_match_still_works() {
    let reader = build_rich_index();
    let index = reader.index();
    // "Animal" exists as a trait — searching with --kind trait should find it normally
    let out = kodex::query::commands::search::cmd_search(
        index, Some("Animal"), 50, Some("trait"), None, &[], true,
    );
    assert!(out.is_found());
    assert!(out.output().contains("Animal"), "should find Animal: {}", out.output());
}

#[test]
fn cmd_search_kind_filter_strict_returns_only_matching_kind() {
    let reader = build_rich_index();
    let index = reader.index();
    // "speak" matches methods. Searching with --kind class should NOT fall back to methods.
    let out = kodex::query::commands::search::cmd_search(
        index, Some("speak"), 50, Some("class"), None, &[], true,
    );
    assert!(!out.is_found());
    // Should suggest the methods under "Found under other kinds"
    assert!(out.output().contains("Found under other kinds"), "should suggest methods: {}", out.output());
    assert!(out.output().contains("method speak"), "should show speak as method: {}", out.output());
}

