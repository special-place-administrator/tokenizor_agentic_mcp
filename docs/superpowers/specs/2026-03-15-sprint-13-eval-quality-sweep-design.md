# Sprint 13 — Eval Quality Sweep

**Date:** 2026-03-15
**Scope:** 14 items (4 bugs, 10 UX) from 5-project evaluation
**Out of scope:** Architecture limitations (L1-L4), trust bugs (deferred to Sprint 14)

---

## Deferred to Sprint 14 — Trust & Reliability

These are higher severity than any Sprint 13 item but require dedicated design work:

1. **batch_rename misses path-qualified calls** — textual rename doesn't catch `Module::symbol` patterns reliably. Test exists (`test_batch_rename_catches_path_qualified_calls`) but coverage is incomplete.
2. **search_text can diverge from disk after partial rename** — FTS index may not reflect disk state after a rename that touches some files but errors on others. Plan exists (`2026-03-14-tool-quality-fixes.md`) with retry logic design.

Sprint 13 is intentionally a **quality sweep for eval feedback**, not a reliability sprint.

---

## Wave 0 — Contract Definitions

Before any code, define semantics for the three areas with API ambiguity risk.

### Contract 0A: `get_file_content` mode semantics

**Problem:** 6 mutually exclusive selection modes with incompatible flag combinations. Users guess wrong, get confusing errors.

**Mode enum:** `lines | symbol | match | chunk | search`

| Mode | Sets defaults for | Legacy flag equivalent |
|------|------------------|----------------------|
| `lines` | `start_line`, `end_line`, `around_line` | Direct line range params |
| `symbol` | `around_symbol`, `symbol_line`, `context_lines` | `around_symbol` param |
| `match` | `around_match`, `match_index` | `around_match` param |
| `chunk` | `chunk_index` | `chunk_index` param |
| `search` | (future — reserved) | N/A |

**Precedence rule:**
1. `mode` sets defaults for its associated flags
2. Explicit legacy flags from the **same mode** override mode defaults (e.g., `mode=symbol, context_lines=10` is valid)
3. Explicit legacy flags from a **different mode** always error (e.g., `mode=symbol` + `around_line=50`). Error says:
   - what combination was received
   - what mode would express the request cleanly
4. When no `mode` is set, infer from which legacy flags are present (current behavior)
5. When `mode` is set but no associated flags are provided, error with guidance on which flag is required for that mode (e.g., `"mode=symbol requires around_symbol"`)
6. `mode=search` is reserved — rejected with `"mode 'search' is not yet implemented"` error

**`around_symbol` behavior (B2 contract):**
- Default: return the full indexed symbol span (start_line..end_line from SymbolEntry)
- Doc comments / attributes / decorators: included if they are part of the symbol's indexed byte range
- `context_lines`: adds N lines before/after the symbol span (default 0)
- `max_lines`: if set, truncates output with `"... truncated (symbol is N lines, showing first M)"` hint
- No `max_lines` = no truncation (full symbol body)
- Very large symbols (>500 lines): no automatic truncation, but add a note in output: `"Symbol spans N lines. Use max_lines to limit."`
- Symbol not found: return error `"Symbol 'name' not found in file. Use search_symbols to find the correct name."` — do NOT fall back to text search or return empty content

**`show_line_numbers` (B3 contract):**
- Orthogonal to all selection modes. Always allowed.
- No validation rejection. Works with `around_symbol`, `around_match`, `around_line`, etc.

### Contract 0B: `search_symbols` browse mode

**Problem:** `query` is currently required. Users can't browse "all structs in src/protocol/" without a name substring.

**Change:** Make `query` optional.

**Guardrails:**
- When `query` is omitted, at least one of `kind` or `path_prefix` is required. Reject if all three are omitted.
- Error message: `"search_symbols requires at least one of: query, kind, or path_prefix"`
- Default limit for browse mode: 20 (vs 50 for query mode)
- Sort order: by file path, then by line number (stable, predictable)
- No cursor/offset for now — revisit if interactive use patterns emerge

### Contract 0C: Noise policy scope

**Problem:** `.gitignore`-aware filtering could accidentally make vendor/generated files invisible everywhere.

**Design decision:** Noise policy is **suppressive** (down-rank/hide in explore), NOT **exclusive** (invisible to all tools).

| Tool | Noise behavior |
|------|---------------|
| `explore` | Gitignored files filtered by default. Override: `include_noise: true` |
| `search_text` | No filtering. All indexed files searched. |
| `search_symbols` | No filtering. All indexed symbols returned. |
| `get_file_context` | No filtering. Explicit path = explicit intent. |
| `get_repo_map` | Noise files shown but tagged `[vendor]` or `[generated]` |
| `inspect_match` | Siblings from noise files down-ranked to end of list |

**Gitignore matching:**
- Respect `.gitignore` at repo root
- Respect nested `.gitignore` files
- Respect negation rules (`!important-vendor-file`)
- Reuse existing gitignore infrastructure from `src/discovery/` if present, otherwise use `ignore` crate
- Files already tracked by git but in `.gitignore` are still classified as noise (matches user intent)

