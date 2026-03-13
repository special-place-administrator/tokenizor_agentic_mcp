# Doc Comment Ranges — Design Spec

## Problem

`replace_symbol_body` and `delete_symbol` leave orphaned doc comments (e.g., `///`, `/** */`, `#`) when editing or removing symbols. The root cause: `SymbolRecord.byte_range` is derived from tree-sitter node boundaries, which start at the symbol keyword (`fn`, `struct`, `class`), not at preceding doc comments. Doc comments are sibling nodes in the tree, not children of the symbol node.

Affects 15 of 16 supported languages (Python docstrings are inside the function body and already covered).

## Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Storage | Separate `doc_byte_range: Option<(u32, u32)>` field on `SymbolRecord` | Keeps existing `byte_range` semantics stable for all consumers; edit tools opt into the wider range |
| Doc detection | Hybrid — language-aware prefixes where they exist, all-adjacent-comments for languages without formal doc syntax (Go, Ruby, Perl) | Correctly handles formal syntaxes while supporting convention-based docs |
| Blank line handling | Stop at first blank line | Matches what tree-sitter-based tools consider "attached"; avoids merging unrelated comment blocks |
| Computation timing | Parse time (in `push_symbol`, using tree-sitter sibling walk) | Tree is already available; per-symbol cost ~microseconds; data always ready without re-computation |
| Implementation | Static config per language with escape hatch callback for Elixir | 15/16 languages fit static config; Elixir needs custom check for `@doc` AST attributes |

## Design

### 1. Data Model

**`SymbolRecord`** gains one field:

```rust
pub struct SymbolRecord {
    pub name: String,
    pub kind: SymbolKind,
    pub depth: u32,
    pub sort_order: u32,
    pub byte_range: (u32, u32),                // unchanged — symbol keyword to closing brace
    pub line_range: (u32, u32),                // unchanged
    pub doc_byte_range: Option<(u32, u32)>,    // NEW — first doc comment byte to symbol start byte
}
```

- `None` = no attached doc comments (or language skips doc detection)
- When present: `doc_byte_range.0` is the start of the first doc comment line; `doc_byte_range.1` equals `byte_range.0` (contiguous)

**`DocCommentSpec`** — per-language configuration:

```rust
pub(super) struct DocCommentSpec {
    /// Tree-sitter node type names that could be doc comments.
    pub comment_node_types: &'static [&'static str],
    /// Text prefixes that distinguish doc comments from regular comments.
    /// None = all comments of matching node types are considered doc comments.
    pub doc_prefixes: Option<&'static [&'static str]>,
    /// Optional custom check for non-comment doc patterns (e.g., Elixir @doc).
    pub custom_doc_check: Option<fn(&Node, &str) -> bool>,
}
```

### 2. Scanning Algorithm

Shared helper in `src/parsing/languages/mod.rs`:

```rust
fn scan_doc_range(node: &Node, source: &str, spec: &DocCommentSpec) -> Option<(u32, u32)>
```

**Steps:**

1. Start at `node.prev_sibling()`.
2. For each sibling:
   - Check if `node.kind()` is in `spec.comment_node_types`, OR `spec.custom_doc_check` returns true.
   - If neither, stop.
3. For comment nodes when `spec.doc_prefixes` is `Some(prefixes)`: extract the node's text from `source`, check it starts with (after trimming) one of the prefixes. If not, stop.
4. **Blank line check:** if the gap between this sibling's end line and the next item's start line is > 1 (i.e., a blank line), stop.
5. Track the earliest `start_byte` across all matched siblings.
6. Return `Some((earliest_start_byte, node.start_byte()))` or `None`.

**Integration:** `push_symbol` and `push_named_symbol` gain `source: &str` and `doc_spec: &DocCommentSpec` parameters. `push_symbol` calls `scan_doc_range` and sets `doc_byte_range`.

### 3. Per-Language Configurations

