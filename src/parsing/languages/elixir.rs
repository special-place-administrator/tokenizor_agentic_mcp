use tree_sitter::Node;

use super::{collect_symbols, push_symbol, walk_children};
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
    // In Elixir's tree-sitter grammar, functions are modeled as calls to 'def'/'defp',
    // and modules are calls to 'defmodule'. All appear as `call` nodes.
    if node.kind() == "call"
        && let Some((symbol_kind, name)) = extract_elixir_def(node, source)
    {
        push_symbol(node, name, symbol_kind, depth, sort_order, symbols);
        walk_children(
            node,
            source,
            depth,
            sort_order,
            symbols,
            Some(symbol_kind),
            walk_node,
        );
        return;
    }

    walk_children(node, source, depth, sort_order, symbols, None, walk_node);
}

/// Check if a `call` node represents a def/defp/defmodule call and extract name.
fn extract_elixir_def(node: &Node, source: &str) -> Option<(SymbolKind, String)> {
    let source_bytes = source.as_bytes();

    // A call node looks like: target arguments do_block
    // target is the function being called (e.g. `def`, `defmodule`)
    let mut cursor = node.walk();
    let mut target_text: Option<String> = None;
    let mut name_text: Option<String> = None;

    for child in node.children(&mut cursor) {
        match child.kind() {
            "identifier" if target_text.is_none() => {
                target_text = child.utf8_text(source_bytes).ok().map(|s| s.to_string());
            }
            "arguments" => {
                // The first child of arguments that's an identifier/call with a simple_identifier is the name
                let mut arg_cursor = child.walk();
                for arg in child.children(&mut arg_cursor) {
                    match arg.kind() {
                        "identifier" => {
                            name_text = arg.utf8_text(source_bytes).ok().map(|s| s.to_string());
                            break;
                        }
                        "call" => {
                            // def foo(args) — the call's target is the function name
                            let mut c2 = arg.walk();
                            for grandchild in arg.children(&mut c2) {
                                if grandchild.kind() == "identifier" {
                                    name_text = grandchild
                                        .utf8_text(source_bytes)
                                        .ok()
                                        .map(|s| s.to_string());
                                    break;
                                }
                            }
                            break;
                        }
                        "alias" => {
                            // defmodule MyApp.Module — alias is the module name
                            name_text = arg
                                .utf8_text(source_bytes)
                                .ok()
                                .map(|s| s.trim().to_string());
                            break;
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    let target = target_text.as_deref()?;
    let name = name_text?;

    let kind = match target {
        "def" | "defp" => SymbolKind::Function,
        "defmodule" => SymbolKind::Module,
        "defmacro" | "defmacrop" => SymbolKind::Function,
        _ => return None,
    };

    Some((kind, name))
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

    fn parse_elixir(source: &str) -> Vec<SymbolRecord> {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_elixir::LANGUAGE.into();
        parser.set_language(&lang).expect("set Elixir language");
        let tree = parser.parse(source, None).expect("parse Elixir source");
        extract_symbols(&tree.root_node(), source)
    }

    #[test]
    fn test_elixir_def_function() {
        let source = "def greet do\n  IO.puts(\"hello\")\nend";
        let symbols = parse_elixir(source);
        let func = symbols.iter().find(|s| s.kind == SymbolKind::Function);
        assert!(
            func.is_some(),
            "should extract def function, got: {:?}",
            symbols
        );
        assert_eq!(func.unwrap().name, "greet");
    }

    #[test]
    fn test_elixir_defmodule() {
        let source = "defmodule MyApp do\n  def hello do\n    :ok\n  end\nend";
        let symbols = parse_elixir(source);
        let m = symbols.iter().find(|s| s.kind == SymbolKind::Module);
        assert!(m.is_some(), "should extract defmodule, got: {:?}", symbols);
        assert_eq!(m.unwrap().name, "MyApp");
    }

    #[test]
    fn test_elixir_process_file_returns_processed() {
        let source = b"defmodule Foo do\n  def bar do\n    :ok\n  end\nend";
        let result = process_file("test.ex", source, LanguageId::Elixir);
        assert_eq!(
            result.outcome,
            FileOutcome::Processed,
            "outcome: {:?}",
            result.outcome
        );
        assert!(!result.symbols.is_empty(), "should have symbols");
    }
}