---

## Wave 1 — Noise Foundation (U7)

### U7: `.gitignore`-aware noise policy

**Files:** `src/live_index/search.rs` (NoisePolicy), `src/discovery/`

**Implementation:**
1. Add gitignore pattern matching to `NoisePolicy` — load `.gitignore` patterns at index time
2. Classify files matching gitignore as `noise_class: Vendor | Generated | Ignored`
3. Existing `NoisePolicy::hide_classified_noise()` gains gitignore awareness
4. Do NOT remove gitignored files from the index — only tag them
5. **Freshness rule:** `.gitignore` patterns are loaded at full reindex only. Mid-session changes to `.gitignore` do not trigger recomputation — users must re-index (or restart daemon) to pick up new ignore rules. This keeps the hot path simple.

**Tests:**
- Gitignored vendor dir filtered by default in explore
- Same file accessible via explicit `get_file_context` / `search_symbols`
- Negation rule (`!vendor/important.js`) exempts file from noise classification
- Nested `.gitignore` respected
- `get_repo_map` shows noise files tagged `[vendor]` or `[generated]` (per Contract 0C)

---

## Wave 2 — Independent Fixes (9 items, parallelizable)

### B1: `batch_insert` extra blank line

**File:** `src/protocol/edit.rs` (`execute_batch_insert`)

**Fix:** Audit newline handling in insert-before logic. The blank line appears between inserted code and the target symbol.

**Test matrix:**
- Insert before first symbol in file
- Insert before symbol with doc comments
- Insert before symbol with attributes/macros
- Insert into file with existing blank line above symbol
- Insert into file with zero blank lines above symbol
- Verify LF consistency (repo uses LF)

### B4: `batch_edit` rollback message

**File:** `src/protocol/edit.rs` (`execute_batch_edit`)

**Fix:** On atomic failure, error output must include:
- Explicit `"ROLLED BACK"` status
- Number of edits attempted vs succeeded before failure
- File paths that were targeted
- Confirmation: `"No files were modified."`

**Tests:**
- Failed atomic edit prints rollback message with edit count
- File paths listed in rollback output
- Index unchanged after rollback

### U2: `search_symbols` browse mode

**File:** `src/protocol/tools.rs` (`SearchSymbolsInput`, `search_symbols_options_from_input`)

**Changes:**
- Make `query` field `Option<String>` (currently required `String`)
- When query omitted: require `kind` or `path_prefix`, default limit 20, sort by path+line
- Reject fully unbounded request with helpful error

**Tests:**
- Browse `kind=struct, path_prefix=src/protocol/` returns structs without name query
- Omit all three params → error with guidance
- Browse mode default limit = 20
- Query mode still defaults to 50 (not regressed by browse-mode changes)

### U3: `inspect_match` sibling cap

**File:** `src/live_index/query.rs` (`capture_inspect_match_view`, siblings collection)

**Fix:** Cap siblings at 10 by default. Add `"... and N more siblings"` overflow hint. Add optional `sibling_limit` param to `InspectMatchInput`.

**Tests:**
- File with 70+ siblings shows 10 + overflow hint
- `sibling_limit=5` shows 5
- `sibling_limit=0` means no siblings

### U4: `analyze_file_impact` when unchanged

**File:** `src/protocol/tools.rs` (`analyze_file_impact` handler)

**Fix:** When file hasn't changed, replace `"already matches"` with:
- Status (mutually exclusive):
  - `"indexed and unchanged"` — file on disk matches indexed content
  - `"changed on disk since last index"` — file differs, triggers re-index
  - `"not found on disk"` — file was indexed but has been deleted
- Last-indexed timestamp
- Symbol count in file
- Suggestion: `"Use what_changed to see recent modifications"`

**Tests:**
- Unchanged file shows indexed timestamp + symbol count
- Changed-on-disk file triggers re-index and shows diff
- File deleted from disk since last index shows `"file not found on disk — removed from index"` status

### U5: `batch_edit` dry-run

**File:** `src/protocol/edit.rs` (`BatchEditInput`, `execute_batch_edit`)

**Implementation:**
- Add `dry_run: Option<bool>` to `BatchEditInput` (serde default false)
- Dry-run executes the **exact same validation and planning path** as real operation
- Only difference: skip `fs::write` calls and skip index mutation
- Output shows per-edit preview: file, symbol, old snippet → new snippet

**Architectural constraint (enforced via code review, not tests):** Dry-run must NOT use a separate code path. Same function, gated by a `write_to_disk: bool` flag deep in the execution. This prevents drift between dry-run and real execution over time.

**Tests:**
- Dry-run validates all edits, shows preview, writes nothing
- Dry-run on invalid edit produces same error as real edit
- Index unchanged after dry-run
- Dry-run output matches real edit output format (same fields, same structure)
- Dry-run preview capped at 20 lines per edit. Larger diffs show first 20 lines + `"... truncated (N lines total)"`. Same cap applies to rollback output when many files are involved.

### U6: Richer `verbosity=signature`

