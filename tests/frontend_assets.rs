/// Integration tests for HTML/Angular, CSS, and SCSS frontend asset parsing (Sprint 12).
///
/// Proves:
///   FA-01: HTML files indexed with tree-sitter-html grammar
///   FA-02: CSS files indexed with tree-sitter-css grammar
///   FA-03: SCSS files indexed with tree-sitter-scss grammar
///   FA-04: search_symbols(kind="variable") finds CSS custom properties and SCSS variables
///   FA-05: get_file_context returns structured outline for all three types
///   FA-06: Edit capability gated at TextEditSafe for all three
///   FA-07: Custom elements extracted at any depth
///   FA-08: Angular control flow extracted via text scanning
///   FA-09: Plain HTML produces no Angular noise
///   FA-10: All existing tests unchanged (regression)
use tokenizor_agentic_mcp::{
    domain::{FileOutcome, LanguageId, SymbolKind},
    parsing::{
        config_extractors::{EditCapability, edit_capability_for_language},
        process_file,
    },
};

// ---------------------------------------------------------------------------
// FA-01: HTML indexing
// ---------------------------------------------------------------------------

#[test]
fn fa01_html_file_indexes_successfully() {
    let source = b"<app-header></app-header><main><app-sidebar></app-sidebar></main>";
    let result = process_file("app.component.html", source, LanguageId::Html);
    assert_eq!(result.outcome, FileOutcome::Processed);
    assert!(!result.symbols.is_empty(), "HTML file should have symbols");
    let names: Vec<&str> = result.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"app-header"), "should find app-header");
    assert!(names.contains(&"app-sidebar"), "should find app-sidebar");
}

// ---------------------------------------------------------------------------
// FA-02: CSS indexing
// ---------------------------------------------------------------------------

#[test]
fn fa02_css_file_indexes_successfully() {
    let source = b".btn { color: red; }\n:root { --primary: blue; }";
    let result = process_file("styles.css", source, LanguageId::Css);
    assert_eq!(result.outcome, FileOutcome::Processed);
    assert!(!result.symbols.is_empty(), "CSS file should have symbols");
}

// ---------------------------------------------------------------------------
// FA-03: SCSS indexing
// ---------------------------------------------------------------------------

#[test]
fn fa03_scss_file_indexes_successfully() {
    let source = b"$gap: 8px;\n@mixin flex { display: flex; }\n.container { width: 100%; }";
    let result = process_file("styles.scss", source, LanguageId::Scss);
    assert_eq!(result.outcome, FileOutcome::Processed);
    assert!(!result.symbols.is_empty(), "SCSS file should have symbols");
    let names: Vec<&str> = result.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"$gap"), "should find $gap variable");
    assert!(names.contains(&"flex"), "should find flex mixin");
}

// ---------------------------------------------------------------------------
// FA-04: Variable discovery by kind
// ---------------------------------------------------------------------------

#[test]
fn fa04_css_custom_properties_discoverable_by_kind() {
    let source = b":root { --primary: blue; --gap: 8px; }\n.btn { color: red; }";
    let result = process_file("tokens.css", source, LanguageId::Css);
    let variables: Vec<&str> = result
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Variable)
        .map(|s| s.name.as_str())
        .collect();
    assert!(
        variables.contains(&"--primary"),
        "should find --primary via kind filter"
    );
    assert!(
        variables.contains(&"--gap"),
        "should find --gap via kind filter"
    );
}

#[test]
fn fa04_scss_variables_discoverable_by_kind() {
    let source = b"$primary: #333;\n$gap: 8px;\n.btn { color: $primary; }";
    let result = process_file("vars.scss", source, LanguageId::Scss);
    let variables: Vec<&str> = result
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Variable)
        .map(|s| s.name.as_str())
        .collect();
    assert!(
        variables.contains(&"$primary"),
        "should find $primary via kind filter"
    );
    assert!(
        variables.contains(&"$gap"),
        "should find $gap via kind filter"
    );
}

// ---------------------------------------------------------------------------
// FA-05: Structured outline
// ---------------------------------------------------------------------------

#[test]
fn fa05_html_produces_structured_outline() {
    let source = b"<app-header></app-header>\n<app-footer></app-footer>";
    let result = process_file("app.component.html", source, LanguageId::Html);
    assert!(
        result.symbols.len() >= 2,
        "HTML outline should have multiple symbols, got: {:?}",
        result.symbols
    );
    for sym in &result.symbols {
        assert!(
            sym.line_range.0 <= sym.line_range.1,
            "invalid line range for {}",
            sym.name
        );
    }
}

#[test]
fn fa05_css_produces_structured_outline() {
    let source = b".btn { color: red; }\n@media (max-width: 768px) { .mobile { display: none; } }";
    let result = process_file("styles.css", source, LanguageId::Css);
    assert!(
        result.symbols.len() >= 2,
        "CSS outline should have multiple symbols, got: {:?}",
        result.symbols
    );
}

#[test]
fn fa05_scss_produces_structured_outline() {
    let source = b"$gap: 8px;\n@mixin flex { display: flex; }\n.container { width: 100%; }";
    let result = process_file("styles.scss", source, LanguageId::Scss);
    assert!(
        result.symbols.len() >= 3,
        "SCSS outline should have multiple symbols, got: {:?}",
        result.symbols
    );
}

// ---------------------------------------------------------------------------
// FA-06: Edit capability gating
// ---------------------------------------------------------------------------

#[test]
fn fa06_edit_capability_gating_for_frontend() {
    assert_eq!(
        edit_capability_for_language(&LanguageId::Html),
        Some(EditCapability::TextEditSafe)
    );
    assert_eq!(
        edit_capability_for_language(&LanguageId::Css),
        Some(EditCapability::TextEditSafe)
    );
    assert_eq!(
        edit_capability_for_language(&LanguageId::Scss),
        Some(EditCapability::TextEditSafe)
    );
    // Regular source languages remain unrestricted
    assert_eq!(edit_capability_for_language(&LanguageId::Rust), None);
}

// ---------------------------------------------------------------------------
// FA-07: Custom elements at any depth
// ---------------------------------------------------------------------------

#[test]
fn fa07_custom_elements_at_any_depth() {
    let source = b"<div><section><my-widget></my-widget></section></div>";
    let result = process_file("deep.html", source, LanguageId::Html);
    let names: Vec<&str> = result.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"my-widget"),
        "should find custom element at depth, got: {:?}",
        names
    );
}

// ---------------------------------------------------------------------------
// FA-08: Angular control flow via text scanning
// ---------------------------------------------------------------------------

#[test]
fn fa08_angular_control_flow_extracted() {
    let source =
        b"@if (show) { <span>yes</span> }\n@for (item of items; track item.id) { <li>hi</li> }";
    let result = process_file("template.html", source, LanguageId::Html);
    let names: Vec<&str> = result.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"@if"), "should find @if, got: {:?}", names);
    assert!(
        names.contains(&"@for"),
        "should find @for, got: {:?}",
        names
    );
}

// ---------------------------------------------------------------------------
// FA-09: Plain HTML no Angular noise
// ---------------------------------------------------------------------------

#[test]
fn fa09_plain_html_no_angular_noise() {
    let source = b"<html><head><title>Test</title></head><body><p>Hello</p></body></html>";
    let result = process_file("index.html", source, LanguageId::Html);
    let angular_noise: Vec<&str> = result
        .symbols
        .iter()
        .filter(|s| s.name.starts_with('@'))
        .map(|s| s.name.as_str())
        .collect();
    assert!(
        angular_noise.is_empty(),
        "plain HTML should have no Angular noise, got: {:?}",
        angular_noise
    );
}
