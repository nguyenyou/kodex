use anyhow::Result;
use prost::Message;
use rayon::prelude::*;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::ingest::provider::DiscoveredFile;
use crate::ingest::printer;
use crate::ingest::types::{IntermediateDoc, IntermediateOccurrence, IntermediateSymbol};
use crate::model::{proto, Access, ReferenceRole, SymbolKind};

/// Load and convert all .semanticdb files in parallel.
/// Each file carries its module_segments from discovery.
pub fn load_all(files: &[DiscoveredFile]) -> Result<Vec<IntermediateDoc>> {
    let parse_errors = AtomicUsize::new(0);
    let docs: Vec<IntermediateDoc> = files
        .par_iter()
        .filter_map(|df| {
            let Ok(bytes) = fs::read(&df.path) else {
                parse_errors.fetch_add(1, Ordering::Relaxed);
                return None;
            };
            let Ok(text_docs) = proto::TextDocuments::decode(bytes.as_slice()) else {
                parse_errors.fetch_add(1, Ordering::Relaxed);
                return None;
            };
            let module_segments = df.module_segments.clone();
            Some(
                text_docs
                    .documents
                    .into_iter()
                    .filter_map(|doc| convert_document(doc, &module_segments))
                    .collect::<Vec<_>>(),
            )
        })
        .flatten()
        .collect();
    let err_count = parse_errors.load(Ordering::Relaxed);
    if err_count > 0 {
        eprintln!("Warning: {err_count} .semanticdb file(s) failed to read or parse");
    }
    Ok(docs)
}

#[allow(clippy::needless_pass_by_value)] // consumed by into_iter()
fn convert_document(
    mut doc: proto::TextDocument,
    module_segments: &str,
) -> Option<IntermediateDoc> {
    if doc.uri.is_empty() {
        return None;
    }

    // Move occurrences out before borrowing doc.symbols for the symtab
    let proto_occurrences = std::mem::take(&mut doc.occurrences);

    // Build a local symtab for the printer (symbol FQN → SymbolInformation)
    let symtab: rustc_hash::FxHashMap<&str, &proto::SymbolInformation> = doc
        .symbols
        .iter()
        .map(|s| (s.symbol.as_str(), s))
        .collect();

    let symbols: Vec<IntermediateSymbol> = doc
        .symbols
        .iter()
        .map(|info| {
            let parents = extract_parent_symbols(info.signature.as_ref());
            let sig = printer::print_info(info, &symtab);

            IntermediateSymbol {
                fqn: info.symbol.clone(),
                display_name: info.display_name.clone(),
                kind: SymbolKind::from_proto(info.kind),
                properties: info.properties as u32,
                signature: sig,
                parents,
                overridden_symbols: info.overridden_symbols.clone(),
                access: Access::from_proto(info.access.as_ref()),
            }
        })
        .collect();

    let occurrences: Vec<IntermediateOccurrence> = proto_occurrences
        .into_iter()
        .map(|occ| {
            let (sl, sc, ec) = match &occ.range {
                Some(r) => (
                    r.start_line as u32,
                    r.start_character as u32,
                    r.end_character as u32,
                ),
                None => (0, 0, 0),
            };
            IntermediateOccurrence {
                symbol: occ.symbol,
                role: ReferenceRole::from_proto(occ.role),
                start_line: sl,
                start_col: sc,
                end_col: ec,
            }
        })
        .collect();

    Some(IntermediateDoc {
        uri: std::mem::take(&mut doc.uri),
        module_segments: module_segments.to_string(),
        symbols,
        occurrences,
    })
}

/// Extract parent type symbols from a ClassSignature.
fn extract_parent_symbols(sig: Option<&proto::Signature>) -> Vec<String> {
    let Some(sig) = sig else { return vec![] };
    let Some(ref sv) = sig.sealed_value else {
        return vec![];
    };
    match sv {
        proto::signature::SealedValue::ClassSignature(cs) => cs
            .parents
            .iter()
            .filter_map(extract_type_symbol_from_type)
            .collect(),
        _ => vec![],
    }
}

fn extract_type_symbol_from_type(tpe: &proto::Type) -> Option<String> {
    let sv = tpe.sealed_value.as_ref()?;
    match sv {
        proto::r#type::SealedValue::TypeRef(tr) => {
            if tr.symbol.is_empty() {
                None
            } else {
                Some(tr.symbol.clone())
            }
        }
        proto::r#type::SealedValue::SingleType(st) => {
            if st.symbol.is_empty() {
                None
            } else {
                Some(st.symbol.clone())
            }
        }
        proto::r#type::SealedValue::ThisType(tt) => {
            if tt.symbol.is_empty() {
                None
            } else {
                Some(tt.symbol.clone())
            }
        }
        proto::r#type::SealedValue::AnnotatedType(at) => at
            .tpe
            .as_ref()
            .and_then(|t| extract_type_symbol_from_type(t)),
        _ => None,
    }
}
