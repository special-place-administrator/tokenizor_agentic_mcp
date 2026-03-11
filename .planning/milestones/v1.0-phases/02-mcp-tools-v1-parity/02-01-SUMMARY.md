---
phase: 02-mcp-tools-v1-parity
plan: "01"
subsystem: live-index-extensions, protocol-formatter
tags: [live-index, formatter, tdd, symbol-kind, schemars]
dependency_graph:
  requires: [01-01, 01-02, 01-03]
  provides: [LiveIndex::empty, LiveIndex::reload, format.rs-all-formatters, SymbolKind-Display]
  affects: [02-02-tool-handlers]
tech_stack:
  added: [schemars-0.8]
  patterns: [pure-formatter-functions, tdd-red-green, in-place-reload]
key_files:
  created: [src/protocol/format.rs, src/protocol/mod.rs]
  modified: [src/live_index/store.rs, src/live_index/query.rs, src/domain/index.rs, src/lib.rs, src/main.rs, Cargo.toml]
decisions:
  - "IndexState::Empty is a first-class variant checked before Ready/CB — empty() is structurally distinct from loading"
  - "reload() validates path exists before discover_files to produce clear errors (ignore crate silently returns empty on invalid paths)"
  - "format.rs functions accept &LiveIndex directly — no intermediate DTOs, maximal composability for tool handlers"
  - "repo_outline accepts project_name parameter — caller provides context, formatter stays pure"
metrics:
  duration_minutes: 7
  completed_date: "2026-03-10"
  tasks_completed: 2
  tests_added: 45
  total_tests: 102
---

# Phase 2 Plan 1: LiveIndex Extensions and Response Formatter Summary

**One-liner:** In-process LiveIndex empty/reload lifecycle with SystemTime tracking plus pure formatter module covering all 10 tool response formats.

## What Was Built

### Task 1 — LiveIndex Extensions

Extended `src/live_index/store.rs` and `src/live_index/query.rs` with:

- **`IndexState::Empty`** — new variant checked before `Ready` and `CircuitBreakerTripped`. `empty()` constructor sets `is_empty: true`; `reload()` clears it.
- **`LiveIndex::empty() -> SharedIndex`** — constructs a zero-file index wrapped in `Arc<RwLock>`. Used when `TOKENIZOR_AUTO_INDEX=false` (Phase 2 server startup).
- **`LiveIndex::reload(&mut self, root: &Path) -> Result<()>`** — in-place reload: validates path, discovers files, parses with Rayon, swaps `self.files`, resets circuit breaker, updates timestamps. Returns `Discovery` error on invalid root.
- **`loaded_at_system: SystemTime`** field — wall-clock time stored on construction and updated on every reload. Exposed via `loaded_at_system()` accessor.
- **`is_empty: bool`** field — gates `is_ready()` and `index_state()` before circuit breaker checks.

Extended `src/domain/index.rs` with:

- **`impl fmt::Display for SymbolKind`** — produces lowercase kind prefixes: `fn`, `class`, `struct`, `enum`, `interface`, `mod`, `const`, `let`, `type`, `trait`, `impl`, `other`. Method maps to `fn` (same as Function in output).

Added `schemars = "0.8"` to `Cargo.toml` (required by rmcp derive macros in Plan 02).

Updated `src/main.rs` with `IndexState::Empty` arm (unreachable after `load()`).

### Task 2 — Response Formatter Module

Created `src/protocol/format.rs` (pure functions, no I/O, no async) and `src/protocol/mod.rs`:

| Function | Description |
|---|---|
| `file_outline` | Indented tree with `{kind:<12} {name:<30} {start}-{end}`, header shows `path  (N symbols)` |
| `symbol_detail` | Source body via `byte_range` slice + `[kind, lines X-Y, N bytes]` footer |
| `search_symbols_result` | Case-insensitive substring match, grouped by file, `N matches in M files` header |
| `search_text_result` | Line-by-line content scan, 1-indexed line numbers, CRLF-safe |
| `repo_outline` | Directory tree with filename/language/symbol count per file, totals header |
| `health_report` | Status/Files/Symbols/Loaded in/Watcher block |
| `what_changed_result` | Compares `since_ts` against `loaded_at_system`, lists all files if index is newer |
| `file_content` | Raw content with optional 1-indexed line range slicing |
| `not_found_file` | "File not found: {path}" |
| `not_found_symbol` | "No symbol {name} in {path}. Symbols in that file: ..." |
| `loading_guard_message` | "Index is loading... try again shortly." |
| `empty_guard_message` | "Index not loaded. Call index_folder to index a directory." |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] discover_files silently returns empty on non-existent path**
- **Found during:** Task 1 — test `test_live_index_reload_invalid_root_returns_error` failed
- **Issue:** `ignore::WalkBuilder::new(path).build()` returns an empty iterator (not an error) when the path doesn't exist. The test expected an error but `reload()` would succeed returning an empty index.
- **Fix:** Added explicit `if !root.exists()` check at the top of `reload()`, returning `TokenizorError::Discovery` with a descriptive message.
- **Files modified:** `src/live_index/store.rs`
- **Commit:** 035277b

## Commits

| Hash | Message |
|---|---|
| 035277b | feat(02-01): LiveIndex empty/reload/SystemTime, IndexState::Empty, SymbolKind Display |
| 368e2c0 | feat(02-01): add src/protocol/format.rs with all formatter functions |

## Test Coverage

- **Before:** 57 tests
- **Added:** 45 tests (13 SymbolKind Display + 6 empty/reload/SystemTime + 2 guard messages + 2 not-found helpers + 32 formatter functions)
- **After:** 102 tests, 0 failures
- `cargo check` passes with zero warnings

## Self-Check: PASSED

- [x] `src/protocol/format.rs` exists
- [x] `src/protocol/mod.rs` exists
- [x] `LiveIndex::empty()` function present in store.rs
- [x] `LiveIndex::reload()` function present in store.rs
- [x] `IndexState::Empty` variant present in store.rs
- [x] `impl fmt::Display for SymbolKind` present in domain/index.rs
- [x] `schemars = "0.8"` in Cargo.toml
- [x] Commits 035277b and 368e2c0 exist
- [x] 102 tests pass, 0 failures
