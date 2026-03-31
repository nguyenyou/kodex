//! Tests that index structural invariants hold for various fixtures.

mod common;

use kodex::ingest::merge::{build_index, validate_index};
use kodex::ingest::types::*;
use kodex::model::*;

#[test]
fn invariants_hold_for_rich_fixture() {
    let docs = common::make_rich_test_docs();
    let index = build_index(&docs, None, ".");
    validate_index(&index);
}

#[test]
fn invariants_hold_for_empty_index() {
    let index = build_index(&[], None, ".");
    validate_index(&index);
}

#[test]
fn invariants_hold_for_single_symbol() {
    let docs = vec![IntermediateDoc {
        uri: "src/Foo.scala".to_string(),
        module_segments: String::new(),
        symbols: vec![IntermediateSymbol {
            fqn: "com/example/Foo#".to_string(),
            display_name: "Foo".to_string(),
            kind: SymbolKind::Class,
            properties: 0,
            signature: "class Foo".to_string(),
            parents: vec![],
            overridden_symbols: vec![],
            access: Access::Public,
        }],
        occurrences: vec![IntermediateOccurrence {
            symbol: "com/example/Foo#".to_string(),
            role: ReferenceRole::Definition,
            start_line: 1,
            start_col: 6,
            end_col: 9,
        }],
    }];
    let index = build_index(&docs, None, ".");
    validate_index(&index);
}

#[test]
fn invariants_hold_for_hub_noise_fixture() {
    let docs = common::make_hub_noise_docs();
    let index = build_index(&docs, None, ".");
    validate_index(&index);
}

#[test]
fn invariants_hold_for_fqn_suggestion_fixture() {
    let docs = common::make_fqn_suggestion_docs();
    let index = build_index(&docs, None, ".");
    validate_index(&index);
}
