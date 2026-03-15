# Frontend Asset Parsing Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add tree-sitter-based parsing for HTML (via Angular grammar), CSS, and SCSS so all existing tools (search, navigation, edit) work on frontend asset files.

**Architecture:** Same tree-sitter pipeline as existing 16 languages — no new abstractions. Each language gets a `LanguageId` variant, grammar crate dependency, extractor in `src/parsing/languages/`, and match arms in `parse_source()`, `extract_symbols()`, and `extract_references()`. Edit capability gated at `TextEditSafe` via a new unified `edit_capability_for_language` function.

**Tech Stack:** `tree-sitter-angular` 0.8.4, `tree-sitter-css` 0.25.0, `tree-sitter-scss` 1.0.0, host `tree-sitter` 0.26.

**Spec:** `docs/superpowers/specs/2026-03-15-frontend-asset-parsing-design.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `Cargo.toml` | Modify | Add 3 grammar crate dependencies |
| `src/domain/index.rs` | Modify | Add `Html`, `Css`, `Scss` to `LanguageId`, extension mapping, Display, support tier |
| `src/parsing/languages/html.rs` | Create | HTML/Angular symbol extractor with dedup |
| `src/parsing/languages/css.rs` | Create | CSS symbol extractor |
| `src/parsing/languages/scss.rs` | Create | SCSS symbol extractor (extends CSS) |
| `src/parsing/languages/mod.rs` | Modify | Add `mod html/css/scss`, match arms in `extract_symbols` |
| `src/parsing/mod.rs` | Modify | Match arms in `parse_source` for 3 new grammars |
| `src/parsing/xref.rs` | Modify | Return empty refs for `Html`/`Css`/`Scss` (NOT `unreachable!()`) |
| `src/parsing/config_extractors/mod.rs` | Modify | Add `edit_capability_for_language` function |
| `src/protocol/tools.rs` | Modify | Rename `check_config_edit_capability` → `check_edit_capability`, call new unified function |

---

## Chunk 1: Dependencies + Domain Model + ABI Validation

### Task 1: Add grammar crate dependencies

**Files:**
- Modify: `Cargo.toml:23-40` (dependencies section)

- [ ] **Step 1: Add 3 grammar crates to Cargo.toml**

After the existing `tree-sitter-elixir` line (line 40), add:

```toml
tree-sitter-angular = "0.8.4"
tree-sitter-css = "0.25.0"
tree-sitter-scss = "1.0.0"
```

- [ ] **Step 2: Run cargo check to verify ABI compatibility**

Run: `cargo check 2>&1 | head -30`
Expected: Successful compilation. If ABI incompatibility (tree-sitter ~0.25 vs host 0.26), see fallback in spec.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "feat: add tree-sitter-angular, tree-sitter-css, tree-sitter-scss dependencies"
```

---

### Task 2: Add LanguageId variants and extension mapping

**Files:**
- Modify: `src/domain/index.rs:6-136` (LanguageId enum, from_extension, extensions, support_tier, Display)

- [ ] **Step 1: Add 3 variants to LanguageId enum**

In `src/domain/index.rs`, add after `Env` (before the closing `}`):

```rust
    Html,
    Css,
    Scss,
```

- [ ] **Step 2: Add extension mappings in `from_extension`**

After the `"env"` arm:

```rust
            "html" => Some(Self::Html),
            "css" => Some(Self::Css),
            "scss" => Some(Self::Scss),
```

- [ ] **Step 3: Add extensions in `extensions`**

After the `Self::Env` arm:

```rust
            Self::Html => &["html"],
            Self::Css => &["css"],
            Self::Scss => &["scss"],
```

- [ ] **Step 4: Add support tier**

In `support_tier`, add `Html`, `Css`, `Scss` to the `SupportTier::Broader` arm alongside the other non-quality-focus languages.

- [ ] **Step 5: Add Display names**

In the `Display` impl:

```rust
            Self::Html => "HTML",
            Self::Css => "CSS",
            Self::Scss => "SCSS",
```

- [ ] **Step 6: Fix all exhaustive matches on LanguageId (compiler-driven)**

Do NOT search for specific variants. Instead, let the compiler tell you what's broken:

1. Run `cargo check 2>&1`
2. For each `non-exhaustive patterns` error, fix the match arm as follows:

**Known locations and their fixes:**

**`src/parsing/languages/mod.rs` — `extract_symbols`:** Add temporary `todo!()` arm (replaced in Task 10 with real extractors). **WARNING:** These `todo!()` arms are runtime traps — they MUST be replaced in Chunk 5 Task 10 before any test runs against frontend files. They exist only to let `cargo check` pass during the domain model phase.

```rust
        LanguageId::Html | LanguageId::Css | LanguageId::Scss => {
            todo!("extractors wired in Task 10")
        }
```

**`src/parsing/mod.rs` — `parse_source`:** Same temporary `todo!()`:

```rust
        LanguageId::Html | LanguageId::Css | LanguageId::Scss => {
            todo!("grammar integration wired in Task 10")
        }
```

**`src/parsing/xref.rs` — `extract_references`:** Add the proper empty-return arm (NOT `unreachable!()`, since these DO enter the tree-sitter pipeline):

```rust
        LanguageId::Html | LanguageId::Css | LanguageId::Scss => {
            return (vec![], HashMap::new());
        }
```

**Any other matches** (e.g. test helpers like `parse_and_extract` in xref.rs ~line 873): Add to the nearest existing fallback arm. For test-only helpers that only exercise established languages, group with the config-language unreachable arm.

**Functions with wildcard matches** (`is_config_language`, `extractor_for`, etc.) will NOT produce compiler errors — they already have `_ =>` arms. No changes needed for those.

3. Keep running `cargo check` until zero errors.

- [ ] **Step 7: Run cargo check**

Run: `cargo check 2>&1 | head -30`
Expected: Zero errors.

- [ ] **Step 8: Commit**

```bash
git add src/domain/index.rs src/parsing/config_extractors/mod.rs src/parsing/languages/mod.rs src/parsing/mod.rs src/parsing/xref.rs
git commit -m "feat: add Html, Css, Scss to LanguageId with extension mapping"
```

---

### Task 3: ABI smoke tests

**Files:**
- Modify: `src/parsing/languages/mod.rs` (add tests at bottom of test module)

- [ ] **Step 1: Write ABI smoke test for tree-sitter-angular**

In the `#[cfg(test)] mod tests` block at the bottom of `src/parsing/languages/mod.rs`:

```rust
    #[test]
    fn test_abi_smoke_angular_grammar() {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_angular::LANGUAGE.into();
        parser.set_language(&lang).expect("set Angular/HTML language");
        let tree = parser.parse("<div></div>", None).expect("parse HTML snippet");
        assert!(!tree.root_node().has_error(), "root should not be error");
    }
```

