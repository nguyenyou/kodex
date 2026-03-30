//! Test that multi-symbol occurrences (`;sym1;sym2`) are handled correctly.
//!
//! SemanticDB spec §3.3: When an identifier resolves to multiple definitions
//! (e.g., overloaded implicit resolution), the symbol field uses semicolons:
//! `;com/example/Foo#bar().;com/example/Foo#bar(+1).`
//!
//! kodex must split these and create reference entries for each symbol.

mod common;

use kodex::query::symbol::edges_from;

/// Fixture simulating a multi-symbol occurrence.
///
///   class Converter {
///     def convert(x: String): Int = ???   // convert().
///     def convert(x: Int): String = ???   // convert(+1).
///   }
///   object Main {
///     def run(): Unit = converter.convert(input)  // compiler can't disambiguate → multi-symbol
///   }
fn make_multi_symbol_docs() -> Vec<kodex::ingest::types::IntermediateDoc> {
    use kodex::ingest::types::*;
    use kodex::model::*;
    vec![
        IntermediateDoc {
            uri: "src/com/example/Converter.scala".to_string(),
            module_segments: "modules.core".to_string(),
            symbols: vec![
                IntermediateSymbol {
                    fqn: "com/example/Converter#".to_string(),
                    display_name: "Converter".to_string(),
                    kind: SymbolKind::Class,
                    properties: 0,
                    signature: "class Converter".to_string(),
                    parents: vec![],
                    overridden_symbols: vec![],
                    access: Access::Public,
                },
                IntermediateSymbol {
                    fqn: "com/example/Converter#convert().".to_string(),
                    display_name: "convert".to_string(),
                    kind: SymbolKind::Method,
                    properties: 0,
                    signature: "def convert(x: String): Int".to_string(),
                    parents: vec![],
                    overridden_symbols: vec![],
                    access: Access::Public,
                },
                IntermediateSymbol {
                    fqn: "com/example/Converter#convert(+1).".to_string(),
                    display_name: "convert".to_string(),
                    kind: SymbolKind::Method,
                    properties: 0,
                    signature: "def convert(x: Int): String".to_string(),
                    parents: vec![],
                    overridden_symbols: vec![],
                    access: Access::Public,
                },
            ],
            occurrences: vec![
                IntermediateOccurrence {
                    symbol: "com/example/Converter#".to_string(),
                    role: ReferenceRole::Definition,
                    start_line: 1,
                    start_col: 6,
                    end_col: 15,
                },
                IntermediateOccurrence {
                    symbol: "com/example/Converter#convert().".to_string(),
                    role: ReferenceRole::Definition,
                    start_line: 2,
                    start_col: 6,
                    end_col: 13,
                },
                IntermediateOccurrence {
                    symbol: "com/example/Converter#convert(+1).".to_string(),
                    role: ReferenceRole::Definition,
                    start_line: 5,
                    start_col: 6,
                    end_col: 13,
                },
            ],
        },
        IntermediateDoc {
            uri: "src/com/example/Main.scala".to_string(),
            module_segments: "modules.core".to_string(),
            symbols: vec![
                IntermediateSymbol {
                    fqn: "com/example/Main.".to_string(),
                    display_name: "Main".to_string(),
                    kind: SymbolKind::Object,
                    properties: 0,
                    signature: "object Main".to_string(),
                    parents: vec![],
                    overridden_symbols: vec![],
                    access: Access::Public,
                },
                IntermediateSymbol {
                    fqn: "com/example/Main.run().".to_string(),
                    display_name: "run".to_string(),
                    kind: SymbolKind::Method,
                    properties: 0,
                    signature: "def run(): Unit".to_string(),
                    parents: vec![],
                    overridden_symbols: vec![],
                    access: Access::Public,
                },
            ],
            occurrences: vec![
                IntermediateOccurrence {
                    symbol: "com/example/Main.".to_string(),
                    role: ReferenceRole::Definition,
                    start_line: 1,
                    start_col: 7,
                    end_col: 11,
                },
                IntermediateOccurrence {
                    symbol: "com/example/Main.run().".to_string(),
                    role: ReferenceRole::Definition,
                    start_line: 2,
                    start_col: 6,
                    end_col: 9,
                },
                // Multi-symbol reference: compiler can't disambiguate which overload
                // SemanticDB encodes this as ";sym1;sym2"
                IntermediateOccurrence {
                    symbol: ";com/example/Converter#convert().;com/example/Converter#convert(+1).".to_string(),
                    role: ReferenceRole::Reference,
                    start_line: 3,
                    start_col: 20,
                    end_col: 27,
                },
            ],
        },
    ]
}

// ── References ────────────────────────────────────────────────────────────────

#[test]
fn multi_symbol_occurrence_creates_references_for_each_symbol() {
    let ti = common::build_and_load_index(make_multi_symbol_docs());
    let index = ti.index();

    // Find both overloads
    let convert_0 = index
        .symbols
        .iter()
        .find(|s| &index.strings[u32::from(s.fqn) as usize] == "com/example/Converter#convert().")
        .expect("convert() should exist");
    let convert_1 = index
        .symbols
        .iter()
        .find(|s| &index.strings[u32::from(s.fqn) as usize] == "com/example/Converter#convert(+1).")
        .expect("convert(+1) should exist");

    // Both overloads should have a REFERENCE from Main.scala line 3
    let refs_0: Vec<_> = index
        .references
        .iter()
        .filter(|rl| u32::from(rl.symbol_id) == u32::from(convert_0.id))
        .flat_map(|rl| rl.refs.iter())
        .filter(|r| r.role == kodex::model::ReferenceRole::Reference)
        .collect();

    let refs_1: Vec<_> = index
        .references
        .iter()
        .filter(|rl| u32::from(rl.symbol_id) == u32::from(convert_1.id))
        .flat_map(|rl| rl.refs.iter())
        .filter(|r| r.role == kodex::model::ReferenceRole::Reference)
        .collect();

    assert!(
        !refs_0.is_empty(),
        "multi-symbol: convert() should have a reference from the multi-symbol occurrence"
    );
    assert!(
        !refs_1.is_empty(),
        "multi-symbol: convert(+1) should have a reference from the multi-symbol occurrence"
    );
}

// ── Call graph ─────────────────────────────────────────────────────────────────

#[test]
fn multi_symbol_occurrence_creates_call_edges() {
    let ti = common::build_and_load_index(make_multi_symbol_docs());
    let index = ti.index();

    let run = index
        .symbols
        .iter()
        .find(|s| &index.strings[u32::from(s.fqn) as usize] == "com/example/Main.run().")
        .expect("Main.run should exist");
    let convert_0 = index
        .symbols
        .iter()
        .find(|s| &index.strings[u32::from(s.fqn) as usize] == "com/example/Converter#convert().")
        .expect("convert() should exist");
    let convert_1 = index
        .symbols
        .iter()
        .find(|s| &index.strings[u32::from(s.fqn) as usize] == "com/example/Converter#convert(+1).")
        .expect("convert(+1) should exist");

    // run() should have call edges to BOTH overloads
    let callees: Vec<u32> = edges_from(&index.call_graph_forward, run.id.into())
        .iter()
        .map(|v| u32::from(*v))
        .collect();

    assert!(
        callees.contains(&u32::from(convert_0.id)),
        "multi-symbol: run() should call convert() (first overload)"
    );
    assert!(
        callees.contains(&u32::from(convert_1.id)),
        "multi-symbol: run() should call convert(+1) (second overload)"
    );
}
