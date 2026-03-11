# Phase 4: Cross-Reference Extraction - Research

**Researched:** 2026-03-10
**Domain:** Tree-sitter query-based cross-reference extraction, reverse index construction, three new MCP tools
**Confidence:** HIGH

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**False Positive Filtering**
- Index ALL references at extraction time; apply filters at query time only
- Hardcoded built-in type filter lists per language (string, number, bool, i32, u8, etc.) — no configuration env vars
- Hardcoded single-letter generic filter lists (T, K, V, E, etc.) — no configuration
- Common names like 'new', 'get', 'set' are indexed and queryable — not filtered by default
- Qualified match when available: find_references('process') matches all; find_references('Vec::new') uses qualified_name to narrow. Leverages tree-sitter's scoped_identifier/field_expression text

**Import Alias Resolution (XREF-05)**
- Per-file alias map: build HashMap<String, String> (alias→original) from import statements during parse
- Stored persistently in the file entry alongside symbols and references
- find_references('HashMap') also matches references via alias 'Map' if 'use HashMap as Map' exists in that file

**get_context_bundle Scope (TOOL-11)**
- Direct callers and callees only — depth 1, no transitive expansion
- Full symbol body included (not just signature) — this is "get_symbol + references in one call"
- Each caller/callee entry shows: symbol name, file path, line number, enclosing function name — one line per reference, no source snippets
- Cap at 20 per section (callers, callees, type usages). If more exist, show count ("...and 15 more callers")
- Must stay under 100ms per success criteria

**find_references Output (TOOL-09)**
- Group results by file (file path headers, references listed with line numbers within each file)
- 3 lines of source context per reference (1 before, the reference line, 1 after)
- Enclosing symbol name shown inline with each result ("in fn handle_request")
- Optional kind filter parameter: kind=call|import|type_usage|all (default: all)

**find_dependents Output (TOOL-10)**
- Returns files that import/use symbols from a given file
- Same compact format as find_references, grouped by file

**Reference Storage Model**
- Per-file storage: each file entry gets Vec<ReferenceRecord> alongside Vec<SymbolRecord>
- Per-file alias map: HashMap<String, String> stored in file entry
- Incremental update: when a file is re-parsed, replace symbols, references, and alias map atomically (matches existing update_file pattern)
- Reverse index (name→locations) built lazily for fast find_references queries
- Reverse index rebuilt synchronously after each file update — always consistent, no stale results
- XREF-08 satisfied by the atomic per-file replacement + synchronous reverse index rebuild

**Extraction Approach**
- Use tree-sitter Query/QueryCursor API (not extending existing walk_node)
- Extraction runs as a parallel pipeline on the same parsed tree as symbol extraction

### Claude's Discretion
- Tree-sitter query patterns per language (research doc has draft templates as starting point)
- ReferenceRecord struct details (research doc has suggested shape)
- Reverse index data structure choice (HashMap, BTreeMap, etc.)
- Exact built-in type and generic filter lists per language
- find_dependents implementation details (likely derived from import references)
- How to determine "callees" for get_context_bundle (references made BY the target symbol)

### Deferred Ideas (OUT OF SCOPE)

