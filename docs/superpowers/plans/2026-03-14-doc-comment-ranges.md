# Doc Comment Ranges Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix orphaned doc comments by adding `doc_byte_range` to `SymbolRecord`, populated at parse time, and used by all edit tools.

**Architecture:** Add a `DocCommentSpec` per language that drives a shared `scan_doc_range` helper in the parsing layer. Edit tools (`replace_symbol_body`, `delete_symbol`, `insert_before`, `edit_within`, `batch_edit`) and body extraction (`render_symbol_detail`, `capture_context_bundle_view`) use the new field when present.

**Tech Stack:** Rust, tree-sitter, postcard serialization

**Spec:** `docs/superpowers/specs/2026-03-14-doc-comment-ranges-design.md`

---

## Chunk 1: Data Model and Core Algorithm

### Task 1: Add `doc_byte_range` field to `SymbolRecord`

**Files:**
- Modify: `src/domain/index.rs:229-236` (struct definition)

This is the foundational change. Every other task depends on it. Adding the field will break compilation at ~108 construction sites — Task 2 fixes them all.

- [ ] **Step 1: Add the field to `SymbolRecord`**

In `src/domain/index.rs`, add `doc_byte_range` after `line_range`:

```rust
pub struct SymbolRecord {
    pub name: String,
    pub kind: SymbolKind,
    pub depth: u32,
    pub sort_order: u32,
    pub byte_range: (u32, u32),
    pub line_range: (u32, u32),
    pub doc_byte_range: Option<(u32, u32)>,
}
```

- [ ] **Step 2: Verify compilation fails at expected sites**

Run: `cargo check 2>&1 | grep "missing field" | head -5`
Expected: Many errors about `missing field 'doc_byte_range'`

Do NOT commit yet — Task 2 fixes the compilation.

---

### Task 2: Fix all `SymbolRecord` construction sites

**Files (54 sites in `src/`, 54 in `tests/`):**
- Modify: `src/parsing/languages/mod.rs:52` (`push_symbol`)
- Modify: `src/live_index/persist.rs` (2 `make_symbol` helpers)
- Modify: `src/live_index/query.rs` (~29 sites — test helpers and `make_symbol` functions)
- Modify: `src/live_index/search.rs` (~2 sites)
- Modify: `src/live_index/store.rs` (~2 sites)
- Modify: `src/protocol/format.rs` (~4 sites)
- Modify: `src/protocol/tools.rs` (~3 sites — test helpers)
- Modify: `src/protocol/edit.rs` (~4 sites — test helpers)
- Modify: `src/protocol/resources.rs` (~1 site)
- Modify: `src/sidecar/handlers.rs` (~2 sites)
- Modify: `src/domain/index.rs` (~4 sites beyond the struct def)
- Modify: `tests/sidecar_integration.rs` (~53 sites)
- Modify: `tests/hook_enrichment_integration.rs` (~1 site)

Every `SymbolRecord { ... }` construction needs `doc_byte_range: None` added. This is a mechanical find-and-replace.

- [ ] **Step 1: Add `doc_byte_range: None` to `push_symbol` in `mod.rs`**

In `src/parsing/languages/mod.rs`, the `push_symbol` function at line 52. Add the field to the `SymbolRecord` construction:

```rust
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
    doc_byte_range: None,
});
```

- [ ] **Step 2: Fix all remaining `src/` construction sites**

Search for all `SymbolRecord {` in `src/` and add `doc_byte_range: None` to each. Key files:

- `src/domain/index.rs` — `find_enclosing_symbol` test helpers, `From` impl
- `src/live_index/persist.rs` — `make_symbol` helpers in tests
- `src/live_index/query.rs` — `make_symbol`, `make_symbol_with_kind_and_line`, `make_symbol_with_kind_line_and_bytes`, `make_indexed_file`, `make_file_with_refs_and_content`, and test helpers
- `src/live_index/search.rs` — `make_symbol` test helper
- `src/live_index/store.rs` — test helpers
- `src/protocol/format.rs` — test helpers
- `src/protocol/resources.rs` — test helpers
- `src/sidecar/handlers.rs` — `SymbolContextParams` defaults

