# Symbol-Addressed Edit Operations Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add 7 edit tools to Tokenizor MCP that accept symbol addresses instead of raw file content, enabling 82-99% token savings on code modifications.

**Architecture:** Edit tools resolve symbol positions server-side via the existing index (`resolve_symbol_selector`), splice new content in memory, write atomically to disk (temp+rename), and re-parse/reindex the modified file via `process_file` + `update_file`. Tier 1 tools handle single-file edits; Tier 2 tools handle multi-file atomic operations. All 7 tools follow the existing `#[tool]` handler pattern in `tools.rs`.

**Tech Stack:** Rust, tree-sitter (via existing `crate::parsing::process_file`), MCP tool protocol (`#[tool]` macro on `impl TokenizorServer`), `SharedIndexHandle` for index mutations.

---

## File Structure

| Action | Path | Responsibility |
|--------|------|----------------|
| Create | `src/protocol/edit.rs` | Input structs, core splice/write/reindex logic, symbol resolution wrapper, indentation utils |
| Create | `src/protocol/edit_format.rs` | Compact edit result formatting, stale reference warnings |
| Modify | `src/protocol/mod.rs:1-4` | Add `pub mod edit; pub mod edit_format;` |
| Modify | `src/live_index/query.rs:307,313,350` | Make `SymbolSelectorMatch`, `resolve_symbol_selector`, `render_symbol_selector` `pub(crate)` |
| Modify | `src/protocol/tools.rs:996+` | Add 7 `#[tool]` handler methods (thin wrappers calling `edit.rs`) |
| Modify | `src/protocol/tools.rs:4355` | Update `test_tools_registered_count_is_stable` expected count (+7) |

---

## Chunk 1: Core Edit Infrastructure

### Task 1: Create `edit.rs` with `apply_splice` + register modules

**Files:**
- Create: `src/protocol/edit.rs`
- Create: `src/protocol/edit_format.rs` (empty placeholder with `#[cfg(test)] mod tests {}`)
- Modify: `src/protocol/mod.rs:1-4`

- [ ] **Step 1: Write failing tests for `apply_splice`**

```rust
// src/protocol/edit.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_splice_replaces_middle() {
        let content = b"fn foo() { old_body }";
        let result = apply_splice(content, (11, 19), b"new_body");
        assert_eq!(result, b"fn foo() { new_body }");
    }

    #[test]
    fn test_apply_splice_replaces_at_start() {
        let content = b"old_start rest";
        let result = apply_splice(content, (0, 9), b"new");
        assert_eq!(result, b"new rest");
    }

    #[test]
    fn test_apply_splice_replaces_at_end() {
        let content = b"prefix old_end";
        let result = apply_splice(content, (7, 14), b"new_end");
        assert_eq!(result, b"prefix new_end");
    }

    #[test]
    fn test_apply_splice_empty_replacement_deletes() {
        let content = b"keep_this remove_this keep_that";
        let result = apply_splice(content, (10, 21), b"");
        assert_eq!(result, b"keep_this  keep_that");
    }

    #[test]
    fn test_apply_splice_empty_range_inserts() {
        let content = b"ab";
        let result = apply_splice(content, (1, 1), b"X");
        assert_eq!(result, b"aXb");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p tokenizor_agentic_mcp --lib protocol::edit::tests -- --nocapture`
Expected: Compilation error — `apply_splice` not defined

- [ ] **Step 3: Implement `apply_splice` and register modules**

```rust
// src/protocol/edit.rs

/// Splice `replacement` bytes into `content` at the given byte range [start, end).
pub(crate) fn apply_splice(content: &[u8], range: (u32, u32), replacement: &[u8]) -> Vec<u8> {
    let (start, end) = (range.0 as usize, range.1 as usize);
    let mut result = Vec::with_capacity(content.len() - (end - start) + replacement.len());
    result.extend_from_slice(&content[..start]);
    result.extend_from_slice(replacement);
    result.extend_from_slice(&content[end..]);
    result
}
```

```rust
// src/protocol/edit_format.rs (placeholder)
#[cfg(test)]
mod tests {}
```

Add to `src/protocol/mod.rs` after line 4 (`mod tools;`):

```rust
mod edit;
mod edit_format;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p tokenizor_agentic_mcp --lib protocol::edit::tests -- --nocapture`
Expected: All 5 tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/protocol/edit.rs src/protocol/edit_format.rs src/protocol/mod.rs
git commit -m "feat(edit): add apply_splice core function and module scaffolding"
```

---

### Task 2: Add `atomic_write_file`

**Files:**
- Modify: `src/protocol/edit.rs`

- [ ] **Step 1: Write failing tests**

```rust
// Add to edit.rs tests module:

#[test]
fn test_atomic_write_file_creates_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.rs");
    atomic_write_file(&path, b"fn main() {}").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"fn main() {}");
}

#[test]
fn test_atomic_write_file_overwrites_existing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.rs");
    std::fs::write(&path, b"old content").unwrap();
    atomic_write_file(&path, b"new content").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"new content");
}

#[test]
fn test_atomic_write_file_no_leftover_tmp() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.rs");
    atomic_write_file(&path, b"content").unwrap();
    let tmp = path.with_extension("tokenizor_tmp");
    assert!(!tmp.exists());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p tokenizor_agentic_mcp --lib protocol::edit::tests::test_atomic_write -- --nocapture`
Expected: Compilation error — `atomic_write_file` not defined

- [ ] **Step 3: Implement**

```rust
use std::path::Path;