None — discussion stayed within phase scope
</user_constraints>

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| XREF-01 | Call site extraction for all 6 languages (Rust, Python, JS, TS, Go, Java) | Tree-sitter Query/QueryCursor API; node types per language fully documented in prior research |
| XREF-02 | Import/dependency tracking — which files import what | Import node types documented per language; alias_map enables per-file import tracking |
| XREF-03 | Type usage extraction — struct/class/enum references across files | type_identifier nodes available in all 6 languages via tree-sitter queries |
| XREF-04 | Built-in type filters (string, number, bool) prevent false positives | Applied at query time, not extraction time; hardcoded per-language lists |
| XREF-05 | Alias map support (use X as Y — references via alias are tracked) | HashMap<String,String> per file entry; query logic checks alias_map at find_references time |
| XREF-06 | Single-letter generic filters (T, K, V) prevent noise | Same mechanism as XREF-04, applied at query time |
| XREF-07 | Enclosing symbol tracked for each reference (which function contains the call) | enclosing_symbol_index field in ReferenceRecord; determined by comparing reference line range to sorted SymbolRecord ranges |
| XREF-08 | References updated incrementally when a file is re-parsed | Atomic per-file replacement: update_file replaces symbols + references + alias_map together; reverse index rebuilt synchronously |
| TOOL-09 | find_references — all call sites for a symbol with context snippets | New tool handler + format function; uses reverse index for O(1) lookup; 3-line context from file.content bytes |
| TOOL-10 | find_dependents — files that import a given file | Derived from import references; a file F depends on G if F has Import references whose import path resolves to G's relative path |
| TOOL-11 | get_context_bundle — one-call full context (symbol + callers + callees + types + imports) | Combines symbol_detail + find_references filtered by kind; depth-1 only; capped at 20 per section |
</phase_requirements>

---

## Summary

Phase 4 adds cross-reference extraction to the existing tree-sitter parse pipeline. The core work is three-layered: (1) extract `ReferenceRecord` values from each file's tree-sitter AST during the existing parse step, (2) maintain a repo-level reverse index (`name → Vec<location>`) that is rebuilt synchronously on every file update, and (3) expose three new MCP tools that query that reverse index.

The implementation has a clean integration surface. `IndexedFile` gains two new fields (`references: Vec<ReferenceRecord>`, `alias_map: HashMap<String, String>`), `LiveIndex` gains a repo-level `reverse_index: HashMap<String, Vec<ReferenceLocation>>`, and `parsing::process_file` returns references alongside symbols. The watcher's `maybe_reindex` passes extracted references through to `update_file`. Tool handlers and formatters follow the exact patterns established in Phase 2.

The prior research document (`docs/summaries/research-xref-extraction-and-file-watching.md`) contains complete node-type tables and draft query strings for all 6 languages. The tree-sitter `Query`/`QueryCursor` API is already present in the `tree-sitter = "0.24"` dependency — no new crates are needed.

**Primary recommendation:** Add a new `src/parsing/xref.rs` module that contains per-language query strings and the `extract_references` function. Keep it entirely separate from the existing `walk_node` symbol extraction. Wire it into `parse_source` in `parsing/mod.rs` so both symbols and references are produced from a single parse tree walk.

---

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| tree-sitter | 0.24 (already in Cargo.toml) | Query/QueryCursor API for reference extraction | Already present; Query + QueryCursor give declarative pattern matching over the AST |
| std::collections::HashMap | stdlib | Reverse index and alias map | O(1) lookup; the chosen data structure for all index maps in this codebase |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tree-sitter-rust / python / javascript / typescript / go / java | 0.23 (already in Cargo.toml) | Grammar crates providing node type definitions | Query strings reference these node types; no version change needed |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| HashMap for reverse index | BTreeMap | BTreeMap gives sorted iteration but O(log n) lookup; HashMap is better for hot query path |
| Query strings embedded in code | .scm files on disk | Files allow runtime editing but add asset loading complexity; embedded strings are simpler for Phase 4 |
| Rebuilding reverse index on every update_file | Incremental reverse index maintenance | Incremental maintenance is complex and error-prone; full rebuild per update is simpler and fast enough given single-file update scope |

**No new dependencies required.** The `tree-sitter` crate at version 0.24 already includes `Query` and `QueryCursor`.

---

## Architecture Patterns

### Recommended Project Structure additions
```
src/
├── parsing/
│   ├── mod.rs          # process_file now returns references too
│   ├── xref.rs         # NEW: extract_references(), per-language query strings
│   └── languages/      # unchanged — symbol extraction only
├── live_index/
│   ├── store.rs        # IndexedFile + 2 new fields; LiveIndex + reverse_index field; rebuild_reverse_index()
│   ├── query.rs        # find_references_for_name(), find_dependents_for_file() queries
│   └── mod.rs          # re-exports
├── protocol/
│   ├── tools.rs        # 3 new tool handlers + input structs
│   └── format.rs       # 3 new formatter functions
└── domain/
    └── index.rs        # ReferenceRecord + ReferenceKind structs
```

