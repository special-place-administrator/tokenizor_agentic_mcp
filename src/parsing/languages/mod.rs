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

use crate::domain::{LanguageId, SymbolKind, SymbolRecord};

type WalkNodeFn = fn(&Node, &str, u32, &mut u32, &mut Vec<SymbolRecord>);

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

pub(super) fn collect_symbols(node: &Node, source: &str, walk: WalkNodeFn) -> Vec<SymbolRecord> {
    let mut symbols = Vec::new();
    let mut sort_order = 0u32;
    walk(node, source, 0, &mut sort_order, &mut symbols);
    symbols
}

pub(super) fn push_symbol(
    node: &Node,
    name: String,
    kind: SymbolKind,
    depth: u32,
    sort_order: &mut u32,
    symbols: &mut Vec<SymbolRecord>,
) {
    symbols.push(SymbolRecord {
        name,
        kind,
        depth,
        sort_order: *sort_order,
        byte_range: (node.start_byte() as u32, node.end_byte() as u32),
        line_range: (
            node.start_position().row as u32,
            node.end_position().row as u32,
        ),
        doc_byte_range: None,
    });
    *sort_order += 1;
}

pub(super) fn push_named_symbol<F>(
    node: &Node,
    source: &str,
    depth: u32,
    sort_order: &mut u32,
    symbols: &mut Vec<SymbolRecord>,
    kind: Option<SymbolKind>,
    find_name: F,
) -> bool
where
    F: FnOnce(&Node, &str, SymbolKind) -> Option<String>,
{
    let Some(symbol_kind) = kind else {
        return false;
    };
    let Some(name) = find_name(node, source, symbol_kind) else {
        return false;
    };
    push_symbol(node, name, symbol_kind, depth, sort_order, symbols);
    true
}

pub(super) fn next_child_depth(kind: Option<SymbolKind>, depth: u32) -> u32 {
    if kind.is_some() { depth + 1 } else { depth }
}

pub(super) fn walk_children(
    node: &Node,
    source: &str,
    depth: u32,
    sort_order: &mut u32,
    symbols: &mut Vec<SymbolRecord>,
    kind: Option<SymbolKind>,
    walk: WalkNodeFn,
) {
    let child_depth = next_child_depth(kind, depth);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(&child, source, child_depth, sort_order, symbols);
    }
}

pub(super) fn find_first_named_child(
    node: &Node,
    source: &str,
    child_kinds: &[&str],
) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child_kinds.iter().any(|kind| child.kind() == *kind) {
            return Some(child.utf8_text(source.as_bytes()).unwrap_or("").to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use tree_sitter::Parser;

    use super::*;
    use crate::domain::SymbolKind;

    fn parse_rust(source: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&lang).expect("set rust grammar");
        parser.parse(source, None).expect("parse rust source")
    }

    fn first_named_descendant<'a>(node: &'a Node<'a>, kind: &str) -> Node<'a> {
        let mut cursor = node.walk();
        node.children(&mut cursor)
            .find(|child| child.kind() == kind)
            .expect("expected descendant")
    }

    #[test]
    fn test_push_named_symbol_records_metadata_and_advances_sort_order() {
        let source = "fn hello() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let function = first_named_descendant(&root, "function_item");

        let mut symbols = Vec::new();
        let mut sort_order = 0u32;

        let pushed = push_named_symbol(
            &function,
            source,
            2,
            &mut sort_order,
            &mut symbols,
            Some(SymbolKind::Function),
            |node, source, _kind| find_first_named_child(node, source, &["identifier"]),
        );

        assert!(pushed);
        assert_eq!(sort_order, 1);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "hello");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
        assert_eq!(symbols[0].depth, 2);
        assert_eq!(symbols[0].sort_order, 0);
        assert_eq!(symbols[0].line_range, (0, 0));
        assert_eq!(symbols[0].byte_range, (0, function.end_byte() as u32));
    }

    #[test]
    fn test_next_child_depth_only_increments_for_symbol_parents() {
        assert_eq!(next_child_depth(Some(SymbolKind::Function), 3), 4);
        assert_eq!(next_child_depth(None, 3), 3);
    }

    #[test]
    fn test_find_first_named_child_returns_first_matching_kind() {
        let source = "struct Example;\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let item = first_named_descendant(&root, "struct_item");

        let found = find_first_named_child(&item, source, &["type_identifier", "identifier"]);

        assert_eq!(found.as_deref(), Some("Example"));
    }
}
