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

/// Fixture with case class, enum (Class-kinded and Interface-kinded), and plain class.
///
///   case class Config(host: String, port: Int)   — PROP_CASE
///   enum Status { Active, Inactive }             — PROP_ENUM (Class-kinded)
///   enum Direction                               — PROP_ENUM (Interface-kinded, Scala 3 top-level)
///   class Engine                                 — plain class (no PROP_CASE, no PROP_ENUM)
#[allow(dead_code)]
pub fn make_property_kind_docs() -> Vec<IntermediateDoc> {
    vec![IntermediateDoc {
        uri: "modules/core/src/com/example/Types.scala".to_string(),
        module_segments: "modules.core".to_string(),
        symbols: vec![
            IntermediateSymbol {
                fqn: "com/example/Config#".to_string(),
                display_name: "Config".to_string(),
                kind: SymbolKind::Class,
                properties: 0x8 | 0x80, // final | case
                signature: "case class Config(host: String, port: Int)".to_string(),
                parents: vec![],
                overridden_symbols: vec![],
                access: Access::Public,
            },
            IntermediateSymbol {
                fqn: "com/example/Status#".to_string(),
                display_name: "Status".to_string(),
                kind: SymbolKind::Class,
                properties: 0x4 | 0x10 | 0x4000, // abstract | sealed | enum
                signature: "enum Status".to_string(),
                parents: vec![],
                overridden_symbols: vec![],
                access: Access::Public,
            },
            IntermediateSymbol {
                fqn: "com/example/Direction#".to_string(),
                display_name: "Direction".to_string(),
                kind: SymbolKind::Interface,
                properties: 0x4 | 0x10 | 0x4000, // abstract | sealed | enum
                signature: "enum Direction".to_string(),
                parents: vec![],
                overridden_symbols: vec![],
                access: Access::Public,
            },
            IntermediateSymbol {
                fqn: "com/example/Engine#".to_string(),
                display_name: "Engine".to_string(),
                kind: SymbolKind::Class,
                properties: 0,
                signature: "class Engine".to_string(),
                parents: vec![],
                overridden_symbols: vec![],
                access: Access::Public,
            },
        ],
        occurrences: vec![
            IntermediateOccurrence {
                symbol: "com/example/Config#".to_string(),
                role: ReferenceRole::Definition,
                start_line: 3,
                start_col: 6,
                end_col: 12,
            },
            IntermediateOccurrence {
                symbol: "com/example/Status#".to_string(),
                role: ReferenceRole::Definition,
                start_line: 5,
                start_col: 6,
                end_col: 12,
            },
            IntermediateOccurrence {
                symbol: "com/example/Direction#".to_string(),
                role: ReferenceRole::Definition,
                start_line: 7,
                start_col: 6,
                end_col: 15,
            },
            IntermediateOccurrence {
                symbol: "com/example/Engine#".to_string(),
                role: ReferenceRole::Definition,
                start_line: 10,
                start_col: 6,
                end_col: 12,
            },
        ],
    }]
}