- [ ] **Step 3: Fix all `tests/` construction sites**

- `tests/sidecar_integration.rs` — ~53 sites
- `tests/hook_enrichment_integration.rs` — ~1 site

- [ ] **Step 4: Verify compilation succeeds**

Run: `cargo check`
Expected: Clean compilation (0 errors)

- [ ] **Step 5: Run all tests to verify no regressions**

Run: `cargo test --all-targets -- --test-threads=1`
Expected: All tests pass (existing behavior unchanged since all `doc_byte_range` values are `None`)

- [ ] **Step 6: Commit**

```bash
git add src/ tests/
git commit -m "feat: add doc_byte_range field to SymbolRecord

Add Option<(u32, u32)> field to track attached doc comment byte
ranges. All construction sites initialized to None. No behavioral
change yet."
```

---

### Task 3: Add `DocCommentSpec` struct and `scan_doc_range` helper

**Files:**
- Modify: `src/parsing/languages/mod.rs` (add struct, helper, tests)

This is the core algorithm. TDD: write tests first using real tree-sitter parsing.

- [ ] **Step 1: Write the `DocCommentSpec` struct**

In `src/parsing/languages/mod.rs`, add after the existing `use` statements (before `extract_symbols`). Note: `use tree_sitter::Node` is already imported at line 18 — do not add it again.

```rust
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
```

- [ ] **Step 2: Write failing tests for `scan_doc_range`**

Add to the `tests` module in `src/parsing/languages/mod.rs`:

```rust
#[test]
fn test_scan_doc_range_rust_doc_comments() {
    let source = "/// Doc line 1\n/// Doc line 2\npub fn foo() {}\n";
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
    let tree = parser.parse(source, None).unwrap();
    let root = tree.root_node();
    // The function_item is the last named child
    let func_node = root.named_child(root.named_child_count() - 1).unwrap();
    assert_eq!(func_node.kind(), "function_item");
    let spec = DocCommentSpec {
        comment_node_types: &["line_comment", "block_comment"],
        doc_prefixes: Some(&["///", "//!", "/**", "/*!"]),
        custom_doc_check: None,
    };
    let range = scan_doc_range(&func_node, source, &spec);
    assert!(range.is_some(), "Expected doc range for /// comments");
    let (start, end) = range.unwrap();
    let doc_text = &source[start as usize..end as usize];
    assert!(doc_text.contains("/// Doc line 1"));
    assert!(doc_text.contains("/// Doc line 2"));
}

#[test]
fn test_scan_doc_range_regular_comment_not_captured() {
    let source = "// Regular comment\npub fn foo() {}\n";
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
    let tree = parser.parse(source, None).unwrap();
    let root = tree.root_node();
    let func_node = root.named_child(root.named_child_count() - 1).unwrap();
    let spec = DocCommentSpec {
        comment_node_types: &["line_comment", "block_comment"],
        doc_prefixes: Some(&["///", "//!", "/**", "/*!"]),
        custom_doc_check: None,
    };
    let range = scan_doc_range(&func_node, source, &spec);
    assert!(range.is_none(), "Regular // comment should not be captured as doc");
}

#[test]
fn test_scan_doc_range_blank_line_stops_scan() {
    let source = "/// Detached doc\n\n/// Attached doc\npub fn foo() {}\n";
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
    let tree = parser.parse(source, None).unwrap();
    let root = tree.root_node();
    let func_node = root.named_child(root.named_child_count() - 1).unwrap();
    let spec = DocCommentSpec {
        comment_node_types: &["line_comment", "block_comment"],
        doc_prefixes: Some(&["///", "//!", "/**", "/*!"]),
        custom_doc_check: None,
    };
    let range = scan_doc_range(&func_node, source, &spec);
    assert!(range.is_some());
    let (start, _end) = range.unwrap();
    let doc_text = &source[start as usize..];
    assert!(!doc_text.contains("Detached"), "Should stop at blank line");
    assert!(doc_text.contains("Attached"));
}

#[test]
fn test_scan_doc_range_no_doc_spec_returns_none() {
    let source = "/// Doc comment\npub fn foo() {}\n";
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
    let tree = parser.parse(source, None).unwrap();
    let root = tree.root_node();
    let func_node = root.named_child(root.named_child_count() - 1).unwrap();
    let range = scan_doc_range(&func_node, source, &NO_DOC_SPEC);
    assert!(range.is_none(), "Empty spec should return None");
}

#[test]
fn test_scan_doc_range_all_adjacent_comments_go_style() {
    let source = "// Package doc\n// More doc\nfunc Foo() {}\n";
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_go::LANGUAGE.into()).unwrap();
    let tree = parser.parse(source, None).unwrap();
    let root = tree.root_node();
    // In Go, function_declaration is a top-level node
    let mut func_node = None;
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "function_declaration" {
            func_node = Some(child);
            break;
        }
    }
    let func_node = func_node.expect("Expected function_declaration node");
    let spec = DocCommentSpec {
        comment_node_types: &["comment"],
        doc_prefixes: None, // Go: all adjacent comments are docs
        custom_doc_check: None,
    };
    let range = scan_doc_range(&func_node, source, &spec);
    assert!(range.is_some());
    let (start, end) = range.unwrap();
    let doc_text = &source[start as usize..end as usize];
    assert!(doc_text.contains("Package doc"));
    assert!(doc_text.contains("More doc"));
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p tokenizor_agentic_mcp --lib -- languages::tests::test_scan_doc_range 2>&1 | head -20`
Expected: FAIL — `scan_doc_range` not found

