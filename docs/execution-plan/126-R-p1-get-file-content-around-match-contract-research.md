# Research: P1 Get File Content Around Match Contract

Related plan:

- [05-P-validation-and-backlog.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)
- [01-P-overview-and-principles.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/01-P-overview-and-principles.md)
- [02-P-workstreams-and-tool-surface.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/02-P-workstreams-and-tool-surface.md)

Goal:

- decide the first explicit `get_file_content` contract for centered reads around a text match without widening into full search-mode parity

## Current Code Reality

`get_file_content` now supports exact-path full-file reads, explicit line ranges, and exact-path `around_line` with symmetric `context_lines` and numbered excerpt rendering. It does not yet support locating text inside a file and returning the surrounding excerpt.

## Candidate Approaches

### Option 1: `around_match` string only, first literal match wins

- smallest input surface
- deterministic if the contract explicitly selects the first matching line in file order
- easy to combine with the existing `context_lines` excerpt renderer

### Option 2: `around_match` plus `match_index`

- handles repeated matches more explicitly
- adds more API and validation before the first match-anchored slice has proven useful

### Option 3: full `search_text` parity inside `get_file_content`

- powerful
- too broad for this step because it drags in regex, whole-word, case-sensitivity, and possibly multi-match selection

## Decision: Start With Exact-Path Literal `around_match`

Recommendation:

- add optional `around_match` to `get_file_content`
- reuse optional `context_lines`
- treat `around_match` as a case-insensitive literal substring search for the first slice
- use the first matching line in file order as the anchor
- reject mixing `around_match` with `start_line`, `end_line`, or `around_line`

## Output Contract

- when `around_match` is present, render the numbered excerpt around the first matching line
- reuse the current `around_line` excerpt shape rather than inventing a second formatter
- if the file does not contain the requested text, return a deterministic not-found message for that path and match text
- keep existing raw full-file, explicit line-range, and `around_line` behavior unchanged

## Why This Is The Smallest Useful Slice

- it closes the next obvious read-navigation gap after `around_line`
- it avoids forcing agents to manually scan for a string after opening a file
- it preserves a compact tool surface while leaving room for later `match_index`, regex, and chunking work

## Recommended Next Implementation Slice

- extend `GetFileContentInput` with `around_match`
- add validation for `around_match` exclusivity against range and `around_line` inputs
- locate the first case-insensitive literal match inside the selected file
- reuse the numbered excerpt renderer around the matched line
- add focused tool, formatter, and integration coverage

Expected touch points:

- `src/protocol/tools.rs`
- `src/protocol/format.rs`
- `src/live_index/search.rs`
- `tests/live_index_integration.rs`

## Carry Forward

- keep the first `around_match` contract exact-path only
- defer `match_index`, regex, highlighting, and chunking to later slices
- preserve current full-file, explicit line-range, and `around_line` compatibility