/// Large codebase fixture for testing noise detection thresholds.
///
/// 50 modules with 4 shared types at different reference/module-spread levels:
///   - `RequestContext` (class): ~200 refs across 25 modules — domain type, should NOT be noise
///   - `DatabaseDriver` (class): ~600 refs across 45 modules — infra type, SHOULD be noise
///   - `AuthContext` (trait): ~150 refs across 20 modules — domain type, should NOT be noise
///   - `StringUtils` (object): ~400 refs across 40 modules — utility type, SHOULD be noise
///
/// Also includes `RequestContext#userId` as a leaf method (many call sites, 0 callees)
/// to test that effect plumbing emits method-level patterns, not type-level.
#[allow(dead_code)]
pub fn make_hub_noise_docs() -> Vec<IntermediateDoc> {
    let num_modules = 50;
    let mut docs = Vec::new();

    // ── Module 0: define shared types ──
    docs.push(IntermediateDoc {
        uri: "modules/mod00/src/com/example/RequestContext.scala".to_string(),
        module_segments: "modules.mod00".to_string(),
        symbols: vec![
            IntermediateSymbol {
                fqn: "com/example/RequestContext#".to_string(),
                display_name: "RequestContext".to_string(),
                kind: SymbolKind::Class,
                properties: 0,
                signature: "class RequestContext".to_string(),
                parents: vec![],
                overridden_symbols: vec![],
                access: Access::Public,
            },
            IntermediateSymbol {
                fqn: "com/example/RequestContext#userId().".to_string(),
                display_name: "userId".to_string(),
                kind: SymbolKind::Method,
                properties: 0,
                signature: "def userId: UserId".to_string(),
                parents: vec![],
                overridden_symbols: vec![],
                access: Access::Public,
            },
            IntermediateSymbol {
                fqn: "com/example/DatabaseDriver#".to_string(),
                display_name: "DatabaseDriver".to_string(),
                kind: SymbolKind::Class,
                properties: 0,
                signature: "class DatabaseDriver".to_string(),
                parents: vec![],
                overridden_symbols: vec![],
                access: Access::Public,
            },
            IntermediateSymbol {
                fqn: "com/example/AuthContext#".to_string(),
                display_name: "AuthContext".to_string(),
                kind: SymbolKind::Trait,
                properties: 0x4, // abstract
                signature: "trait AuthContext".to_string(),
                parents: vec![],
                overridden_symbols: vec![],
                access: Access::Public,
            },
            IntermediateSymbol {
                fqn: "com/example/StringUtils.".to_string(),
                display_name: "StringUtils".to_string(),
                kind: SymbolKind::Object,
                properties: 0x8, // final
                signature: "object StringUtils".to_string(),
                parents: vec![],
                overridden_symbols: vec![],
                access: Access::Public,
            },
        ],
        occurrences: vec![
            IntermediateOccurrence {
                symbol: "com/example/RequestContext#".to_string(),
                role: ReferenceRole::Definition,
                start_line: 3, start_col: 6, end_col: 20,
            },
            IntermediateOccurrence {
                symbol: "com/example/RequestContext#userId().".to_string(),
                role: ReferenceRole::Definition,
                start_line: 4, start_col: 6, end_col: 12,
            },
            IntermediateOccurrence {
                symbol: "com/example/DatabaseDriver#".to_string(),
                role: ReferenceRole::Definition,
                start_line: 10, start_col: 6, end_col: 20,
            },
            IntermediateOccurrence {
                symbol: "com/example/AuthContext#".to_string(),
                role: ReferenceRole::Definition,
                start_line: 15, start_col: 6, end_col: 17,
            },
            IntermediateOccurrence {
                symbol: "com/example/StringUtils.".to_string(),
                role: ReferenceRole::Definition,
                start_line: 20, start_col: 7, end_col: 18,
            },
        ],
    });

    // ── Modules 1..49: reference shared types at varying rates ──
    for i in 1..num_modules {
        let module_name = format!("modules.mod{i:02}");
        let uri = format!("modules/mod{i:02}/src/com/example/Mod{i:02}Service.scala");

        // Each module has its own service class
        let local_fqn = format!("com/example/Mod{i:02}Service#");
        let local_method_fqn = format!("com/example/Mod{i:02}Service#handle().");

        let symbols = vec![
            IntermediateSymbol {
                fqn: local_fqn.clone(),
                display_name: format!("Mod{i:02}Service"),
                kind: SymbolKind::Class,
                properties: 0,
                signature: format!("class Mod{i:02}Service"),
                parents: vec![],
                overridden_symbols: vec![],
                access: Access::Public,
            },
            IntermediateSymbol {
                fqn: local_method_fqn.clone(),
                display_name: "handle".to_string(),
                kind: SymbolKind::Method,
                properties: 0,
                signature: "def handle(): Unit".to_string(),
                parents: vec![],
                overridden_symbols: vec![],
                access: Access::Public,
            },
        ];

        let mut occurrences = vec![
            IntermediateOccurrence {
                symbol: local_fqn,
                role: ReferenceRole::Definition,
                start_line: 3, start_col: 6, end_col: 20,
            },
            IntermediateOccurrence {
                symbol: local_method_fqn.clone(),
                role: ReferenceRole::Definition,
                start_line: 5, start_col: 6, end_col: 12,
            },
        ];

        // RequestContext: 25 modules (i < 25), ~8 refs each = ~200 total
        if i < 25 {
            for j in 0..8 {
                occurrences.push(IntermediateOccurrence {
                    symbol: "com/example/RequestContext#".to_string(),
                    role: ReferenceRole::Reference,
                    start_line: 10 + j, start_col: 4, end_col: 18,
                });
            }
            // Also reference userId (leaf method) — builds call graph for effect plumbing
            for j in 0..6 {
                occurrences.push(IntermediateOccurrence {
                    symbol: "com/example/RequestContext#userId().".to_string(),
                    role: ReferenceRole::Reference,
                    start_line: 20 + j, start_col: 4, end_col: 10,
                });
            }
        }

        // DatabaseDriver: 45 modules (i < 45), ~13 refs each = ~585 total
        if i < 45 {
            for j in 0..13 {
                occurrences.push(IntermediateOccurrence {
                    symbol: "com/example/DatabaseDriver#".to_string(),
                    role: ReferenceRole::Reference,
                    start_line: 30 + j, start_col: 4, end_col: 18,
                });
            }
        }

        // AuthContext: 20 modules (i < 20), ~7 refs each = ~140 total
        if i < 20 {
            for j in 0..7 {
                occurrences.push(IntermediateOccurrence {
                    symbol: "com/example/AuthContext#".to_string(),
                    role: ReferenceRole::Reference,
                    start_line: 50 + j, start_col: 4, end_col: 15,
                });
            }
        }

        // StringUtils: 40 modules (i < 40), ~10 refs each = ~400 total
        if i < 40 {
            for j in 0..10 {
                occurrences.push(IntermediateOccurrence {
                    symbol: "com/example/StringUtils.".to_string(),
                    role: ReferenceRole::Reference,
                    start_line: 60 + j, start_col: 4, end_col: 15,
                });
            }
        }

        // Note: handle() → userId() call graph edges are created by the
        // Reference occurrences above (within handle()'s body scope)

        docs.push(IntermediateDoc {
            uri,
            module_segments: module_name,
            symbols,
            occurrences,
        });
    }

    docs
}