### Pattern 1: ReferenceRecord and ReferenceKind in domain/index.rs

Add alongside the existing `SymbolRecord` and `SymbolKind`. The `enclosing_symbol_index` field references into the file's `symbols` Vec by index (not by name) to avoid string duplication:

```rust
// Source: docs/summaries/research-xref-extraction-and-file-watching.md (adapted)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceRecord {
    /// Simple name at the call/import/type site (e.g. "HashMap", "process", "Vec")
    pub name: String,
    /// Best-effort qualified name when syntactically available (e.g. "Vec::new", "self.process")
    pub qualified_name: Option<String>,
    pub kind: ReferenceKind,
    pub byte_range: (u32, u32),
    pub line_range: (u32, u32),
    /// Index into the file's symbols Vec for the enclosing definition, if any.
    pub enclosing_symbol_index: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceKind {
    Call,
    Import,
    TypeUsage,
    MacroUse,
}
```

### Pattern 2: Per-language query strings in src/parsing/xref.rs

Embed query strings as `const &str` per language. Use the `tree-sitter` `Query` API to compile once per language. The research doc contains complete draft query strings for all 6 languages:

```rust
// Source: docs/summaries/research-xref-extraction-and-file-watching.md
// Rust query draft
const RUST_XREF_QUERY: &str = r#"
;; Function calls — simple
(call_expression function: (identifier) @ref.call)
;; Qualified calls (Vec::new)
(call_expression function: (scoped_identifier name: (identifier) @ref.call))
;; Method calls (self.foo)
(call_expression function: (field_expression field: (field_identifier) @ref.method_call))
;; Macro invocations
(macro_invocation macro: (identifier) @ref.macro)
(macro_invocation macro: (scoped_identifier name: (identifier) @ref.macro))
;; Imports
(use_declaration argument: (identifier) @ref.import)
(use_declaration argument: (scoped_identifier) @ref.import)
;; Type references
(type_identifier) @ref.type
(generic_type type: (type_identifier) @ref.type)
"#;
```

### Pattern 3: IndexedFile gains two new fields

```rust
// Extend store.rs — IndexedFile
pub struct IndexedFile {
    // ... existing fields unchanged ...
    /// Cross-references extracted from this file's AST (XREF-01..03, XREF-07)
    pub references: Vec<ReferenceRecord>,
    /// Import alias map: alias → original name (XREF-05)
    /// e.g. "Map" → "HashMap" from `use std::collections::HashMap as Map`
    pub alias_map: HashMap<String, String>,
}
```

### Pattern 4: LiveIndex gains a reverse index field

```rust
// Extend store.rs — LiveIndex
pub struct LiveIndex {
    // ... existing fields unchanged ...
    /// Repo-level reverse index: name → list of locations (XREF-01..03)
    /// Built from all files' references; rebuilt synchronously on every update_file.
    pub(crate) reverse_index: HashMap<String, Vec<ReferenceLocation>>,
}

#[derive(Clone, Debug)]
pub struct ReferenceLocation {
    pub file_path: String,
    pub reference_idx: u32, // index into IndexedFile.references
}
```

### Pattern 5: rebuild_reverse_index() called after every update_file

The decision (locked): reverse index is always consistent, never stale. Rebuild it synchronously on every file update. Since individual file updates happen at most once per debounce window (200ms), the rebuild cost is small:

```rust
impl LiveIndex {
    fn rebuild_reverse_index(&mut self) {
        let mut new_index: HashMap<String, Vec<ReferenceLocation>> = HashMap::new();
        for (path, file) in &self.files {
            for (idx, ref_rec) in file.references.iter().enumerate() {
                new_index
                    .entry(ref_rec.name.clone())
                    .or_default()
                    .push(ReferenceLocation {
                        file_path: path.clone(),
                        reference_idx: idx as u32,
                    });
            }
        }
        self.reverse_index = new_index;
    }
}
```

