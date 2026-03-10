mod c;
mod cpp;
mod go;
mod java;
mod javascript;
mod python;
mod rust;
mod typescript;

use tree_sitter::Node;

use crate::domain::{LanguageId, SymbolRecord};

pub fn extract_symbols(node: &Node, source: &str, language: &LanguageId) -> Vec<SymbolRecord> {
    match language {
        LanguageId::Rust => rust::extract_symbols(node, source),
        LanguageId::Python => python::extract_symbols(node, source),
        LanguageId::JavaScript => javascript::extract_symbols(node, source),
        LanguageId::TypeScript => typescript::extract_symbols(node, source),
        LanguageId::Go => go::extract_symbols(node, source),
        LanguageId::Java => java::extract_symbols(node, source),
        LanguageId::C => c::extract_symbols(node, source),
        LanguageId::Cpp => cpp::extract_symbols(node, source),
        _ => vec![],
    }
}
