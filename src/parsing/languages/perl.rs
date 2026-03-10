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
        "subroutine_declaration_statement" => Some(SymbolKind::Function),
        "package_statement" => Some(SymbolKind::Module),
        _ => None,
    };

    if let Some(symbol_kind) = kind {
        if let Some(name) = find_name(node, source, symbol_kind) {
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

fn find_name(node: &Node, source: &str, kind: SymbolKind) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        // For subroutine_declaration_statement, look for identifier after 'sub'
        // For package_statement, look for package_name or identifier
        if child.kind() == "name" || child.kind() == "identifier" || child.kind() == "package_name" {
            return Some(child.utf8_text(source.as_bytes()).unwrap_or("").to_string());
        }
        // Some versions use 'subroutine_name' node
        if kind == SymbolKind::Function && child.kind() == "subroutine_name" {
            return Some(child.utf8_text(source.as_bytes()).unwrap_or("").to_string());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
// Note: Available Perl grammar crates require ABI 15 or 26 — incompatible with
// tree-sitter 0.24 host (max ABI 14). Tests verify graceful failure only.

#[cfg(test)]
mod tests {
    use crate::domain::{FileOutcome, LanguageId};
    use crate::parsing::process_file;

    #[test]
    fn test_perl_process_file_returns_failed_gracefully() {
        // Perl grammar crates require incompatible ABI — returns Failed outcome, not a panic
        let source = b"sub greet { print \"hello\\n\"; }";
        let result = process_file("test.pl", source, LanguageId::Perl);
        assert!(
            matches!(result.outcome, FileOutcome::Failed { .. }),
            "Perl without ABI-compatible grammar should return Failed, not panic: {:?}", result.outcome
        );
    }
}
