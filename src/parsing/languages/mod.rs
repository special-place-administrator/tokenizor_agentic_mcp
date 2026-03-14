mod c;
mod cpp;
mod csharp;
mod dart;
mod elixir;
mod go;
mod java;
mod javascript;
mod kotlin;
mod perl;
mod php;
mod python;
mod ruby;
mod rust;
mod swift;
mod typescript;

use tree_sitter::Node;

use crate::domain::{LanguageId, SymbolKind, SymbolRecord};

/// Per-language configuration for detecting doc comments.
pub(super) struct DocCommentSpec {
    /// Tree-sitter node type names that could be doc comments.
    pub comment_node_types: &'static [&'static str],
    /// Text prefixes that distinguish doc from regular comments.
    /// `None` = all comments of matching node types are doc comments.
    pub doc_prefixes: Option<&'static [&'static str]>,
    /// Optional custom check for non-comment doc patterns (e.g., Elixir `@doc`).
    pub custom_doc_check: Option<fn(&Node, &str) -> bool>,
}

/// Spec for languages with no doc comment detection (Python, Dart).
pub(super) const NO_DOC_SPEC: DocCommentSpec = DocCommentSpec {
    comment_node_types: &[],
    doc_prefixes: None,
    custom_doc_check: None,
};

/// Walk backward through `node`'s preceding siblings to find attached doc comments.
/// Returns `Some((earliest_start_byte, latest_end_byte))` or `None`.
pub(super) fn scan_doc_range(
    node: &Node,
    source: &str,
    spec: &DocCommentSpec,
) -> Option<(u32, u32)> {
    if spec.comment_node_types.is_empty() && spec.custom_doc_check.is_none() {
        return None;
    }

    let mut earliest_start: Option<u32> = None;
    let mut latest_end: Option<u32> = None;
    let mut next_start_row = node.start_position().row;
    let mut sibling_opt = node.prev_sibling();

    while let Some(sibling) = sibling_opt {
        let is_comment_node = spec.comment_node_types.contains(&sibling.kind());
        let is_custom_doc = spec
            .custom_doc_check
            .map_or(false, |check| check(&sibling, source));

        if !is_comment_node && !is_custom_doc {
            break;
        }

        // Blank line check: gap > 1 line means detached.
        // Use start_position().row because some grammars include trailing
        // newlines in the node span, inflating end_position().row.
        let sibling_start_row = sibling.start_position().row;
        if next_start_row > sibling_start_row + 1 {
            break;
        }

        // If doc_prefixes is set, check the text prefix.
        if is_comment_node {
            if let Some(prefixes) = spec.doc_prefixes {
                let text_start = sibling.start_byte();
                let text_end = sibling.end_byte();
                if text_end <= source.len() {
                    let text = &source[text_start..text_end];
                    let trimmed = text.trim_start();
                    if !prefixes.iter().any(|p| trimmed.starts_with(p)) {
                        break;
                    }
                }
            }
        }

        let sb = sibling.start_byte() as u32;
        let eb = sibling.end_byte() as u32;
        earliest_start = Some(earliest_start.map_or(sb, |prev| prev.min(sb)));
        if latest_end.is_none() {
            latest_end = Some(eb);
        }

        next_start_row = sibling.start_position().row;
        sibling_opt = sibling.prev_sibling();
    }

    earliest_start.map(|start| (start, latest_end.unwrap()))
}

type WalkNodeFn = fn(&Node, &str, u32, &mut u32, &mut Vec<SymbolRecord>);

pub fn extract_symbols(node: &Node, source: &str, language: &LanguageId) -> Vec<SymbolRecord> {
    match language {
        LanguageId::Rust => rust::extract_symbols(node, source),
        LanguageId::Python => python::extract_symbols(node, source),
        LanguageId::JavaScript => javascript::extract_symbols(node, source),
        LanguageId::TypeScript => typescript::extract_symbols(node, source),
        LanguageId::Go => go::extract_symbols(node, source),
        LanguageId::Java => java::extract_symbols(node, source),
        LanguageId::C => c::extract_symbols(node, source),
        LanguageId::Cpp => cpp::extract_symbols(node, source),
        LanguageId::CSharp => csharp::extract_symbols(node, source),
        LanguageId::Ruby => ruby::extract_symbols(node, source),
        LanguageId::Php => php::extract_symbols(node, source),
        LanguageId::Swift => swift::extract_symbols(node, source),
        LanguageId::Kotlin => kotlin::extract_symbols(node, source),
        LanguageId::Dart => dart::extract_symbols(node, source),
        LanguageId::Perl => perl::extract_symbols(node, source),
        LanguageId::Elixir => elixir::extract_symbols(node, source),
    }
}

