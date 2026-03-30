use crate::model::{Access, ReferenceRole, SymbolKind};

/// A parsed SemanticDB document, before merging into the index.
pub struct IntermediateDoc {
    pub uri: String,
    /// Mill module segment path from discovery (exact, from out/ path).
    pub module_segments: String,
    pub symbols: Vec<IntermediateSymbol>,
    pub occurrences: Vec<IntermediateOccurrence>,
}

pub struct IntermediateSymbol {
    pub fqn: String,
    pub display_name: String,
    pub kind: SymbolKind,
    pub properties: u32,
    pub signature: String,
    pub parents: Vec<String>,
    pub overridden_symbols: Vec<String>,
    pub access: Access,
}

pub struct IntermediateOccurrence {
    pub symbol: String,
    pub role: ReferenceRole,
    pub start_line: u32,
    pub start_col: u32,
    pub end_col: u32,
}
