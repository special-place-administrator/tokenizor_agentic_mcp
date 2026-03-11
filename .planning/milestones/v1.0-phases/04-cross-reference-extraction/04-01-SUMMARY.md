---
phase: 04-cross-reference-extraction
plan: 01
subsystem: parsing
tags: [tree-sitter, xref, cross-references, reverse-index, rust, python, javascript, typescript, go, java]

# Dependency graph
requires:
  - phase: 03-file-watcher-freshness
    provides: LiveIndex mutation methods (update_file, add_file, remove_file, reload) that xref storage extends
  - phase: 01-liveindex-foundation
    provides: IndexedFile, FileProcessingResult, SymbolRecord types used as base for extension

provides:
  - ReferenceRecord and ReferenceKind types in domain/index.rs
  - find_enclosing_symbol helper in domain/index.rs
  - extract_references function in parsing/xref.rs for all 6 languages
  - OnceLock-cached tree-sitter queries per language
  - Extended FileProcessingResult with references + alias_map fields
  - Extended IndexedFile with references + alias_map fields
  - ReferenceLocation struct and LiveIndex.reverse_index
  - rebuild_reverse_index called on every LiveIndex mutation

affects: [04-02, 04-03, 04-cross-reference-extraction]

# Tech tracking
tech-stack:
  added: [streaming-iterator 0.1 (direct dep — tree-sitter 0.24 QueryCursor uses StreamingIterator not Iterator)]
  patterns:
    - OnceLock-cached compiled Query per language (zero per-file recompilation)
    - streaming_iterator::StreamingIterator advance()/get() pattern for QueryCursor
    - Parse-before-lock: xref extraction happens during parse step alongside symbol extraction
    - Destructure-before-consume: FileProcessingResult destructured before into_iter() to satisfy borrow checker

key-files:
  created:
    - src/parsing/xref.rs
  modified:
    - src/domain/index.rs
    - src/domain/mod.rs
    - src/live_index/store.rs
    - src/live_index/mod.rs
    - src/parsing/mod.rs
    - src/live_index/query.rs
    - src/protocol/format.rs
    - src/protocol/tools.rs
    - Cargo.toml

key-decisions:
  - "streaming-iterator added as direct dep: tree-sitter 0.24's QueryCursor::matches returns QueryMatches<StreamingIterator>, not an Iterator — for..in loop does not work, must use advance()/get() pattern"
  - "use_as_clause has path/alias fields (not path/name) — discovered from tree-sitter-rust grammar.js"
  - "FileProcessingResult destructured before consuming references via into_iter(): Rust borrow checker disallows borrowing result.symbols inside closure that also moves result.references"
  - "Definition-site filter applied in from_parse_result: references whose byte_range exactly matches a SymbolRecord's byte_range are skipped (Pitfall 1 from RESEARCH.md)"

patterns-established:
  - "OnceLock pattern: each language has a static OnceLock<Query> and a fn that calls get_or_init"
  - "StreamingIterator pattern: while let Some(m) = { matches.advance(); matches.get() } for tree-sitter 0.24"
  - "Alias map built inline during xref extraction pass — import.original + import.alias captures populate HashMap<String,String>"

requirements-completed: [XREF-01, XREF-02, XREF-03, XREF-07]

# Metrics
duration: 13min
completed: 2026-03-10
---

# Phase 4 Plan 01: Cross-Reference Data Foundation Summary

**ReferenceRecord/ReferenceKind types, OnceLock-cached tree-sitter query extraction for all 6 languages, and LiveIndex reverse_index rebuilt on every mutation**

## Performance

- **Duration:** 13 min
- **Started:** 2026-03-10T19:13:33Z
- **Completed:** 2026-03-10T19:26:43Z
- **Tasks:** 2
- **Files modified:** 9 (+ 1 created)

## Accomplishments
- Added `ReferenceRecord`, `ReferenceKind`, `ReferenceLocation`, and `find_enclosing_symbol` to the domain layer with full test coverage
- Implemented `extract_references` in `src/parsing/xref.rs` with per-language tree-sitter queries for Rust, Python, JS, TypeScript, Go, and Java — all 6 languages produce correct Call/Import/TypeUsage/MacroUse references
- Wired xref extraction into the `parse_source` → `process_file` pipeline so every parsed file now carries populated `references` and `alias_map`
- Extended `IndexedFile` and `LiveIndex` with references/alias_map storage and a `reverse_index` (name → Vec<ReferenceLocation>) rebuilt synchronously on every mutation
- Alias maps captured for Rust `use X as Y` and Python `import X as Y` patterns
- Qualified names captured for scoped calls (`Vec::new`, `fmt.Println`, etc.)
- 21 new xref tests + 10 new domain/store tests, all 185 lib tests pass

