use tree_sitter::Parser;

#[test]
fn test_rust_grammar_loads_and_parses() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("failed to load Rust grammar");
    let tree = parser.parse("fn main() {}", None).expect("parse returned None");
    assert!(!tree.root_node().kind().is_empty());
}

#[test]
fn test_python_grammar_loads_and_parses() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .expect("failed to load Python grammar");
    let tree = parser.parse("def hello(): pass", None).expect("parse returned None");
    assert!(!tree.root_node().kind().is_empty());
}

#[test]
fn test_javascript_grammar_loads_and_parses() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .expect("failed to load JavaScript grammar");
    let tree = parser
        .parse("function hello() {}", None)
        .expect("parse returned None");
    assert!(!tree.root_node().kind().is_empty());
}

#[test]
fn test_typescript_grammar_loads_and_parses() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .expect("failed to load TypeScript grammar");
    let tree = parser
        .parse("function hello(): void {}", None)
        .expect("parse returned None");
    assert!(!tree.root_node().kind().is_empty());
}

#[test]
fn test_java_grammar_loads_and_parses() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_java::LANGUAGE.into())
        .expect("failed to load Java grammar");
    let tree = parser
        .parse("public class App { public void run() {} }", None)
        .expect("parse returned None");
    assert!(!tree.root_node().kind().is_empty());
    assert!(!tree.root_node().has_error());
}

#[test]
fn test_go_grammar_loads_and_parses() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_go::LANGUAGE.into())
        .expect("failed to load Go grammar");
    let tree = parser
        .parse("package main\nfunc main() {}", None)
        .expect("parse returned None");
    assert!(!tree.root_node().kind().is_empty());
}
