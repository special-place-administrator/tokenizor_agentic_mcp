use tree_sitter::Node;

use super::{
    DocCommentSpec, collect_symbols, find_first_named_child, push_named_symbol, walk_children,
};

pub(super) const DOC_SPEC: DocCommentSpec = DocCommentSpec {
    comment_node_types: &["comment"],
    doc_prefixes: Some(&["/**"]),
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
        "function_definition" => Some(SymbolKind::Function),
        "class_declaration" => Some(SymbolKind::Class),
        "interface_declaration" => Some(SymbolKind::Interface),
        "trait_declaration" => Some(SymbolKind::Trait),
        "method_declaration" => Some(SymbolKind::Method),
        "enum_declaration" => Some(SymbolKind::Enum),
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
    find_first_named_child(node, source, &["name", "identifier"])
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::domain::{FileOutcome, LanguageId, SymbolKind};
    use crate::parsing::process_file;

    #[test]
    fn test_php_process_file_extracts_class_and_method() {
        let source = b"<?php class Foo { public function bar() {} }";
        let result = process_file("test.php", source, LanguageId::Php);
        assert!(
            matches!(
                result.outcome,
                FileOutcome::Processed | FileOutcome::PartialParse { .. }
            ),
            "PHP should parse successfully: {:?}",
            result.outcome
        );
        assert!(
            result
                .symbols
                .iter()
                .any(|s| s.kind == SymbolKind::Class && s.name == "Foo"),
            "should extract Foo class, symbols: {:?}",
            result.symbols
        );
    }
}
