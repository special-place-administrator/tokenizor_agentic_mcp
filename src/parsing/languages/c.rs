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
        _ => None,
    };

    if let Some(symbol_kind) = kind {
        if let Some(name) = find_c_name(node, source) {
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

    // Recurse into children, but skip struct/enum bodies to avoid re-extracting nested types
    // as children of the outer specifier (they get their own entry when directly encountered)
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let child_depth = if kind.is_some() { depth + 1 } else { depth };
        walk_node(&child, source, child_depth, sort_order, symbols);
    }
}

/// Find the name for C declarations.
///
/// - `function_definition`: walk the declarator chain to find the function name.
///   C declarator grammar is recursive: declarator -> pointer_declarator -> function_declarator -> identifier.
/// - `struct_specifier` / `enum_specifier`: find the child `type_identifier`.
/// - `type_definition`: find the aliased name (last `type_identifier` child), or fall back to inner specifier name.
fn find_c_name(node: &Node, source: &str) -> Option<String> {
    match node.kind() {
        "function_definition" => find_function_name(node, source),
        "struct_specifier" | "enum_specifier" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "type_identifier" {
                    return Some(child.utf8_text(source.as_bytes()).unwrap_or("").to_string());
                }
            }
            None
        }
        "type_definition" => {
            // typedef struct Foo { ... } Foo_t;
            // The aliased name is the last `type_identifier` that appears directly under type_definition
            // (not inside the inner specifier body). Walk children from right to left to find it.
            let mut cursor = node.walk();
            let children: Vec<_> = node.children(&mut cursor).collect();
            // The typedef alias is the last type_identifier or identifier before the semicolon
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

/// Walk the declarator chain for a function_definition to extract the function name.
/// The chain is: function_definition -> declarator (pointer_declarator*) -> function_declarator -> identifier/qualified_identifier
fn find_function_name(node: &Node, source: &str) -> Option<String> {
    // Find the 'declarator' child of function_definition
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "declarator"
            || child.kind() == "pointer_declarator"
            || child.kind() == "function_declarator"
        {
            if let Some(name) = extract_declarator_name(&child, source) {
                return Some(name);
            }
        }
    }
    None
}

/// Recursively walk a declarator node to find the identifier.
fn extract_declarator_name(node: &Node, source: &str) -> Option<String> {
    match node.kind() {
        "identifier" => Some(node.utf8_text(source.as_bytes()).unwrap_or("").to_string()),
        "qualified_identifier" => {
            // C++ style: take the last identifier segment
            let mut cursor = node.walk();
            let mut last_ident = None;
            for child in node.children(&mut cursor) {
                if child.kind() == "identifier" || child.kind() == "type_identifier" {
                    last_ident = Some(child.utf8_text(source.as_bytes()).unwrap_or("").to_string());
                }
            }
            last_ident
        }
        _ => {
            // Recurse into pointer_declarator, function_declarator, abstract_declarator etc.
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

    fn parse_c(source: &str) -> Vec<SymbolRecord> {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_c::LANGUAGE.into();
        parser.set_language(&lang).expect("set C language");
        let tree = parser.parse(source, None).expect("parse C source");
        extract_symbols(&tree.root_node(), source)
    }

    #[test]
    fn test_c_language_function_definition() {
        let source = "int add(int a, int b) { return a + b; }";
        let symbols = parse_c(source);
        let func = symbols.iter().find(|s| s.kind == SymbolKind::Function);
        assert!(func.is_some(), "should extract function, got: {:?}", symbols);
        assert_eq!(func.unwrap().name, "add");
    }

    #[test]
    fn test_c_language_struct_specifier() {
        let source = "struct Point { int x; int y; };";
        let symbols = parse_c(source);
        let s = symbols.iter().find(|s| s.kind == SymbolKind::Struct);
        assert!(s.is_some(), "should extract struct, got: {:?}", symbols);
        assert_eq!(s.unwrap().name, "Point");
    }

    #[test]
    fn test_c_language_enum_specifier() {
        let source = "enum Color { RED, GREEN, BLUE };";
        let symbols = parse_c(source);
        let e = symbols.iter().find(|s| s.kind == SymbolKind::Enum);
        assert!(e.is_some(), "should extract enum, got: {:?}", symbols);
        assert_eq!(e.unwrap().name, "Color");
    }

    #[test]
    fn test_c_language_typedef() {
        let source = "typedef struct Point { int x; int y; } Point_t;";
        let symbols = parse_c(source);
        let t = symbols.iter().find(|s| s.kind == SymbolKind::Type);
        assert!(t.is_some(), "should extract typedef, got: {:?}", symbols);
        assert_eq!(t.unwrap().name, "Point_t");
    }

    #[test]
    fn test_c_language_pointer_function() {
        let source = "void *malloc_wrapper(size_t size) { return 0; }";
        let symbols = parse_c(source);
        let func = symbols.iter().find(|s| s.kind == SymbolKind::Function);
        assert!(func.is_some(), "should extract pointer-return function, got: {:?}", symbols);
        assert_eq!(func.unwrap().name, "malloc_wrapper");
    }

    #[test]
    fn test_c_language_process_file_returns_processed() {
        let source = b"int main(int argc, char **argv) { return 0; }\nstruct Node { int val; };";
        let result = process_file("test.c", source, LanguageId::C);
        assert_eq!(result.outcome, FileOutcome::Processed, "outcome: {:?}", result.outcome);
        assert!(!result.symbols.is_empty(), "should have symbols");
        let func = result.symbols.iter().find(|s| s.kind == SymbolKind::Function && s.name == "main");
        assert!(func.is_some(), "should have main function");
    }
}
