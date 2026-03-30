use kodex::index::reader::IndexReader;
use kodex::index::writer::write_index;
use kodex::ingest::merge::build_index;
use kodex::ingest::types::*;
use kodex::model::*;

/// Billing fixture with Service trait + ServiceImpl class (1 module, 5 symbols).
///
///   trait Service { def process(): Unit }
///   class ServiceImpl extends Service {
///     def process(): Unit    — overrides Service.process, calls save()
///     def save(): Unit       — callee target
///   }
#[allow(dead_code)]
pub fn make_billing_test_docs() -> Vec<IntermediateDoc> {
    vec![
        IntermediateDoc {
            uri: "modules/billing/src/com/example/Service.scala".to_string(),
            module_segments: "modules.billing".to_string(),
            symbols: vec![
                IntermediateSymbol {
                    fqn: "com/example/Service#".to_string(),
                    display_name: "Service".to_string(),
                    kind: SymbolKind::Trait,
                    properties: 0x4,
                    signature: "trait Service".to_string(),
                    parents: vec!["scala/AnyRef#".to_string()],
                    overridden_symbols: vec![],
                    access: Access::Public,
                },
                IntermediateSymbol {
                    fqn: "com/example/Service#process().".to_string(),
                    display_name: "process".to_string(),
                    kind: SymbolKind::Method,
                    properties: 0x4,
                    signature: "def process(): Unit".to_string(),
                    parents: vec![],
                    overridden_symbols: vec![],
                    access: Access::Public,
                },
            ],
            occurrences: vec![
                IntermediateOccurrence {
                    symbol: "com/example/Service#".to_string(),
                    role: ReferenceRole::Definition,
                    start_line: 3,
                    start_col: 6,
                    end_col: 13,
                },
                IntermediateOccurrence {
                    symbol: "com/example/Service#process().".to_string(),
                    role: ReferenceRole::Definition,
                    start_line: 5,
                    start_col: 6,
                    end_col: 13,
                },
            ],
        },
        IntermediateDoc {
            uri: "modules/billing/src/com/example/ServiceImpl.scala".to_string(),
            module_segments: "modules.billing".to_string(),
            symbols: vec![
                IntermediateSymbol {
                    fqn: "com/example/ServiceImpl#".to_string(),
                    display_name: "ServiceImpl".to_string(),
                    kind: SymbolKind::Class,
                    properties: 0,
                    signature: "class ServiceImpl extends Service".to_string(),
                    parents: vec!["com/example/Service#".to_string()],
                    overridden_symbols: vec![],
                    access: Access::Public,
                },
                IntermediateSymbol {
                    fqn: "com/example/ServiceImpl#process().".to_string(),
                    display_name: "process".to_string(),
                    kind: SymbolKind::Method,
                    properties: 0,
                    signature: "def process(): Unit".to_string(),
                    parents: vec![],
                    overridden_symbols: vec!["com/example/Service#process().".to_string()],
                    access: Access::Public,
                },
                IntermediateSymbol {
                    fqn: "com/example/ServiceImpl#save().".to_string(),
                    display_name: "save".to_string(),
                    kind: SymbolKind::Method,
                    properties: 0,
                    signature: "def save(): Unit".to_string(),
                    parents: vec![],
                    overridden_symbols: vec![],
                    access: Access::Private,
                },
            ],
            occurrences: vec![
                IntermediateOccurrence {
                    symbol: "com/example/ServiceImpl#".to_string(),
                    role: ReferenceRole::Definition,
                    start_line: 3,
                    start_col: 6,
                    end_col: 17,
                },
                IntermediateOccurrence {
                    symbol: "com/example/ServiceImpl#process().".to_string(),
                    role: ReferenceRole::Definition,
                    start_line: 5,
                    start_col: 6,
                    end_col: 13,
                },
                IntermediateOccurrence {
                    symbol: "com/example/ServiceImpl#save().".to_string(),
                    role: ReferenceRole::Definition,
                    start_line: 12,
                    start_col: 6,
                    end_col: 10,
                },
                IntermediateOccurrence {
                    symbol: "com/example/ServiceImpl#save().".to_string(),
                    role: ReferenceRole::Reference,
                    start_line: 8,
                    start_col: 4,
                    end_col: 8,
                },
                IntermediateOccurrence {
                    symbol: "com/example/Service#".to_string(),
                    role: ReferenceRole::Reference,
                    start_line: 6,
                    start_col: 10,
                    end_col: 17,
                },
            ],
        },
    ]
}