Call `rebuild_reverse_index()` at the end of `update_file`, `add_file`, `remove_file`, and `reload`.

### Pattern 6: process_file returns references alongside symbols

Extend `FileProcessingResult` in `domain/index.rs`:

```rust
pub struct FileProcessingResult {
    // ... existing fields unchanged ...
    pub references: Vec<ReferenceRecord>,
    pub alias_map: HashMap<String, String>,
}
```

And extend `parsing/mod.rs` `parse_source` to call `xref::extract_references(&root, source, &language)` in addition to `languages::extract_symbols`.

### Pattern 7: Watcher's maybe_reindex passes references through

`IndexedFile::from_parse_result` needs to accept and store the new fields:

```rust
IndexedFile {
    // ... existing fields ...
    references: result.references,
    alias_map: result.alias_map,
}
```

### Pattern 8: Three new tool handlers follow loading_guard! macro pattern

```rust
// In tools.rs — input structs
#[derive(Deserialize, JsonSchema)]
pub struct FindReferencesInput {
    pub name: String,
    pub kind: Option<String>, // "call" | "import" | "type_usage" | "all"
}

#[derive(Deserialize, JsonSchema)]
pub struct FindDependentsInput {
    pub path: String, // relative path of the file to find dependents for
}

#[derive(Deserialize, JsonSchema)]
pub struct GetContextBundleInput {
    pub path: String,
    pub name: String,
    pub kind: Option<String>,
}
```

### Enclosing Symbol Determination (XREF-07)

Finding which symbol encloses a reference requires checking the reference's `line_range.0` (start line) against each `SymbolRecord`'s `line_range`. The simplest approach: at extraction time, after extracting all symbols for a file (which happens first in `parse_source`), pass the symbol list to `extract_references` so it can assign `enclosing_symbol_index` during extraction.

Alternative: assign during `IndexedFile::from_parse_result` by iterating over references and matching by line range. This is cleaner separation.

```rust
fn find_enclosing_symbol(symbols: &[SymbolRecord], ref_line: u32) -> Option<u32> {
    // Find innermost symbol that contains this line
    symbols.iter().enumerate()
        .filter(|(_, s)| s.line_range.0 <= ref_line && ref_line <= s.line_range.1)
        .max_by_key(|(_, s)| s.line_range.0) // deepest match (latest start line)
        .map(|(i, _)| i as u32)
}
```

### Anti-Patterns to Avoid
- **Extending walk_node with reference extraction:** The existing walk_node handles symbol definitions only. Mixing reference extraction into it creates a maintenance burden across 6 language files. Use the separate `xref.rs` module with Query-based extraction.
- **Holding write lock during tree-sitter parse:** The existing parse-before-lock pattern in `maybe_reindex` must be preserved. Reference extraction happens in the same parse step — before acquiring the write lock.
- **Rebuilding reverse index inside the write lock for full reload:** `reload()` already rebuilds the entire `files` HashMap. Call `rebuild_reverse_index()` at the end of reload, inside the same code path that swaps `self.files`.
- **Including type_identifier nodes from definition sites:** When walking for type references, avoid capturing the type name from the definition itself (e.g., `struct MyStruct { ... }` defines `MyStruct` — that is already a SymbolRecord, not a reference). Filter by checking that the parent node is not a definition kind.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| AST pattern matching for references | Custom node type string matching in walk_node | tree-sitter Query + QueryCursor | Query API is declarative, handles nested alternations, field access constraints, and wildcards. Hand-rolling 6 × N node type checks is fragile |
| Cross-file qualified name resolution | Full path resolution algorithm | Simple name matching + optional qualified text from tree-sitter | Tree-sitter gives qualified text syntactically (e.g. `Vec::new` from scoped_identifier) — no resolver needed |
| Alias expansion | Per-file symbol table lookups | HashMap<String,String> alias_map from import statements | Import statements are syntactically simple to extract; the map handles the O(1) lookup |