- [ ] **Step 4: Implement `scan_doc_range`**

In `src/parsing/languages/mod.rs`, add the helper function:

```rust
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
        let sibling_end_row = sibling.end_position().row;
        if next_start_row > sibling_end_row + 1 {
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
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p tokenizor_agentic_mcp --lib -- languages::tests::test_scan_doc_range -v`
Expected: All 5 new tests PASS

- [ ] **Step 6: Run full test suite**

Run: `cargo test --all-targets -- --test-threads=1`
Expected: All tests pass

- [ ] **Step 7: Commit**

```bash
git add src/parsing/languages/mod.rs
git commit -m "feat: add DocCommentSpec and scan_doc_range algorithm

Implements the core doc comment detection logic. Walks backward
through tree-sitter prev_sibling() nodes, matching against
per-language comment node types and doc prefixes. Stops at blank
lines. Includes unit tests for Rust and Go grammars."
```

---

### Task 4: Update `push_symbol` / `push_named_symbol` to call `scan_doc_range`

**Files:**
- Modify: `src/parsing/languages/mod.rs:52-93` (both functions)

- [ ] **Step 1: Update `push_symbol` signature and body**

Change `push_symbol` to accept `source` and `doc_spec`, and call `scan_doc_range`:

```rust
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
```

- [ ] **Step 2: Update `push_named_symbol` to pass `doc_spec` through**

```rust
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
    push_symbol(node, source, name, symbol_kind, depth, sort_order, symbols, doc_spec);
    true
}
```

- [ ] **Step 3: Verify compilation fails at call sites**

Run: `cargo check 2>&1 | grep "error" | head -10`
Expected: Errors in all 16 language files that call `push_symbol` or `push_named_symbol` — these are fixed in Task 5.

- [ ] **Step 4: Update existing `push_named_symbol` test in `mod.rs`**

The test `test_push_named_symbol_records_metadata_and_advances_sort_order` (around line 152) calls `push_named_symbol` and will fail compilation with the new `doc_spec` parameter. Update the call to pass `&NO_DOC_SPEC` as the last argument.

Do NOT commit yet — Task 5 fixes the language file callers.

---

## Chunk 2: Language Specs, Persistence, and Wiring

### Task 5: Add per-language `DOC_SPEC` constants and update call sites

**Files (16 language files):**
- Modify: `src/parsing/languages/rust.rs`
- Modify: `src/parsing/languages/python.rs`
- Modify: `src/parsing/languages/javascript.rs`
- Modify: `src/parsing/languages/typescript.rs`
- Modify: `src/parsing/languages/go.rs`
- Modify: `src/parsing/languages/java.rs`
- Modify: `src/parsing/languages/c.rs`
- Modify: `src/parsing/languages/cpp.rs`
- Modify: `src/parsing/languages/csharp.rs`
- Modify: `src/parsing/languages/ruby.rs`
- Modify: `src/parsing/languages/php.rs`
- Modify: `src/parsing/languages/swift.rs`
- Modify: `src/parsing/languages/kotlin.rs`
- Modify: `src/parsing/languages/dart.rs`
- Modify: `src/parsing/languages/perl.rs`
- Modify: `src/parsing/languages/elixir.rs`