/// Billing fixture extended with a generated file (protobuf-like).
/// Adds a generated `ServiceProto` class that:
///   - extends Service (so it appears as an implementation)
///   - has a `process` method that calls `save` (so it appears as a caller)
#[allow(dead_code)]
pub fn make_billing_with_generated_docs() -> Vec<IntermediateDoc> {
    let mut docs = make_billing_test_docs();
    docs.push(IntermediateDoc {
        uri: "out/modules/billing/compileScalaPB.dest/com/example/ServiceProto.scala".to_string(),
        module_segments: "modules.billing".to_string(),
        symbols: vec![
            IntermediateSymbol {
                fqn: "com/example/ServiceProto#".to_string(),
                display_name: "ServiceProto".to_string(),
                kind: SymbolKind::Class,
                properties: 0x8, // final
                signature: "final class ServiceProto extends Service".to_string(),
                parents: vec!["com/example/Service#".to_string()],
                overridden_symbols: vec![],
                access: Access::Public,
            },
            IntermediateSymbol {
                fqn: "com/example/ServiceProto#process().".to_string(),
                display_name: "process".to_string(),
                kind: SymbolKind::Method,
                properties: 0,
                signature: "def process(): Unit".to_string(),
                parents: vec![],
                overridden_symbols: vec!["com/example/Service#process().".to_string()],
                access: Access::Public,
            },
        ],
        occurrences: vec![
            IntermediateOccurrence {
                symbol: "com/example/ServiceProto#".to_string(),
                role: ReferenceRole::Definition,
                start_line: 5,
                start_col: 6,
                end_col: 18,
            },
            IntermediateOccurrence {
                symbol: "com/example/ServiceProto#process().".to_string(),
                role: ReferenceRole::Definition,
                start_line: 7,
                start_col: 6,
                end_col: 13,
            },
            // ServiceProto.process calls save — makes ServiceProto a caller of save
            IntermediateOccurrence {
                symbol: "com/example/ServiceImpl#save().".to_string(),
                role: ReferenceRole::Reference,
                start_line: 9,
                start_col: 4,
                end_col: 8,
            },
            // Reference to Service trait (extends)
            IntermediateOccurrence {
                symbol: "com/example/Service#".to_string(),
                role: ReferenceRole::Reference,
                start_line: 5,
                start_col: 30,
                end_col: 37,
            },
        ],
    });
    docs
}

/// Fixture with nested classes to test recursive owner.member resolution.
///
///   class Component {
///     class Backend {
///       def render(): Unit
///     }
///   }
#[allow(dead_code)]
pub fn make_nested_class_docs() -> Vec<IntermediateDoc> {
    vec![IntermediateDoc {
        uri: "modules/app/src/com/example/Component.scala".to_string(),
        module_segments: "modules.app".to_string(),
        symbols: vec![
            IntermediateSymbol {
                fqn: "com/example/Component#".to_string(),
                display_name: "Component".to_string(),
                kind: SymbolKind::Class,
                properties: 0,
                signature: "class Component".to_string(),
                parents: vec![],
                overridden_symbols: vec![],
                access: Access::Public,
            },
            IntermediateSymbol {
                fqn: "com/example/Component#Backend#".to_string(),
                display_name: "Backend".to_string(),
                kind: SymbolKind::Class,
                properties: 0,
                signature: "class Backend".to_string(),
                parents: vec![],
                overridden_symbols: vec![],
                access: Access::Public,
            },
            IntermediateSymbol {
                fqn: "com/example/Component#Backend#render().".to_string(),
                display_name: "render".to_string(),
                kind: SymbolKind::Method,
                properties: 0,
                signature: "def render(): Unit".to_string(),
                parents: vec![],
                overridden_symbols: vec![],
                access: Access::Public,
            },
        ],
        occurrences: vec![
            IntermediateOccurrence {
                symbol: "com/example/Component#".to_string(),
                role: ReferenceRole::Definition,
                start_line: 1,
                start_col: 6,
                end_col: 15,
            },
            IntermediateOccurrence {
                symbol: "com/example/Component#Backend#".to_string(),
                role: ReferenceRole::Definition,
                start_line: 3,
                start_col: 8,
                end_col: 15,
            },
            IntermediateOccurrence {
                symbol: "com/example/Component#Backend#render().".to_string(),
                role: ReferenceRole::Definition,
                start_line: 5,
                start_col: 10,
                end_col: 16,
            },
        ],
    }]
}