pub(super) fn collect_symbols(node: &Node, source: &str, walk: WalkNodeFn) -> Vec<SymbolRecord> {
    let mut symbols = Vec::new();
    let mut sort_order = 0u32;
    walk(node, source, 0, &mut sort_order, &mut symbols);
    symbols
}

pub(super) fn push_symbol(
    node: &Node,
    source: &str,
    name: String,
    kind: SymbolKind,
    depth: u32,
    sort_order: &mut u32,
    symbols: &mut Vec<SymbolRecord>,
    doc_spec: &DocCommentSpec,
) {
    let doc_byte_range = scan_doc_range(node, source, doc_spec);
    symbols.push(SymbolRecord {
        name,
        kind,
        depth,
        sort_order: *sort_order,
        byte_range: (node.start_byte() as u32, node.end_byte() as u32),
        line_range: (
            node.start_position().row as u32,
            node.end_position().row as u32,
        ),
        doc_byte_range,
    });
    *sort_order += 1;
}

pub(super) fn push_named_symbol<F>(
    node: &Node,
    source: &str,
    depth: u32,
    sort_order: &mut u32,
    symbols: &mut Vec<SymbolRecord>,
    kind: Option<SymbolKind>,
    find_name: F,
    doc_spec: &DocCommentSpec,
) -> bool
where
    F: FnOnce(&Node, &str, SymbolKind) -> Option<String>,
{
    let Some(symbol_kind) = kind else {
        return false;
    };
    let Some(name) = find_name(node, source, symbol_kind) else {
        return false;
    };
    push_symbol(
        node,
        source,
        name,
        symbol_kind,
        depth,
        sort_order,
        symbols,
        doc_spec,
    );
    true
}

pub(super) fn next_child_depth(kind: Option<SymbolKind>, depth: u32) -> u32 {
    if kind.is_some() { depth + 1 } else { depth }
}

pub(super) fn walk_children(
    node: &Node,
    source: &str,
    depth: u32,
    sort_order: &mut u32,
    symbols: &mut Vec<SymbolRecord>,
    kind: Option<SymbolKind>,
    walk: WalkNodeFn,
) {
    let child_depth = next_child_depth(kind, depth);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(&child, source, child_depth, sort_order, symbols);
    }
}

