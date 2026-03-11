use tree_sitter::Node;

use super::{
    collect_symbols, find_first_named_child, push_named_symbol, push_symbol, walk_children,
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
        "function_declaration" => Some(SymbolKind::Function),
        "method_declaration" => Some(SymbolKind::Method),
        "type_declaration" => {
            extract_type_declarations(node, source, depth, sort_order, symbols);
            return;
        }
        "const_declaration" | "var_declaration" => {
            extract_var_declarations(node, source, depth, sort_order, symbols);
            return;
        }
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

fn extract_type_declarations(
    node: &Node,
    source: &str,
    depth: u32,
    sort_order: &mut u32,
    symbols: &mut Vec<SymbolRecord>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "type_spec"
            && let Some(name) = find_name(&child, source)
        {
            let kind = classify_type_spec(&child);
            push_symbol(&child, name, kind, depth, sort_order, symbols);
        }
    }
}

fn classify_type_spec(node: &Node) -> SymbolKind {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "struct_type" => return SymbolKind::Struct,
            "interface_type" => return SymbolKind::Interface,
            _ => {}
        }
    }
    SymbolKind::Type
}

fn extract_var_declarations(
    node: &Node,
    source: &str,
    depth: u32,
    sort_order: &mut u32,
    symbols: &mut Vec<SymbolRecord>,
) {
    let is_const = node.kind() == "const_declaration";
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if (child.kind() == "const_spec" || child.kind() == "var_spec")
            && let Some(name) = find_name(&child, source)
        {
            push_symbol(
                &child,
                name,
                if is_const {
                    SymbolKind::Constant
                } else {
                    SymbolKind::Variable
                },
                depth,
                sort_order,
                symbols,
            );
        }
    }
}

fn find_name(node: &Node, source: &str) -> Option<String> {
    find_first_named_child(node, source, &["identifier", "type_identifier"])
}
