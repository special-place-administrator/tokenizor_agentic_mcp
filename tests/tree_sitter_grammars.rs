use tree_sitter::Parser;
use tokenizor_agentic_mcp::domain::LanguageId;
use tokenizor_agentic_mcp::parsing::process_file;

#[test]
fn test_rust_grammar_loads_and_parses() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("failed to load Rust grammar");
    let tree = parser
        .parse("fn main() {}", None)
        .expect("parse returned None");
    assert!(!tree.root_node().kind().is_empty());
}

#[test]
fn test_python_grammar_loads_and_parses() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .expect("failed to load Python grammar");
    let tree = parser
        .parse("def hello(): pass", None)
        .expect("parse returned None");
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

#[test]
fn test_c_grammar_loads_and_parses() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_c::LANGUAGE.into())
        .expect("failed to load C grammar — possible ABI mismatch");
    let source = "struct Point { int x; int y; };\nint add(int a, int b) { return a + b; }";
    let tree = parser.parse(source, None).expect("parse returned None");
    assert!(!tree.root_node().kind().is_empty());
    assert!(!tree.root_node().has_error(), "C source should parse without syntax errors");

    // Verify symbols extracted via process_file
    let result = process_file("test.c", source.as_bytes(), LanguageId::C);
    use tokenizor_agentic_mcp::domain::{FileOutcome, SymbolKind};
    assert_eq!(result.outcome, FileOutcome::Processed);
    assert!(
        result.symbols.iter().any(|s| s.kind == SymbolKind::Struct && s.name == "Point"),
        "should extract Point struct, symbols: {:?}", result.symbols
    );
    assert!(
        result.symbols.iter().any(|s| s.kind == SymbolKind::Function && s.name == "add"),
        "should extract add function, symbols: {:?}", result.symbols
    );
}

#[test]
fn test_cpp_grammar_loads_and_parses() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_cpp::LANGUAGE.into())
        .expect("failed to load C++ grammar — possible ABI mismatch");
    let source = "namespace myns {\n  class Foo { public: void bar(); };\n  void Foo::bar() { }\n}";
    let tree = parser.parse(source, None).expect("parse returned None");
    assert!(!tree.root_node().kind().is_empty());
    assert!(!tree.root_node().has_error(), "C++ source should parse without syntax errors");

    // Verify symbols extracted via process_file
    let result = process_file("test.cpp", source.as_bytes(), LanguageId::Cpp);
    use tokenizor_agentic_mcp::domain::{FileOutcome, SymbolKind};
    assert_eq!(result.outcome, FileOutcome::Processed);
    assert!(
        result.symbols.iter().any(|s| s.kind == SymbolKind::Module && s.name == "myns"),
        "should extract myns namespace, symbols: {:?}", result.symbols
    );
    assert!(
        result.symbols.iter().any(|s| s.kind == SymbolKind::Class && s.name == "Foo"),
        "should extract Foo class, symbols: {:?}", result.symbols
    );
}
