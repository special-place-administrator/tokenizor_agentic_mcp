mod c;
mod cpp;
mod csharp;
mod dart;
mod elixir;
mod go;
mod java;
mod javascript;
mod kotlin;
mod perl;
mod php;
mod python;
mod ruby;
mod rust;
mod swift;
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
        LanguageId::CSharp => csharp::extract_symbols(node, source),
        LanguageId::Ruby => ruby::extract_symbols(node, source),
        LanguageId::Php => php::extract_symbols(node, source),
        LanguageId::Swift => swift::extract_symbols(node, source),
        LanguageId::Kotlin => kotlin::extract_symbols(node, source),
        LanguageId::Dart => dart::extract_symbols(node, source),
        LanguageId::Perl => perl::extract_symbols(node, source),
        LanguageId::Elixir => elixir::extract_symbols(node, source),
    }
}