| Language | `comment_node_types` | `doc_prefixes` | `custom_doc_check` |
|---|---|---|---|
| Rust | `["line_comment", "block_comment"]` | `Some(["///", "//!", "/**", "/*!"])` | None |
| Python | `[]` | None | None |
| JavaScript | `["comment"]` | `Some(["/**"])` | None |
| TypeScript | `["comment"]` | `Some(["/**"])` | None |
| Go | `["comment"]` | None | None |
| Java | `["line_comment", "block_comment"]` | `Some(["/**"])` | None |
| C | `["comment"]` | `Some(["///", "/**", "//!", "/*!"])` | None |
| C++ | `["comment"]` | `Some(["///", "/**", "//!", "/*!"])` | None |
| C# | `["comment"]` | `Some(["///"])` | None |
| Ruby | `["comment"]` | None | None |
| PHP | `["comment"]` | `Some(["/**"])` | None |
| Swift | `["comment", "multiline_comment"]` | `Some(["///", "/**"])` | None |
| Kotlin | `["line_comment", "multiline_comment"]` | `Some(["/**"])` | None |
| Dart | `[]` | None | None |
| Perl | `["comment"]` | None | None |
| Elixir | `["comment"]` | None | `Some(is_elixir_doc)` |

Each language file defines `pub(super) const DOC_SPEC: DocCommentSpec`.

**Dart note:** tree-sitter-dart uses hidden comment tokens (`_SINGLE_LINE_COMMENT`, `_MULTI_LINE_COMMENT`). These may not appear as named siblings. Skip for now; revisit if runtime testing shows they are accessible.

**Elixir `is_elixir_doc`:** checks if the preceding sibling is a `call` or `unary_operator` node whose text starts with `@doc`, `@moduledoc`, or `@typedoc`.

### 4. Edit Tool Changes

**`replace_symbol_body`** (`src/protocol/tools.rs`):
- Current: `line_start` = start of line containing `sym.byte_range.0`
- New: if `sym.doc_byte_range.is_some()`, use start of line containing `doc_byte_range.0` as the splice start
- The caller provides the full replacement including doc comments (they already read the symbol body first, which will now include docs via `get_symbol`)

**`build_delete`** (`src/protocol/edit.rs`):
- Current: extends to start of line from `sym.byte_range.0`
- New: if `sym.doc_byte_range.is_some()`, extend to start of line from `doc_byte_range.0`
- Doc comments are deleted along with the symbol

**No changes needed to:** `get_symbol` body extraction (should use `doc_byte_range.0` as start when present to include docs in the returned body), outlines, search, format. These continue using `byte_range` for positioning. Future: outlines could optionally show first doc line using the range.

### 5. Serialization / Persistence

`SymbolRecord` is persisted in the index. `doc_byte_range` needs to be included in:
- `src/live_index/persist.rs` — serialization/deserialization
- Any test helpers that construct `SymbolRecord` (add `doc_byte_range: None` to existing test fixtures)

### 6. Files Modified

| File | Change |
|---|---|
| `src/domain/index.rs` | Add `doc_byte_range` field to `SymbolRecord` |
| `src/parsing/languages/mod.rs` | Add `DocCommentSpec`, `scan_doc_range`, update `push_symbol`/`push_named_symbol` signatures |
| `src/parsing/languages/*.rs` (16 files) | Add `DOC_SPEC` constant, pass to `push_named_symbol` |
| `src/protocol/tools.rs` | `replace_symbol_body` uses `doc_byte_range` for splice start |
| `src/protocol/edit.rs` | `build_delete` uses `doc_byte_range` for delete start |
| `src/live_index/persist.rs` | Serialize/deserialize `doc_byte_range` |
| Tests | Add `doc_byte_range: None` to existing fixtures; add new tests for doc range scanning |

### 7. Testing Strategy

- **Unit tests in `src/parsing/languages/mod.rs`**: test `scan_doc_range` with synthetic tree-sitter trees for Rust (`///`), Java (`/** */`), Go (all adjacent), and Elixir (`@doc`).
- **Integration tests**: parse real source snippets through `extract_symbols` and verify `doc_byte_range` is set correctly for each language.
- **Edit tests**: verify `replace_symbol_body` and `delete_symbol` include doc comments in the replaced/deleted range.
- **Regression tests**: ensure existing behavior is unchanged when `doc_byte_range` is `None`.