**File:** `src/protocol/format.rs` (signature formatting)

**Fix:** Signature mode currently strips to name+params. Add:
- Visibility (`pub`, `pub(crate)`, etc.)
- Return type
- Generic parameters
- Keep it one-line. No language-specific prose.

**Tests:**
- `pub fn foo<T: Display>(x: T) -> Result<String>` shows full signature, not just `foo(x)`
- Stable across Rust, TypeScript, Python, Go

### U8: Health partial parse file list

**Files:** `src/live_index/query.rs` (`HealthStats`), `src/daemon.rs` (`ProjectHealth`)

**Fix:** Add `partial_parse_files` to `HealthStats`. Output format:
- Count always shown
- First 10 file paths listed
- If >10: `"... and N more partial files"` (no `--verbose` flag — full list available via `search_symbols(kind=..., path_prefix=...)` or external tooling)
- Listed paths are repo-relative, unique, and sorted alphabetically before taking the first 10 (deterministic across runs)

**Tests:**
- Health with 3 partial files lists all 3
- Health with 50 partial files lists first 10 + overflow hint

### U9: Tool-use counters

**File:** `src/sidecar/mod.rs` (`TokenStats`), `src/protocol/mod.rs`

**Fix:** Add per-tool call counter to `TokenStats`. Increment on each tool invocation. Scope: since daemon process start (resets on restart). Exposed in health output.

**Tests:**
- Counter increments on tool call (unit test on `TokenStats`)
- Health output shows per-tool counts
- Counter resets on daemon restart (manual verification — no daemon restart harness in test suite)

---

## Wave 3 — `get_file_content` selection fixes (B2, B3)

Depends on Wave 0 contract being finalized. Both items touch `file_content_options_from_input` — **implement B2 then B3 sequentially** (not parallel) to avoid merge conflicts in the same function.

### B2: `around_symbol` returns full symbol span

**Files:** `src/protocol/tools.rs`, `src/live_index/search.rs`

**Fix:** Change `around_symbol` to use the symbol's indexed byte range (start_line..end_line) instead of a fixed context window. Per Contract 0A.

**Tests:**
- `around_symbol` returns full function body (not 3-7 lines)
- Large symbol (>100 lines) returned in full without truncation
- `max_lines=20` truncates with hint
- Doc comments within symbol range included
- `context_lines=5` adds 5 lines before/after symbol span
- Symbol not found returns error (per Contract 0A)

### B3: `show_line_numbers` unrestricted

**File:** `src/protocol/tools.rs` (`file_content_options_from_input`)

**Fix:** Remove validation that rejects `show_line_numbers` with `around_symbol` / `around_match`.

**Tests:**
- `show_line_numbers=true` + `around_symbol` works
- `show_line_numbers=true` + `around_match` works
- Line numbers are correct (match actual file line numbers)

---

## Wave 4 — Explore noise filtering (U1)

Depends on Wave 1 (U7 noise policy foundation).

### U1: `explore` filters noise by default

**File:** `src/protocol/explore.rs`, `src/protocol/tools.rs`

**Fix:**
1. Apply `NoisePolicy` in explore's Phase 1 (symbol collection) and Phase 2 (text search)
2. Gitignored files / vendor / markdown / generated HTML down-ranked or hidden
3. Add `include_noise: Option<bool>` param to `ExploreInput` (default false)
4. When noise is filtered, add hint: `"N results from vendor/generated files hidden. Use include_noise=true to include."`

**Tests:**
- Explore on repo with vendor dir: vendor files filtered by default
- `include_noise=true` brings them back
- Markdown noise (README, CHANGELOG) not in top results for code concepts
- Hint shows count of hidden results

---

## Wave 5 — `get_file_content` mode enum (U10)

Last because it touches the same code as B2/B3 and is the most API-sensitive change.

### U10: Mode enum for `get_file_content`

**File:** `src/protocol/tools.rs` (`GetFileContentInput`, `file_content_options_from_input`)

**Implementation per Contract 0A:**
1. Add `mode: Option<String>` to `GetFileContentInput`
2. Mode sets defaults, explicit flags override
3. Invalid combos error with: what was received + what mode to use
4. No `mode` = infer from flags (backward compatible)

**Tests:**
- `mode=symbol, around_symbol=foo` works
- `mode=symbol` without `around_symbol` errors helpfully
- `mode=lines, around_symbol=foo` errors: `"mode=lines conflicts with around_symbol. Use mode=symbol."`
- No mode + legacy flags = current behavior unchanged
- `mode=symbol, start_line=5` errors: received + suggestion
- `mode=search` errors: `"mode 'search' is not yet implemented"`

---

## Acceptance Criteria

Sprint 13 is complete when:
1. All 14 items pass their listed tests
2. `cargo test --all-targets -- --test-threads=1` green
3. `cargo fmt -- --check` clean
4. No regressions in praised tools (get_file_context, search_text, bundle mode, diff_symbols, edit tools, error messages)
5. Trust bugs documented as Sprint 14 P0 in PLAN.md