/// Rich test fixture with 2 modules, covering all relationship types:
///
/// Module "modules.core":
///   trait Animal { def speak(): String }        — abstract method
///   class Dog extends Animal {
///     def speak(): String                       — overrides Animal.speak, calls bark()
///     def bark(): String                        — callee target
///   }
///
/// Module "modules.app" (depends on core):
///   object PetStore {
///     def adopt(): Animal                       — calls Dog.bark cross-module
///     val name: String                          — val accessor
///   }
#[allow(unused)]
pub fn make_rich_test_docs() -> Vec<IntermediateDoc> {
    vec![
        // ── Module: modules.core ──
        IntermediateDoc {
            uri: "modules/core/src/com/example/Animal.scala".to_string(),
            module_segments: "modules.core".to_string(),
            symbols: vec![
                IntermediateSymbol {
                    fqn: "com/example/Animal#".to_string(),
                    display_name: "Animal".to_string(),
                    kind: SymbolKind::Trait,
                    properties: 0x4, // abstract
                    signature: "trait Animal".to_string(),
                    parents: vec!["java/lang/Object#".to_string()],
                    overridden_symbols: vec![],
                    access: Access::Public,
                },
                IntermediateSymbol {
                    fqn: "com/example/Animal#speak().".to_string(),
                    display_name: "speak".to_string(),
                    kind: SymbolKind::Method,
                    properties: 0x4, // abstract
                    signature: "abstract method speak: String".to_string(),
                    parents: vec![],
                    overridden_symbols: vec![],
                    access: Access::Public,
                },
            ],
            occurrences: vec![
                IntermediateOccurrence {
                    symbol: "com/example/Animal#".to_string(),
                    role: ReferenceRole::Definition,
                    start_line: 3,
                    start_col: 6,
                    end_col: 12,
                },
                IntermediateOccurrence {
                    symbol: "com/example/Animal#speak().".to_string(),
                    role: ReferenceRole::Definition,
                    start_line: 4,
                    start_col: 6,
                    end_col: 11,
                },
            ],
        },
        IntermediateDoc {
            uri: "modules/core/src/com/example/Dog.scala".to_string(),
            module_segments: "modules.core".to_string(),
            symbols: vec![
                IntermediateSymbol {
                    fqn: "com/example/Dog#".to_string(),
                    display_name: "Dog".to_string(),
                    kind: SymbolKind::Class,
                    properties: 0,
                    signature: "class Dog extends Animal".to_string(),
                    parents: vec!["com/example/Animal#".to_string()],
                    overridden_symbols: vec![],
                    access: Access::Public,
                },
                IntermediateSymbol {
                    fqn: "com/example/Dog#speak().".to_string(),
                    display_name: "speak".to_string(),
                    kind: SymbolKind::Method,
                    properties: 0,
                    signature: "method speak: String".to_string(),
                    parents: vec![],
                    overridden_symbols: vec!["com/example/Animal#speak().".to_string()],
                    access: Access::Public,
                },
                IntermediateSymbol {
                    fqn: "com/example/Dog#bark().".to_string(),
                    display_name: "bark".to_string(),
                    kind: SymbolKind::Method,
                    properties: 0,
                    signature: "method bark: String".to_string(),
                    parents: vec![],
                    overridden_symbols: vec![],
                    access: Access::Public,
                },
            ],
            occurrences: vec![
                IntermediateOccurrence {
                    symbol: "com/example/Dog#".to_string(),
                    role: ReferenceRole::Definition,
                    start_line: 3,
                    start_col: 6,
                    end_col: 9,
                },
                IntermediateOccurrence {
                    symbol: "com/example/Dog#speak().".to_string(),
                    role: ReferenceRole::Definition,
                    start_line: 5,
                    start_col: 6,
                    end_col: 11,
                },
                IntermediateOccurrence {
                    symbol: "com/example/Dog#bark().".to_string(),
                    role: ReferenceRole::Definition,
                    start_line: 12,
                    start_col: 6,
                    end_col: 10,
                },
                // Dog.speak() calls bark() at line 8
                IntermediateOccurrence {
                    symbol: "com/example/Dog#bark().".to_string(),
                    role: ReferenceRole::Reference,
                    start_line: 8,
                    start_col: 4,
                    end_col: 8,
                },
                // Dog extends Animal (reference at line 3)
                IntermediateOccurrence {
                    symbol: "com/example/Animal#".to_string(),
                    role: ReferenceRole::Reference,
                    start_line: 3,
                    start_col: 20,
                    end_col: 26,
                },
            ],
        },
        // ── Module: modules.app ──
        IntermediateDoc {
            uri: "modules/app/src/com/example/PetStore.scala".to_string(),
            module_segments: "modules.app".to_string(),
            symbols: vec![
                IntermediateSymbol {
                    fqn: "com/example/PetStore.".to_string(),
                    display_name: "PetStore".to_string(),
                    kind: SymbolKind::Object,
                    properties: 0x8, // final
                    signature: "object PetStore".to_string(),
                    parents: vec!["java/lang/Object#".to_string()],
                    overridden_symbols: vec![],
                    access: Access::Public,
                },
                IntermediateSymbol {
                    fqn: "com/example/PetStore.adopt().".to_string(),
                    display_name: "adopt".to_string(),
                    kind: SymbolKind::Method,
                    properties: 0,
                    signature: "method adopt: Animal".to_string(),
                    parents: vec![],
                    overridden_symbols: vec![],
                    access: Access::Public,
                },
                IntermediateSymbol {
                    fqn: "com/example/PetStore.name.".to_string(),
                    display_name: "name".to_string(),
                    kind: SymbolKind::Method,
                    properties: 0x400, // val
                    signature: "val method name: String".to_string(),
                    parents: vec![],
                    overridden_symbols: vec![],
                    access: Access::Public,
                },
            ],
            occurrences: vec![
                IntermediateOccurrence {
                    symbol: "com/example/PetStore.".to_string(),
                    role: ReferenceRole::Definition,
                    start_line: 3,
                    start_col: 7,
                    end_col: 15,
                },
                IntermediateOccurrence {
                    symbol: "com/example/PetStore.adopt().".to_string(),
                    role: ReferenceRole::Definition,
                    start_line: 5,
                    start_col: 6,
                    end_col: 11,
                },
                IntermediateOccurrence {
                    symbol: "com/example/PetStore.name.".to_string(),
                    role: ReferenceRole::Definition,
                    start_line: 10,
                    start_col: 6,
                    end_col: 10,
                },
                // adopt() calls Dog.bark() at line 7 (cross-module)
                IntermediateOccurrence {
                    symbol: "com/example/Dog#bark().".to_string(),
                    role: ReferenceRole::Reference,
                    start_line: 7,
                    start_col: 4,
                    end_col: 8,
                },
                // adopt() references Animal at line 5
                IntermediateOccurrence {
                    symbol: "com/example/Animal#".to_string(),
                    role: ReferenceRole::Reference,
                    start_line: 5,
                    start_col: 15,
                    end_col: 21,
                },
            ],
        },
    ]
}

/// Wrapper that keeps the tempdir alive alongside the IndexReader.
#[allow(dead_code)]
pub struct TestIndex {
    pub reader: IndexReader,
    _dir: tempfile::TempDir,
}

#[allow(dead_code)]
impl TestIndex {
    pub fn index(&self) -> &kodex::model::ArchivedKodexIndex {
        self.reader.index()
    }
}

/// Build an index from docs and load it via roundtrip (serialize → mmap → deserialize).
#[allow(dead_code)]
pub fn build_and_load_index(docs: Vec<IntermediateDoc>) -> TestIndex {
    build_and_load_index_with_metadata(docs, None)
}

/// Build an index with optional build metadata (for ivy deps, module deps, etc.).
#[allow(dead_code)]
pub fn build_and_load_index_with_metadata(
    docs: Vec<IntermediateDoc>,
    metadata: Option<&kodex::ingest::provider::BuildMetadata>,
) -> TestIndex {
    let index = build_index(&docs, metadata, ".");
    let dir = tempfile::tempdir().unwrap();
    let idx_path = dir.path().join("kodex.idx");
    write_index(&index, &idx_path).unwrap();
    let reader = IndexReader::open(&idx_path).unwrap();
    TestIndex { reader, _dir: dir }
}