/// Fixture for testing FQN not-found suggestions.
///
/// Two services with overlapping method names:
///   - `LoginService` with `login()` method
///   - `LoginWithMFAService` with `login()` method
///
/// When user passes wrong FQN `LoginService#loginWithMFA().`, the suggestion
/// should find `LoginWithMFAService` by extracting the method name.
#[allow(dead_code)]
pub fn make_fqn_suggestion_docs() -> Vec<IntermediateDoc> {
    vec![IntermediateDoc {
        uri: "modules/auth/src/com/example/Auth.scala".to_string(),
        module_segments: "modules.auth".to_string(),
        symbols: vec![
            IntermediateSymbol {
                fqn: "com/example/LoginService#".to_string(),
                display_name: "LoginService".to_string(),
                kind: SymbolKind::Class,
                properties: 0,
                signature: "class LoginService".to_string(),
                parents: vec![],
                overridden_symbols: vec![],
                access: Access::Public,
            },
            IntermediateSymbol {
                fqn: "com/example/LoginService#login().".to_string(),
                display_name: "login".to_string(),
                kind: SymbolKind::Method,
                properties: 0,
                signature: "def login(): Token".to_string(),
                parents: vec![],
                overridden_symbols: vec![],
                access: Access::Public,
            },
            IntermediateSymbol {
                fqn: "com/example/LoginWithMFAService#".to_string(),
                display_name: "LoginWithMFAService".to_string(),
                kind: SymbolKind::Class,
                properties: 0,
                signature: "class LoginWithMFAService".to_string(),
                parents: vec![],
                overridden_symbols: vec![],
                access: Access::Public,
            },
            IntermediateSymbol {
                fqn: "com/example/LoginWithMFAService#loginWithMFA().".to_string(),
                display_name: "loginWithMFA".to_string(),
                kind: SymbolKind::Method,
                properties: 0,
                signature: "def loginWithMFA(): Token".to_string(),
                parents: vec![],
                overridden_symbols: vec![],
                access: Access::Public,
            },
            IntermediateSymbol {
                fqn: "com/example/LoginWithMFAService#login().".to_string(),
                display_name: "login".to_string(),
                kind: SymbolKind::Method,
                properties: 0,
                signature: "def login(): Token".to_string(),
                parents: vec![],
                overridden_symbols: vec![],
                access: Access::Public,
            },
        ],
        occurrences: vec![
            IntermediateOccurrence {
                symbol: "com/example/LoginService#".to_string(),
                role: ReferenceRole::Definition,
                start_line: 3, start_col: 6, end_col: 18,
            },
            IntermediateOccurrence {
                symbol: "com/example/LoginService#login().".to_string(),
                role: ReferenceRole::Definition,
                start_line: 5, start_col: 6, end_col: 11,
            },
            IntermediateOccurrence {
                symbol: "com/example/LoginWithMFAService#".to_string(),
                role: ReferenceRole::Definition,
                start_line: 10, start_col: 6, end_col: 25,
            },
            IntermediateOccurrence {
                symbol: "com/example/LoginWithMFAService#loginWithMFA().".to_string(),
                role: ReferenceRole::Definition,
                start_line: 12, start_col: 6, end_col: 18,
            },
            IntermediateOccurrence {
                symbol: "com/example/LoginWithMFAService#login().".to_string(),
                role: ReferenceRole::Definition,
                start_line: 15, start_col: 6, end_col: 11,
            },
        ],
    }]
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
