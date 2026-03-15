use tree_sitter::Node;

use super::{collect_symbols, push_symbol, NO_DOC_SPEC};
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
    match node.kind() {
        "rule_set" => {
            // Extract the full selector text as the symbol name.
            // Find the "selectors" child node by kind (not field name).
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "selectors" {
                    let name = child
                        .utf8_text(source.as_bytes())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    if !name.is_empty() {
                        push_symbol(
                            node,
                            source,
                            name,
                            SymbolKind::Other,
                            depth,
                            sort_order,
                            symbols,
                            &NO_DOC_SPEC,
                        );
                    }
                    break;
                }
            }
            // Recurse into the block to find custom properties.
            walk_children(node, source, depth + 1, sort_order, symbols);
        }
        "declaration" => {
            // Check if this is a custom property (--*).
            // Find the "property_name" child by kind.
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "property_name" {
                    let prop_text = child
                        .utf8_text(source.as_bytes())
                        .unwrap_or("");
                    if prop_text.starts_with("--") {
                        push_symbol(
                            node,
                            source,
                            prop_text.to_string(),
                            SymbolKind::Variable,
                            depth,
                            sort_order,
                            symbols,
                            &NO_DOC_SPEC,
                        );
                    }
                    break;
                }
            }
        }
        "media_statement" => {
            let name = at_rule_name(node, source);
            push_symbol(
                node,
                source,
                name,
                SymbolKind::Module,
                depth,
                sort_order,
                symbols,
                &NO_DOC_SPEC,
            );
            // Recurse to pick up nested rule_sets.
            walk_children(node, source, depth + 1, sort_order, symbols);
        }
        "keyframes_statement" => {
            let name = at_rule_name(node, source);
            push_symbol(
                node,
                source,
                name,
                SymbolKind::Module,
                depth,
                sort_order,
                symbols,
                &NO_DOC_SPEC,
            );
            // Do NOT recurse — skip inner keyframe steps.
        }
        _ => {
            // Recurse into children for any other node type.
            walk_children(node, source, depth, sort_order, symbols);
        }
    }
}

/// Walk all children of a node.
fn walk_children(
    node: &Node,
    source: &str,
    depth: u32,
    sort_order: &mut u32,
    symbols: &mut Vec<SymbolRecord>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_node(&child, source, depth, sort_order, symbols);
    }
}

/// Extract the at-rule name: text from the node start up to (but not
/// including) the opening `{`, trimmed.
fn at_rule_name(node: &Node, source: &str) -> String {
    let text = node.utf8_text(source.as_bytes()).unwrap_or("");
    match text.find('{') {
        Some(pos) => text[..pos].trim().to_string(),
        None => text.trim().to_string(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::SymbolKind;
    use tree_sitter::Parser;

    fn parse_css(source: &str) -> Vec<SymbolRecord> {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_css::LANGUAGE.into();
        parser.set_language(&lang).expect("set CSS language");
        let tree = parser.parse(source, None).expect("parse CSS source");
        extract_symbols(&tree.root_node(), source)
    }

    #[test]
    fn test_css_selector_block_extracted() {
        let symbols = parse_css(".btn { color: red; }");
        let rule = symbols.iter().find(|s| s.kind == SymbolKind::Other);
        assert!(rule.is_some(), "should extract rule_set, got: {:?}", symbols);
        assert_eq!(rule.unwrap().name, ".btn");
    }

    #[test]
    fn test_css_selector_list_single_symbol() {
        let symbols = parse_css(".btn, .btn-primary { color: red; }");
        let rules: Vec<_> = symbols.iter().filter(|s| s.kind == SymbolKind::Other).collect();
        assert_eq!(
            rules.len(),
            1,
            "selector list should produce exactly 1 symbol, got: {:?}",
            rules
        );
        assert!(
            rules[0].name.contains(".btn"),
            "name should contain .btn, got: {}",
            rules[0].name
        );
        assert!(
            rules[0].name.contains(".btn-primary"),
            "name should contain .btn-primary, got: {}",
            rules[0].name
        );
    }

    #[test]
    fn test_css_custom_property_extracted() {
        let symbols = parse_css(":root { --primary-color: blue; }");
        let var = symbols.iter().find(|s| s.kind == SymbolKind::Variable);
        assert!(
            var.is_some(),
            "should extract custom property as Variable, got: {:?}",
            symbols
        );
        assert_eq!(var.unwrap().name, "--primary-color");
    }

    #[test]
    fn test_css_media_query_extracted() {
        let symbols = parse_css("@media (max-width: 768px) { .a { color: red; } }");
        let media = symbols.iter().find(|s| s.kind == SymbolKind::Module);
        assert!(
            media.is_some(),
            "should extract @media as Module, got: {:?}",
            symbols
        );
        assert!(
            media.unwrap().name.starts_with("@media"),
            "name should start with @media, got: {}",
            media.unwrap().name
        );
    }

    #[test]
    fn test_css_keyframes_outer_extracted_inner_skipped() {
        let symbols = parse_css(
            "@keyframes fade-in { 0% { opacity: 0; } 100% { opacity: 1; } }",
        );
        let kf: Vec<_> = symbols.iter().filter(|s| s.kind == SymbolKind::Module).collect();
        assert_eq!(
            kf.len(),
            1,
            "should extract exactly 1 @keyframes Module, got: {:?}",
            kf
        );
        assert!(
            kf[0].name.contains("fade-in"),
            "name should contain fade-in, got: {}",
            kf[0].name
        );
        // Inner steps (0%, 100%) should NOT be extracted as symbols.
        assert_eq!(
            symbols.len(),
            1,
            "only the @keyframes itself should appear, no inner steps, got: {:?}",
            symbols
        );
    }

    #[test]
    fn test_css_empty_file() {
        let symbols = parse_css("");
        assert!(symbols.is_empty(), "empty file should produce zero symbols");
    }
}
