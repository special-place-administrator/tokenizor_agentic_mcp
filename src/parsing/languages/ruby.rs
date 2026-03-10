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
        "method" => Some(SymbolKind::Function),
        "singleton_method" => Some(SymbolKind::Method),
        "class" => Some(SymbolKind::Class),
        "module" => Some(SymbolKind::Module),
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
        if child.kind() == "identifier" || child.kind() == "constant" {
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

    fn parse_ruby(source: &str) -> Vec<SymbolRecord> {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_ruby::LANGUAGE.into();
        parser.set_language(&lang).expect("set Ruby language");
        let tree = parser.parse(source, None).expect("parse Ruby source");
        extract_symbols(&tree.root_node(), source)
    }

    #[test]
    fn test_ruby_method_definition() {
        let source = "def greet\n  puts 'hello'\nend";
        let symbols = parse_ruby(source);
        let func = symbols.iter().find(|s| s.kind == SymbolKind::Function);
        assert!(func.is_some(), "should extract method, got: {:?}", symbols);
        assert_eq!(func.unwrap().name, "greet");
    }

    #[test]
    fn test_ruby_class_definition() {
        let source = "class Animal\n  def speak\n  end\nend";
        let symbols = parse_ruby(source);
        let cls = symbols.iter().find(|s| s.kind == SymbolKind::Class);
        assert!(cls.is_some(), "should extract class, got: {:?}", symbols);
        assert_eq!(cls.unwrap().name, "Animal");
    }

    #[test]
    fn test_ruby_module_definition() {
        let source = "module Helpers\n  def help\n  end\nend";
        let symbols = parse_ruby(source);
        let m = symbols.iter().find(|s| s.kind == SymbolKind::Module);
        assert!(m.is_some(), "should extract module, got: {:?}", symbols);
        assert_eq!(m.unwrap().name, "Helpers");
    }

    #[test]
    fn test_ruby_process_file_returns_processed() {
        let source = b"class Foo\n  def bar\n  end\nend";
        let result = process_file("test.rb", source, LanguageId::Ruby);
        assert_eq!(result.outcome, FileOutcome::Processed, "outcome: {:?}", result.outcome);
        assert!(!result.symbols.is_empty(), "should have symbols");
    }
}