**Key insight:** This phase is strictly syntactic. Tree-sitter hands us the text of identifiers at call sites. We record that text and build a name-keyed index. No type inference, no cross-file resolution algorithm, no language server.

---

## Common Pitfalls

### Pitfall 1: type_identifier captures definition sites, not just usage sites
**What goes wrong:** The query `(type_identifier) @ref.type` captures every occurrence of a type identifier in the file, including the definition (e.g., `struct Foo` captures `Foo` as a type reference). This inflates reference counts and confuses `find_references`.
**Why it happens:** Tree-sitter queries match purely syntactically — the type identifier at the definition site looks identical to one at a usage site.
**How to avoid:** For Rust, the struct name appears inside `struct_item > type_identifier`. Add a negative ancestor constraint or, simpler, post-filter: skip any reference whose `byte_range` exactly matches a `SymbolRecord`'s name range. This is a post-extraction filter in `from_parse_result`.
**Warning signs:** `find_references("MyStruct")` includes one result from the same file at the definition line.

### Pitfall 2: Reverse index gets stale after remove_file
**What goes wrong:** `remove_file` removes the file from `self.files` but the reverse index still contains entries pointing to references in that file.
**Why it happens:** If `rebuild_reverse_index()` is not called after `remove_file`, stale `ReferenceLocation` entries remain.
**How to avoid:** Call `rebuild_reverse_index()` at the end of `remove_file` (only when a file was actually removed — the existing pattern already conditions on `self.files.remove(path).is_some()`).

### Pitfall 3: Reverse index rebuild on full reload scans all files twice
**What goes wrong:** `reload()` replaces `self.files` and then `rebuild_reverse_index()` iterates all files again. For a 1000-file repo, this is an extra O(N × R) iteration (N files, R refs per file). This is fine — it is one-time work on full reload which is not latency-sensitive.
**Why it happens:** Expected behavior; not a bug. Document it so implementors don't over-optimize.
**How to avoid:** No avoidance needed. Acceptable cost on reload.

### Pitfall 4: enclosing_symbol_index pointing into stale symbol list after re-parse
**What goes wrong:** `enclosing_symbol_index` is an integer index into `IndexedFile.symbols`. If the file is re-parsed and the symbol order changes, stored `ReferenceRecord` values may point to the wrong symbol.
**Why it happens:** Index into a Vec is stable only for the lifetime of that Vec. After `update_file`, both symbols and references are replaced atomically.
**How to avoid:** The locked decision (atomic replacement) prevents this: `update_file` replaces both `symbols` and `references` together. `enclosing_symbol_index` values are always valid for the symbols list in the same `IndexedFile`. Do not cache cross-file symbol indices.

### Pitfall 5: Query compilation is expensive — do it once
**What goes wrong:** Creating a `tree_sitter::Query` object is not free (it compiles the S-expression). If done per-file or per-parse, it adds measurable latency.
**Why it happens:** Treat Query as a template compiled once.
**How to avoid:** Compile queries once per language, ideally as `OnceLock<Query>` statics in `xref.rs`. Reuse the compiled Query across all files of the same language.

```rust
static RUST_QUERY: OnceLock<Query> = OnceLock::new();

fn rust_query(lang: &Language) -> &'static Query {
    RUST_QUERY.get_or_init(|| {
        Query::new(lang, RUST_XREF_QUERY).expect("valid rust xref query")
    })
}
```

### Pitfall 6: find_dependents requires mapping import text to file paths
**What goes wrong:** A file F has `import "path/to/bar"` (JS/TS) or `use crate::bar::Baz` (Rust). The import reference text does not directly match the `relative_path` key in `LiveIndex.files`.
**Why it happens:** Import paths are language-specific and may be relative, absolute, crate-qualified, or module-qualified.
**How to avoid:** `find_dependents("src/bar.rs")` should search the reverse index for all Import references and filter those where the import path text is a suffix of or contained within the given file path. This is heuristic-based, not precise resolution. Document the limitation in the tool description.

