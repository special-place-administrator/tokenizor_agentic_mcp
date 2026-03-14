use tree_sitter::Node;

use super::{
    DocCommentSpec, collect_symbols, find_first_named_child, push_named_symbol, walk_children,
};

pub(super) const DOC_SPEC: DocCommentSpec = DocCommentSpec {
    comment_node_types: &["comment"],
    doc_prefixes: Some(&["///"]),
    custom_doc_check: None,
};
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
        "class_declaration" => Some(SymbolKind::Class),
        "struct_declaration" => Some(SymbolKind::Struct),
        "interface_declaration" => Some(SymbolKind::Interface),
        "enum_declaration" => Some(SymbolKind::Enum),
        "method_declaration" => Some(SymbolKind::Method),
        "constructor_declaration" => Some(SymbolKind::Function),
        "namespace_declaration" => Some(SymbolKind::Module),
        "property_declaration" => Some(SymbolKind::Variable),
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
        &DOC_SPEC,
    );
    walk_children(node, source, depth, sort_order, symbols, kind, walk_node);
}

fn find_name(node: &Node, source: &str) -> Option<String> {
    find_first_named_child(node, source, &["identifier"])
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

    fn parse_csharp(source: &str) -> Vec<SymbolRecord> {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_c_sharp::LANGUAGE.into();
        parser.set_language(&lang).expect("set C# language");
        let tree = parser.parse(source, None).expect("parse C# source");
        extract_symbols(&tree.root_node(), source)
    }

    #[test]
    fn test_csharp_class_declaration() {
        let source = "public class Greeter { }";
        let symbols = parse_csharp(source);
        let cls = symbols.iter().find(|s| s.kind == SymbolKind::Class);
        assert!(cls.is_some(), "should extract class, got: {:?}", symbols);
        assert_eq!(cls.unwrap().name, "Greeter");
    }

    #[test]
    fn test_csharp_interface_declaration() {
        let source = "public interface IRunnable { void Run(); }";
        let symbols = parse_csharp(source);
        let iface = symbols.iter().find(|s| s.kind == SymbolKind::Interface);
        assert!(
            iface.is_some(),
            "should extract interface, got: {:?}",
            symbols
        );
        assert_eq!(iface.unwrap().name, "IRunnable");
    }

    #[test]
    fn test_csharp_enum_declaration() {
        let source = "public enum Color { Red, Green, Blue }";
        let symbols = parse_csharp(source);
        let e = symbols.iter().find(|s| s.kind == SymbolKind::Enum);
        assert!(e.is_some(), "should extract enum, got: {:?}", symbols);
        assert_eq!(e.unwrap().name, "Color");
    }

    #[test]
    fn test_csharp_process_file_returns_processed() {
        let source = b"public class Foo { public void Bar() { } }";
        let result = process_file("test.cs", source, LanguageId::CSharp);
        assert_eq!(
            result.outcome,
            FileOutcome::Processed,
            "outcome: {:?}",
            result.outcome
        );
        assert!(!result.symbols.is_empty(), "should have symbols");
    }
}
