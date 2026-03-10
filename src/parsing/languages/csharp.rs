use tree_sitter::Node;

use crate::domain::{SymbolKind, SymbolRecord};

pub fn extract_symbols(node: &Node, source: &str) -> Vec<SymbolRecord> {
    let mut symbols = Vec::new();
    let mut sort_order = 0u32;
    walk_node(node, source, 0, &mut sort_order, &mut symbols);
    symbols
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

    if let Some(symbol_kind) = kind {
        if let Some(name) = find_name(node, source) {
            symbols.push(SymbolRecord {
                name,
                kind: symbol_kind,
                depth,
                sort_order: *sort_order,
                byte_range: (node.start_byte() as u32, node.end_byte() as u32),
                line_range: (
                    node.start_position().row as u32,
                    node.end_position().row as u32,
                ),
            });
            *sort_order += 1;
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let child_depth = if kind.is_some() { depth + 1 } else { depth };
        walk_node(&child, source, child_depth, sort_order, symbols);
    }
}

fn find_name(node: &Node, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            return Some(child.utf8_text(source.as_bytes()).unwrap_or("").to_string());
        }
    }
    None
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
        assert!(iface.is_some(), "should extract interface, got: {:?}", symbols);
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
        assert_eq!(result.outcome, FileOutcome::Processed, "outcome: {:?}", result.outcome);
        assert!(!result.symbols.is_empty(), "should have symbols");
    }
}