Each language file needs: (a) a `DOC_SPEC` constant, and (b) updated calls to `push_symbol`/`push_named_symbol` to pass `source` and `&DOC_SPEC`.

- [ ] **Step 1: Rust — add DOC_SPEC and update walk_node**

In `src/parsing/languages/rust.rs`:

```rust
use super::{DocCommentSpec, push_named_symbol, walk_children};

pub(super) const DOC_SPEC: DocCommentSpec = DocCommentSpec {
    comment_node_types: &["line_comment", "block_comment"],
    doc_prefixes: Some(&["///", "//!", "/**", "/*!"]),
    custom_doc_check: None,
};
```

Update `walk_node` — the `push_named_symbol` call needs `&DOC_SPEC` as the last argument:

```rust
push_named_symbol(
    node, source, depth, sort_order, symbols, kind,
    |node, source, _| find_name(node, source),
    &DOC_SPEC,
);
```

- [ ] **Step 2: Python, Dart — use NO_DOC_SPEC**

These languages have no sibling-based doc comments. In `src/parsing/languages/python.rs`:

```rust
use super::{DocCommentSpec, push_named_symbol, walk_children, NO_DOC_SPEC};
```

Update `push_named_symbol` call to pass `&NO_DOC_SPEC`.

Same for `src/parsing/languages/dart.rs`.

- [ ] **Step 3: JavaScript, TypeScript — JSDoc spec**

In both `src/parsing/languages/javascript.rs` and `typescript.rs`:

```rust
pub(super) const DOC_SPEC: DocCommentSpec = DocCommentSpec {
    comment_node_types: &["comment"],
    doc_prefixes: Some(&["/**"]),
    custom_doc_check: None,
};
```

Update both `push_named_symbol` AND direct `push_symbol` calls in `extract_variable_declarations` to pass `source` and `&DOC_SPEC`.

For `push_symbol` calls, the old signature was:
```rust
push_symbol(node, name, kind, depth, sort_order, symbols);
```
New signature:
```rust
push_symbol(node, source, name, kind, depth, sort_order, symbols, &DOC_SPEC);
```

- [ ] **Step 4: Go — all-adjacent-comments spec**

In `src/parsing/languages/go.rs`:

```rust
pub(super) const DOC_SPEC: DocCommentSpec = DocCommentSpec {
    comment_node_types: &["comment"],
    doc_prefixes: None, // All adjacent comments are godoc
    custom_doc_check: None,
};
```

Update `push_named_symbol` in `walk_node` and both direct `push_symbol` calls in `extract_type_declarations` and `extract_var_declarations`. Note: Go passes `&child` (not `node`) to `push_symbol` — preserve this.

- [ ] **Step 5: Java — Javadoc spec**

In `src/parsing/languages/java.rs`:

```rust
pub(super) const DOC_SPEC: DocCommentSpec = DocCommentSpec {
    comment_node_types: &["line_comment", "block_comment"],
    doc_prefixes: Some(&["/**"]),
    custom_doc_check: None,
};
```

Update `push_named_symbol` in `walk_node` and direct `push_symbol` call in `extract_field`.

- [ ] **Step 6: C, C++ — Doxygen spec**

In both `src/parsing/languages/c.rs` and `cpp.rs`:

```rust
pub(super) const DOC_SPEC: DocCommentSpec = DocCommentSpec {
    comment_node_types: &["comment"],
    doc_prefixes: Some(&["///", "/**", "//!", "/*!"]),
    custom_doc_check: None,
};
```

- [ ] **Step 7: C# — XML doc comments spec**

In `src/parsing/languages/csharp.rs`:

```rust
pub(super) const DOC_SPEC: DocCommentSpec = DocCommentSpec {
    comment_node_types: &["comment"],
    doc_prefixes: Some(&["///"]),
    custom_doc_check: None,
};
```

