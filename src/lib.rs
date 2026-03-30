// Enforce high code quality standards
#![warn(clippy::pedantic)]
// Allow these pedantic lints that conflict with the codebase style
#![allow(
    clippy::cast_possible_truncation,  // u32 ↔ usize conversions are intentional throughout
    clippy::cast_lossless,             // same: u32 as usize is fine
    clippy::cast_sign_loss,            // protobuf i32 → u32 is safe for our property bitmasks
    clippy::cast_precision_loss,       // usize as f64 for byte-size display is fine
    clippy::module_name_repetitions,   // e.g. MillProvider in mill.rs is clear
    clippy::missing_errors_doc,        // CLI tool, not a public library API
    clippy::missing_panics_doc,        // same
    clippy::must_use_candidate,        // too noisy for internal helpers
    clippy::similar_names,             // fqn/fid/sid naming is intentional
    clippy::too_many_lines,            // build_index is inherently long
    clippy::too_many_arguments,        // command functions take many params by design
    clippy::struct_excessive_bools,    // FileEntry has is_test/is_generated, that's fine
    clippy::wildcard_imports,          // used for model::* in merge.rs
    clippy::doc_markdown,              // too noisy for SemanticDB/Mill domain terms
    clippy::items_after_statements,    // phase!() macro pattern in build_index
    clippy::unreadable_literal,        // property bitmask constants are clearer as hex
)]

pub mod hash;
pub mod index;
pub mod ingest;
pub mod model;
pub mod query;
pub mod symbol;