### Pitfall 7: Qualified name for scoped_identifier requires capturing the full node text
**What goes wrong:** For `Vec::new`, capturing just `name: (identifier)` gives `"new"` not `"Vec::new"`. The `qualified_name` field needs the full `scoped_identifier` text.
**Why it happens:** The capture `@ref.call` on the inner identifier captures only that node's text.
**How to avoid:** In the Rust query, add a parallel capture on the parent:
```scheme
(call_expression
  function: (scoped_identifier) @ref.qualified_call)
```
Then in the extraction code, when you see a `@ref.qualified_call` match, set `qualified_name = Some(node.utf8_text(...))` and `name = last segment of that text`.

### Pitfall 8: tree-sitter grammar version split (flagged in STATE.md)
**What goes wrong:** Python/JS/Go grammars are already at ^0.25.8 (actually currently pinned at 0.23 in Cargo.toml — the STATE.md concern was speculative). Current Cargo.toml shows all language grammars at 0.23. The Query API does not differ between 0.23 and 0.24 grammar crates for our use cases.
**Why it happens:** Speculative concern from STATE.md.
**How to avoid:** No action needed for Phase 4. All 6 grammar crates are currently at 0.23 in Cargo.toml. The `tree-sitter` core is 0.24. Queries work across these versions. Do not bump grammar crate versions as part of Phase 4 work.

---

## Code Examples

Verified patterns from the existing codebase and tree-sitter 0.24 API:

### QueryCursor usage pattern (tree-sitter 0.24)
```rust
// Source: docs/summaries/research-xref-extraction-and-file-watching.md
use tree_sitter::{Query, QueryCursor};

let query = Query::new(&language, query_source).expect("valid query");
let mut cursor = QueryCursor::new();

for m in cursor.matches(&query, root_node, source.as_bytes()) {
    for capture in m.captures {
        let capture_name = query.capture_names()[capture.index as usize];
        let text = capture.node.utf8_text(source.as_bytes()).unwrap_or("");
        let line_start = capture.node.start_position().row as u32;
        let line_end   = capture.node.end_position().row as u32;
        let byte_start = capture.node.start_byte() as u32;
        let byte_end   = capture.node.end_byte() as u32;
        // map capture_name ("ref.call", "ref.import", etc.) → ReferenceKind
    }
}
```

### Alias map extraction for Rust (use_as_clause)
The Rust grammar has `use_as_clause` for `use foo::Bar as Baz`:
```scheme
;; Captures both the original and the alias
(use_declaration
  argument: (use_as_clause
    path: (_) @import.original
    alias: (identifier) @import.alias))
```
In extraction code: when both `import.original` and `import.alias` are captured together, add `alias_map.insert(alias_text, original_text)`.

### update_file extension to include references
```rust
// In store.rs — update_file signature extension
pub fn update_file(&mut self, path: String, file: IndexedFile) {
    self.files.insert(path, file);
    self.loaded_at_system = SystemTime::now();
    self.rebuild_reverse_index(); // synchronous, always consistent
}
```

### find_references formatter output format
```
find_references("process")
3 references in 2 files

src/handler.rs
  42: fn handle_request() {     [in fn handle_request]
  43:     process(payload);
  44: }

src/worker.rs
  17: pub fn run() {            [in fn run]
  18:     process(item);
  19: }
```

### get_context_bundle formatter output format
```
get_context_bundle("src/lib.rs", "process")

fn process(data: &[u8]) -> Result<()> {
    // ... full body ...
}
[fn, lines 10-25, 340 bytes]

Callers (2):
  handle_request  src/handler.rs:43  in fn handle_request
  run             src/worker.rs:18   in fn run

Callees (1):
  validate        src/validate.rs:7   in fn process

Type usages (0):
```