Note: If `tree-sitter-angular` does not expose `LANGUAGE` as a constant, check for `language()` function instead. The crate API may differ — adjust the import accordingly.

- [ ] **Step 2: Write ABI smoke test for tree-sitter-css**

```rust
    #[test]
    fn test_abi_smoke_css_grammar() {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_css::LANGUAGE.into();
        parser.set_language(&lang).expect("set CSS language");
        let tree = parser.parse(".a { color: red; }", None).expect("parse CSS snippet");
        assert!(!tree.root_node().has_error(), "root should not be error");
    }
```

- [ ] **Step 3: Write ABI smoke test for tree-sitter-scss**

```rust
    #[test]
    fn test_abi_smoke_scss_grammar() {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_scss::LANGUAGE.into();
        parser.set_language(&lang).expect("set SCSS language");
        let tree = parser.parse("$x: 1;", None).expect("parse SCSS snippet");
        assert!(!tree.root_node().has_error(), "root should not be error");
    }
```

- [ ] **Step 4: Run smoke tests**

Run: `cargo test test_abi_smoke -- --test-threads=1 -v 2>&1`
Expected: All 3 pass. If any fail, investigate grammar API (may need `language()` function instead of `LANGUAGE` constant, or version pin).

- [ ] **Step 5: Commit**

```bash
git add src/parsing/languages/mod.rs
git commit -m "test: add ABI smoke tests for Angular, CSS, SCSS grammars"
```

---

## Chunk 2: CSS Extractor

CSS is the simplest extractor and provides the foundation SCSS will extend.

### Task 4: CSS extractor — failing tests

**Files:**
- Create: `src/parsing/languages/css.rs`

- [ ] **Step 1: Create css.rs with test scaffolding and failing tests**

Create `src/parsing/languages/css.rs`:

```rust
use tree_sitter::Node;

use super::{push_symbol, NO_DOC_SPEC};
use crate::domain::{SymbolKind, SymbolRecord};

pub fn extract_symbols(node: &Node, source: &str) -> Vec<SymbolRecord> {
    let mut symbols = Vec::new();
    let mut sort_order = 0u32;
    walk_node(node, source, 0, &mut sort_order, &mut symbols);
    symbols
}

fn walk_node(
    _node: &Node,
    _source: &str,
    _depth: u32,
    _sort_order: &mut u32,
    _symbols: &mut Vec<SymbolRecord>,
) {
    // Implementation coming next
}

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
        let source = ".btn { color: red; }";
        let symbols = parse_css(source);
        let sel = symbols.iter().find(|s| s.kind == SymbolKind::Other);
        assert!(sel.is_some(), "should extract selector, got: {:?}", symbols);
        assert_eq!(sel.unwrap().name, ".btn");
    }

    #[test]
    fn test_css_selector_list_single_symbol() {
        let source = ".btn, .btn-primary { color: red; }";
        let symbols = parse_css(source);
        let sels: Vec<_> = symbols.iter().filter(|s| s.kind == SymbolKind::Other).collect();
        assert_eq!(sels.len(), 1, "selector list should produce one symbol, got: {:?}", sels);
        assert_eq!(sels[0].name, ".btn, .btn-primary");
    }

    #[test]
    fn test_css_custom_property_extracted() {
        let source = ":root { --primary-color: blue; }";
        let symbols = parse_css(source);
        let var = symbols.iter().find(|s| s.kind == SymbolKind::Variable);
        assert!(var.is_some(), "should extract custom property, got: {:?}", symbols);
        assert_eq!(var.unwrap().name, "--primary-color");
    }

    #[test]
    fn test_css_media_query_extracted() {
        let source = "@media (max-width: 768px) { .a { color: red; } }";
        let symbols = parse_css(source);
        let m = symbols.iter().find(|s| s.kind == SymbolKind::Module && s.name.starts_with("@media"));
        assert!(m.is_some(), "should extract @media, got: {:?}", symbols);
    }

    #[test]
    fn test_css_keyframes_outer_extracted_inner_skipped() {
        let source = "@keyframes fade-in { 0% { opacity: 0; } 100% { opacity: 1; } }";
        let symbols = parse_css(source);
        let kf = symbols.iter().find(|s| s.name.contains("fade-in"));
        assert!(kf.is_some(), "should extract @keyframes, got: {:?}", symbols);
        assert_eq!(kf.unwrap().kind, SymbolKind::Module);
        // Inner steps (0%, 100%) should NOT be extracted
        let steps: Vec<_> = symbols.iter().filter(|s| s.name.contains('%')).collect();
        assert!(steps.is_empty(), "inner keyframe steps should be skipped, got: {:?}", steps);
    }

    // NOTE: @layer is deferred — known gap. tree-sitter-css 0.25.0 may not
    // have a node type for @layer (CSS 2022 feature). Can be added in a future
    // sprint once grammar support is confirmed.

    #[test]
    fn test_css_empty_file() {
        let symbols = parse_css("");
        assert!(symbols.is_empty(), "empty file should produce zero symbols");
    }
}
```

- [ ] **Step 2: Register the module**

In `src/parsing/languages/mod.rs`, add after the `mod typescript;` line:

```rust
mod css;
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test css::tests -- --test-threads=1 -v 2>&1`
Expected: Tests compile but FAIL (walk_node is a no-op).

- [ ] **Step 4: Commit**

```bash
git add src/parsing/languages/css.rs src/parsing/languages/mod.rs
git commit -m "test: add failing CSS extractor tests"
```

---

### Task 5: CSS extractor — implementation

**Files:**
- Modify: `src/parsing/languages/css.rs`

- [ ] **Step 1: Implement walk_node for CSS**

Replace the `walk_node` function in `css.rs`:

