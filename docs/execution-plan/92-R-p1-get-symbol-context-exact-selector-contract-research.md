# Research: P1 Get Symbol Context Exact Selector Contract

Related plan:

- [05-P-validation-and-backlog.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)
- [88-R-p1-find-references-exact-selector-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/88-R-p1-find-references-exact-selector-contract-research.md)
- [90-R-p1-get-context-bundle-exact-selector-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/90-R-p1-get-context-bundle-exact-selector-contract-research.md)
- [91-T-p1-get-context-bundle-exact-selector-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/91-T-p1-get-context-bundle-exact-selector-shell.md)

Goal:

- choose the smallest exact-selector contract that lets current `search_symbols` output drive a materially less noisy `get_symbol_context` flow without first introducing stable symbol ids

## Current Code Reality

Current public `get_symbol_context` exposes:

- `name`
- optional `file`

Current sidecar implementation still does:

- `find_references_for_name(name, None, false)`
- optional post-filtering by `file`
- grouped rendering capped at 10 matches

That means `file` is currently an output filter, not an exact symbol selector. It cannot chain directly from `search_symbols` output and it cannot disambiguate common names or duplicate local definitions.

## Candidate Approaches

### Option 1: stable symbol id now

- strongest end state
- still too large for the next P1 slice because it touches symbol storage and all follow-up consumers together

### Option 2: overload existing `file`

- no new field names
- would conflate two meanings:
  - selector path
  - output file filter
- risks breaking current users who rely on `file` as a display filter

### Option 3: add exact-selector fields alongside current filter

- keep `name`
- keep `file` as the optional output file filter
- add optional `path`
- add optional `symbol_kind`
- add optional `symbol_line`

This matches the `find_references` exact-selector shell and chains cleanly from `search_symbols` output without breaking current `file` filter behavior.

## Decision: Add `path`, `symbol_kind`, And `symbol_line`

Recommendation:

- preserve current name-only behavior when `path` is absent
- if `path` is present, switch to exact-selector mode using:
  - `path`
  - `name`
  - optional `symbol_kind`
  - optional `symbol_line`
- keep `file` as an optional result-file filter after the reference set is chosen

## Query Semantics Recommendation

Recommendation:

- make the first shell exact about symbol selection
- reuse the exact-selector dependency-scoped reference collection already added for `find_references` and `get_context_bundle`
- keep the current grouped compact output and 10-match cap

Smallest useful first-shell algorithm:

1. if `path` is absent, preserve current name-only grouped behavior
2. if `path` is present, resolve the symbol exactly using the shared selector rules
3. collect exact references for the selected symbol
4. optionally apply the current `file` filter to the grouped output
5. render the existing compact grouped format with the current cap and overflow summary

## Output Contract

Recommendation:

- keep successful grouped output unchanged
- return stable user-facing error strings for:
  - file not found
  - symbol not found in file
  - ambiguous selector without `symbol_line`

Why:

- `get_symbol_context` is sidecar-backed and string-oriented today
- reusing the stable exact-selector error wording keeps the tool family coherent without forcing a larger view-model migration first

## Recommended Next Implementation Slice

- extend `GetSymbolContextInput` with `path`, `symbol_kind`, and `symbol_line`
- extend `SymbolContextParams` and the sidecar handler accordingly
- reuse the shared exact-selector query helper from `src/live_index/query.rs`
- preserve current `file` filtering as a post-selection output filter
- add focused tests for:
  - exact-selector mode excluding unrelated same-name files
  - ambiguity without `symbol_line`
  - backward-compatible name-only mode
  - `file` filter still narrowing exact-selector output

Expected touch points:

- `src/protocol/tools.rs`
- `src/sidecar/handlers.rs`
- likely `src/live_index/query.rs`

## Carry Forward

- keep this slice separate from stable symbol-id work
- preserve the current compact output cap and token-budget behavior
- prefer shared exact-selector helpers over sidecar-local matching logic
