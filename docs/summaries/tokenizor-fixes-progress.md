# Tokenizor MCP Fixes — Progress State

## All 5 Fixes Complete (v0.4.2)

All 768 tests pass. `cargo check` clean.

### Fix 1: find_dependents/find_references unbounded output (DONE)
- Added `OutputLimits` struct to `src/protocol/format.rs` (lines 7-30)
- Added `limit` and `max_per_file` optional params to `FindDependentsInput` and `FindReferencesInput` in `src/protocol/tools.rs`
- Updated both formatters with truncation + "... and N more" indicators
- Updated handlers to construct `OutputLimits::new()` from input params (defaults: 20 files, 10 per file)

### Fix 2: analyze_file_impact phantom diffs (DONE)
- **File**: `src/sidecar/handlers.rs` — `handle_edit_impact`
- Replaced naive name+kind first-match with positional proximity matching
- Uses `matched_pre`/`matched_post` boolean vectors and closest `line_range.0` for duplicate name+kind pairs
- Correctly handles `fn drop` x2, `fn run_git` x2, etc.

### Fix 3: search_text relevance ranking (DONE)
- **File**: `src/live_index/search.rs` — `collect_text_matches`
- Two-pass approach: first pass counts matches per file, sorts by count descending (alphabetical tiebreak), second pass collects with limits
- High-relevance files now surface first regardless of alphabetical position

### Fix 4: Inline test module noise filtering (DONE)
- **File**: `src/live_index/search.rs` — `collect_text_matches` (same function as Fix 3)
- Added `SymbolKind` import
- When `include_tests: false` and file is Rust, precomputes test module line ranges from symbols
- Skips matches falling within `mod tests` (kind Module) in both counting and collection passes

### Fix 5: get_file_content error messages (DONE)
- **File**: `src/protocol/tools.rs` — `file_content_options_from_input`
- Error messages now show valid parameter combinations (e.g. "Valid with `around_symbol`: `symbol_line`, `context_lines`.")
- Clarified "ordinary read" → "full-file reads or explicit-range reads (`start_line`/`end_line`)"
- Updated 4 test assertions to match new messages

## Key Patterns Learned
- Input structs derive `Deserialize, Serialize, JsonSchema` — all new fields must be `Option<T>`
- Formatter functions in format.rs are pure (no I/O, no async)
- Handler methods in tools.rs's `impl TokenizorServer` call formatters
- Tests use `Parameters(super::StructName { ... })` pattern
- `cargo check` is fast (~0.2s incremental)
- Tests assert exact error message strings — update tests when changing error text
