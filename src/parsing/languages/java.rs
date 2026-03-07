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
        "class_declaration" | "record_declaration" => Some(SymbolKind::Class),
        "interface_declaration" | "annotation_type_declaration" => Some(SymbolKind::Interface),
        "enum_declaration" => Some(SymbolKind::Enum),
        "method_declaration" => Some(SymbolKind::Method),
        "constructor_declaration" => Some(SymbolKind::Function),
        "field_declaration" => {
            extract_field(node, source, depth, sort_order, symbols);
            return;
        }
        "constant_declaration" => Some(SymbolKind::Constant),
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
                line_range: (node.start_position().row as u32, node.end_position().row as u32),
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

fn extract_field(
    node: &Node,
    source: &str,
    depth: u32,
    sort_order: &mut u32,
    symbols: &mut Vec<SymbolRecord>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "variable_declarator" {
            if let Some(name) = find_name(&child, source) {
                symbols.push(SymbolRecord {
                    name,
                    kind: SymbolKind::Variable,
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
    }
}

fn find_name(node: &Node, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            return Some(child.utf8_text(source.as_bytes()).unwrap_or("").to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::SymbolKind;

    fn parse_java(source: &str) -> Vec<SymbolRecord> {
        let mut parser = tree_sitter::Parser::new();
        let language = tree_sitter_java::LANGUAGE.into();
        parser.set_language(&language).unwrap();
        let tree = parser.parse(source, None).unwrap();
        extract_symbols(&tree.root_node(), source)
    }

    #[test]
    fn test_extract_class_with_methods() {
        let source = r#"
public class Greeter {
    public void greet() {}
    public String getName() { return ""; }
}
"#;
        let symbols = parse_java(source);
        let class = symbols.iter().find(|s| s.kind == SymbolKind::Class);
        assert!(class.is_some());
        assert_eq!(class.unwrap().name, "Greeter");
        assert_eq!(class.unwrap().depth, 0);

        let methods: Vec<_> = symbols.iter().filter(|s| s.kind == SymbolKind::Method).collect();
        assert_eq!(methods.len(), 2);
        assert_eq!(methods[0].name, "greet");
        assert_eq!(methods[1].name, "getName");
        assert_eq!(methods[0].depth, 1);
    }

    #[test]
    fn test_extract_interface() {
        let source = r#"
public interface Runnable {
    void run();
}
"#;
        let symbols = parse_java(source);
        let iface = symbols.iter().find(|s| s.kind == SymbolKind::Interface);
        assert!(iface.is_some());
        assert_eq!(iface.unwrap().name, "Runnable");
    }

    #[test]
    fn test_extract_enum() {
        let source = r#"
public enum Color {
    RED, GREEN, BLUE
}
"#;
        let symbols = parse_java(source);
        let e = symbols.iter().find(|s| s.kind == SymbolKind::Enum);
        assert!(e.is_some());
        assert_eq!(e.unwrap().name, "Color");
    }

    #[test]
    fn test_extract_constructor() {
        let source = r#"
public class Foo {
    public Foo(int x) {}
}
"#;
        let symbols = parse_java(source);
        let ctor = symbols.iter().find(|s| s.kind == SymbolKind::Function);
        assert!(ctor.is_some());
        assert_eq!(ctor.unwrap().name, "Foo");
        assert_eq!(ctor.unwrap().depth, 1);
    }

    #[test]
    fn test_extract_field() {
        let source = r#"
public class Config {
    private int count;
    public String name;
}
"#;
        let symbols = parse_java(source);
        let fields: Vec<_> = symbols.iter().filter(|s| s.kind == SymbolKind::Variable).collect();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "count");
        assert_eq!(fields[1].name, "name");
    }

    #[test]
    fn test_nested_class_depth() {
        let source = r#"
public class Outer {
    public class Inner {
        public void doWork() {}
    }
}
"#;
        let symbols = parse_java(source);
        let outer = symbols.iter().find(|s| s.name == "Outer").unwrap();
        assert_eq!(outer.depth, 0);
        let inner = symbols.iter().find(|s| s.name == "Inner").unwrap();
        assert_eq!(inner.depth, 1);
        let method = symbols.iter().find(|s| s.name == "doWork").unwrap();
        assert_eq!(method.depth, 2);
    }

    #[test]
    fn test_sort_order_increments() {
        let source = r#"
public class A {}
public class B {}
public class C {}
"#;
        let symbols = parse_java(source);
        assert_eq!(symbols.len(), 3);
        assert!(symbols[0].sort_order < symbols[1].sort_order);
        assert!(symbols[1].sort_order < symbols[2].sort_order);
    }
}
