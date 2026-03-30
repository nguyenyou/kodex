pub mod overview;
pub mod search;
pub mod info;
pub mod calls;
pub mod trace;
pub mod refs;
pub mod noise;



use crate::model::ArchivedKodexIndex;
use crate::query::symbol::suggest_similar;

/// Result of a query command: either found output or not-found output.
pub enum CommandResult {
    /// Symbol/module/file was found — output to print.
    Found(String),
    /// Nothing matched the query — output to print (includes suggestions).
    NotFound(String),
}

impl CommandResult {
    /// Convenience: true if this is a Found result.
    pub fn is_found(&self) -> bool {
        matches!(self, Self::Found(_))
    }

    /// Get the output string regardless of variant.
    pub fn output(&self) -> &str {
        match self {
            Self::Found(s) | Self::NotFound(s) => s,
        }
    }

    /// Symbol not found — includes "Did you mean?" suggestions from fuzzy matching.
    pub fn symbol_not_found(index: &ArchivedKodexIndex, query: &str) -> Self {
        let mut out = format!("Not found: No symbol found matching '{query}'\n");
        let hints = suggest_similar(index, query);
        if !hints.is_empty() {
            out.push_str(&hints);
        }
        Self::NotFound(out)
    }

}

impl std::fmt::Display for CommandResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.output())
    }
}
