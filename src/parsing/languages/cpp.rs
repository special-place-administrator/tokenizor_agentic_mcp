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
        "function_definition" => Some(SymbolKind::Function),
        "struct_specifier" => Some(SymbolKind::Struct),
        "enum_specifier" => Some(SymbolKind::Enum),
        "type_definition" => Some(SymbolKind::Type),
        "class_specifier" => Some(SymbolKind::Class),
        "namespace_definition" => Some(SymbolKind::Module),
        // template_declaration: extract the inner symbol, not the template itself
        "template_declaration" => None,
        _ => None,
    };

    if let Some(symbol_kind) = kind {
        if let Some(name) = find_cpp_name(node, source) {
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

/// Find the name for C++ declarations.
fn find_cpp_name(node: &Node, source: &str) -> Option<String> {
    match node.kind() {
        "function_definition" => find_function_name(node, source),
        "struct_specifier" | "enum_specifier" | "class_specifier" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "type_identifier" || child.kind() == "name" {
                    return Some(child.utf8_text(source.as_bytes()).unwrap_or("").to_string());
                }
            }
            None
        }
        "namespace_definition" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "identifier" || child.kind() == "namespace_identifier" {
                    return Some(child.utf8_text(source.as_bytes()).unwrap_or("").to_string());
                }
            }
            // Anonymous namespace
            None
        }
        "type_definition" => {
            let mut cursor = node.walk();
            let children: Vec<_> = node.children(&mut cursor).collect();
            for child in children.iter().rev() {
                if child.kind() == "type_identifier" || child.kind() == "identifier" {
                    return Some(child.utf8_text(source.as_bytes()).unwrap_or("").to_string());
                }
            }
            None
        }
        _ => None,
    }
}

/// Walk the declarator chain for a function_definition to find the function name.
/// In C++, declarators may contain qualified_identifier (Foo::bar) or plain identifier.
fn find_function_name(node: &Node, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "declarator" | "pointer_declarator" | "reference_declarator" | "function_declarator" => {
                if let Some(name) = extract_declarator_name(&child, source) {
                    return Some(name);
                }
            }
            _ => {}
        }
    }
    None
}

/// Recursively walk a declarator node to extract the function identifier.
/// Handles: identifier, qualified_identifier (Foo::bar -> "bar"), pointer chains.
fn extract_declarator_name(node: &Node, source: &str) -> Option<String> {
    match node.kind() {
        "identifier" => Some(node.utf8_text(source.as_bytes()).unwrap_or("").to_string()),
        "qualified_identifier" => {
            // Take the last identifier segment: "Foo::bar" -> "bar"
            let mut cursor = node.walk();
            let mut last_ident = None;
            for child in node.children(&mut cursor) {
                if child.kind() == "identifier" || child.kind() == "type_identifier" {
                    last_ident = Some(child.utf8_text(source.as_bytes()).unwrap_or("").to_string());
                }
            }
            last_ident
        }
        "destructor_name" => {
            // ~ClassName
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "identifier" || child.kind() == "type_identifier" {
                    let name = child.utf8_text(source.as_bytes()).unwrap_or("");
                    return Some(format!("~{name}"));
                }
            }
            None
        }
        _ => {
            // Recurse through pointer_declarator, function_declarator, reference_declarator etc.
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if let Some(name) = extract_declarator_name(&child, source) {
                    return Some(name);
                }
            }
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{FileOutcome, LanguageId};
    use crate::parsing::process_file;
    use tree_sitter::Parser;

    fn parse_cpp(source: &str) -> Vec<SymbolRecord> {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_cpp::LANGUAGE.into();
        parser.set_language(&lang).expect("set C++ language");
        let tree = parser.parse(source, None).expect("parse C++ source");
        extract_symbols(&tree.root_node(), source)
    }

    #[test]
    fn test_cpp_language_function_definition() {
        let source = "int add(int a, int b) { return a + b; }";
        let symbols = parse_cpp(source);
        let func = symbols.iter().find(|s| s.kind == SymbolKind::Function);
        assert!(func.is_some(), "should extract function, got: {:?}", symbols);
        assert_eq!(func.unwrap().name, "add");
    }

    #[test]
    fn test_cpp_language_struct_specifier() {
        let source = "struct Point { int x; int y; };";
        let symbols = parse_cpp(source);
        let s = symbols.iter().find(|s| s.kind == SymbolKind::Struct);
        assert!(s.is_some(), "should extract struct, got: {:?}", symbols);
        assert_eq!(s.unwrap().name, "Point");
    }

    #[test]
    fn test_cpp_language_enum_specifier() {
        let source = "enum Color { RED, GREEN, BLUE };";
        let symbols = parse_cpp(source);
        let e = symbols.iter().find(|s| s.kind == SymbolKind::Enum);
        assert!(e.is_some(), "should extract enum, got: {:?}", symbols);
        assert_eq!(e.unwrap().name, "Color");
    }

    #[test]
    fn test_cpp_language_class_specifier() {
        let source = "class MyClass { public: int x; };";
        let symbols = parse_cpp(source);
        let c = symbols.iter().find(|s| s.kind == SymbolKind::Class);
        assert!(c.is_some(), "should extract class, got: {:?}", symbols);
        assert_eq!(c.unwrap().name, "MyClass");
    }

    #[test]
    fn test_cpp_language_namespace_definition() {
        let source = "namespace myns { int x = 0; }";
        let symbols = parse_cpp(source);
        let ns = symbols.iter().find(|s| s.kind == SymbolKind::Module);
        assert!(ns.is_some(), "should extract namespace, got: {:?}", symbols);
        assert_eq!(ns.unwrap().name, "myns");
    }

    #[test]
    fn test_cpp_language_template_declaration_inner_class() {
        let source = "template<typename T> class Stack { T data; };";
        let symbols = parse_cpp(source);
        // template_declaration itself doesn't create a symbol; inner class_specifier does
        let c = symbols.iter().find(|s| s.kind == SymbolKind::Class);
        assert!(c.is_some(), "should extract inner class from template, got: {:?}", symbols);
        assert_eq!(c.unwrap().name, "Stack");
    }

    #[test]
    fn test_cpp_language_method_with_qualified_identifier() {
        let source = "void Foo::bar(int x) { }";
        let symbols = parse_cpp(source);
        let func = symbols.iter().find(|s| s.kind == SymbolKind::Function);
        assert!(func.is_some(), "should extract qualified method, got: {:?}", symbols);
        // Should extract just the method name (last segment)
        assert_eq!(func.unwrap().name, "bar");
    }

    #[test]
    fn test_cpp_language_process_file_returns_processed() {
        let source = b"#include <vector>\nclass Foo { public: void bar(); };\nvoid Foo::bar() { }\n";
        let result = process_file("test.cpp", source, LanguageId::Cpp);
        assert_eq!(result.outcome, FileOutcome::Processed, "outcome: {:?}", result.outcome);
        assert!(!result.symbols.is_empty(), "should have symbols");
        let class = result.symbols.iter().find(|s| s.kind == SymbolKind::Class && s.name == "Foo");
        assert!(class.is_some(), "should have Foo class");
    }
}