### find_dependents output format
```
find_dependents("src/db.rs")
2 files depend on src/db.rs

src/handler.rs
  3: use crate::db::Connection;   [import]

src/worker.rs
  1: use crate::db;               [import]
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Manual walk_node reference extraction | tree-sitter Query/QueryCursor | Phase 4 design | Declarative queries are maintainable across 6 languages; official tags.scm patterns are battle-tested |
| No reverse index (scan all files for each query) | HashMap reverse index per name | Phase 4 | find_references is O(1) per name lookup instead of O(N×R) full scan |
| No cross-reference data in IndexedFile | Per-file Vec<ReferenceRecord> + HashMap<String,String> alias_map | Phase 4 | Enables incremental updates without full re-extraction |

**Deprecated/outdated:**
- `walk_node` for reference extraction: NEVER extend the language-specific walk_node functions with reference extraction. The existing walk_node is definition-only by design.

---

## Open Questions

1. **Qualified name capture for Go selector_expression**
   - What we know: `selector_expression` gives `pkg.Symbol` — the field is the symbol name, the operand is the package/receiver.
   - What's unclear: Does the query need to capture the full selector text to populate `qualified_name`, or is the field name (`field_identifier`) sufficient for the name field?
   - Recommendation: Capture the full `selector_expression` as `@ref.qualified_call` and extract field text as `name`, full text as `qualified_name`. Test against a Go file that uses `fmt.Println`.

2. **TypeScript TSX JSX tag names as references**
   - What we know: JSX `<MyComponent />` creates an `identifier` reference to `MyComponent`. The tree-sitter-typescript grammar (tsx variant) uses `jsx_opening_element` and `jsx_self_closing_element`.
   - What's unclear: Whether `.tsx` files are parsed with `LANGUAGE_TSX` or `LANGUAGE_TYPESCRIPT` in the current codebase. Checking `parsing/languages/typescript.rs` would confirm.
   - Recommendation: Check `typescript.rs` to determine if tsx is handled. If not, out of scope for Phase 4 — TypeScript queries cover `.ts` files only.

3. **Python type hint extraction depth**
   - What we know: Python type annotations can be arbitrarily nested (`Optional[Dict[str, List[MyType]]]`). Capturing only the outermost type misses inner types.
   - What's unclear: Whether recursive query patterns are needed or whether all type_identifier nodes inside annotations are captured naturally by the `(type_identifier) @ref.type` pattern.
   - Recommendation: Use `(type (identifier) @ref.type)` and also `(identifier) @ref.type` under annotation contexts. Test against a Python file with complex type annotations.

---

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust's built-in `#[test]` + `#[tokio::test]` + cargo test |
| Config file | none (no external config; `Cargo.toml` dev-dependencies) |
| Quick run command | `cargo test --lib 2>&1 \| tail -20` |
| Full suite command | `cargo test 2>&1 \| tail -30` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| XREF-01 | Call site extraction — all 6 languages produce ReferenceRecord(kind=Call) | unit | `cargo test xref:: -q` | ❌ Wave 0 |
| XREF-02 | Import references extracted; alias_map populated correctly | unit | `cargo test xref:: -q` | ❌ Wave 0 |
| XREF-03 | Type usage references extracted for all 6 languages | unit | `cargo test xref:: -q` | ❌ Wave 0 |
| XREF-04 | Built-in type names (string, bool, int, etc.) filtered at query time | unit | `cargo test find_references -q` | ❌ Wave 0 |
| XREF-05 | Alias map: find_references('HashMap') matches alias 'Map' in same file | unit | `cargo test alias_map -q` | ❌ Wave 0 |
| XREF-06 | Single-letter generics (T, K, V) not returned by default queries | unit | `cargo test generic_filter -q` | ❌ Wave 0 |
| XREF-07 | enclosing_symbol_index correctly identifies enclosing function | unit | `cargo test enclosing_symbol -q` | ❌ Wave 0 |
| XREF-08 | After file re-parse, reverse index reflects updated references; old refs gone | unit + integration | `cargo test xref_incremental -q` | ❌ Wave 0 |
| TOOL-09 | find_references tool returns grouped results with 3-line context | unit (format.rs) | `cargo test --lib format:: -q` | ❌ Wave 0 |
| TOOL-09 | find_references kind= filter returns only matching kinds | unit | `cargo test find_references_kind_filter -q` | ❌ Wave 0 |
| TOOL-10 | find_dependents returns files that import the given file | unit | `cargo test find_dependents -q` | ❌ Wave 0 |
| TOOL-11 | get_context_bundle returns symbol body + callers + callees + type usages | unit (format.rs) | `cargo test get_context_bundle -q` | ❌ Wave 0 |
| TOOL-11 | get_context_bundle caps at 20 per section with overflow count | unit | `cargo test context_bundle_cap -q` | ❌ Wave 0 |
| TOOL-11 | get_context_bundle responds under 100ms on a 100-file test index | perf | `cargo test context_bundle_perf -q` | ❌ Wave 0 |
| XREF-04 | find_references("string") on a TS repo returns <10 results | integration | `cargo test --test xref_integration ts_builtin_filter -q` | ❌ Wave 0 |
| XREF-08 | After watcher re-parse, find_references returns updated results | integration | `cargo test --test xref_integration incremental_update -q` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test --lib 2>&1 | tail -20`
- **Per wave merge:** `cargo test 2>&1 | tail -30`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `src/parsing/xref.rs` — cross-reference extraction module (covers XREF-01..03, XREF-07)
- [ ] `tests/xref_integration.rs` — integration tests (covers XREF-04, XREF-05, XREF-06, XREF-08 end-to-end)
- [ ] Unit tests for `format::find_references_result`, `format::find_dependents_result`, `format::context_bundle_result` in `src/protocol/format.rs` (covers TOOL-09, TOOL-10, TOOL-11)
- [ ] Unit tests for `LiveIndex::rebuild_reverse_index` and `find_references_for_name` in `src/live_index/query.rs` (covers reverse index correctness)

---

## Sources

### Primary (HIGH confidence)
- `docs/summaries/research-xref-extraction-and-file-watching.md` — complete node type tables, query string drafts, tree-sitter API examples for all 6 languages and the `notify` crate
- `Cargo.toml` — confirms tree-sitter 0.24, grammar crates at 0.23, no new dependencies needed
- `src/live_index/store.rs` — `IndexedFile`, `LiveIndex`, `update_file`, `add_file`, `remove_file` — exact integration surface
- `src/parsing/mod.rs` — `process_file` and `parse_source` — exact extension points
- `src/protocol/tools.rs` — `loading_guard!` macro, `#[tool_router]` pattern, input struct conventions
- `src/protocol/format.rs` — formatter function conventions (all accept `&LiveIndex`)
- `src/watcher/mod.rs` — `maybe_reindex` lock discipline — parse-before-lock pattern preserved
- `src/domain/index.rs` — `SymbolRecord`, `SymbolKind` — models to mirror for `ReferenceRecord`

### Secondary (MEDIUM confidence)
- `tests/watcher_integration.rs` — test structure pattern for integration tests (tokio multi_thread, debounce waits, TempDir helpers)
- `src/parsing/languages/rust.rs` — walk_node pattern to understand why xref should NOT extend it

### Tertiary (LOW confidence)
- None in this phase — all findings are grounded in the actual source code and prior research document.

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — Cargo.toml confirms all needed crates are present at correct versions; no new dependencies
- Architecture: HIGH — integration points read directly from source code; all extension patterns are established
- Pitfalls: HIGH — most pitfalls derived from reading existing code and well-understood tree-sitter behavior
- Query string correctness: MEDIUM — query drafts are from prior research but must be validated against actual grammar node types during implementation

**Research date:** 2026-03-10
**Valid until:** 2026-04-10 (tree-sitter 0.24 API is stable; grammar crate versions pinned in Cargo.toml)