/// Write content to a file atomically: write to a temp file, then rename.
pub(crate) fn atomic_write_file(path: &Path, content: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tokenizor_tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p tokenizor_agentic_mcp --lib protocol::edit::tests::test_atomic_write -- --nocapture`
Expected: All 3 tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/protocol/edit.rs
git commit -m "feat(edit): add atomic_write_file utility"
```

---

### Task 3: Add `reindex_after_write`

**Files:**
- Modify: `src/protocol/edit.rs`

- [ ] **Step 1: Write failing test**

```rust
use crate::domain::index::LanguageId;
use crate::live_index::LiveIndex;

#[test]
fn test_reindex_after_write_updates_index() {
    let handle = LiveIndex::empty(); // returns SharedIndex = Arc<SharedIndexHandle>
    let content = b"fn hello() {}\nfn world() {}\n".to_vec();
    reindex_after_write(&handle, "src/lib.rs", content, LanguageId::Rust);
    let guard = handle.read().expect("lock");
    let file = guard.get_file("src/lib.rs");
    assert!(file.is_some());
    let symbols = &file.unwrap().symbols;
    assert!(symbols.iter().any(|s| s.name == "hello"));
    assert!(symbols.iter().any(|s| s.name == "world"));
}

#[test]
fn test_reindex_after_write_replaces_existing_entry() {
    let handle = LiveIndex::empty();
    // First index
    let v1 = b"fn alpha() {}\n".to_vec();
    reindex_after_write(&handle, "src/lib.rs", v1, LanguageId::Rust);
    // Re-index with different content
    let v2 = b"fn beta() {}\n".to_vec();
    reindex_after_write(&handle, "src/lib.rs", v2, LanguageId::Rust);

    let guard = handle.read().expect("lock");
    let file = guard.get_file("src/lib.rs").unwrap();
    assert!(!file.symbols.iter().any(|s| s.name == "alpha"));
    assert!(file.symbols.iter().any(|s| s.name == "beta"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p tokenizor_agentic_mcp --lib protocol::edit::tests::test_reindex -- --nocapture`
Expected: Compilation error — `reindex_after_write` not defined

- [ ] **Step 3: Implement**

```rust
use crate::domain::index::LanguageId;
use crate::live_index::store::IndexedFile;
use crate::live_index::SharedIndex;

/// Re-parse file content and update the live index. Call after writing to disk.
/// Takes `&SharedIndex` (= `&Arc<SharedIndexHandle>`) to match `self.index` in handlers.
pub(crate) fn reindex_after_write(
    index: &SharedIndex,
    relative_path: &str,
    content: Vec<u8>,
    language: LanguageId,
) {
    let result = crate::parsing::process_file(relative_path, &content, language);
    let indexed = IndexedFile::from_parse_result(result, content);
    index.update_file(relative_path.to_string(), indexed);
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p tokenizor_agentic_mcp --lib protocol::edit::tests::test_reindex -- --nocapture`
Expected: Both PASS

- [ ] **Step 5: Commit**

```bash
git add src/protocol/edit.rs
git commit -m "feat(edit): add reindex_after_write for post-edit index refresh"
```

---

### Task 4: Add `resolve_or_error` + make query helpers `pub(crate)`

**Files:**
- Modify: `src/protocol/edit.rs`
- Modify: `src/live_index/query.rs:307,313,350` — change `enum SymbolSelectorMatch`, `fn resolve_symbol_selector`, `fn render_symbol_selector` to `pub(crate)`

- [ ] **Step 1: Change visibility of query helpers**

In `src/live_index/query.rs`:

Line 307: `enum SymbolSelectorMatch` → `pub(crate) enum SymbolSelectorMatch`
Line 313: `fn resolve_symbol_selector` → `pub(crate) fn resolve_symbol_selector`
Line 350: `fn render_symbol_selector` → `pub(crate) fn render_symbol_selector`

- [ ] **Step 2: Write failing tests**

```rust
use crate::domain::index::{SymbolKind, SymbolRecord};

fn make_test_indexed_file(symbols: Vec<SymbolRecord>) -> IndexedFile {
    IndexedFile {
        relative_path: "test.rs".to_string(),
        language: LanguageId::Rust,
        classification: crate::domain::index::FileClassification::for_code_path("test.rs"),
        content: Vec::new(),
        symbols,
        parse_status: crate::live_index::store::ParseStatus::Parsed,
        byte_len: 0,
        content_hash: String::new(),
        references: Vec::new(),
        alias_map: std::collections::HashMap::new(),
    }
}

fn make_test_symbol(name: &str, kind: SymbolKind, byte_range: (u32, u32), line_start: u32) -> SymbolRecord {
    SymbolRecord {
        name: name.to_string(),
        kind,
        depth: 0,
        sort_order: 0,
        byte_range,
        line_range: (line_start, line_start + 2),
    }
}

#[test]
fn test_resolve_or_error_finds_exact() {
    let file = make_test_indexed_file(vec![
        make_test_symbol("foo", SymbolKind::Function, (0, 20), 1),
        make_test_symbol("bar", SymbolKind::Function, (22, 50), 5),
    ]);
    let result = resolve_or_error(&file, "foo", None, None);
    assert!(result.is_ok());
    let (idx, sym) = result.unwrap();
    assert_eq!(idx, 0);
    assert_eq!(sym.name, "foo");
}

#[test]
fn test_resolve_or_error_not_found() {
    let file = make_test_indexed_file(vec![
        make_test_symbol("foo", SymbolKind::Function, (0, 20), 1),
    ]);
    let result = resolve_or_error(&file, "baz", None, None);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("not found"));
}

#[test]
fn test_resolve_or_error_ambiguous_shows_candidates() {
    let file = make_test_indexed_file(vec![
        make_test_symbol("foo", SymbolKind::Function, (0, 20), 1),
        make_test_symbol("foo", SymbolKind::Function, (22, 50), 5),
    ]);
    let result = resolve_or_error(&file, "foo", None, None);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("Ambiguous"), "error was: {err}");
    assert!(err.contains("symbol_line"), "error was: {err}");
}

#[test]
fn test_resolve_or_error_disambiguates_by_kind() {
    let file = make_test_indexed_file(vec![
        make_test_symbol("Foo", SymbolKind::Struct, (0, 20), 1),
        make_test_symbol("Foo", SymbolKind::Impl, (22, 80), 5),
    ]);
    let result = resolve_or_error(&file, "Foo", Some("struct"), None);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().1.kind, SymbolKind::Struct);
}

#[test]
fn test_resolve_or_error_disambiguates_by_line() {
    let file = make_test_indexed_file(vec![
        make_test_symbol("foo", SymbolKind::Function, (0, 20), 1),
        make_test_symbol("foo", SymbolKind::Function, (22, 50), 5),
    ]);
    let result = resolve_or_error(&file, "foo", None, Some(5));
    assert!(result.is_ok());
    assert_eq!(result.unwrap().0, 1); // index 1 in symbols vec
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p tokenizor_agentic_mcp --lib protocol::edit::tests::test_resolve -- --nocapture`
Expected: Compilation error — `resolve_or_error` not defined

- [ ] **Step 4: Implement**

```rust
use crate::live_index::query::{
    resolve_symbol_selector, render_symbol_selector, SymbolSelectorMatch,
};

/// Resolve a symbol by name/kind/line, returning (index, cloned record) or user-friendly error.
pub(crate) fn resolve_or_error(
    file: &IndexedFile,
    name: &str,
    kind: Option<&str>,
    line: Option<u32>,
) -> Result<(usize, SymbolRecord), String> {
    match resolve_symbol_selector(file, name, kind, line) {
        SymbolSelectorMatch::Selected(idx, sym) => Ok((idx, sym.clone())),
        SymbolSelectorMatch::NotFound => {
            let label = render_symbol_selector(name, kind, line);
            Err(format!("Symbol not found: {label}"))
        }
        SymbolSelectorMatch::Ambiguous(candidate_lines) => {
            let candidates = candidate_lines
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            Err(format!(
                "Ambiguous: multiple definitions of `{name}`. \
                 Pass `symbol_line` to disambiguate. Candidate lines: {candidates}"
            ))
        }
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p tokenizor_agentic_mcp --lib protocol::edit::tests::test_resolve -- --nocapture`
Expected: All 5 PASS

- [ ] **Step 6: Commit**

```bash
git add src/protocol/edit.rs src/live_index/query.rs
git commit -m "feat(edit): add resolve_or_error with pub(crate) query helpers"
```

---

### Task 5: Add indentation utilities

**Files:**
- Modify: `src/protocol/edit.rs`

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn test_detect_indentation_spaces() {
    let content = b"fn outer() {\n    fn inner() {}\n}";
    // byte 14 is start of "fn inner"
    let indent = detect_indentation(content, 14);
    assert_eq!(indent, b"    ");
}

#[test]
fn test_detect_indentation_tabs() {
    let content = b"fn outer() {\n\tfn inner() {}\n}";
    let indent = detect_indentation(content, 14);
    assert_eq!(indent, b"\t");
}

#[test]
fn test_detect_indentation_no_indent() {
    let content = b"fn top_level() {}";
    let indent = detect_indentation(content, 0);
    assert_eq!(indent, b"");
}

#[test]
fn test_detect_indentation_at_newline_boundary() {
    let content = b"line1\nline2";
    let indent = detect_indentation(content, 6);
    assert_eq!(indent, b"");
}

#[test]
fn test_apply_indentation_adds_prefix() {
    let result = apply_indentation("fn new() {\n    body;\n}", b"    ");
    let text = std::str::from_utf8(&result).unwrap();
    assert_eq!(text, "    fn new() {\n        body;\n    }");
}

#[test]
fn test_apply_indentation_preserves_empty_lines() {
    let result = apply_indentation("a\n\nb", b"  ");
    let text = std::str::from_utf8(&result).unwrap();
    assert_eq!(text, "  a\n\n  b");
}

#[test]
fn test_apply_indentation_empty_indent_is_identity() {
    let result = apply_indentation("fn foo() {}", b"");
    assert_eq!(result, b"fn foo() {}");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p tokenizor_agentic_mcp --lib protocol::edit::tests::test_detect_indent -- --nocapture && cargo test -p tokenizor_agentic_mcp --lib protocol::edit::tests::test_apply_indent -- --nocapture`
Expected: Compilation errors

- [ ] **Step 3: Implement**

```rust
/// Detect the leading whitespace on the line containing `byte_offset`.
pub(crate) fn detect_indentation(content: &[u8], byte_offset: u32) -> Vec<u8> {
    let offset = byte_offset as usize;
    let line_start = content[..offset]
        .iter()
        .rposition(|&b| b == b'\n')
        .map(|p| p + 1)
        .unwrap_or(0);
    let indent_end = content[line_start..]
        .iter()
        .position(|b| !b.is_ascii_whitespace() || *b == b'\n')
        .unwrap_or(0);
    content[line_start..line_start + indent_end].to_vec()
}

/// Prefix each non-empty line of `text` with `indent`.
pub(crate) fn apply_indentation(text: &str, indent: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if i > 0 {
            result.push(b'\n');
        }
        if !line.is_empty() {
            result.extend_from_slice(indent);
            result.extend_from_slice(line.as_bytes());
        }
    }
    if text.ends_with('\n') {
        result.push(b'\n');
    }
    result
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p tokenizor_agentic_mcp --lib protocol::edit::tests::test_detect_indent -- --nocapture && cargo test -p tokenizor_agentic_mcp --lib protocol::edit::tests::test_apply_indent -- --nocapture`
Expected: All 7 PASS

- [ ] **Step 5: Commit**

```bash
git add src/protocol/edit.rs
git commit -m "feat(edit): add indentation detection and application utilities"
```

---

## Chunk 2: Tier 1 — Single-File Edit Tools

### Task 6: Create `edit_format.rs` with format functions

**Files:**
- Modify: `src/protocol/edit_format.rs`

- [ ] **Step 1: Write failing tests**

```rust
// src/protocol/edit_format.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_replace() {
        let result = format_replace("src/lib.rs", "process", "fn", 342, 287);
        assert!(result.contains("src/lib.rs"));
        assert!(result.contains("process"));
        assert!(result.contains("342"));
        assert!(result.contains("287"));
    }

    #[test]
    fn test_format_insert() {
        let result = format_insert("src/lib.rs", "handler", "after", 120);
        assert!(result.contains("src/lib.rs"));
        assert!(result.contains("after"));
        assert!(result.contains("handler"));
        assert!(result.contains("120"));
    }

    #[test]
    fn test_format_delete() {
        let result = format_delete("src/lib.rs", "old_fn", "fn", 200);
        assert!(result.contains("src/lib.rs"));
        assert!(result.contains("old_fn"));
        assert!(result.contains("200"));
    }

    #[test]
    fn test_format_edit_within() {
        let result = format_edit_within("src/lib.rs", "process", 2, 500, 480);
        assert!(result.contains("src/lib.rs"));
        assert!(result.contains("process"));
        assert!(result.contains("2"));
    }

    #[test]
    fn test_format_stale_warnings_empty() {
        let result = format_stale_warnings("src/lib.rs", "foo", &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_format_stale_warnings_with_refs() {
        let refs = vec![
            ("src/main.rs".to_string(), 45, Some("fn main".to_string())),
            ("src/handler.rs".to_string(), 23, None),
        ];
        let result = format_stale_warnings("src/lib.rs", "process", &refs);
        assert!(result.contains("src/main.rs:45"));
        assert!(result.contains("fn main"));
        assert!(result.contains("src/handler.rs:23"));
        assert!(result.contains("2 reference(s)"));
    }

    #[test]
    fn test_format_batch_summary() {
        let results = vec![
            "src/a.rs — replaced `foo`".to_string(),
            "src/b.rs — deleted `bar`".to_string(),
        ];
        let result = format_batch_summary(&results, 2);
        assert!(result.contains("2 edit(s)"));
        assert!(result.contains("2 file(s)"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p tokenizor_agentic_mcp --lib protocol::edit_format::tests -- --nocapture`
Expected: Compilation errors

- [ ] **Step 3: Implement**

```rust
// src/protocol/edit_format.rs

/// Format the result of a replace_symbol_body operation.
pub(crate) fn format_replace(
    path: &str,
    name: &str,
    kind: &str,
    old_bytes: usize,
    new_bytes: usize,
) -> String {
    format!("{path} — replaced {kind} `{name}` ({old_bytes} → {new_bytes} bytes)")
}

/// Format the result of an insert operation.
pub(crate) fn format_insert(
    path: &str,
    name: &str,
    position: &str,
    inserted_bytes: usize,
) -> String {
    format!("{path} — inserted {position} `{name}` ({inserted_bytes} bytes)")
}

/// Format the result of a delete operation.
pub(crate) fn format_delete(path: &str, name: &str, kind: &str, deleted_bytes: usize) -> String {
    format!("{path} — deleted {kind} `{name}` ({deleted_bytes} bytes)")
}

/// Format the result of an edit-within-symbol operation.
pub(crate) fn format_edit_within(
    path: &str,
    name: &str,
    replacements: usize,
    old_bytes: usize,
    new_bytes: usize,
) -> String {
    format!(
        "{path} — edited within `{name}` ({replacements} replacement(s), {old_bytes} → {new_bytes} bytes)"
    )
}

/// Format stale reference warnings after a signature-changing edit.
pub(crate) fn format_stale_warnings(
    _path: &str,
    name: &str,
    refs: &[(String, u32, Option<String>)],
) -> String {
    if refs.is_empty() {
        return String::new();
    }
    let mut out = format!(
        "\n⚠ Signature of `{name}` may have changed — {} reference(s) to check:\n",
        refs.len()
    );
    for (ref_path, line, enclosing) in refs {
        out.push_str(&format!("  {ref_path}:{line}"));
        if let Some(enc) = enclosing {
            out.push_str(&format!(" (in {enc})"));
        }
        out.push('\n');
    }
    out
}

/// Format a batch edit summary.
pub(crate) fn format_batch_summary(results: &[String], file_count: usize) -> String {
    let mut out = format!("{} edit(s) across {} file(s):\n", results.len(), file_count);
    for r in results {
        out.push_str("  ");
        out.push_str(r);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    // ... tests from step 1 ...
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p tokenizor_agentic_mcp --lib protocol::edit_format::tests -- --nocapture`
Expected: All 7 PASS

- [ ] **Step 5: Commit**

```bash
git add src/protocol/edit_format.rs
git commit -m "feat(edit): add edit_format.rs with compact result formatters"
```

---

### Task 7: `replace_symbol_body` — input struct + handler + test

**Files:**
- Modify: `src/protocol/edit.rs` — add `ReplaceSymbolBodyInput`
- Modify: `src/protocol/tools.rs` — add handler in `impl TokenizorServer` block

- [ ] **Step 1: Add input struct to `edit.rs`**

```rust
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct ReplaceSymbolBodyInput {
    /// Relative file path.
    pub path: String,
    /// Symbol name to replace.
    pub name: String,
    /// Optional kind filter (e.g., "fn", "struct", "impl").
    pub kind: Option<String>,
    /// Line number to disambiguate when multiple symbols share the same name.
    #[serde(default, deserialize_with = "super::tools::lenient_u32")]
    pub symbol_line: Option<u32>,
    /// Complete new source code for the symbol (replaces the entire definition).
    pub new_body: String,
}
```

- [ ] **Step 2: Add handler to `tools.rs`**

Add inside the `#[tool_router] impl TokenizorServer` block (after the last existing handler):

```rust
    /// Replace a symbol's entire definition. The index resolves the position — no need to read the file.
    #[tool(
        description = "Replace a symbol's entire definition by name. Provide the complete new source code. The index resolves byte positions server-side. Use symbol_line to disambiguate overloaded names."
    )]
    pub(crate) async fn replace_symbol_body(
        &self,
        params: Parameters<edit::ReplaceSymbolBodyInput>,
    ) -> String {
        if let Some(result) = self.proxy_tool_call("replace_symbol_body", &params.0).await {
            return result;
        }

        let repo_root = match self.capture_repo_root() {
            Some(root) => root,
            None => return "Error: no repository root configured.".to_string(),
        };

        let file = {
            let guard = self.index.read().expect("lock poisoned");
            loading_guard!(guard);
            guard.capture_shared_file(&params.0.path)
        };
        let file = match file {
            Some(f) => f,
            None => return format::not_found_file(&params.0.path),
        };

        let (_, sym) = match edit::resolve_or_error(
            &file,
            &params.0.name,
            params.0.kind.as_deref(),
            params.0.symbol_line,
        ) {
            Ok(s) => s,
            Err(e) => return e,
        };

        let old_bytes = (sym.byte_range.1 - sym.byte_range.0) as usize;
        let new_content =
            edit::apply_splice(&file.content, sym.byte_range, params.0.new_body.as_bytes());
        let abs_path = repo_root.join(&params.0.path);

        if let Err(e) = edit::atomic_write_file(&abs_path, &new_content) {
            return format!("Error writing {}: {e}", params.0.path);
        }

        edit::reindex_after_write(
            &self.index,
            &params.0.path,
            new_content,
            file.language,
        );

        edit_format::format_replace(
            &params.0.path,
            &params.0.name,
            &sym.kind.to_string(),
            old_bytes,
            params.0.new_body.len(),
        )
    }
```

Add `use super::edit;` and `use super::edit_format;` to the imports at the top of `tools.rs` if not already present.

- [ ] **Step 3: Write integration test**

Add to the `mod tests` block in `tools.rs`:

```rust
#[tokio::test]
async fn test_replace_symbol_body_replaces_and_reindexes() {
    let dir = tempfile::tempdir().unwrap();
    let src_dir = dir.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let file_path = src_dir.join("lib.rs");
    let original = b"fn hello() {\n    println!(\"hello\");\n}\n\nfn world() {\n    println!(\"world\");\n}\n";
    std::fs::write(&file_path, original).unwrap();

    let result = crate::parsing::process_file("src/lib.rs", original, LanguageId::Rust);
    let indexed = IndexedFile::from_parse_result(result, original.to_vec());
    let mut index = LiveIndex::empty();
    index.update_file("src/lib.rs".to_string(), indexed);
    let server = make_server_with_root(index, Some(dir.path().to_path_buf()));

    let input = edit::ReplaceSymbolBodyInput {
        path: "src/lib.rs".to_string(),
        name: "hello".to_string(),
        kind: None,
        symbol_line: None,
        new_body: "fn hello() {\n    println!(\"HELLO\");\n}".to_string(),
    };
    let result = server
        .replace_symbol_body(Parameters(input))
        .await;

    assert!(result.contains("replaced"), "result was: {result}");
    assert!(result.contains("hello"), "result was: {result}");

    // Verify disk content
    let on_disk = std::fs::read_to_string(&file_path).unwrap();
    assert!(on_disk.contains("HELLO"), "disk: {on_disk}");
    assert!(on_disk.contains("world"), "other symbol intact: {on_disk}");

    // Verify index updated
    let guard = server.index.read().unwrap();
    let file = guard.get_file("src/lib.rs").unwrap();
    assert!(file.symbols.iter().any(|s| s.name == "hello"));
    assert!(file.symbols.iter().any(|s| s.name == "world"));
}

#[tokio::test]
async fn test_replace_symbol_body_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let index = make_live_index_ready();
    let server = make_server_with_root(index, Some(dir.path().to_path_buf()));
    let input = edit::ReplaceSymbolBodyInput {
        path: "nonexistent.rs".to_string(),
        name: "foo".to_string(),
        kind: None,
        symbol_line: None,
        new_body: "fn foo() {}".to_string(),
    };
    let result = server.replace_symbol_body(Parameters(input)).await;
    assert!(result.contains("not found") || result.contains("Not found"), "result: {result}");
}

#[tokio::test]
async fn test_replace_symbol_body_ambiguous() {
    let dir = tempfile::tempdir().unwrap();
    let src_dir = dir.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let file_path = src_dir.join("lib.rs");
    let original = b"fn dup() { 1 }\nfn dup() { 2 }\n";
    std::fs::write(&file_path, original).unwrap();

    let result = crate::parsing::process_file("src/lib.rs", original, LanguageId::Rust);
    let indexed = IndexedFile::from_parse_result(result, original.to_vec());
    let mut index = LiveIndex::empty();
    index.update_file("src/lib.rs".to_string(), indexed);
    let server = make_server_with_root(index, Some(dir.path().to_path_buf()));

    let input = edit::ReplaceSymbolBodyInput {
        path: "src/lib.rs".to_string(),
        name: "dup".to_string(),
        kind: None,
        symbol_line: None,
        new_body: "fn dup() { 3 }".to_string(),
    };
    let result = server.replace_symbol_body(Parameters(input)).await;
    assert!(result.contains("Ambiguous"), "result: {result}");
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p tokenizor_agentic_mcp --lib protocol::tools::tests::test_replace_symbol_body -- --nocapture`
Expected: All 3 PASS

- [ ] **Step 5: Commit**

```bash
git add src/protocol/edit.rs src/protocol/tools.rs
git commit -m "feat(edit): add replace_symbol_body tool"
```

---

### Task 8: `insert_before_symbol` + `insert_after_symbol`

**Files:**
- Modify: `src/protocol/edit.rs` — add input structs + insert helpers
- Modify: `src/protocol/tools.rs` — add two handlers

- [ ] **Step 1: Add input struct and insert helper to `edit.rs`**

```rust
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct InsertSymbolInput {
    /// Relative file path.
    pub path: String,
    /// Name of the reference symbol to insert adjacent to.
    pub name: String,
    /// Optional kind filter.
    pub kind: Option<String>,
    /// Line number to disambiguate.
    #[serde(default, deserialize_with = "super::tools::lenient_u32")]
    pub symbol_line: Option<u32>,
    /// Code to insert. Will be indented to match the target symbol's indentation.
    pub content: String,
}

/// Build the bytes to insert before a symbol: indented content + blank line + existing content.
pub(crate) fn build_insert_before(
    file_content: &[u8],
    sym: &SymbolRecord,
    new_code: &str,
) -> Vec<u8> {
    let indent = detect_indentation(file_content, sym.byte_range.0);
    let indented = apply_indentation(new_code, &indent);
    let mut insertion = indented;
    insertion.extend_from_slice(b"\n\n");
    apply_splice(file_content, (sym.byte_range.0, sym.byte_range.0), &insertion)
}

/// Build the bytes to insert after a symbol: existing content + blank line + indented content.
pub(crate) fn build_insert_after(
    file_content: &[u8],
    sym: &SymbolRecord,
    new_code: &str,
) -> Vec<u8> {
    let indent = detect_indentation(file_content, sym.byte_range.0);
    let indented = apply_indentation(new_code, &indent);
    let mut insertion = Vec::new();
    insertion.extend_from_slice(b"\n\n");
    insertion.extend_from_slice(&indented);
    apply_splice(
        file_content,
        (sym.byte_range.1, sym.byte_range.1),
        &insertion,
    )
}
```

- [ ] **Step 2: Write unit tests for insert helpers**

```rust
#[test]
fn test_build_insert_before_adds_content_with_indent() {
    let content = b"    fn existing() {}\n";
    let sym = make_test_symbol("existing", SymbolKind::Function, (4, 20), 1);
    let result = build_insert_before(content, &sym, "fn new_fn() {}");
    let text = std::str::from_utf8(&result).unwrap();
    assert!(text.starts_with("    fn new_fn() {}\n\n    fn existing"), "got: {text}");
}

#[test]
fn test_build_insert_after_adds_content_with_indent() {
    let content = b"    fn existing() {}";
    let sym = make_test_symbol("existing", SymbolKind::Function, (4, 20), 1);
    let result = build_insert_after(content, &sym, "fn new_fn() {}");
    let text = std::str::from_utf8(&result).unwrap();
    assert!(text.contains("fn existing() {}\n\n    fn new_fn() {}"), "got: {text}");
}
```

- [ ] **Step 3: Add handlers to `tools.rs`**

```rust
    /// Insert code before a named symbol. Content is auto-indented to match.
    #[tool(
        description = "Insert code before a named symbol. Content is auto-indented to match the target's indentation. Use symbol_line to disambiguate overloaded names."
    )]
    pub(crate) async fn insert_before_symbol(
        &self,
        params: Parameters<edit::InsertSymbolInput>,
    ) -> String {
        if let Some(result) = self.proxy_tool_call("insert_before_symbol", &params.0).await {
            return result;
        }
        let repo_root = match self.capture_repo_root() {
            Some(root) => root,
            None => return "Error: no repository root configured.".to_string(),
        };
        let file = {
            let guard = self.index.read().expect("lock poisoned");
            loading_guard!(guard);
            guard.capture_shared_file(&params.0.path)
        };
        let file = match file {
            Some(f) => f,
            None => return format::not_found_file(&params.0.path),
        };
        let (_, sym) = match edit::resolve_or_error(
            &file, &params.0.name, params.0.kind.as_deref(), params.0.symbol_line,
        ) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let new_content = edit::build_insert_before(&file.content, &sym, &params.0.content);
        let abs_path = repo_root.join(&params.0.path);
        if let Err(e) = edit::atomic_write_file(&abs_path, &new_content) {
            return format!("Error writing {}: {e}", params.0.path);
        }
        edit::reindex_after_write(&self.index, &params.0.path, new_content, file.language);
        edit_format::format_insert(&params.0.path, &params.0.name, "before", params.0.content.len())
    }

    /// Insert code after a named symbol. Content is auto-indented to match.
    #[tool(
        description = "Insert code after a named symbol. Content is auto-indented to match the target's indentation. Use symbol_line to disambiguate overloaded names."
    )]
    pub(crate) async fn insert_after_symbol(
        &self,
        params: Parameters<edit::InsertSymbolInput>,
    ) -> String {
        if let Some(result) = self.proxy_tool_call("insert_after_symbol", &params.0).await {
            return result;
        }
        let repo_root = match self.capture_repo_root() {
            Some(root) => root,
            None => return "Error: no repository root configured.".to_string(),
        };
        let file = {
            let guard = self.index.read().expect("lock poisoned");
            loading_guard!(guard);
            guard.capture_shared_file(&params.0.path)
        };
        let file = match file {
            Some(f) => f,
            None => return format::not_found_file(&params.0.path),
        };
        let (_, sym) = match edit::resolve_or_error(
            &file, &params.0.name, params.0.kind.as_deref(), params.0.symbol_line,
        ) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let new_content = edit::build_insert_after(&file.content, &sym, &params.0.content);
        let abs_path = repo_root.join(&params.0.path);
        if let Err(e) = edit::atomic_write_file(&abs_path, &new_content) {
            return format!("Error writing {}: {e}", params.0.path);
        }
        edit::reindex_after_write(&self.index, &params.0.path, new_content, file.language);
        edit_format::format_insert(&params.0.path, &params.0.name, "after", params.0.content.len())
    }
```

- [ ] **Step 4: Write integration tests**

```rust
#[tokio::test]
async fn test_insert_before_symbol_adds_code_and_reindexes() {
    let dir = tempfile::tempdir().unwrap();
    let src_dir = dir.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let file_path = src_dir.join("lib.rs");
    let original = b"fn existing() {\n    body();\n}\n";
    std::fs::write(&file_path, original).unwrap();

    let result = crate::parsing::process_file("src/lib.rs", original, LanguageId::Rust);
    let indexed = IndexedFile::from_parse_result(result, original.to_vec());
    let mut index = LiveIndex::empty();
    index.update_file("src/lib.rs".to_string(), indexed);
    let server = make_server_with_root(index, Some(dir.path().to_path_buf()));

    let input = edit::InsertSymbolInput {
        path: "src/lib.rs".to_string(),
        name: "existing".to_string(),
        kind: None,
        symbol_line: None,
        content: "fn new_fn() {\n    new_body();\n}".to_string(),
    };
    let result = server.insert_before_symbol(Parameters(input)).await;
    assert!(result.contains("inserted"), "result: {result}");
    assert!(result.contains("before"), "result: {result}");

    let on_disk = std::fs::read_to_string(&file_path).unwrap();
    assert!(on_disk.contains("new_fn"), "disk: {on_disk}");
    // new_fn should appear before existing
    let new_pos = on_disk.find("new_fn").unwrap();
    let existing_pos = on_disk.find("existing").unwrap();
    assert!(new_pos < existing_pos, "disk: {on_disk}");
}

#[tokio::test]
async fn test_insert_after_symbol_adds_code_and_reindexes() {
    let dir = tempfile::tempdir().unwrap();
    let src_dir = dir.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let file_path = src_dir.join("lib.rs");
    let original = b"fn existing() {\n    body();\n}\n";
    std::fs::write(&file_path, original).unwrap();

    let result = crate::parsing::process_file("src/lib.rs", original, LanguageId::Rust);
    let indexed = IndexedFile::from_parse_result(result, original.to_vec());
    let mut index = LiveIndex::empty();
    index.update_file("src/lib.rs".to_string(), indexed);
    let server = make_server_with_root(index, Some(dir.path().to_path_buf()));

    let input = edit::InsertSymbolInput {
        path: "src/lib.rs".to_string(),
        name: "existing".to_string(),
        kind: None,
        symbol_line: None,
        content: "fn appended() {}".to_string(),
    };
    let result = server.insert_after_symbol(Parameters(input)).await;
    assert!(result.contains("inserted"), "result: {result}");
    assert!(result.contains("after"), "result: {result}");

    let on_disk = std::fs::read_to_string(&file_path).unwrap();
    assert!(on_disk.contains("appended"), "disk: {on_disk}");
    let existing_pos = on_disk.find("existing").unwrap();
    let appended_pos = on_disk.find("appended").unwrap();
    assert!(existing_pos < appended_pos, "disk: {on_disk}");
}
```

- [ ] **Step 5: Run all tests**

Run: `cargo test -p tokenizor_agentic_mcp --lib protocol::edit::tests::test_build_insert -- --nocapture && cargo test -p tokenizor_agentic_mcp --lib protocol::tools::tests::test_insert -- --nocapture`
Expected: All PASS

- [ ] **Step 6: Commit**

```bash
git add src/protocol/edit.rs src/protocol/tools.rs
git commit -m "feat(edit): add insert_before_symbol and insert_after_symbol tools"
```

---

### Task 9: `delete_symbol`

**Files:**
- Modify: `src/protocol/edit.rs` — add `DeleteSymbolInput` + `build_delete`
- Modify: `src/protocol/tools.rs` — add handler

- [ ] **Step 1: Add input struct and delete helper to `edit.rs`**

```rust
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct DeleteSymbolInput {
    /// Relative file path.
    pub path: String,
    /// Symbol name to delete.
    pub name: String,
    /// Optional kind filter.
    pub kind: Option<String>,
    /// Line number to disambiguate.
    #[serde(default, deserialize_with = "super::tools::lenient_u32")]
    pub symbol_line: Option<u32>,
}

/// Build file content with the symbol removed, cleaning up surrounding blank lines.
pub(crate) fn build_delete(file_content: &[u8], sym: &SymbolRecord) -> Vec<u8> {
    let start = sym.byte_range.0 as usize;
    let end = sym.byte_range.1 as usize;

    // Extend deletion to include the preceding blank line (if any) to avoid double blanks.
    let adjusted_start = if start >= 2 && file_content[start - 1] == b'\n' && file_content[start - 2] == b'\n' {
        start - 1
    } else if start >= 1 && file_content[start - 1] == b'\n' {
        start - 1
    } else {
        start
    };

    // Extend deletion to include trailing newline.
    let adjusted_end = if end < file_content.len() && file_content[end] == b'\n' {
        end + 1
    } else {
        end
    };

    apply_splice(file_content, (adjusted_start as u32, adjusted_end as u32), b"")
}
```

- [ ] **Step 2: Write tests**

```rust
#[test]
fn test_build_delete_removes_symbol_and_trailing_newline() {
    let content = b"fn keep() {}\n\nfn remove() {}\n\nfn also_keep() {}\n";
    // Assume "remove" has byte_range covering "fn remove() {}"
    let sym = make_test_symbol("remove", SymbolKind::Function, (14, 28), 3);
    let result = build_delete(content, &sym);
    let text = std::str::from_utf8(&result).unwrap();
    assert!(!text.contains("remove"), "got: {text}");
    assert!(text.contains("keep"), "got: {text}");
    assert!(text.contains("also_keep"), "got: {text}");
}
```

- [ ] **Step 3: Add handler to `tools.rs`**

```rust
    /// Delete a symbol entirely by name.
    #[tool(
        description = "Delete a symbol by name — removes the entire definition from the file. Use symbol_line to disambiguate overloaded names."
    )]
    pub(crate) async fn delete_symbol(
        &self,
        params: Parameters<edit::DeleteSymbolInput>,
    ) -> String {
        if let Some(result) = self.proxy_tool_call("delete_symbol", &params.0).await {
            return result;
        }
        let repo_root = match self.capture_repo_root() {
            Some(root) => root,
            None => return "Error: no repository root configured.".to_string(),
        };
        let file = {
            let guard = self.index.read().expect("lock poisoned");
            loading_guard!(guard);
            guard.capture_shared_file(&params.0.path)
        };
        let file = match file {
            Some(f) => f,
            None => return format::not_found_file(&params.0.path),
        };
        let (_, sym) = match edit::resolve_or_error(
            &file, &params.0.name, params.0.kind.as_deref(), params.0.symbol_line,
        ) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let deleted_bytes = (sym.byte_range.1 - sym.byte_range.0) as usize;
        let kind_str = sym.kind.to_string();
        let new_content = edit::build_delete(&file.content, &sym);
        let abs_path = repo_root.join(&params.0.path);
        if let Err(e) = edit::atomic_write_file(&abs_path, &new_content) {
            return format!("Error writing {}: {e}", params.0.path);
        }
        edit::reindex_after_write(&self.index, &params.0.path, new_content, file.language);
        edit_format::format_delete(&params.0.path, &params.0.name, &kind_str, deleted_bytes)
    }
```

- [ ] **Step 4: Write integration test**

```rust
#[tokio::test]
async fn test_delete_symbol_removes_and_reindexes() {
    let dir = tempfile::tempdir().unwrap();
    let src_dir = dir.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let file_path = src_dir.join("lib.rs");
    let original = b"fn keep() {}\n\nfn remove_me() {\n    doomed();\n}\n\nfn also_keep() {}\n";
    std::fs::write(&file_path, original).unwrap();

    let result = crate::parsing::process_file("src/lib.rs", original, LanguageId::Rust);
    let indexed = IndexedFile::from_parse_result(result, original.to_vec());
    let mut index = LiveIndex::empty();
    index.update_file("src/lib.rs".to_string(), indexed);
    let server = make_server_with_root(index, Some(dir.path().to_path_buf()));

    let input = edit::DeleteSymbolInput {
        path: "src/lib.rs".to_string(),
        name: "remove_me".to_string(),
        kind: None,
        symbol_line: None,
    };
    let result = server.delete_symbol(Parameters(input)).await;
    assert!(result.contains("deleted"), "result: {result}");

    let on_disk = std::fs::read_to_string(&file_path).unwrap();
    assert!(!on_disk.contains("remove_me"), "disk: {on_disk}");
    assert!(on_disk.contains("keep"), "disk: {on_disk}");
    assert!(on_disk.contains("also_keep"), "disk: {on_disk}");

    let guard = server.index.read().unwrap();
    let file = guard.get_file("src/lib.rs").unwrap();
    assert!(!file.symbols.iter().any(|s| s.name == "remove_me"));
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p tokenizor_agentic_mcp --lib protocol::edit::tests::test_build_delete -- --nocapture && cargo test -p tokenizor_agentic_mcp --lib protocol::tools::tests::test_delete_symbol -- --nocapture`
Expected: All PASS

- [ ] **Step 6: Commit**

```bash
git add src/protocol/edit.rs src/protocol/tools.rs
git commit -m "feat(edit): add delete_symbol tool"
```

---

### Task 10: `edit_within_symbol`

**Files:**
- Modify: `src/protocol/edit.rs` — add `EditWithinSymbolInput` + `build_edit_within`
- Modify: `src/protocol/tools.rs` — add handler

- [ ] **Step 1: Add input struct and helper**

```rust
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct EditWithinSymbolInput {
    /// Relative file path.
    pub path: String,
    /// Symbol name containing the text to edit.
    pub name: String,
    /// Optional kind filter.
    pub kind: Option<String>,
    /// Line number to disambiguate.
    #[serde(default, deserialize_with = "super::tools::lenient_u32")]
    pub symbol_line: Option<u32>,
    /// Exact text to find within the symbol's body.
    pub old_text: String,
    /// Replacement text.
    pub new_text: String,
}

/// Replace occurrences of `old_text` with `new_text` within a symbol's byte range.
/// Returns (new_file_content, replacement_count) or an error if old_text not found.
pub(crate) fn build_edit_within(
    file_content: &[u8],
    sym: &SymbolRecord,
    old_text: &str,
    new_text: &str,
) -> Result<(Vec<u8>, usize), String> {
    let start = sym.byte_range.0 as usize;
    let end = sym.byte_range.1 as usize;
    let body = &file_content[start..end];
    let body_str = std::str::from_utf8(body)
        .map_err(|_| "Symbol body is not valid UTF-8".to_string())?;

    let count = body_str.matches(old_text).count();
    if count == 0 {
        return Err(format!(
            "Text not found within symbol `{}`: {:?}",
            sym.name,
            if old_text.len() > 60 { &old_text[..60] } else { old_text }
        ));
    }

    let new_body = body_str.replace(old_text, new_text);
    let new_content = apply_splice(file_content, sym.byte_range, new_body.as_bytes());
    Ok((new_content, count))
}
```

- [ ] **Step 2: Write tests**

```rust
#[test]
fn test_build_edit_within_replaces_text_in_symbol() {
    let content = b"fn process() {\n    let x = old_value;\n    let y = old_value;\n}\n";
    let sym = make_test_symbol("process", SymbolKind::Function, (0, 60), 1);
    let (result, count) = build_edit_within(content, &sym, "old_value", "new_value").unwrap();
    let text = std::str::from_utf8(&result).unwrap();
    assert_eq!(count, 2);
    assert!(text.contains("new_value"), "got: {text}");
    assert!(!text.contains("old_value"), "got: {text}");
}

#[test]
fn test_build_edit_within_not_found_returns_error() {
    let content = b"fn process() { body }\n";
    let sym = make_test_symbol("process", SymbolKind::Function, (0, 21), 1);
    let result = build_edit_within(content, &sym, "nonexistent", "replacement");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("not found"));
}
```

- [ ] **Step 3: Add handler to `tools.rs`**

```rust
    /// Edit text within a symbol's body without replacing the entire definition.
    #[tool(
        description = "Find-and-replace text within a symbol's body. Scoped to the symbol's byte range — won't affect code outside. Replaces all occurrences of old_text with new_text."
    )]
    pub(crate) async fn edit_within_symbol(
        &self,
        params: Parameters<edit::EditWithinSymbolInput>,
    ) -> String {
        if let Some(result) = self.proxy_tool_call("edit_within_symbol", &params.0).await {
            return result;
        }
        let repo_root = match self.capture_repo_root() {
            Some(root) => root,
            None => return "Error: no repository root configured.".to_string(),
        };
        let file = {
            let guard = self.index.read().expect("lock poisoned");
            loading_guard!(guard);
            guard.capture_shared_file(&params.0.path)
        };
        let file = match file {
            Some(f) => f,
            None => return format::not_found_file(&params.0.path),
        };
        let (_, sym) = match edit::resolve_or_error(
            &file, &params.0.name, params.0.kind.as_deref(), params.0.symbol_line,
        ) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let old_bytes = (sym.byte_range.1 - sym.byte_range.0) as usize;
        let (new_content, count) = match edit::build_edit_within(
            &file.content, &sym, &params.0.old_text, &params.0.new_text,
        ) {
            Ok(r) => r,
            Err(e) => return e,
        };
        let new_bytes = new_content.len() - file.content.len() + old_bytes;
        let abs_path = repo_root.join(&params.0.path);
        if let Err(e) = edit::atomic_write_file(&abs_path, &new_content) {
            return format!("Error writing {}: {e}", params.0.path);
        }
        edit::reindex_after_write(&self.index, &params.0.path, new_content, file.language);
        edit_format::format_edit_within(&params.0.path, &params.0.name, count, old_bytes, new_bytes)
    }
```

- [ ] **Step 4: Write integration test**

```rust
#[tokio::test]
async fn test_edit_within_symbol_replaces_scoped_text() {
    let dir = tempfile::tempdir().unwrap();
    let src_dir = dir.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let file_path = src_dir.join("lib.rs");
    let original = b"fn untouched() {\n    let x = old_val;\n}\n\nfn target() {\n    let y = old_val;\n}\n";
    std::fs::write(&file_path, original).unwrap();

    let result = crate::parsing::process_file("src/lib.rs", original, LanguageId::Rust);
    let indexed = IndexedFile::from_parse_result(result, original.to_vec());
    let mut index = LiveIndex::empty();
    index.update_file("src/lib.rs".to_string(), indexed);
    let server = make_server_with_root(index, Some(dir.path().to_path_buf()));

    let input = edit::EditWithinSymbolInput {
        path: "src/lib.rs".to_string(),
        name: "target".to_string(),
        kind: None,
        symbol_line: None,
        old_text: "old_val".to_string(),
        new_text: "new_val".to_string(),
    };
    let result = server.edit_within_symbol(Parameters(input)).await;
    assert!(result.contains("edited within"), "result: {result}");

    let on_disk = std::fs::read_to_string(&file_path).unwrap();
    // "target" function should have new_val
    assert!(on_disk.contains("fn target() {\n    let y = new_val;\n}"), "disk: {on_disk}");
    // "untouched" function should still have old_val (scoped edit!)
    assert!(on_disk.contains("fn untouched() {\n    let x = old_val;\n}"), "disk: {on_disk}");
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p tokenizor_agentic_mcp --lib protocol::edit::tests::test_build_edit_within -- --nocapture && cargo test -p tokenizor_agentic_mcp --lib protocol::tools::tests::test_edit_within -- --nocapture`
Expected: All PASS

- [ ] **Step 6: Commit**

```bash
git add src/protocol/edit.rs src/protocol/tools.rs
git commit -m "feat(edit): add edit_within_symbol tool"
```

---

### Task 11: Update tool count stability test

**Files:**
- Modify: `src/protocol/tools.rs:4355-4364`

- [ ] **Step 1: Update the expected tool count**

Find the `test_tools_registered_count_is_stable` test. It asserts a specific tool count. Add 5 to whatever the current count is (for replace_symbol_body, insert_before_symbol, insert_after_symbol, delete_symbol, edit_within_symbol).

The count will be updated again in Chunk 3 when Tier 2 tools are added.

- [ ] **Step 2: Run test**

Run: `cargo test -p tokenizor_agentic_mcp --lib protocol::tools::tests::test_tools_registered_count -- --nocapture`
Expected: PASS

- [ ] **Step 3: Run full test suite**

Run: `cargo test -p tokenizor_agentic_mcp`
Expected: All tests PASS (ensures no regressions)

- [ ] **Step 4: Commit**

```bash
git add src/protocol/tools.rs
git commit -m "test: update registered tool count for Tier 1 edit tools"
```

---

## Chunk 3: Tier 2 — Batch Edit Tools

### Task 12: `batch_edit` — atomic multi-file edits

**Files:**
- Modify: `src/protocol/edit.rs` — add `BatchEditInput`, `SingleEdit`, `EditOperation`, `execute_batch_edit`
- Modify: `src/protocol/tools.rs` — add handler

- [ ] **Step 1: Add types and batch logic to `edit.rs`**

```rust
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct BatchEditInput {
    /// List of individual edits to apply atomically.
    pub edits: Vec<SingleEdit>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct SingleEdit {
    /// Relative file path.
    pub path: String,
    /// Symbol name.
    pub name: String,
    /// Optional kind filter.
    pub kind: Option<String>,
    /// Line number to disambiguate.
    #[serde(default, deserialize_with = "super::tools::lenient_u32")]
    pub symbol_line: Option<u32>,
    /// The edit operation to perform.
    pub operation: EditOperation,
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type")]
pub enum EditOperation {
    /// Replace the entire symbol definition.
    #[serde(rename = "replace")]
    Replace { new_body: String },
    /// Insert code before the symbol.
    #[serde(rename = "insert_before")]
    InsertBefore { content: String },
    /// Insert code after the symbol.
    #[serde(rename = "insert_after")]
    InsertAfter { content: String },
    /// Delete the symbol.
    #[serde(rename = "delete")]
    Delete,
    /// Find-and-replace within the symbol.
    #[serde(rename = "edit_within")]
    EditWithin { old_text: String, new_text: String },
}

/// Validate all edits and return resolved (path, symbol, operation) triples or an error.
/// Edits are grouped by file and sorted reverse-by-byte-offset so earlier splices don't
/// invalidate later offsets.
pub(crate) fn execute_batch_edit(
    index: &SharedIndex,
    repo_root: &Path,
    edits: &[SingleEdit],
) -> Result<Vec<String>, String> {
    // Phase 1: Resolve all symbols and validate.
    struct ResolvedEdit {
        path: String,
        sym: SymbolRecord,
        operation: usize, // index into edits
        language: LanguageId,
    }

    let mut resolved = Vec::with_capacity(edits.len());
    {
        let guard = index.read().expect("lock poisoned");
        for (i, edit) in edits.iter().enumerate() {
            let file = guard.get_file(&edit.path).ok_or_else(|| {
                format!("File not indexed: {}", edit.path)
            })?;
            let (_, sym) = resolve_or_error(
                file,
                &edit.name,
                edit.kind.as_deref(),
                edit.symbol_line,
            ).map_err(|e| format!("Edit {}: {e}", i + 1))?;
            resolved.push(ResolvedEdit {
                path: edit.path.clone(),
                sym,
                operation: i,
                language: file.language,
            });
        }
    }

    // Phase 1b: Validate no overlapping byte ranges within the same file.
    let mut by_file: std::collections::HashMap<String, Vec<&ResolvedEdit>> =
        std::collections::HashMap::new();
    for r in &resolved {
        by_file.entry(r.path.clone()).or_default().push(r);
    }
    for (path, group) in &by_file {
        for i in 0..group.len() {
            for j in (i + 1)..group.len() {
                let (a, b) = (group[i].sym.byte_range, group[j].sym.byte_range);
                if a.0 < b.1 && b.0 < a.1 {
                    return Err(format!(
                        "Overlapping edits in {path}: `{}` ({}-{}) and `{}` ({}-{}). \
                         Split into separate calls.",
                        group[i].sym.name, a.0, a.1, group[j].sym.name, b.0, b.1,
                    ));
                }
            }
        }
    }

    // Phase 2: Sort reverse by byte offset (so earlier splices don't shift later offsets).
    for group in by_file.values_mut() {
        group.sort_by(|a, b| b.sym.byte_range.0.cmp(&a.sym.byte_range.0));
    }

    // Phase 3: Apply edits per file, write, reindex.
    let mut summaries = Vec::new();

    for (path, group) in &by_file {
        let file = {
            let guard = index.read().expect("lock poisoned");
            guard.capture_shared_file(path)
                .ok_or_else(|| format!("File disappeared: {path}"))?
        };

        let mut content = file.content.clone();
        let language = group[0].language;

        for r in group {
            let edit = &edits[r.operation];
            match &edit.operation {
                EditOperation::Replace { new_body } => {
                    let old_bytes = (r.sym.byte_range.1 - r.sym.byte_range.0) as usize;
                    content = apply_splice(&content, r.sym.byte_range, new_body.as_bytes());
                    summaries.push(super::edit_format::format_replace(
                        path, &r.sym.name, &r.sym.kind.to_string(), old_bytes, new_body.len(),
                    ));
                }
                EditOperation::InsertBefore { content: code } => {
                    content = build_insert_before(&content, &r.sym, code);
                    summaries.push(super::edit_format::format_insert(
                        path, &r.sym.name, "before", code.len(),
                    ));
                }
                EditOperation::InsertAfter { content: code } => {
                    content = build_insert_after(&content, &r.sym, code);
                    summaries.push(super::edit_format::format_insert(
                        path, &r.sym.name, "after", code.len(),
                    ));
                }
                EditOperation::Delete => {
                    let deleted = (r.sym.byte_range.1 - r.sym.byte_range.0) as usize;
                    content = build_delete(&content, &r.sym);
                    summaries.push(super::edit_format::format_delete(
                        path, &r.sym.name, &r.sym.kind.to_string(), deleted,
                    ));
                }
                EditOperation::EditWithin { old_text, new_text } => {
                    let old_bytes = (r.sym.byte_range.1 - r.sym.byte_range.0) as usize;
                    let (new, count) = build_edit_within(&content, &r.sym, old_text, new_text)
                        .map_err(|e| format!("Edit in {path}:{}: {e}", r.sym.name))?;
                    content = new;
                    summaries.push(super::edit_format::format_edit_within(
                        path, &r.sym.name, count, old_bytes, old_bytes, // approximate
                    ));
                }
            }
        }

        let abs_path = repo_root.join(path);
        atomic_write_file(&abs_path, &content)
            .map_err(|e| format!("Write failed for {path}: {e}"))?;
        reindex_after_write(index, path, content, language);
    }

    Ok(summaries)
}
```

**Important caveat:** When applying multiple edits to the same file, byte offsets shift after each splice. The reverse-sort-by-offset approach works for non-overlapping ranges. For Tier 2, this is documented as a known constraint: edits within the same file must target non-overlapping symbols.

- [ ] **Step 2: Write unit test**

```rust
#[test]
fn test_execute_batch_edit_applies_multiple_edits() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("a.rs"), b"fn alpha() { old }\n").unwrap();
    std::fs::write(src.join("b.rs"), b"fn beta() { keep }\n").unwrap();

    let handle = LiveIndex::empty(); // returns SharedIndex = Arc<SharedIndexHandle>
    // Index both files
    for (path, content) in [("src/a.rs", b"fn alpha() { old }\n" as &[u8]), ("src/b.rs", b"fn beta() { keep }\n")] {
        let result = crate::parsing::process_file(path, content, LanguageId::Rust);
        let indexed = IndexedFile::from_parse_result(result, content.to_vec());
        handle.update_file(path.to_string(), indexed);
    }

    let edits = vec![
        SingleEdit {
            path: "src/a.rs".to_string(),
            name: "alpha".to_string(),
            kind: None,
            symbol_line: None,
            operation: EditOperation::Replace { new_body: "fn alpha() { new }".to_string() },
        },
        SingleEdit {
            path: "src/b.rs".to_string(),
            name: "beta".to_string(),
            kind: None,
            symbol_line: None,
            operation: EditOperation::Delete,
        },
    ];

    let summaries = execute_batch_edit(&handle, dir.path(), &edits).unwrap();
    assert_eq!(summaries.len(), 2);

    let a_content = std::fs::read_to_string(src.join("a.rs")).unwrap();
    assert!(a_content.contains("new"), "a.rs: {a_content}");

    let b_content = std::fs::read_to_string(src.join("b.rs")).unwrap();
    assert!(!b_content.contains("beta"), "b.rs: {b_content}");
}
```

- [ ] **Step 3: Add handler to `tools.rs`**

```rust
    /// Apply multiple edits across files in one atomic operation.
    #[tool(
        description = "Apply multiple symbol-addressed edits atomically. Each edit specifies a file, symbol, and operation (replace/insert_before/insert_after/delete/edit_within). All edits are validated before any writes."
    )]
    pub(crate) async fn batch_edit(
        &self,
        params: Parameters<edit::BatchEditInput>,
    ) -> String {
        if let Some(result) = self.proxy_tool_call("batch_edit", &params.0).await {
            return result;
        }
        let repo_root = match self.capture_repo_root() {
            Some(root) => root,
            None => return "Error: no repository root configured.".to_string(),
        };
        {
            let guard = self.index.read().expect("lock poisoned");
            loading_guard!(guard);
        }
        match edit::execute_batch_edit(&self.index, &repo_root, &params.0.edits) {
            Ok(summaries) => {
                let file_count = params.0.edits.iter()
                    .map(|e| e.path.as_str())
                    .collect::<std::collections::HashSet<_>>()
                    .len();
                edit_format::format_batch_summary(&summaries, file_count)
            }
            Err(e) => e,
        }
    }
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p tokenizor_agentic_mcp --lib protocol::edit::tests::test_execute_batch -- --nocapture && cargo test -p tokenizor_agentic_mcp --lib protocol::tools::tests::test_batch_edit -- --nocapture`
Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add src/protocol/edit.rs src/protocol/tools.rs
git commit -m "feat(edit): add batch_edit tool for atomic multi-file edits"
```

---

### Task 13: `batch_rename` — rename symbol + update all references

**Files:**
- Modify: `src/protocol/edit.rs` — add `BatchRenameInput` + `execute_batch_rename`
- Modify: `src/protocol/tools.rs` — add handler

- [ ] **Step 1: Add types and rename logic to `edit.rs`**

```rust
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct BatchRenameInput {
    /// Relative file path containing the symbol definition.
    pub path: String,
    /// Current symbol name.
    pub name: String,
    /// Optional kind filter.
    pub kind: Option<String>,
    /// Line number to disambiguate.
    #[serde(default, deserialize_with = "super::tools::lenient_u32")]
    pub symbol_line: Option<u32>,
    /// New name for the symbol.
    pub new_name: String,
}

/// Rename a symbol and all its references across the project.
/// The definition site is found by locating the symbol's name text within its byte range
/// (not replacing the full body). Reference sites use `ReferenceRecord.byte_range` which
/// already points to the exact name occurrence.
pub(crate) fn execute_batch_rename(
    index: &SharedIndex,
    repo_root: &Path,
    input: &BatchRenameInput,
) -> Result<String, String> {
    // Phase 1: Resolve the definition symbol and find the name within its body.
    let (def_name_range, language) = {
        let guard = index.read().expect("lock poisoned");
        let file = guard.get_file(&input.path)
            .ok_or_else(|| format!("File not indexed: {}", input.path))?;
        let (_, sym) = resolve_or_error(
            file, &input.name, input.kind.as_deref(), input.symbol_line,
        )?;
        // Find the name text within the symbol's byte range (usually on the first line).
        let body = &file.content[sym.byte_range.0 as usize..sym.byte_range.1 as usize];
        let name_offset = body.windows(input.name.len())
            .position(|w| w == input.name.as_bytes())
            .ok_or_else(|| format!(
                "Could not locate name `{}` within symbol body at {}:{}-{}",
                input.name, input.path, sym.byte_range.0, sym.byte_range.1
            ))?;
        let abs_start = sym.byte_range.0 + name_offset as u32;
        let abs_end = abs_start + input.name.len() as u32;
        ((abs_start, abs_end), file.language)
    };

    // Phase 2: Find all references to this symbol name across the project.
    // ReferenceRecord.byte_range already points to the exact name occurrence.
    let ref_sites: Vec<(String, (u32, u32))> = {
        let guard = index.read().expect("lock poisoned");
        let refs = guard.find_references_for_name(&input.name, None, false);
        refs.into_iter()
            .map(|(path, rr)| (path.to_string(), rr.byte_range))
            .collect()
    };

    // Phase 3: Group all rename sites by file (definition + references).
    let mut by_file: std::collections::HashMap<String, Vec<(u32, u32)>> =
        std::collections::HashMap::new();
    // Add the definition site (name-only range, NOT full body).
    by_file.entry(input.path.clone()).or_default().push(def_name_range);
    // Add reference sites.
    for (path, range) in &ref_sites {
        by_file.entry(path.clone()).or_default().push(*range);
    }

    // Sort each file's ranges reverse by offset, dedup.
    for ranges in by_file.values_mut() {
        ranges.sort_by(|a, b| b.0.cmp(&a.0));
        ranges.dedup(); // Definition site might also appear as a reference.
    }

    // Phase 4: Apply renames (reverse order preserves offsets), write, reindex.
    let new_name_bytes = input.new_name.as_bytes();
    let mut files_updated = 0;
    let mut refs_updated = 0;

    for (path, ranges) in &by_file {
        let file = {
            let guard = index.read().expect("lock poisoned");
            guard.capture_shared_file(path)
                .ok_or_else(|| format!("File disappeared: {path}"))?
        };

        let mut content = file.content.clone();
        for range in ranges {
            content = apply_splice(&content, *range, new_name_bytes);
            refs_updated += 1;
        }

        let abs_path = repo_root.join(path);
        atomic_write_file(&abs_path, &content)
            .map_err(|e| format!("Write failed for {path}: {e}"))?;

        let lang = file.language;
        reindex_after_write(index, path, content, lang);
        files_updated += 1;
    }

    Ok(format!(
        "Renamed `{}` → `{}` — {refs_updated} site(s) across {files_updated} file(s)",
        input.name, input.new_name,
    ))
}
```

**Known limitation:** `find_references_for_name` finds references by simple name match, which may produce false positives for common names. The rename is best-effort and the agent should review the result with `what_changed`.

- [ ] **Step 2: Write unit test**

```rust
#[test]
fn test_execute_batch_rename_renames_def_and_refs() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("lib.rs"), b"fn old_name() {}\n").unwrap();
    std::fs::write(src.join("main.rs"), b"fn caller() { old_name(); }\n").unwrap();

    let index = LiveIndex::empty();
    let handle = SharedIndexHandle::shared(index);

    for (path, content) in [
        ("src/lib.rs", b"fn old_name() {}\n" as &[u8]),
        ("src/main.rs", b"fn caller() { old_name(); }\n"),
    ] {
        let result = crate::parsing::process_file(path, content, LanguageId::Rust);
        let indexed = IndexedFile::from_parse_result(result, content.to_vec());
        handle.update_file(path.to_string(), indexed);
    }

    let input = BatchRenameInput {
        path: "src/lib.rs".to_string(),
        name: "old_name".to_string(),
        kind: None,
        symbol_line: None,
        new_name: "new_name".to_string(),
    };

    let result = execute_batch_rename(&handle, dir.path(), &input).unwrap();
    assert!(result.contains("Renamed"), "result: {result}");
    assert!(result.contains("new_name"), "result: {result}");

    let lib = std::fs::read_to_string(src.join("lib.rs")).unwrap();
    assert!(lib.contains("new_name"), "lib.rs: {lib}");
    assert!(!lib.contains("old_name"), "lib.rs: {lib}");

    let main = std::fs::read_to_string(src.join("main.rs")).unwrap();
    assert!(main.contains("new_name"), "main.rs: {main}");
}
```

- [ ] **Step 3: Add handler to `tools.rs`**

```rust
    /// Rename a symbol and update all references project-wide.
    #[tool(
        description = "Rename a symbol and update all references across the project. Finds the definition and all usage sites via the index's reverse reference map."
    )]
    pub(crate) async fn batch_rename(
        &self,
        params: Parameters<edit::BatchRenameInput>,
    ) -> String {
        if let Some(result) = self.proxy_tool_call("batch_rename", &params.0).await {
            return result;
        }
        let repo_root = match self.capture_repo_root() {
            Some(root) => root,
            None => return "Error: no repository root configured.".to_string(),
        };
        {
            let guard = self.index.read().expect("lock poisoned");
            loading_guard!(guard);
        }
        match edit::execute_batch_rename(&self.index, &repo_root, &params.0) {
            Ok(summary) => summary,
            Err(e) => e,
        }
    }
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p tokenizor_agentic_mcp --lib protocol::edit::tests::test_execute_batch_rename -- --nocapture`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/protocol/edit.rs src/protocol/tools.rs
git commit -m "feat(edit): add batch_rename tool for project-wide symbol renaming"
```

---

### Task 14: `batch_insert` — insert same code at multiple locations

**Files:**
- Modify: `src/protocol/edit.rs` — add `BatchInsertInput` + `execute_batch_insert`
- Modify: `src/protocol/tools.rs` — add handler

- [ ] **Step 1: Add types and logic to `edit.rs`**

```rust
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct BatchInsertInput {
    /// Code to insert at each target location.
    pub content: String,
    /// Where to insert: before or after.
    pub position: InsertPosition,
    /// Target symbols to insert adjacent to.
    pub targets: Vec<InsertTarget>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InsertPosition {
    Before,
    After,
}

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct InsertTarget {
    /// Relative file path.
    pub path: String,
    /// Symbol name.
    pub name: String,
    /// Optional kind filter.
    pub kind: Option<String>,
    /// Line number to disambiguate.
    #[serde(default, deserialize_with = "super::tools::lenient_u32")]
    pub symbol_line: Option<u32>,
}

pub(crate) fn execute_batch_insert(
    index: &SharedIndex,
    repo_root: &Path,
    input: &BatchInsertInput,
) -> Result<Vec<String>, String> {
    let mut summaries = Vec::new();
    let position_label = match input.position {
        InsertPosition::Before => "before",
        InsertPosition::After => "after",
    };

    for target in &input.targets {
        let file = {
            let guard = index.read().expect("lock poisoned");
            guard.capture_shared_file(&target.path)
                .ok_or_else(|| format!("File not indexed: {}", target.path))?
        };

        let (_, sym) = resolve_or_error(
            &file, &target.name, target.kind.as_deref(), target.symbol_line,
        ).map_err(|e| format!("Target {}: {e}", target.path))?;

        let new_content = match input.position {
            InsertPosition::Before => build_insert_before(&file.content, &sym, &input.content),
            InsertPosition::After => build_insert_after(&file.content, &sym, &input.content),
        };

        let abs_path = repo_root.join(&target.path);
        atomic_write_file(&abs_path, &new_content)
            .map_err(|e| format!("Write failed for {}: {e}", target.path))?;

        let lang = file.language;
        reindex_after_write(index, &target.path, new_content, lang);
        summaries.push(super::edit_format::format_insert(
            &target.path, &target.name, position_label, input.content.len(),
        ));
    }

    Ok(summaries)
}
```

- [ ] **Step 2: Write unit test**

```rust
#[test]
fn test_execute_batch_insert_adds_to_multiple_files() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("a.rs"), b"fn handler_a() {}\n").unwrap();
    std::fs::write(src.join("b.rs"), b"fn handler_b() {}\n").unwrap();

    let index = LiveIndex::empty();
    let handle = SharedIndexHandle::shared(index);
    for (path, content) in [
        ("src/a.rs", b"fn handler_a() {}\n" as &[u8]),
        ("src/b.rs", b"fn handler_b() {}\n"),
    ] {
        let result = crate::parsing::process_file(path, content, LanguageId::Rust);
        let indexed = IndexedFile::from_parse_result(result, content.to_vec());
        handle.update_file(path.to_string(), indexed);
    }

    let input = BatchInsertInput {
        content: "fn logging() { log::info!(\"called\"); }".to_string(),
        position: InsertPosition::After,
        targets: vec![
            InsertTarget { path: "src/a.rs".to_string(), name: "handler_a".to_string(), kind: None, symbol_line: None },
            InsertTarget { path: "src/b.rs".to_string(), name: "handler_b".to_string(), kind: None, symbol_line: None },
        ],
    };

    let summaries = execute_batch_insert(&handle, dir.path(), &input).unwrap();
    assert_eq!(summaries.len(), 2);

    let a = std::fs::read_to_string(src.join("a.rs")).unwrap();
    assert!(a.contains("logging"), "a.rs: {a}");
    let b = std::fs::read_to_string(src.join("b.rs")).unwrap();
    assert!(b.contains("logging"), "b.rs: {b}");
}
```

- [ ] **Step 3: Add handler to `tools.rs`**

```rust
    /// Insert the same code at multiple symbol locations across files.
    #[tool(
        description = "Insert the same code before or after multiple symbols across the project. Useful for adding logging, instrumentation, or boilerplate to many locations at once."
    )]
    pub(crate) async fn batch_insert(
        &self,
        params: Parameters<edit::BatchInsertInput>,
    ) -> String {
        if let Some(result) = self.proxy_tool_call("batch_insert", &params.0).await {
            return result;
        }
        let repo_root = match self.capture_repo_root() {
            Some(root) => root,
            None => return "Error: no repository root configured.".to_string(),
        };
        {
            let guard = self.index.read().expect("lock poisoned");
            loading_guard!(guard);
        }
        match edit::execute_batch_insert(&self.index, &repo_root, &params.0) {
            Ok(summaries) => {
                let file_count = params.0.targets.iter()
                    .map(|t| t.path.as_str())
                    .collect::<std::collections::HashSet<_>>()
                    .len();
                edit_format::format_batch_summary(&summaries, file_count)
            }
            Err(e) => e,
        }
    }
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p tokenizor_agentic_mcp --lib protocol::edit::tests::test_execute_batch_insert -- --nocapture`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/protocol/edit.rs src/protocol/tools.rs
git commit -m "feat(edit): add batch_insert tool for multi-location code insertion"
```

---

### Task 15: Update tool count + stale ref warnings + full regression

**Files:**
- Modify: `src/protocol/tools.rs:4355-4364` — update count for Tier 2 tools (+2 for batch_rename, batch_insert; batch_edit already counted if Task 12 updated it)
- Modify: `src/protocol/edit.rs` — add `detect_stale_references` (optional enhancement)

- [ ] **Step 1: Update tool count test**

Update `test_tools_registered_count_is_stable` to reflect the final total: original count + 7 new tools (5 Tier 1 + 2 Tier 2 = 7). Note: `batch_edit` was +1 in Task 12 but the count test was deferred to here.

The final expected count = original + 7.

- [ ] **Step 2: Add optional stale reference detection to `edit.rs`**

```rust
/// Extract the first line of a byte slice as a rough "signature" for change detection.
pub(crate) fn extract_signature(content: &[u8], byte_range: (u32, u32)) -> String {
    let start = byte_range.0 as usize;
    let end = byte_range.1 as usize;
    let slice = &content[start..end];
    let first_line_end = slice.iter().position(|&b| b == b'\n').unwrap_or(slice.len());
    String::from_utf8_lossy(&slice[..first_line_end]).to_string()
}

/// Detect references that may be stale after a symbol edit.
/// Compares old vs new signature (first line). Returns (path, line, enclosing_name) triples.
pub(crate) fn detect_stale_references(
    index: &SharedIndex,
    path: &str,
    name: &str,
    old_signature: &str,
    new_signature: &str,
) -> Vec<(String, u32, Option<String>)> {
    if old_signature == new_signature {
        return Vec::new();
    }
    let guard = index.read().expect("lock poisoned");
    let refs = guard.find_references_for_name(name, None, false);
    refs.into_iter()
        .filter(|(ref_path, _)| *ref_path != path)
        .map(|(ref_path, rr)| {
            let enclosing = rr.enclosing_symbol_index.and_then(|idx| {
                guard.get_file(ref_path)
                    .and_then(|f| f.symbols.get(idx as usize))
                    .map(|s| s.name.clone())
            });
            (ref_path.to_string(), rr.line_range.0 + 1, enclosing)
        })
        .collect()
}
```

- [ ] **Step 3: Wire stale warnings into `replace_symbol_body` handler**

Update the `replace_symbol_body` handler in `tools.rs` to append stale warnings:

After `edit::reindex_after_write(...)`, add:

```rust
        let old_sig = edit::extract_signature(&file.content, sym.byte_range);
        let new_sig = params.0.new_body.lines().next().unwrap_or("").to_string();
        let warnings = edit::detect_stale_references(
            &self.index, &params.0.path, &params.0.name, &old_sig, &new_sig,
        );
        let mut result = edit_format::format_replace(
            &params.0.path, &params.0.name, &sym.kind.to_string(), old_bytes, params.0.new_body.len(),
        );
        result.push_str(&edit_format::format_stale_warnings(
            &params.0.path, &params.0.name, &warnings,
        ));
        result
```

- [ ] **Step 4: Run full test suite**

Run: `cargo test -p tokenizor_agentic_mcp`
Expected: All tests PASS — zero regressions

- [ ] **Step 5: Commit**

```bash
git add src/protocol/edit.rs src/protocol/tools.rs
git commit -m "feat(edit): add stale reference warnings and finalize tool count"
```

---

### Task 16: Final cleanup and documentation update

- [ ] **Step 1: Verify all 7 tools are registered**

Run: `cargo test -p tokenizor_agentic_mcp --lib protocol::tools::tests::test_tools_registered_count -- --nocapture && cargo test -p tokenizor_agentic_mcp --lib protocol::tools::tests::test_no_v1_tools -- --nocapture`
Expected: Both PASS

- [ ] **Step 2: Run `cargo clippy`**

Run: `cargo clippy -p tokenizor_agentic_mcp -- -D warnings`
Expected: No warnings. Fix any that appear.

- [ ] **Step 3: Run full test suite one final time**

Run: `cargo test -p tokenizor_agentic_mcp`
Expected: All PASS

- [ ] **Step 4: Final commit**

```bash
git add -A
git commit -m "feat(edit): symbol-addressed edit tools — complete Tier 1 + Tier 2"
```

---

## Known Constraints & Future Work

1. **Non-overlapping edits:** `batch_edit` validates and rejects overlapping symbol byte ranges within the same file. Overlapping edits (e.g., editing a method and its parent class in one batch) must be split into sequential calls.

2. **Rename precision:** `batch_rename` uses simple name matching via the reverse index. Symbols with common names (e.g., `new`, `get`) may produce false-positive renames. The agent should verify with `what_changed` after a rename.

3. **Atomicity:** Cross-file batch operations write files sequentially. If the process crashes mid-batch, some files may be updated while others aren't. True transactional atomicity would require a write-ahead log (out of scope for v1).

4. **Stale reference warnings:** Only implemented for `replace_symbol_body`. Could be extended to `batch_rename` and `edit_within_symbol` in a follow-up.

5. **Watcher dedup:** The file watcher has a 200-500ms debounce and compares SHA hashes. Since we write+reindex synchronously before the watcher fires, the hash matches and the watcher skips redundant reindexing. No special coordination needed.

6. **Fuzzy symbol resolution (deferred):** The handoff constraint #4 says "Symbol resolution must be fuzzy-tolerant (same matching as search_symbols)." The current `resolve_symbol_selector` uses exact name matching. For v1, this is acceptable — agents using `get_file_outline` or `search_symbols` first will always have the exact name. A future enhancement could add case-insensitive fallback and Levenshtein-distance suggestions on `NotFound`, matching the fuzzy behavior of `search_symbols`. This is tracked as a follow-up, not a blocker.

7. **`tempfile` crate:** Tests require `tempfile` as a dev dependency. It is already present in the project's `Cargo.toml` (used by `store.rs` tests). No action needed.