## Task Commits

Each task was committed atomically:

1. **Task 1: Domain types + LiveIndex storage extensions** - `49f3cbe` (test+feat — TDD combined commit)
2. **Task 2: Tree-sitter xref extraction for all 6 languages** - `12f1904` (feat)

## Files Created/Modified
- `src/parsing/xref.rs` - Created: extract_references with OnceLock-cached queries for all 6 languages
- `src/domain/index.rs` - Added ReferenceRecord, ReferenceKind, find_enclosing_symbol; extended FileProcessingResult
- `src/domain/mod.rs` - Re-exported new domain types
- `src/live_index/store.rs` - Extended IndexedFile + LiveIndex; added ReferenceLocation; rebuild_reverse_index
- `src/live_index/mod.rs` - Re-exported ReferenceLocation
- `src/parsing/mod.rs` - Extended parse_source tuple; wired xref extraction; added pub mod xref
- `src/live_index/query.rs` - Updated IndexedFile/LiveIndex test helpers for new fields
- `src/protocol/format.rs` - Updated IndexedFile/LiveIndex test helpers for new fields
- `src/protocol/tools.rs` - Updated IndexedFile/LiveIndex test helpers for new fields
- `Cargo.toml` - Added streaming-iterator = "0.1" direct dependency

## Decisions Made
- **streaming-iterator as direct dep:** tree-sitter 0.24's `QueryCursor::matches` returns `QueryMatches<StreamingIterator>`, not a standard `Iterator`. The `for..in` loop does not compile. Added `streaming-iterator` as a direct dependency and used the `advance()`/`get()` pattern.
- **use_as_clause fields:** Grammar introspection showed the correct fields are `path` and `alias` (not `path` and `name`). Fixed before first test run.
- **Destructure before consume:** The borrow checker disallows borrowing `result.symbols` inside the closure that also consumes `result.references` via `.into_iter()`. Solution: destructure `FileProcessingResult` into its parts first.
- **No definition-site filtering needed in tests:** The filter in `from_parse_result` filters references whose byte_range exactly matches a SymbolRecord's byte_range. This doesn't affect xref-only tests since they don't go through `from_parse_result`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed use_as_clause field name in Rust query**
- **Found during:** Task 2 (xref extraction)
- **Issue:** Query used `name:` field for the alias in `use_as_clause` but the grammar uses `alias:` field
- **Fix:** Changed `name: (identifier) @import.alias` to `alias: (identifier) @import.alias` in RUST_XREF_QUERY
- **Files modified:** src/parsing/xref.rs
- **Verification:** `test_rust_use_as_clause_populates_alias_map` passes

**2. [Rule 3 - Blocking] Added streaming-iterator direct dependency**
- **Found during:** Task 2 (xref extraction)
- **Issue:** tree-sitter 0.24 QueryCursor returns a StreamingIterator, causing compile error with `for..in`
- **Fix:** Added `streaming-iterator = "0.1"` to Cargo.toml; rewrote loop with `advance()`/`get()` pattern
- **Files modified:** Cargo.toml, src/parsing/xref.rs
- **Verification:** All xref tests and existing tests pass

---

**Total deviations:** 2 auto-fixed (1 bug fix, 1 blocking dependency)
**Impact on plan:** Both fixes required for correctness. No scope creep.

## Issues Encountered
- tree-sitter 0.24 uses `StreamingIterator` (not standard `Iterator`) for `QueryCursor::matches` — the research doc's code sample used `for m in cursor.matches(...)` which was for an older API. Fixed by using `advance()`/`get()` pattern and adding `streaming-iterator` as a direct dep.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- All domain types, extraction pipeline, and storage structures are in place
- Phase 04-02 can now implement `find_references`, `find_dependents`, and `get_context_bundle` MCP tools directly on top of this foundation
- `LiveIndex::reverse_index` is populated and ready for query use
- Per-file `alias_map` is stored and ready for import-alias resolution in queries

---
*Phase: 04-cross-reference-extraction*
*Completed: 2026-03-10*