```rust
fn walk_node(
    node: &Node,
    source: &str,
    depth: u32,
    sort_order: &mut u32,
    symbols: &mut Vec<SymbolRecord>,
) {
    match node.kind() {
        "rule_set" => {
            // Extract full selector text as symbol name
            if let Some(selectors_node) = node.child_by_field_name("selectors") {
                let name = selectors_node
                    .utf8_text(source.as_bytes())
                    .unwrap_or("")
                    .to_string();
                if !name.is_empty() {
                    push_symbol(node, source, name, SymbolKind::Other, depth, sort_order, symbols, &NO_DOC_SPEC);
                }
            }
            // Walk inside rule_set for custom properties
            walk_children(node, source, depth + 1, sort_order, symbols);
        }
        "declaration" => {
            // Check for custom properties (--var-name)
            if let Some(prop_node) = node.child_by_field_name("property") {
                let prop_name = prop_node
                    .utf8_text(source.as_bytes())
                    .unwrap_or("");
                if prop_name.starts_with("--") {
                    push_symbol(node, source, prop_name.to_string(), SymbolKind::Variable, depth, sort_order, symbols, &NO_DOC_SPEC);
                }
            }
        }
        // Only extract @media and @keyframes — intentionally selective.
        // @layer is deferred (tree-sitter-css 0.25.0 may lack node type).
        // Other at-rules (@import, @charset, @supports, etc.) are not
        // definitions and would create noise similar to what SCSS skips.
        "media_statement" => {
            let name = at_rule_name(node, source, "@media");
            push_symbol(node, source, name, SymbolKind::Module, depth, sort_order, symbols, &NO_DOC_SPEC);
            walk_children(node, source, depth + 1, sort_order, symbols);
        }
        "keyframes_statement" => {
            // Extract outer @keyframes only, skip inner steps
            let name = at_rule_name(node, source, "@keyframes");
            push_symbol(node, source, name, SymbolKind::Module, depth, sort_order, symbols, &NO_DOC_SPEC);
            // Do NOT recurse — inner steps are skipped
        }
        _ => {
            walk_children(node, source, depth, sort_order, symbols);
        }
    }
}

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

/// Build a descriptive name for an at-rule.
/// For `@media (max-width: 768px) { ... }` → `@media (max-width: 768px)`
/// For `@keyframes fade-in { ... }` → `@keyframes fade-in`
fn at_rule_name(node: &Node, source: &str, prefix: &str) -> String {
    let full_text = node.utf8_text(source.as_bytes()).unwrap_or("");
    // Take everything before the first `{`
    let before_brace = full_text.split('{').next().unwrap_or(full_text).trim();
    if before_brace.is_empty() {
        prefix.to_string()
    } else {
        before_brace.to_string()
    }
}
```

**Important:** The exact node types (`rule_set`, `declaration`, `at_rule`, `media_statement`, `keyframes_statement`) are based on tree-sitter-css grammar. During implementation, if tests fail, debug by printing the tree-sitter node tree for a CSS snippet:

```rust
// Debug helper — add temporarily if node types don't match
fn debug_tree(node: &Node, source: &str, depth: usize) {
    let indent = " ".repeat(depth * 2);
    let text = node.utf8_text(source.as_bytes()).unwrap_or("???");
    let preview: String = text.chars().take(40).collect();
    eprintln!("{indent}{} [{}-{}] «{preview}»", node.kind(), node.start_byte(), node.end_byte());
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        debug_tree(&child, source, depth + 1);
    }
}
```

- [ ] **Step 2: Run CSS tests**

Run: `cargo test css::tests -- --test-threads=1 -v 2>&1`
Expected: All pass. If node types don't match, use the debug helper above to inspect the parse tree and adjust node type strings.

- [ ] **Step 3: Commit**

```bash
git add src/parsing/languages/css.rs
git commit -m "feat: implement CSS symbol extractor"
```

---

## Chunk 3: SCSS Extractor

SCSS extends CSS with `$variable`, `@mixin`, `@function`. The extractor reuses CSS patterns.

### Task 6: SCSS extractor — failing tests

**Files:**
- Create: `src/parsing/languages/scss.rs`

- [ ] **Step 1: Create scss.rs with test scaffolding and failing tests**

Create `src/parsing/languages/scss.rs`:

```rust
use tree_sitter::Node;

use super::{push_symbol, NO_DOC_SPEC};
use crate::domain::{SymbolKind, SymbolRecord};

pub fn extract_symbols(node: &Node, source: &str) -> Vec<SymbolRecord> {
    let mut symbols = Vec::new();
    let mut sort_order = 0u32;
    walk_node(node, source, 0, &mut sort_order, &mut symbols);
    symbols
}

fn walk_node(
    _node: &Node,
    _source: &str,
    _depth: u32,
    _sort_order: &mut u32,
    _symbols: &mut Vec<SymbolRecord>,
) {
    // Implementation coming next
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::SymbolKind;
    use tree_sitter::Parser;

    fn parse_scss(source: &str) -> Vec<SymbolRecord> {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_scss::LANGUAGE.into();
        parser.set_language(&lang).expect("set SCSS language");
        let tree = parser.parse(source, None).expect("parse SCSS source");
        extract_symbols(&tree.root_node(), source)
    }

    #[test]
    fn test_scss_variable_extracted() {
        let source = "$primary-color: #333;";
        let symbols = parse_scss(source);
        let var = symbols.iter().find(|s| s.kind == SymbolKind::Variable);
        assert!(var.is_some(), "should extract $variable, got: {:?}", symbols);
        assert_eq!(var.unwrap().name, "$primary-color");
    }

    #[test]
    fn test_scss_mixin_extracted() {
        let source = "@mixin button-base { display: inline; }";
        let symbols = parse_scss(source);
        let m = symbols.iter().find(|s| s.kind == SymbolKind::Function);
        assert!(m.is_some(), "should extract @mixin, got: {:?}", symbols);
        assert_eq!(m.unwrap().name, "button-base");
    }

    #[test]
    fn test_scss_function_extracted() {
        let source = "@function darken-color($color) { @return $color; }";
        let symbols = parse_scss(source);
        let f = symbols.iter().find(|s| s.kind == SymbolKind::Function);
        assert!(f.is_some(), "should extract @function, got: {:?}", symbols);
        assert_eq!(f.unwrap().name, "darken-color");
    }

    #[test]
    fn test_scss_include_not_extracted() {
        let source = "@include button-base;";
        let symbols = parse_scss(source);
        assert!(symbols.is_empty(), "@include should be skipped, got: {:?}", symbols);
    }

    #[test]
    fn test_scss_use_forward_not_extracted() {
        let source = "@use 'variables';\n@forward 'mixins';";
        let symbols = parse_scss(source);
        assert!(symbols.is_empty(), "@use/@forward should be skipped, got: {:?}", symbols);
    }

    #[test]
    fn test_scss_css_selectors_also_work() {
        let source = ".btn { color: red; }";
        let symbols = parse_scss(source);
        let sel = symbols.iter().find(|s| s.kind == SymbolKind::Other);
        assert!(sel.is_some(), "CSS selector should work in SCSS, got: {:?}", symbols);
        assert_eq!(sel.unwrap().name, ".btn");
    }

    #[test]
    fn test_scss_custom_property_extracted() {
        let source = ":root { --gap: 8px; }";
        let symbols = parse_scss(source);
        let var = symbols.iter().find(|s| s.name == "--gap");
        assert!(var.is_some(), "custom property should work in SCSS, got: {:?}", symbols);
    }

    #[test]
    fn test_scss_empty_file() {
        let symbols = parse_scss("");
        assert!(symbols.is_empty(), "empty file should produce zero symbols");
    }
}
```

- [ ] **Step 2: Register the module**

In `src/parsing/languages/mod.rs`, add after the `mod css;` line:

```rust
mod scss;
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test scss::tests -- --test-threads=1 -v 2>&1`
Expected: Compile and FAIL (walk_node is a no-op).

- [ ] **Step 4: Commit**

```bash
git add src/parsing/languages/scss.rs src/parsing/languages/mod.rs
git commit -m "test: add failing SCSS extractor tests"
```

---

### Task 7: SCSS extractor — implementation

**Files:**
- Modify: `src/parsing/languages/scss.rs`

- [ ] **Step 1: Implement walk_node for SCSS**

Replace the `walk_node` function in `scss.rs`. SCSS grammar node types may differ from CSS — the SCSS grammar (serenadeai) has its own tree-sitter node structure. Key SCSS-specific node types to look for:
- `scss_variable` or similar for `$variable` declarations
- `mixin_statement` for `@mixin`
- `function_statement` for `@function`
- `include_statement` for `@include` (skip)
- `use_statement` / `forward_statement` for `@use`/`@forward` (skip)

```rust
fn walk_node(
    node: &Node,
    source: &str,
    depth: u32,
    sort_order: &mut u32,
    symbols: &mut Vec<SymbolRecord>,
) {
    match node.kind() {
        // CSS-compatible: rule sets
        "rule_set" => {
            if let Some(selectors_node) = node.child_by_field_name("selectors") {
                let name = selectors_node
                    .utf8_text(source.as_bytes())
                    .unwrap_or("")
                    .to_string();
                if !name.is_empty() {
                    push_symbol(node, source, name, SymbolKind::Other, depth, sort_order, symbols, &NO_DOC_SPEC);
                }
            }
            walk_children(node, source, depth + 1, sort_order, symbols);
        }
        // CSS-compatible: custom properties
        "declaration" => {
            if let Some(prop_node) = node.child_by_field_name("property") {
                let prop_name = prop_node.utf8_text(source.as_bytes()).unwrap_or("");
                if prop_name.starts_with("--") {
                    push_symbol(node, source, prop_name.to_string(), SymbolKind::Variable, depth, sort_order, symbols, &NO_DOC_SPEC);
                }
            }
        }
        // SCSS: $variable declaration
        "scss_declaration" | "variable_declaration" => {
            // Try to find the variable name child
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                let text = child.utf8_text(source.as_bytes()).unwrap_or("");
                if text.starts_with('$') {
                    push_symbol(node, source, text.to_string(), SymbolKind::Variable, depth, sort_order, symbols, &NO_DOC_SPEC);
                    break;
                }
            }
        }
        // SCSS: @mixin
        "mixin_statement" => {
            if let Some(name) = find_scss_name(node, source) {
                push_symbol(node, source, name, SymbolKind::Function, depth, sort_order, symbols, &NO_DOC_SPEC);
            }
        }
        // SCSS: @function
        "function_statement" => {
            if let Some(name) = find_scss_name(node, source) {
                push_symbol(node, source, name, SymbolKind::Function, depth, sort_order, symbols, &NO_DOC_SPEC);
            }
        }
        // Skip @include, @use, @forward
        "include_statement" | "use_statement" | "forward_statement" => {}
        // CSS-compatible: @media, @keyframes
        "media_statement" => {
            let name = at_rule_name(node, source, "@media");
            push_symbol(node, source, name, SymbolKind::Module, depth, sort_order, symbols, &NO_DOC_SPEC);
            walk_children(node, source, depth + 1, sort_order, symbols);
        }
        "keyframes_statement" => {
            let name = at_rule_name(node, source, "@keyframes");
            push_symbol(node, source, name, SymbolKind::Module, depth, sort_order, symbols, &NO_DOC_SPEC);
            // Do NOT recurse — inner steps skipped
        }
        // @layer deferred — not extracting generic at_rule nodes.
        // Only @media and @keyframes (handled above) are extracted.
        _ => {
            walk_children(node, source, depth, sort_order, symbols);
        }
    }
}

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

/// Extract the name identifier from an SCSS @mixin or @function node.
fn find_scss_name(node: &Node, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" || child.kind() == "name" {
            return Some(child.utf8_text(source.as_bytes()).unwrap_or("").to_string());
        }
    }
    None
}

/// Build descriptive name for an at-rule (text before `{`).
fn at_rule_name(node: &Node, source: &str, prefix: &str) -> String {
    let full_text = node.utf8_text(source.as_bytes()).unwrap_or("");
    let before_brace = full_text.split('{').next().unwrap_or(full_text).trim();
    if before_brace.is_empty() { prefix.to_string() } else { before_brace.to_string() }
}
```

**Important:** The SCSS tree-sitter node types above are best-guesses based on common tree-sitter-scss conventions. During implementation, if tests fail, use the debug tree helper (same as CSS Task 5) to print actual node types and adjust accordingly.

- [ ] **Step 2: Run SCSS tests**

Run: `cargo test scss::tests -- --test-threads=1 -v 2>&1`
Expected: All pass. Adjust node type strings if needed.

- [ ] **Step 3: Commit**

```bash
git add src/parsing/languages/scss.rs
git commit -m "feat: implement SCSS symbol extractor"
```

---

## Chunk 4: HTML/Angular Extractor

The most complex extractor. Has custom element detection (tag contains `-`), control-flow blocks, template refs, `@let`, and byte-range dedup.

### Task 8: HTML/Angular extractor — failing tests

**Files:**
- Create: `src/parsing/languages/html.rs`

- [ ] **Step 1: Create html.rs with test scaffolding and failing tests**

Create `src/parsing/languages/html.rs`:

```rust
use std::collections::HashSet;
use tree_sitter::Node;

use super::{push_symbol, NO_DOC_SPEC};
use crate::domain::{SymbolKind, SymbolRecord};

pub fn extract_symbols(node: &Node, source: &str) -> Vec<SymbolRecord> {
    let mut symbols = Vec::new();
    let mut sort_order = 0u32;
    let mut emitted: HashSet<(u32, u32)> = HashSet::new();
    walk_node(node, source, 0, &mut sort_order, &mut symbols, &mut emitted);
    symbols
}

fn walk_node(
    _node: &Node,
    _source: &str,
    _depth: u32,
    _sort_order: &mut u32,
    _symbols: &mut Vec<SymbolRecord>,
    _emitted: &mut HashSet<(u32, u32)>,
) {
    // Implementation coming next
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::SymbolKind;
    use tree_sitter::Parser;

    fn parse_html(source: &str) -> Vec<SymbolRecord> {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_angular::LANGUAGE.into();
        parser.set_language(&lang).expect("set Angular/HTML language");
        let tree = parser.parse(source, None).expect("parse HTML source");
        extract_symbols(&tree.root_node(), source)
    }

    #[test]
    fn test_html_top_level_element() {
        let source = "<header>content</header>";
        let symbols = parse_html(source);
        let el = symbols.iter().find(|s| s.name == "header");
        assert!(el.is_some(), "should extract top-level element, got: {:?}", symbols);
        assert_eq!(el.unwrap().kind, SymbolKind::Other);
    }

    #[test]
    fn test_html_custom_element_any_depth() {
        let source = "<div><section><app-header></app-header></section></div>";
        let symbols = parse_html(source);
        let custom = symbols.iter().find(|s| s.name == "app-header");
        assert!(custom.is_some(), "should extract custom element at any depth, got: {:?}", symbols);
    }

    #[test]
    fn test_html_ng_template() {
        let source = "<ng-template>content</ng-template>";
        let symbols = parse_html(source);
        let tmpl = symbols.iter().find(|s| s.name == "ng-template");
        assert!(tmpl.is_some(), "should extract ng-template, got: {:?}", symbols);
    }

    #[test]
    fn test_html_control_flow_if() {
        let source = "@if (condition) { <span>yes</span> }";
        let symbols = parse_html(source);
        let ctrl = symbols.iter().find(|s| s.name == "@if");
        assert!(ctrl.is_some(), "should extract @if, got: {:?}", symbols);
        assert_eq!(ctrl.unwrap().kind, SymbolKind::Module);
    }

    #[test]
    fn test_html_control_flow_for() {
        let source = "@for (item of items; track item.id) { <li>{{ item.name }}</li> }";
        let symbols = parse_html(source);
        let ctrl = symbols.iter().find(|s| s.name == "@for");
        assert!(ctrl.is_some(), "should extract @for, got: {:?}", symbols);
        assert_eq!(ctrl.unwrap().kind, SymbolKind::Module);
    }

    #[test]
    fn test_html_control_flow_switch() {
        let source = "@switch (value) { @case (1) { <span>one</span> } }";
        let symbols = parse_html(source);
        let ctrl = symbols.iter().find(|s| s.name == "@switch");
        assert!(ctrl.is_some(), "should extract @switch, got: {:?}", symbols);
        assert_eq!(ctrl.unwrap().kind, SymbolKind::Module);
    }

    #[test]
    fn test_html_control_flow_defer() {
        let source = "@defer (on viewport) { <app-heavy></app-heavy> }";
        let symbols = parse_html(source);
        let ctrl = symbols.iter().find(|s| s.name == "@defer");
        assert!(ctrl.is_some(), "should extract @defer, got: {:?}", symbols);
        assert_eq!(ctrl.unwrap().kind, SymbolKind::Module);
    }

    #[test]
    fn test_html_else_not_extracted() {
        let source = "@if (x) { <span>yes</span> } @else { <span>no</span> }";
        let symbols = parse_html(source);
        let else_sym = symbols.iter().find(|s| s.name.contains("@else"));
        assert!(else_sym.is_none(), "@else should NOT be separate symbol, got: {:?}", symbols);
    }

    #[test]
    fn test_html_empty_not_extracted() {
        let source = "@for (item of items; track item.id) { <p>{{ item.name }}</p> } @empty { <p>No items</p> }";
        let symbols = parse_html(source);
        let empty_sym = symbols.iter().find(|s| s.name.contains("@empty"));
        assert!(empty_sym.is_none(), "@empty should NOT be separate symbol, got: {:?}", symbols);
    }

    #[test]
    fn test_html_template_ref() {
        let source = "<input #myInput />";
        let symbols = parse_html(source);
        let tref = symbols.iter().find(|s| s.name == "myInput" && s.kind == SymbolKind::Variable);
        assert!(tref.is_some(), "should extract template ref, got: {:?}", symbols);
    }

    #[test]
    fn test_html_let_declaration() {
        let source = "@let user = currentUser();";
        let symbols = parse_html(source);
        let letvar = symbols.iter().find(|s| s.name == "user" && s.kind == SymbolKind::Variable);
        assert!(letvar.is_some(), "should extract @let, got: {:?}", symbols);
    }

    #[test]
    fn test_html_generic_nested_skipped() {
        let source = "<div><p><span>text</span></p></div>";
        let symbols = parse_html(source);
        // Only top-level div should be extracted
        assert_eq!(symbols.len(), 1, "only top-level element, got: {:?}", symbols);
        assert_eq!(symbols[0].name, "div");
    }

    #[test]
    fn test_html_plain_no_angular_noise() {
        let source = "<div><p>text</p></div>";
        let symbols = parse_html(source);
        let div = symbols.iter().find(|s| s.name == "div");
        assert!(div.is_some(), "should extract top-level div");
        // No Angular-specific symbols should appear
        let angular_noise: Vec<_> = symbols.iter().filter(|s| s.name.starts_with('@')).collect();
        assert!(angular_noise.is_empty(), "plain HTML should have no Angular noise, got: {:?}", angular_noise);
    }

    #[test]
    fn test_html_top_level_custom_element_not_duped() {
        let source = "<app-root>content</app-root>";
        let symbols = parse_html(source);
        let matches: Vec<_> = symbols.iter().filter(|s| s.name == "app-root").collect();
        assert_eq!(matches.len(), 1, "top-level custom element should appear once, got: {:?}", matches);
    }

    #[test]
    fn test_html_empty_file() {
        let symbols = parse_html("");
        assert!(symbols.is_empty(), "empty file should produce zero symbols");
    }
}
```

- [ ] **Step 2: Register the module**

In `src/parsing/languages/mod.rs`, add after the `mod css;` line (or alphabetically):

```rust
mod html;
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test html::tests -- --test-threads=1 -v 2>&1`
Expected: Compile and FAIL.

- [ ] **Step 4: Commit**

```bash
git add src/parsing/languages/html.rs src/parsing/languages/mod.rs
git commit -m "test: add failing HTML/Angular extractor tests"
```

---

### Task 9: HTML/Angular extractor — implementation

**Files:**
- Modify: `src/parsing/languages/html.rs`

- [ ] **Step 1: Implement walk_node for HTML/Angular**

Replace the `walk_node` function. The Angular grammar (`tree-sitter-angular`) may use node types like:
- `element` for HTML elements, with `start_tag` → `tag_name` children
- `if_block`, `for_block`, `switch_block`, `defer_block` for control flow
- `else_block`, `empty_block` for subordinate branches (skip)
- `template_ref` or attribute `#name` syntax
- `let_declaration` for `@let`

```rust
fn walk_node(
    node: &Node,
    source: &str,
    depth: u32,
    sort_order: &mut u32,
    symbols: &mut Vec<SymbolRecord>,
    emitted: &mut HashSet<(u32, u32)>,
) {
    let byte_key = (node.start_byte() as u32, node.end_byte() as u32);

    match node.kind() {
        "element" | "self_closing_tag" => {
            if let Some(tag_name) = extract_tag_name(node, source) {
                let is_top_level = depth == 0;
                let is_custom = tag_name.contains('-');
                let is_ng_template = tag_name == "ng-template";

                if (is_top_level || is_custom || is_ng_template) && emitted.insert(byte_key) {
                    push_symbol(node, source, tag_name, SymbolKind::Other, depth, sort_order, symbols, &NO_DOC_SPEC);
                }
            }
            // Always recurse to find nested custom elements
            walk_children(node, source, depth + 1, sort_order, symbols, emitted);
        }
        // Angular control flow
        "if_block" | "for_block" | "switch_block" | "defer_block" => {
            let name = format!("@{}", node.kind().trim_end_matches("_block"));
            if emitted.insert(byte_key) {
                push_symbol(node, source, name, SymbolKind::Module, depth, sort_order, symbols, &NO_DOC_SPEC);
            }
            walk_children(node, source, depth + 1, sort_order, symbols, emitted);
        }
        // Skip subordinate branches
        "else_block" | "else_if_block" | "empty_block" => {
            // Still recurse for nested custom elements
            walk_children(node, source, depth, sort_order, symbols, emitted);
        }
        // @let declaration
        "let_declaration" => {
            if let Some(name) = find_let_name(node, source) {
                if emitted.insert(byte_key) {
                    push_symbol(node, source, name, SymbolKind::Variable, depth, sort_order, symbols, &NO_DOC_SPEC);
                }
            }
        }
        _ => {
            // Check for template refs (#name) in attributes
            check_template_ref(node, source, depth, sort_order, symbols, emitted);
            walk_children(node, source, depth, sort_order, symbols, emitted);
        }
    }
}

fn walk_children(
    node: &Node,
    source: &str,
    depth: u32,
    sort_order: &mut u32,
    symbols: &mut Vec<SymbolRecord>,
    emitted: &mut HashSet<(u32, u32)>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_node(&child, source, depth, sort_order, symbols, emitted);
    }
}

/// Extract tag name from an element node.
/// For `element` nodes, the tag name lives inside a `start_tag` child.
/// For `self_closing_tag` nodes matched at the top-level, the `tag_name`
/// is a direct child — the fallback check below handles this case.
fn extract_tag_name(node: &Node, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "start_tag" || child.kind() == "self_closing_tag" {
            let mut inner_cursor = child.walk();
            for grandchild in child.children(&mut inner_cursor) {
                if grandchild.kind() == "tag_name" {
                    return Some(grandchild.utf8_text(source.as_bytes()).unwrap_or("").to_string());
                }
            }
        }
        // Fallback: tag_name as direct child (essential for self_closing_tag nodes)
        if child.kind() == "tag_name" {
            return Some(child.utf8_text(source.as_bytes()).unwrap_or("").to_string());
        }
    }
    None
}

/// Extract name from `@let name = expr`
fn find_let_name(node: &Node, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" || child.kind() == "name" {
            return Some(child.utf8_text(source.as_bytes()).unwrap_or("").to_string());
        }
    }
    // Fallback: parse from text
    let text = node.utf8_text(source.as_bytes()).unwrap_or("");
    if text.starts_with("@let ") {
        let rest = &text[5..];
        let name = rest.split(|c: char| !c.is_alphanumeric() && c != '_').next()?;
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }
    None
}

/// Check if a node is a template ref attribute (#name) and extract it.
fn check_template_ref(
    node: &Node,
    source: &str,
    depth: u32,
    sort_order: &mut u32,
    symbols: &mut Vec<SymbolRecord>,
    emitted: &mut HashSet<(u32, u32)>,
) {
    // Template refs appear as attributes like `#myRef`
    if node.kind() == "attribute" || node.kind() == "template_ref" {
        let text = node.utf8_text(source.as_bytes()).unwrap_or("");
        if text.starts_with('#') && text.len() > 1 {
            let ref_name = &text[1..];
            // Remove any value part (e.g., `#myRef="ngModel"` → `myRef`)
            let ref_name = ref_name.split('=').next().unwrap_or(ref_name).trim_matches('"');
            if !ref_name.is_empty() {
                let byte_key = (node.start_byte() as u32, node.end_byte() as u32);
                if emitted.insert(byte_key) {
                    push_symbol(node, source, ref_name.to_string(), SymbolKind::Variable, depth, sort_order, symbols, &NO_DOC_SPEC);
                }
            }
        }
    }
}
```

**Important:** Angular grammar node types are best-guesses. The `tree-sitter-angular` 0.8.4 grammar may use different names. During implementation:
1. Use the debug tree helper to inspect actual node types
2. Adjust `match` arms to match the real grammar
3. The core logic (dedup, custom-element detection, depth-based filtering) stays the same regardless of node type names

- [ ] **Step 2: Run HTML tests**

Run: `cargo test html::tests -- --test-threads=1 -v 2>&1`
Expected: All pass. Likely needs node type adjustments — iterate.

- [ ] **Step 3: Commit**

```bash
git add src/parsing/languages/html.rs
git commit -m "feat: implement HTML/Angular symbol extractor with dedup"
```

---

## Chunk 5: Pipeline Integration + Edit Gating

### Task 10: Wire extractors into parse_source and extract_symbols

**Files:**
- Modify: `src/parsing/mod.rs:127-168` (parse_source)
- Modify: `src/parsing/languages/mod.rs:112-136` (extract_symbols)

- [ ] **Step 1: Add grammar imports and match arms in parse_source**

In `src/parsing/mod.rs`, replace the `todo!()` arm added in Task 2 with:

```rust
        LanguageId::Html => tree_sitter_angular::LANGUAGE.into(),
        LanguageId::Css => tree_sitter_css::LANGUAGE.into(),
        LanguageId::Scss => tree_sitter_scss::LANGUAGE.into(),
```

Note: If `tree-sitter-angular` uses `language()` function instead of `LANGUAGE` constant, adjust to `tree_sitter_angular::language()`.

- [ ] **Step 2: Add match arms in extract_symbols**

In `src/parsing/languages/mod.rs`, replace the `todo!()` arm with:

```rust
        LanguageId::Html => html::extract_symbols(node, source),
        LanguageId::Css => css::extract_symbols(node, source),
        LanguageId::Scss => scss::extract_symbols(node, source),
```

- [ ] **Step 3: Run full test suite**

Run: `cargo test --all-targets -- --test-threads=1 2>&1 | tail -20`
Expected: All existing tests pass + all new extractor tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/parsing/mod.rs src/parsing/languages/mod.rs
git commit -m "feat: wire HTML, CSS, SCSS extractors into parsing pipeline"
```

---

### Task 11: Add edit_capability_for_language and rename check function

**Files:**
- Modify: `src/parsing/config_extractors/mod.rs` (add `edit_capability_for_language`)
- Modify: `src/protocol/tools.rs:2476-2502` (rename + update logic)

- [ ] **Step 1: Write failing tests for edit capability**

In `src/parsing/config_extractors/mod.rs`, add to the existing test module:

```rust
    #[test]
    fn test_edit_capability_for_language_frontend() {
        use super::edit_capability_for_language;
        use crate::domain::LanguageId;

        // Frontend languages should return TextEditSafe
        assert_eq!(edit_capability_for_language(&LanguageId::Html), Some(EditCapability::TextEditSafe));
        assert_eq!(edit_capability_for_language(&LanguageId::Css), Some(EditCapability::TextEditSafe));
        assert_eq!(edit_capability_for_language(&LanguageId::Scss), Some(EditCapability::TextEditSafe));

        // Config languages delegate to their extractor
        assert_eq!(edit_capability_for_language(&LanguageId::Json), Some(EditCapability::StructuralEditSafe));

        // Regular source languages return None (unrestricted)
        assert_eq!(edit_capability_for_language(&LanguageId::Rust), None);
        assert_eq!(edit_capability_for_language(&LanguageId::Python), None);
    }
```

In `src/protocol/tools.rs`, add to the existing test module (these test `check_edit_capability` which is private — must be unit tests, not integration tests):

```rust
    #[test]
    fn test_check_edit_capability_blocks_structural_for_frontend() {
        // replace_symbol_body requires StructuralEditSafe; Html is TextEditSafe → blocked
        let warning = TokenizorServer::check_edit_capability(
            &crate::domain::LanguageId::Html,
            crate::parsing::config_extractors::EditCapability::StructuralEditSafe,
            "replace_symbol_body",
        );
        assert!(warning.is_some(), "replace_symbol_body should be blocked for HTML");
        assert!(warning.as_ref().unwrap().contains("does not support structural edits"));
    }

    #[test]
    fn test_check_edit_capability_allows_text_edit_for_frontend() {
        // edit_within_symbol requires TextEditSafe; Css is TextEditSafe → allowed
        let warning = TokenizorServer::check_edit_capability(
            &crate::domain::LanguageId::Css,
            crate::parsing::config_extractors::EditCapability::TextEditSafe,
            "edit_within_symbol",
        );
        assert!(warning.is_none(), "edit_within_symbol should be allowed for CSS");
    }
```

**Note:** These tests reference `check_edit_capability` after the rename (Step 5). They will fail at Step 2 because the function doesn't exist yet — that's expected. They compile and pass after Steps 3-5.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_edit_capability_for_language -- --test-threads=1 -v 2>&1`
Expected: FAIL (function doesn't exist yet).

- [ ] **Step 3: Add edit_capability_for_language function**

In `src/parsing/config_extractors/mod.rs`, after the existing `edit_capability_for` function:

```rust
/// Unified edit capability check for all languages (config + source).
/// Returns `None` for languages with no edit restrictions (mature tree-sitter languages).
pub fn edit_capability_for_language(language: &LanguageId) -> Option<EditCapability> {
    // Config languages — delegate to their extractor
    if let Some(cap) = edit_capability_for(language) {
        return Some(cap);
    }
    // Source languages with restricted editing
    match language {
        LanguageId::Html | LanguageId::Css | LanguageId::Scss => Some(EditCapability::TextEditSafe),
        // All other source languages → None (unrestricted)
        _ => None,
    }
}
```

Add the necessary import if not already present:
```rust
use crate::domain::LanguageId;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test test_edit_capability_for_language -- --test-threads=1 -v 2>&1`
Expected: PASS.

- [ ] **Step 5: Rename check_config_edit_capability → check_edit_capability in tools.rs**

In `src/protocol/tools.rs`, rename the function and update its body:

1. Rename `check_config_edit_capability` → `check_edit_capability` (function definition ~line 2476)
2. Change the `use` line inside from `edit_capability_for` to `edit_capability_for_language`
3. Replace `edit_capability_for(language)` call with `edit_capability_for_language(language)`
4. Update the trailing comment from `// Non-config files (source code) → no restriction` to `// No capability restriction`
5. Update all 3 call sites:
   - `Self::check_config_edit_capability` → `Self::check_edit_capability` (~lines 2532, 2686, 2749)

The updated function:

```rust
    fn check_edit_capability(
        language: &crate::domain::LanguageId,
        required: crate::parsing::config_extractors::EditCapability,
        tool_name: &str,
    ) -> Option<String> {
        use crate::parsing::config_extractors::{EditCapability, edit_capability_for_language};
        if let Some(cap) = edit_capability_for_language(language) {
            let allowed = match required {
                EditCapability::IndexOnly => false,
                EditCapability::TextEditSafe => {
                    matches!(
                        cap,
                        EditCapability::TextEditSafe | EditCapability::StructuralEditSafe
                    )
                }
                EditCapability::StructuralEditSafe => {
                    matches!(cap, EditCapability::StructuralEditSafe)
                }
            };
            if !allowed {
                return Some(format!(
                    "{tool_name}: This file type ({language}) does not support structural edits via Tokenizor. Use edit_within_symbol for scoped text replacements, or the built-in Edit tool for raw text edits."
                ));
            }
        }
        None // No capability restriction
    }
```

- [ ] **Step 6: Run full test suite**

Run: `cargo test --all-targets -- --test-threads=1 2>&1 | tail -20`
Expected: All pass. The rename is mechanical — no behavior change for existing languages.

- [ ] **Step 7: Commit**

```bash
git add src/parsing/config_extractors/mod.rs src/protocol/tools.rs
git commit -m "feat: add unified edit_capability_for_language, rename check_edit_capability"
```

---

## Chunk 6: Integration Tests + Regression

### Task 12: Integration tests

**Files:**
- Create: `tests/frontend_assets.rs`

- [ ] **Step 1: Create integration test file**

Create `tests/frontend_assets.rs`:

```rust
//! Integration tests for HTML/Angular, CSS, and SCSS frontend asset parsing.

use tokenizor_agentic_mcp::domain::{LanguageId, SymbolKind};
use tokenizor_agentic_mcp::parsing::process_file;
use tokenizor_agentic_mcp::parsing::config_extractors::{EditCapability, edit_capability_for_language};

// ─── Indexing acceptance criteria ──────────────────────────────────────

#[test]
fn test_html_file_indexes_successfully() {
    let source = b"<app-header></app-header><main><app-sidebar></app-sidebar></main>";
    let result = process_file("app.component.html", source, LanguageId::Html);
    assert!(!result.symbols.is_empty(), "HTML file should have symbols");
    let names: Vec<&str> = result.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"app-header"), "should find app-header");
    assert!(names.contains(&"app-sidebar"), "should find app-sidebar");
}

#[test]
fn test_css_file_indexes_successfully() {
    let source = b".btn { color: red; }\n:root { --primary: blue; }";
    let result = process_file("styles.css", source, LanguageId::Css);
    assert!(!result.symbols.is_empty(), "CSS file should have symbols");
}

#[test]
fn test_scss_file_indexes_successfully() {
    let source = b"$gap: 8px;\n@mixin flex { display: flex; }\n.container { width: 100%; }";
    let result = process_file("styles.scss", source, LanguageId::Scss);
    assert!(!result.symbols.is_empty(), "SCSS file should have symbols");
    let names: Vec<&str> = result.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"$gap"), "should find $gap variable");
    assert!(names.contains(&"flex"), "should find flex mixin");
}

// ─── search_symbols(kind="variable") acceptance criteria ───────────────

#[test]
fn test_css_custom_properties_discoverable_by_kind() {
    let source = b":root { --primary: blue; --gap: 8px; }\n.btn { color: red; }";
    let result = process_file("tokens.css", source, LanguageId::Css);
    let variables: Vec<&str> = result.symbols.iter()
        .filter(|s| s.kind == SymbolKind::Variable)
        .map(|s| s.name.as_str())
        .collect();
    assert!(variables.contains(&"--primary"), "should find --primary via kind filter");
    assert!(variables.contains(&"--gap"), "should find --gap via kind filter");
}

#[test]
fn test_scss_variables_discoverable_by_kind() {
    let source = b"$primary: #333;\n$gap: 8px;\n.btn { color: $primary; }";
    let result = process_file("vars.scss", source, LanguageId::Scss);
    let variables: Vec<&str> = result.symbols.iter()
        .filter(|s| s.kind == SymbolKind::Variable)
        .map(|s| s.name.as_str())
        .collect();
    assert!(variables.contains(&"$primary"), "should find $primary via kind filter");
    assert!(variables.contains(&"$gap"), "should find $gap via kind filter");
}

// ─── get_file_context acceptance criteria ──────────────────────────────
// get_file_context relies on process_file producing a structured symbol
// outline. Verify the outline structure is suitable for all three types.

#[test]
fn test_html_file_produces_structured_outline() {
    let source = b"<app-header></app-header>\n@if (show) { <app-body></app-body> }\n<app-footer></app-footer>";
    let result = process_file("app.component.html", source, LanguageId::Html);
    assert!(result.symbols.len() >= 3, "HTML outline should have multiple symbols, got: {:?}", result.symbols);
    // Verify symbols have valid line ranges (needed by get_file_context)
    for sym in &result.symbols {
        assert!(sym.line_range.0 <= sym.line_range.1, "invalid line range for {}", sym.name);
    }
}

#[test]
fn test_css_file_produces_structured_outline() {
    let source = b".btn { color: red; }\n@media (max-width: 768px) { .mobile { display: none; } }";
    let result = process_file("styles.css", source, LanguageId::Css);
    assert!(result.symbols.len() >= 2, "CSS outline should have multiple symbols, got: {:?}", result.symbols);
}

#[test]
fn test_scss_file_produces_structured_outline() {
    let source = b"$gap: 8px;\n@mixin flex { display: flex; }\n.container { width: 100%; }";
    let result = process_file("styles.scss", source, LanguageId::Scss);
    assert!(result.symbols.len() >= 3, "SCSS outline should have multiple symbols, got: {:?}", result.symbols);
}

// ─── Edit safety acceptance criteria ───────────────────────────────────

#[test]
fn test_edit_capability_gating_for_frontend() {
    // All frontend languages gated at TextEditSafe
    assert_eq!(edit_capability_for_language(&LanguageId::Html), Some(EditCapability::TextEditSafe));
    assert_eq!(edit_capability_for_language(&LanguageId::Css), Some(EditCapability::TextEditSafe));
    assert_eq!(edit_capability_for_language(&LanguageId::Scss), Some(EditCapability::TextEditSafe));
}

// NOTE: replace_symbol_body/edit_within_symbol blocking tests live in
// src/protocol/tools.rs as unit tests (check_edit_capability is private).
// See Task 11 Step 1 for those tests.
```

**API visibility resolution:**
- `process_file` — `pub fn` in `src/parsing/mod.rs`, accessible from integration tests via `tokenizor_agentic_mcp::parsing::process_file`
- `edit_capability_for_language` — `pub fn` in `src/parsing/config_extractors/mod.rs`, accessible via `tokenizor_agentic_mcp::parsing::config_extractors::edit_capability_for_language`
- `EditCapability` — `pub enum` in same module, accessible
- `check_edit_capability` — **private** method on `TokenizorServer`. Tests for this function (replace_symbol_body blocked, edit_within_symbol allowed) are unit tests in `src/protocol/tools.rs` (added in Task 11 Step 1)

- [ ] **Step 2: Run integration tests**

Run: `cargo test frontend_assets -- --test-threads=1 -v 2>&1`
Expected: All pass.

- [ ] **Step 3: Run full regression suite**

Run: `cargo test --all-targets -- --test-threads=1 2>&1 | tail -30`
Expected: All 932+ existing tests still pass, plus all new tests.

- [ ] **Step 4: Run cargo fmt check**

Run: `cargo fmt -- --check 2>&1`
Expected: No formatting issues.

- [ ] **Step 5: Commit**

```bash
git add tests/frontend_assets.rs
git commit -m "test: add frontend asset integration tests"
```

---

### Task 13: Final verification

- [ ] **Step 1: Full test suite**

Run: `cargo test --all-targets -- --test-threads=1 2>&1 | tail -5`
Expected: All tests pass, zero failures.

- [ ] **Step 2: cargo fmt**

Run: `cargo fmt -- --check`
Expected: Clean.

- [ ] **Step 3: cargo check**

Run: `cargo check 2>&1 | tail -5`
Expected: Clean compilation.

- [ ] **Step 4: Verify symbol count increase**

The crate should now support 19 source languages (16 original + Html + Css + Scss) plus 5 config formats = 24 total `LanguageId` variants.

- [ ] **Step 5: Commit any remaining formatting fixes**

```bash
cargo fmt
git add -A
git commit -m "style: apply rustfmt formatting"
```