- [ ] **Step 8: Ruby, Perl — all-adjacent-comments spec**

In both `src/parsing/languages/ruby.rs` and `perl.rs`:

```rust
pub(super) const DOC_SPEC: DocCommentSpec = DocCommentSpec {
    comment_node_types: &["comment"],
    doc_prefixes: None,
    custom_doc_check: None,
};
```

- [ ] **Step 9: Swift — Swift doc spec**

In `src/parsing/languages/swift.rs`:

```rust
pub(super) const DOC_SPEC: DocCommentSpec = DocCommentSpec {
    comment_node_types: &["comment", "multiline_comment"],
    doc_prefixes: Some(&["///", "/**"]),
    custom_doc_check: None,
};
```

- [ ] **Step 10: Kotlin — KDoc spec**

In `src/parsing/languages/kotlin.rs`:

```rust
pub(super) const DOC_SPEC: DocCommentSpec = DocCommentSpec {
    comment_node_types: &["line_comment", "multiline_comment"],
    doc_prefixes: Some(&["/**"]),
    custom_doc_check: None,
};
```

Note: verify node type names against `tree-sitter-kotlin-sg` at runtime. If tests fail, check actual node types by parsing a Kotlin file and inspecting.

- [ ] **Step 11: PHP — PHPDoc spec**

In `src/parsing/languages/php.rs`:

```rust
pub(super) const DOC_SPEC: DocCommentSpec = DocCommentSpec {
    comment_node_types: &["comment"],
    doc_prefixes: Some(&["/**"]),
    custom_doc_check: None,
};
```

- [ ] **Step 12: Elixir — custom doc check**

In `src/parsing/languages/elixir.rs`:

```rust
use super::{DocCommentSpec, push_symbol, walk_children};

fn is_elixir_doc(node: &tree_sitter::Node, source: &str) -> bool {
    let start = node.start_byte();
    let end = node.end_byte();
    if end > source.len() {
        return false;
    }
    let text = source[start..end].trim_start();
    text.starts_with("@doc") || text.starts_with("@moduledoc") || text.starts_with("@typedoc")
}

pub(super) const DOC_SPEC: DocCommentSpec = DocCommentSpec {
    comment_node_types: &["comment"],
    doc_prefixes: None,
    custom_doc_check: Some(is_elixir_doc),
};
```

Update the `push_symbol` call in `walk_node` to pass `source` and `&DOC_SPEC`.

- [ ] **Step 13: Verify compilation succeeds**

Run: `cargo check`
Expected: Clean compilation

- [ ] **Step 14: Run all tests**

Run: `cargo test --all-targets -- --test-threads=1`
Expected: All tests pass

- [ ] **Step 15: Commit**

```bash
git add src/parsing/languages/
git commit -m "feat: add per-language DocCommentSpec and wire into push_symbol

Each of 16 languages gets a DOC_SPEC constant defining its doc
comment node types and prefixes. push_symbol and push_named_symbol
now call scan_doc_range to populate doc_byte_range on every
SymbolRecord."
```

---

### Task 6: Add integration tests for doc range detection

**Files:**
- Modify: `src/parsing/languages/mod.rs` (test module) or a new integration test

Verify `extract_symbols` populates `doc_byte_range` correctly end-to-end.

- [ ] **Step 1: Write integration tests**

Add to the `tests` module in `src/parsing/languages/mod.rs`:

```rust
#[test]
fn test_extract_symbols_rust_populates_doc_range() {
    let source = "/// My function\n/// Does stuff\npub fn foo() {}\n";
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
    let tree = parser.parse(source, None).unwrap();
    let symbols = extract_symbols(&tree.root_node(), source, &crate::domain::LanguageId::Rust);
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "foo");
    let doc_range = symbols[0].doc_byte_range.expect("Should have doc_byte_range");
    let doc_text = &source[doc_range.0 as usize..doc_range.1 as usize];
    assert!(doc_text.contains("/// My function"));
    assert!(doc_text.contains("/// Does stuff"));
}

#[test]
fn test_extract_symbols_python_no_doc_range() {
    let source = "# A comment\ndef foo():\n    pass\n";
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_python::LANGUAGE.into()).unwrap();
    let tree = parser.parse(source, None).unwrap();
    let symbols = extract_symbols(&tree.root_node(), source, &crate::domain::LanguageId::Python);
    assert_eq!(symbols.len(), 1);
    assert!(symbols[0].doc_byte_range.is_none(), "Python should not detect # as doc");
}

#[test]
fn test_extract_symbols_java_javadoc() {
    let source = "/** Javadoc */\nclass Foo {}\n";
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
    let tree = parser.parse(source, None).unwrap();
    let symbols = extract_symbols(&tree.root_node(), source, &crate::domain::LanguageId::Java);
    let class_sym = symbols.iter().find(|s| s.name == "Foo").expect("Should find Foo");
    let doc_range = class_sym.doc_byte_range.expect("Should have Javadoc range");
    let doc_text = &source[doc_range.0 as usize..doc_range.1 as usize];
    assert!(doc_text.contains("/** Javadoc */"));
}
```

- [ ] **Step 2: Run the integration tests**

Run: `cargo test -p tokenizor_agentic_mcp --lib -- languages::tests::test_extract_symbols -v`
Expected: All 3 new tests PASS

- [ ] **Step 3: Commit**

```bash
git add src/parsing/languages/mod.rs
git commit -m "test: add integration tests for doc_byte_range population

Verify extract_symbols correctly populates doc_byte_range for
Rust (/// comments), Java (/** Javadoc */), and correctly skips
Python (# comments not doc comments)."
```

---

### Task 7: Bump persistence version

**Files:**
- Modify: `src/live_index/persist.rs:21`

- [ ] **Step 1: Bump CURRENT_VERSION from 2 to 3**

In `src/live_index/persist.rs:21`:

```rust
const CURRENT_VERSION: u32 = 3;
```

- [ ] **Step 2: Run tests**

Run: `cargo test --all-targets -- --test-threads=1`
Expected: All tests pass (snapshot tests may use `CURRENT_VERSION` — verify they adapt)

- [ ] **Step 3: Commit**

```bash
git add src/live_index/persist.rs
git commit -m "feat: bump index snapshot version to 3 for doc_byte_range

Old v2 snapshots will fail deserialization and trigger automatic
re-index on first load. No manual migration needed."
```

---

## Chunk 3: Body Extraction, Edit Tools, and Final Tests

### Task 8: Update body extraction to include doc comments

**Files:**
- Modify: `src/protocol/format.rs:130-165` (`render_symbol_detail`)
- Modify: `src/live_index/query.rs:1552-1665` (`capture_context_bundle_view`)

Note: `context_bundle_result` (format.rs:1835) delegates to `capture_context_bundle_view` — updating query.rs covers it transitively.

- [ ] **Step 1: Add helper function `effective_start`**

In `src/domain/index.rs`, add a method or free function near the `SymbolRecord` struct:

```rust
impl SymbolRecord {
    /// Returns the effective start byte, including doc comments if present.
    pub fn effective_start(&self) -> u32 {
        self.doc_byte_range.map_or(self.byte_range.0, |(start, _)| start)
    }
}
```

- [ ] **Step 2: Update `render_symbol_detail` in `format.rs`**

In `src/protocol/format.rs`, change the body extraction in `render_symbol_detail` (around line 148):

From:
```rust
let start = s.byte_range.0 as usize;
let end = s.byte_range.1 as usize;
```

To:
```rust
let start = s.effective_start() as usize;
let end = s.byte_range.1 as usize;
```

- [ ] **Step 3: Update `capture_context_bundle_view` in `query.rs`**

In `src/live_index/query.rs`, change the body extraction (around line 1598):

From:
```rust
let start = sym_rec.byte_range.0 as usize;
let end = sym_rec.byte_range.1 as usize;
```

To:
```rust
let start = sym_rec.effective_start() as usize;
let end = sym_rec.byte_range.1 as usize;
```

- [ ] **Step 4: Run tests**

Run: `cargo test --all-targets -- --test-threads=1`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add src/domain/index.rs src/protocol/format.rs src/live_index/query.rs
git commit -m "feat: include doc comments in symbol body extraction