pub(super) fn find_first_named_child(
    node: &Node,
    source: &str,
    child_kinds: &[&str],
) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child_kinds.iter().any(|kind| child.kind() == *kind) {
            return Some(child.utf8_text(source.as_bytes()).unwrap_or("").to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use tree_sitter::Parser;

    use super::*;
    use crate::domain::SymbolKind;

    fn parse_rust(source: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&lang).expect("set rust grammar");
        parser.parse(source, None).expect("parse rust source")
    }

    fn first_named_descendant<'a>(node: &'a Node<'a>, kind: &str) -> Node<'a> {
        let mut cursor = node.walk();
        node.children(&mut cursor)
            .find(|child| child.kind() == kind)
            .expect("expected descendant")
    }

    #[test]
    fn test_push_named_symbol_records_metadata_and_advances_sort_order() {
        let source = "fn hello() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let function = first_named_descendant(&root, "function_item");

        let mut symbols = Vec::new();
        let mut sort_order = 0u32;

        let pushed = push_named_symbol(
            &function,
            source,
            2,
            &mut sort_order,
            &mut symbols,
            Some(SymbolKind::Function),
            |node, source, _kind| find_first_named_child(node, source, &["identifier"]),
            &NO_DOC_SPEC,
        );

        assert!(pushed);
        assert_eq!(sort_order, 1);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "hello");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
        assert_eq!(symbols[0].depth, 2);
        assert_eq!(symbols[0].sort_order, 0);
        assert_eq!(symbols[0].line_range, (0, 0));
        assert_eq!(symbols[0].byte_range, (0, function.end_byte() as u32));
    }

    #[test]
    fn test_next_child_depth_only_increments_for_symbol_parents() {
        assert_eq!(next_child_depth(Some(SymbolKind::Function), 3), 4);
        assert_eq!(next_child_depth(None, 3), 3);
    }

    #[test]
    fn test_find_first_named_child_returns_first_matching_kind() {
        let source = "struct Example;\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let item = first_named_descendant(&root, "struct_item");

        let found = find_first_named_child(&item, source, &["type_identifier", "identifier"]);

        assert_eq!(found.as_deref(), Some("Example"));
    }

    // --- scan_doc_range tests ---

    fn parse_go(source: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_go::LANGUAGE.into();
        parser.set_language(&lang).expect("set go grammar");
        parser.parse(source, None).expect("parse go source")
    }

    const RUST_DOC_SPEC: DocCommentSpec = DocCommentSpec {
        comment_node_types: &["line_comment", "block_comment"],
        doc_prefixes: Some(&["///", "//!", "/**", "/*!"]),
        custom_doc_check: None,
    };

    const GO_DOC_SPEC: DocCommentSpec = DocCommentSpec {
        comment_node_types: &["comment"],
        doc_prefixes: None,
        custom_doc_check: None,
    };

    #[test]
    fn test_scan_doc_range_rust_doc_comments() {
        let source = "/// Doc line 1\n/// Doc line 2\npub fn foo() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let function = first_named_descendant(&root, "function_item");

        let range = scan_doc_range(&function, source, &RUST_DOC_SPEC);

        assert!(range.is_some(), "expected doc range for /// comments");
        let (start, end) = range.unwrap();
        let doc_text = &source[start as usize..end as usize];
        assert!(
            doc_text.contains("Doc line 1"),
            "should contain first doc line"
        );
        assert!(
            doc_text.contains("Doc line 2"),
            "should contain second doc line"
        );
    }

    #[test]
    fn test_scan_doc_range_regular_comment_not_captured() {
        let source = "// Regular comment\npub fn foo() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let function = first_named_descendant(&root, "function_item");

        let range = scan_doc_range(&function, source, &RUST_DOC_SPEC);

        assert!(
            range.is_none(),
            "regular // comment should not be captured as doc"
        );
    }

    #[test]
    fn test_scan_doc_range_blank_line_stops_scan() {
        let source = "/// Detached doc\n\n/// Attached doc\npub fn foo() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let function = first_named_descendant(&root, "function_item");

        let range = scan_doc_range(&function, source, &RUST_DOC_SPEC);

        assert!(range.is_some(), "expected doc range for attached comment");
        let (start, end) = range.unwrap();
        let doc_text = &source[start as usize..end as usize];
        assert!(
            doc_text.contains("Attached doc"),
            "should contain attached doc"
        );
        assert!(
            !doc_text.contains("Detached doc"),
            "should NOT contain detached doc"
        );
    }

    #[test]
    fn test_scan_doc_range_no_doc_spec_returns_none() {
        let source = "/// Doc comment\npub fn foo() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let function = first_named_descendant(&root, "function_item");

        let range = scan_doc_range(&function, source, &NO_DOC_SPEC);

        assert!(range.is_none(), "NO_DOC_SPEC should always return None");
    }

    #[test]
    fn test_scan_doc_range_all_adjacent_comments_go_style() {
        let source = "// Package doc\n// More doc\nfunc Foo() {}\n";
        let tree = parse_go(source);
        let root = tree.root_node();
        let function = root
            .children(&mut root.walk())
            .find(|child| child.kind() == "function_declaration")
            .expect("expected function_declaration");

        let range = scan_doc_range(&function, source, &GO_DOC_SPEC);

        assert!(range.is_some(), "expected doc range for Go comments");
        let (start, end) = range.unwrap();
        let doc_text = &source[start as usize..end as usize];
        assert!(
            doc_text.contains("Package doc"),
            "should contain first doc line"
        );
        assert!(
            doc_text.contains("More doc"),
            "should contain second doc line"
        );
    }
}
