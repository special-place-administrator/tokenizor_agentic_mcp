use tree_sitter::Node;

use super::{collect_symbols, find_first_named_child, push_named_symbol, walk_children};
use crate::domain::{SymbolKind, SymbolRecord};

pub fn extract_symbols(node: &Node, source: &str) -> Vec<SymbolRecord> {
    collect_symbols(node, source, walk_node)
}

fn walk_node(
    node: &Node,
    source: &str,
    depth: u32,
    sort_order: &mut u32,
    symbols: &mut Vec<SymbolRecord>,
) {
    let kind = match node.kind() {
        "function_signature" => Some(SymbolKind::Function),
        "class_definition" => Some(SymbolKind::Class),
        "enum_declaration" => Some(SymbolKind::Enum),
        "method_signature" => Some(SymbolKind::Method),
        _ => None,
    };

    push_named_symbol(
        node,
        source,
        depth,
        sort_order,
        symbols,
        kind,
        |node, source, _| find_name(node, source),
    );
    walk_children(node, source, depth, sort_order, symbols, kind, walk_node);
}

fn find_name(node: &Node, source: &str) -> Option<String> {
    find_first_named_child(node, source, &["identifier", "type_identifier"])
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{FileOutcome, LanguageId, SymbolKind};
    use crate::parsing::process_file;
    use tree_sitter::Parser;

    fn parse_dart(source: &str) -> Vec<SymbolRecord> {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_dart::language();
        parser.set_language(&lang).expect("set Dart language");
        let tree = parser.parse(source, None).expect("parse Dart source");
        extract_symbols(&tree.root_node(), source)
    }

    #[test]
    fn test_dart_class_definition() {
        let source = "class Animal { void speak() {} }";
        let symbols = parse_dart(source);
        let cls = symbols.iter().find(|s| s.kind == SymbolKind::Class);
        assert!(cls.is_some(), "should extract class, got: {:?}", symbols);
        assert_eq!(cls.unwrap().name, "Animal");
    }

    #[test]
    fn test_dart_enum_declaration() {
        let source = "enum Color { red, green, blue }";
        let symbols = parse_dart(source);
        let e = symbols.iter().find(|s| s.kind == SymbolKind::Enum);
        assert!(e.is_some(), "should extract enum, got: {:?}", symbols);
        assert_eq!(e.unwrap().name, "Color");
    }

    #[test]
    fn test_dart_process_file_returns_processed() {
        let source = b"class Foo { void bar() {} }";
        let result = process_file("test.dart", source, LanguageId::Dart);
        assert_eq!(
            result.outcome,
            FileOutcome::Processed,
            "outcome: {:?}",
            result.outcome
        );
        assert!(!result.symbols.is_empty(), "should have symbols");
    }
}