render_symbol_detail and capture_context_bundle_view now use
effective_start() which includes doc_byte_range when present.
get_symbol and get_symbol_context(bundle=true) now return doc
comments as part of the symbol body."
```

---

### Task 9: Update edit tools to use `doc_byte_range`

**Files:**
- Modify: `src/protocol/tools.rs:2270-2347` (`replace_symbol_body`)
- Modify: `src/protocol/edit.rs:129-147` (`build_insert_before`)
- Modify: `src/protocol/edit.rs:173-200` (`build_delete`)
- Modify: `src/protocol/edit.rs:225-261` (`build_edit_within`)
- Modify: `src/protocol/edit.rs:380-541` (`execute_batch_edit`)

- [ ] **Step 1: Update `replace_symbol_body` in `tools.rs`**

In `src/protocol/tools.rs`, change the `line_start` calculation (around line 2303):

From:
```rust
let sym_start = sym.byte_range.0 as usize;
let line_start = file.content[..sym_start]
    .iter()
    .rposition(|&b| b == b'\n')
    .map(|p| p + 1)
    .unwrap_or(0) as u32;
```

To:
```rust
let effective = sym.effective_start() as usize;
let line_start = file.content[..effective]
    .iter()
    .rposition(|&b| b == b'\n')
    .map(|p| p + 1)
    .unwrap_or(0) as u32;
