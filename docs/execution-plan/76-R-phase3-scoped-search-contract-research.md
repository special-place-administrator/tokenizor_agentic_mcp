# Research: Phase 3 Scoped Search Contract

Related plan:

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [02-P-workstreams-and-tool-surface.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/02-P-workstreams-and-tool-surface.md)
- [23-R-phase1-text-lane-boundary-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/23-R-phase1-text-lane-boundary-research.md)
- [64-R-phase1-file-classification-heuristics-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/64-R-phase1-file-classification-heuristics-research.md)
- [68-T-phase1-explicit-current-tool-option-defaults-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/68-T-phase1-explicit-current-tool-option-defaults-shell.md)
- [76-T-phase3-scoped-search-contract-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/76-T-phase3-scoped-search-contract-research.md)

Goal:

- choose the smallest public `search_text` contract extension that meaningfully replaces common `rg` workflows on the current code lane

## Current Code Reality

Current public `SearchTextInput` only exposes:

- `query`
- `terms`
- `regex`

But the internal substrate already has useful seams:

- `TextSearchOptions.path_scope`
- `TextSearchOptions.search_scope`
- `TextSearchOptions.noise_policy`

And current classification metadata already supports:

- language-aware code files
- generated/test/vendor noise tags
- a reserved future `Text` class without actually populating a text registry yet

That means the first Phase 3 shell should expose the smallest subset that maps cleanly to this existing internal model instead of inventing a large new tool shape all at once.

## Decision: Scope And Noise First, Context Later

Recommendation:

- first extend `search_text` with scoping, deterministic caps, and noise suppression
- defer context-window rendering to the next slice

Why:

- scope and caps are the smallest direct shell-escape reduction for current indexed code workflows
- current output already groups by file and includes line numbers, so it remains usable while filters land
- context windows require a larger formatter and match-merging redesign, which is easier once the filtered result contract is stable

## Preferred First Public Contract

Keep existing fields:

- `query`
- `terms`
- `regex`

Add first-shell fields:

- `path_prefix: Option<String>`
- `language: Option<String>`
- `limit: Option<u32>`
- `max_per_file: Option<u32>`
- `include_generated: Option<bool>`
- `include_tests: Option<bool>`

Default posture:

- search the current semantic code lane only
- keep regex support as-is
- keep case-insensitive substring behavior as the default literal mode
- use deterministic bounded output when `limit` / `max_per_file` are omitted

Recommended starting defaults:

- `limit`: 50 total matches
- `max_per_file`: 5

These defaults are small enough to control noise while still preserving multi-file discovery.

## Rejected For The First Shell

### `glob` and `exclude_glob`

Why defer:

- useful, but more expressive than the first shell needs
- they complicate path-selection semantics before `path_prefix` and caps are validated in production
- they remain additive later once the simpler scope contract is stable

### `whole_word`

Why defer:

- it changes match semantics more deeply than simple scoping does
- it can land after the first scoped shell without invalidating the initial contract

### `before`, `after`, and `context`

Why defer:

- they change output shape and match grouping, not just candidate selection
- a smaller scope-first slice is easier to verify against existing formatter behavior

### Public Mixed-Lane Scope Knob

Why defer:

- there is still no authoritative text-lane registry
- exposing `scope=all` or `include_text=true` now would promise a lane that production does not actually maintain
- future text participation should be added only after the text registry exists

## Language And Noise Semantics

### `language`

Recommendation:

- code-lane filter only in the first shell
- interpret it against current indexed semantic files

This is honest today and still additive later.

### `include_generated` and `include_tests`

Recommendation:

- default them to `false` once the scoped shell lands
- let callers opt noisy classes back in explicitly

Why:

- the Phase 3 acceptance explicitly calls for excluding generated code and tests by default
- classification metadata already exists to support this cleanly

Vendor handling:

- keep vendor policy internal for now
- do not expose `include_vendor` publicly in the first shell unless later evidence shows it is needed

## Recommended Next Implementation Slice

- extend `SearchTextInput` with:
  - `path_prefix`
  - `language`
  - `limit`
  - `max_per_file`
  - `include_generated`
  - `include_tests`
- extend `TextSearchOptions` and matching/collection logic to honor those fields
- keep output formatting structurally the same for now
- add focused tests for:
  - path-prefix scoping
  - language filtering
  - generated/test suppression defaults
  - total and per-file caps

Expected touch points:

- `src/protocol/tools.rs`
- `src/live_index/search.rs`
- `src/protocol/format.rs`

## Carry Forward

- first Phase 3 shell should be scope/filter first, not context-window first
- stay code-lane only until a real text registry exists
- keep the initial contract additive to current `search_text`, not a new tool or a full redesign
