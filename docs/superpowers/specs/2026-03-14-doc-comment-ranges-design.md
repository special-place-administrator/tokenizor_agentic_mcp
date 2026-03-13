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
- When present: `doc_byte_range.0` is the start byte of the first doc comment line; `doc_byte_range.1` is the end byte of the last doc comment sibling (not `byte_range.0` — there may be attributes or whitespace between the last doc comment and the symbol keyword)

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

1. Start with `sibling = node.prev_sibling()`.
2. For each `sibling`:
   - Check if `sibling.kind()` is in `spec.comment_node_types`, OR `spec.custom_doc_check` returns true for `sibling`.
   - If neither, stop.
3. For comment nodes when `spec.doc_prefixes` is `Some(prefixes)`: extract the sibling's text from `source`, check it starts with (after trimming) one of the prefixes. If not, stop.
4. **Blank line check:** if the gap between this sibling's end line and the next item's start line is > 1 (i.e., a blank line), stop.
5. Track the earliest `start_byte` and latest `end_byte` across all matched siblings.
6. Return `Some((earliest_start_byte, latest_end_byte))` or `None`.
7. Advance: `sibling = sibling.prev_sibling()`.

**Integration:** `push_symbol` gains `source: &str` and `doc_spec: &DocCommentSpec` parameters (2 new params). `push_named_symbol` gains only `doc_spec: &DocCommentSpec` (1 new param — it already has `source`). `push_symbol` calls `scan_doc_range` and sets `doc_byte_range`.

**Direct callers of `push_symbol`:** `push_symbol` is called directly (bypassing `push_named_symbol`) in `elixir.rs`, `javascript.rs`, `typescript.rs`, `java.rs`, and `go.rs`. All these callers have `source` in scope and need updating to pass the new parameters.

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

### 4. Symbol Body Extraction Changes

**`get_symbol` / `render_symbol_detail`** (`src/protocol/format.rs`, `src/live_index/query.rs`):
- Body extraction currently uses `byte_range` to slice the source content.
- Change: when `doc_byte_range` is present, use `doc_byte_range.0` as the start of the extracted body so that doc comments are included in the returned source.
- This ensures that `replace_symbol_body` callers see (and can preserve) doc comments when they read the symbol before replacing it.
- `context_bundle_result` in `src/protocol/format.rs` also uses `byte_range` for body extraction and needs the same change.

### 5. Edit Tool Changes

**`replace_symbol_body`** (`src/protocol/tools.rs`):
- Current: `line_start` = start of line containing `sym.byte_range.0`
- New: if `sym.doc_byte_range.is_some()`, use start of line containing `doc_byte_range.0` as the splice start
- The caller provides the full replacement including doc comments (they see docs in `get_symbol` output per section 4)

**`build_delete`** (`src/protocol/edit.rs`):
- Current: extends to start of line from `sym.byte_range.0`
- New: if `sym.doc_byte_range.is_some()`, extend to start of line from `doc_byte_range.0`
- Doc comments are deleted along with the symbol

**`batch_edit`** (`src/protocol/edit.rs`):
- The `Replace` and `Delete` branches have inline splice logic that reads `sym.byte_range`. These also need updating to use `doc_byte_range` when present, matching the single-symbol edit tools.
- **Overlap validation** (Phase 1b, `execute_batch_edit` lines ~412-439): currently compares `sym.byte_range` pairs. After this change, two symbols with non-overlapping `byte_range` could have overlapping effective splice ranges when doc comments are included. The overlap check must use the effective range (`doc_byte_range.0..byte_range.1` when doc range is present) to prevent silent data corruption.
- **Phase 2 sort key** (~line 460): sorts by `sym.byte_range.0` descending for reverse-order application. Must use the effective start (`doc_byte_range.0` when present, else `byte_range.0`) as the sort key, otherwise splices starting from doc comments could be applied in wrong order.

