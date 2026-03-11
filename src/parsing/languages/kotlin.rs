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
    // Note: tree-sitter-kotlin-sg maps both 'class' and 'interface' keywords to
    // 'class_declaration'. We use Class for both since the grammar doesn't distinguish.
    let kind = match node.kind() {
        "function_declaration" => Some(SymbolKind::Function),
        "class_declaration" => Some(SymbolKind::Class),
        "object_declaration" => Some(SymbolKind::Module),
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
        // Kotlin class/interface names are 'type_identifier'; function names are 'simple_identifier'
        if child.kind() == "type_identifier" || child.kind() == "simple_identifier" {
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

    fn parse_kotlin(source: &str) -> Vec<SymbolRecord> {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_kotlin_sg::LANGUAGE.into();
        parser.set_language(&lang).expect("set Kotlin language");
        let tree = parser.parse(source, None).expect("parse Kotlin source");
        extract_symbols(&tree.root_node(), source)
    }

    #[test]
    fn test_kotlin_function_declaration() {
        let source = "fun greet() { println(\"hello\") }";
        let symbols = parse_kotlin(source);
        let func = symbols.iter().find(|s| s.kind == SymbolKind::Function);
        assert!(func.is_some(), "should extract function, got: {:?}", symbols);
        assert_eq!(func.unwrap().name, "greet");
    }

    #[test]
    fn test_kotlin_class_declaration() {
        // tree-sitter-kotlin-sg maps both class and interface to class_declaration
        let source = "class Animal { fun speak() { } }";
        let symbols = parse_kotlin(source);
        // Grammar may report has_error for some constructs but still extracts symbols
        let cls = symbols.iter().find(|s| s.kind == SymbolKind::Class && s.name == "Animal");
        assert!(cls.is_some(), "should extract Animal class, got: {:?}", symbols);
    }

    #[test]
    fn test_kotlin_interface_maps_to_class() {
        // In tree-sitter-kotlin-sg, 'interface' keyword creates class_declaration nodes
        let source = "interface Runnable { fun run() }";
        let symbols = parse_kotlin(source);
        // Interface maps to Class kind in this grammar
        let cls = symbols.iter().find(|s| s.kind == SymbolKind::Class && s.name == "Runnable");
        assert!(cls.is_some(), "should extract Runnable as Class, got: {:?}", symbols);
    }

    #[test]
    fn test_kotlin_process_file_extracts_symbols() {
        // Note: tree-sitter-kotlin-sg may report parse errors on some valid Kotlin
        // but still extracts symbols. Accept both Processed and PartialParse.
        let source = b"class Foo { fun bar() { } }";
        let result = process_file("test.kt", source, LanguageId::Kotlin);
        assert!(
            matches!(result.outcome, FileOutcome::Processed | FileOutcome::PartialParse { .. }),
            "should be Processed or PartialParse, got: {:?}", result.outcome
        );
        assert!(!result.symbols.is_empty(), "should have symbols, got: {:?}", result.symbols);
    }
}