```

- [ ] **Step 2: Update `build_delete` in `edit.rs`**

In `src/protocol/edit.rs`, change the start calculation (around line 175):

From:
```rust
let start = {
    let s = sym.byte_range.0 as usize;
    file_content[..s]
```

To:
```rust
let start = {
    let s = sym.effective_start() as usize;
    file_content[..s]
```

- [ ] **Step 3: Update `build_insert_before` in `edit.rs`**

In `src/protocol/edit.rs`, change the start calculation (around line 133):

From:
```rust
let sym_start = sym.byte_range.0 as usize;
let line_start = file_content[..sym_start]
```

To:
```rust
let sym_start = sym.effective_start() as usize;
let line_start = file_content[..sym_start]
```

- [ ] **Step 4: Update `build_edit_within` in `edit.rs`**

In `src/protocol/edit.rs`, change the scope extraction (around line 231):

From:
```rust
let sym_start = sym.byte_range.0 as usize;
let sym_end = sym.byte_range.1 as usize;
```

To:
```rust
let sym_start = sym.effective_start() as usize;
let sym_end = sym.byte_range.1 as usize;
```

- [ ] **Step 5: Update `execute_batch_edit` overlap validation in `edit.rs`**

In `src/protocol/edit.rs`, change the overlap check (around line 425):

From:
```rust
let (a, b) = (
    resolved[indices[i]].sym.byte_range,
    resolved[indices[j]].sym.byte_range,
);
if a.0 < b.1 && b.0 < a.1 {
```

To:
```rust
let a = (resolved[indices[i]].sym.effective_start(), resolved[indices[i]].sym.byte_range.1);
let b = (resolved[indices[j]].sym.effective_start(), resolved[indices[j]].sym.byte_range.1);
if a.0 < b.1 && b.0 < a.1 {
```

- [ ] **Step 6: Update `execute_batch_edit` sort key in `edit.rs`**

In `src/protocol/edit.rs`, change the sort (around line 447):

From:
```rust
indices.sort_by(|&a, &b| {
    resolved[b].sym.byte_range.0.cmp(&resolved[a].sym.byte_range.0)
});
```

To:
```rust
indices.sort_by(|&a, &b| {
    resolved[b].sym.effective_start().cmp(&resolved[a].sym.effective_start())
});
```

- [ ] **Step 7: Update `execute_batch_edit` Replace branch in `edit.rs`**

In the Replace branch (around line 470):

From:
```rust
let sym_start = r.sym.byte_range.0 as usize;
let line_start = content[..sym_start]
```

To:
```rust
let effective = r.sym.effective_start() as usize;
let line_start = content[..effective]
```

- [ ] **Step 8: Run tests**

Run: `cargo test --all-targets -- --test-threads=1`
Expected: All tests pass

- [ ] **Step 9: Commit**

```bash
git add src/protocol/tools.rs src/protocol/edit.rs
git commit -m "feat: edit tools use doc_byte_range for splice boundaries

replace_symbol_body, build_delete, build_insert_before,
build_edit_within, and execute_batch_edit now use
effective_start() to include doc comments in their operation
range. batch_edit overlap validation and sort key also updated."
```

---

### Task 10: End-to-end edit tests with doc comments

**Files:**
- Modify: `src/protocol/edit.rs` (test module)

- [ ] **Step 1: Write test for delete including doc comments**

Add to edit.rs tests:

```rust
#[test]
fn test_build_delete_includes_doc_comments() {
    let content = b"/// Doc line 1\n/// Doc line 2\npub fn foo() {}\n\nfn bar() {}\n";
    let sym = SymbolRecord {
        name: "foo".to_string(),
        kind: SymbolKind::Function,
        depth: 0,
        sort_order: 0,
        // byte_range starts at "pub fn foo", NOT at the doc comments
        byte_range: (30, 46),  // "pub fn foo() {}"
        line_range: (2, 2),
        doc_byte_range: Some((0, 29)),  // "/// Doc line 1\n/// Doc line 2"
    };
    let result = build_delete(content, &sym);
    let result_str = String::from_utf8(result).unwrap();
    // Doc comments should be deleted along with the function
    assert!(!result_str.contains("/// Doc line 1"));
    assert!(!result_str.contains("pub fn foo"));
    assert!(result_str.contains("fn bar()"));
}
```

Note: exact byte offsets will need to be verified against the actual content bytes at implementation time. The test author should count bytes carefully or use `.find()` to compute offsets.

- [ ] **Step 2: Write test for insert_before with doc comments**

```rust
#[test]
fn test_build_insert_before_goes_above_doc_comments() {
    let content = b"/// Doc for foo\npub fn foo() {}\n";
    let sym = SymbolRecord {
        name: "foo".to_string(),
        kind: SymbolKind::Function,
        depth: 0,
        sort_order: 0,
        byte_range: (16, 31),  // "pub fn foo() {}"
        line_range: (1, 1),
        doc_byte_range: Some((0, 15)),  // "/// Doc for foo"
    };
    let result = build_insert_before(content, &sym, "use std::io;");
    let result_str = String::from_utf8(result).unwrap();
    // Insertion should appear BEFORE the doc comments
    let use_pos = result_str.find("use std::io;").unwrap();
    let doc_pos = result_str.find("/// Doc for foo").unwrap();
    assert!(use_pos < doc_pos, "Insert should go above doc comments");
}
```

- [ ] **Step 3: Run the new tests**

Run: `cargo test -p tokenizor_agentic_mcp --lib -- edit::tests::test_build_delete_includes_doc_comments edit::tests::test_build_insert_before_goes_above_doc_comments -v`
Expected: Both PASS

- [ ] **Step 4: Run full test suite**

Run: `cargo test --all-targets -- --test-threads=1`
Expected: All tests pass

- [ ] **Step 5: Format check**

Run: `cargo fmt -- --check`
Expected: No formatting differences

- [ ] **Step 6: Commit**

```bash
git add src/protocol/edit.rs
git commit -m "test: add end-to-end tests for doc-aware edit operations

Verify build_delete removes doc comments along with symbols and
build_insert_before inserts above doc comments, not between them."
```

---

## Dependency Graph

```
Task 1 (SymbolRecord field)
  └─► Task 2 (fix construction sites) ── compile gate
        └─► Task 3 (DocCommentSpec + scan_doc_range)
              └─► Task 4 (push_symbol wiring)
                    └─► Task 5 (language DOC_SPECs) ── compile gate
                          ├─► Task 6 (integration tests)
                          ├─► Task 7 (persistence version)
                          ├─► Task 8 (body extraction)
                          └─► Task 9 (edit tools)
                                └─► Task 10 (edit tests)
```

Tasks 6, 7, 8 are independent of each other and can be parallelized after Task 5.
Task 9 depends on Task 8 (needs `effective_start()` from Step 1).
Task 10 depends on Task 9.