**`edit_within_symbol`** (`src/protocol/edit.rs`):
- `build_edit_within` uses `byte_range` for the search scope. Since `get_symbol` now returns doc comments in the body, users may provide `old_text` that spans doc comments. Change: use `doc_byte_range.0` (when present) as the search scope start so edits within doc comments work correctly.

**`insert_before_symbol` / `insert_after_symbol`:**
- `insert_before_symbol` (`build_insert_before` in `edit.rs`): currently inserts before the line of `sym.byte_range.0`. Decision: when `doc_byte_range` is present, insert before `doc_byte_range.0` instead. Inserting between doc comments and the symbol keyword would be surprising — doc comments are logically part of the symbol.
- `insert_after_symbol` (`build_insert_after`): unaffected — it inserts after `byte_range.1`, which remains correct.

**No changes needed to:** outlines, search, format positioning. These continue using `byte_range` for line/byte positioning. Future: outlines could optionally show first doc line using the range.

### 6. Serialization / Persistence

`SymbolRecord` is persisted in the index via `postcard` serialization. Adding `doc_byte_range` changes the wire format.

- `src/live_index/persist.rs` — add `doc_byte_range` to serialization/deserialization
- Bump `CURRENT_VERSION` from 2 to 3
- **Migration path:** `load_snapshot` deserializes before checking the version. Old v2 snapshots will fail `postcard` deserialization, fall into the existing error path (log warning, return `None`), and trigger a full re-index. This is acceptable — no data loss, just a one-time re-index on upgrade.
- All `SymbolRecord` construction sites (54 occurrences across 11 files including test helpers) need `doc_byte_range: None` added

### 7. Files Modified

| File | Change |
|---|---|
| `src/domain/index.rs` | Add `doc_byte_range` field to `SymbolRecord` |
| `src/parsing/languages/mod.rs` | Add `DocCommentSpec`, `scan_doc_range`, update `push_symbol`/`push_named_symbol` signatures |
| `src/parsing/languages/*.rs` (16 files) | Add `DOC_SPEC` constant, update `push_symbol`/`push_named_symbol` call sites |
| `src/protocol/format.rs` | `render_symbol_detail` and `context_bundle_result` use `doc_byte_range` for body extraction |
| `src/protocol/tools.rs` | `replace_symbol_body` uses `doc_byte_range` for splice start |
| `src/protocol/edit.rs` | `build_delete`, `batch_edit`, `build_edit_within`, `build_insert_before` use `doc_byte_range` |
| `src/live_index/persist.rs` | Serialize/deserialize `doc_byte_range`; bump `CURRENT_VERSION` to 3 |
| 11 `src/` files with `SymbolRecord` construction sites | Add `doc_byte_range: None` (54 sites in `query.rs`, `persist.rs`, `search.rs`, `store.rs`, `format.rs`, `sidecar/handlers.rs`, `resources.rs`, etc.) |
| 2 `tests/` files with `SymbolRecord` construction sites | Add `doc_byte_range: None` (53 sites in `tests/sidecar_integration.rs`, 1 in `tests/hook_enrichment_integration.rs`) |

**Kotlin note:** The spec lists `["line_comment", "multiline_comment"]` based on `tree-sitter-kotlin-sg` research. These node type names should be verified at implementation time with a test that parses a Kotlin file with a KDoc comment and asserts `doc_byte_range` is populated.

### 8. Testing Strategy

- **Unit tests in `src/parsing/languages/mod.rs`**: parse small source snippets through tree-sitter and test `scan_doc_range` on the resulting nodes for Rust (`///`), Java (`/** */`), Go (all adjacent), and Elixir (`@doc`). (Tree-sitter nodes cannot be constructed programmatically — must parse real source.)
- **Integration tests**: parse real source snippets through `extract_symbols` and verify `doc_byte_range` is set correctly for each language.
- **Edit tests**: verify `replace_symbol_body` and `delete_symbol` include doc comments in the replaced/deleted range.
- **Regression tests**: ensure existing behavior is unchanged when `doc_byte_range` is `None`.
