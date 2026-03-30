pub mod commands;
pub mod filter;
pub mod format;
pub mod graph;
pub mod symbol;

use crate::model::{ArchivedFileEntry, ArchivedKodexIndex, ArchivedSymbol};

/// Get the string for a `StringId` from the archived index.
#[inline]
pub fn s(index: &ArchivedKodexIndex, string_id: impl Into<u32>) -> &str {
    &index.strings[string_id.into() as usize]
}

/// Get a symbol by ID with debug bounds checking.
#[inline]
pub fn sym(index: &ArchivedKodexIndex, symbol_id: impl Into<u32>) -> &ArchivedSymbol {
    let id = symbol_id.into() as usize;
    debug_assert!(id < index.symbols.len(), "symbol ID {id} out of bounds (len {})", index.symbols.len());
    &index.symbols[id]
}

/// Get a file entry by ID with debug bounds checking.
#[inline]
pub fn file_entry(index: &ArchivedKodexIndex, file_id: impl Into<u32>) -> &ArchivedFileEntry {
    let id = file_id.into() as usize;
    debug_assert!(id < index.files.len(), "file ID {id} out of bounds (len {})", index.files.len());
    &index.files[id]
}
