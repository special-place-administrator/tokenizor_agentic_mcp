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
        "function_declaration" => Some(SymbolKind::Function),
        "class_declaration" => Some(SymbolKind::Class),
        "struct_declaration" => Some(SymbolKind::Struct),
        "enum_declaration" => Some(SymbolKind::Enum),
        "protocol_declaration" => Some(SymbolKind::Interface),
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
        if child.kind() == "simple_identifier" || child.kind() == "type_identifier" {
            return Some(child.utf8_text(source.as_bytes()).unwrap_or("").to_string());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
// Note: tree-sitter-swift 0.7.1 requires ABI 15 which is incompatible with
// tree-sitter 0.24 host (max ABI 14). Tests use process_file which returns Failed.

#[cfg(test)]
mod tests {
    use crate::domain::{FileOutcome, LanguageId};
    use crate::parsing::process_file;

    #[test]
    fn test_swift_process_file_returns_failed_gracefully() {
        // Swift grammar crate requires ABI 15 — returns Failed outcome, not a panic
        let source = b"class Foo { func bar() -> Int { return 0 } }";
        let result = process_file("test.swift", source, LanguageId::Swift);
        assert!(
            matches!(result.outcome, FileOutcome::Failed { .. }),
            "Swift without ABI-compatible grammar should return Failed, not panic: {:?}", result.outcome
        );
    }
}
