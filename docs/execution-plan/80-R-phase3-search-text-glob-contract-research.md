# Research: Phase 3 Search Text Glob Contract

Related plan:

- [04-P-phase-plan.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [05-P-validation-and-backlog.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)
- [76-R-phase3-scoped-search-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/76-R-phase3-scoped-search-contract-research.md)
- [79-T-phase3-search-text-context-window-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/79-T-phase3-search-text-context-window-shell.md)
- [80-T-phase3-search-text-glob-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/80-T-phase3-search-text-glob-contract-research.md)

Goal:

- choose the smallest `glob` / `exclude_glob` contract that meaningfully replaces common scoped grep workflows without reopening the full future search grammar

## Current Code Reality

After tasks 77 and 79, public `search_text` already supports:

- `path_prefix`
- `language`
- `limit`
- `max_per_file`
- `include_generated`
- `include_tests`
- `context`

Current path filtering is still limited to `PathScope::Any`, `Exact`, and `Prefix`.

That means:

- `src/**/*.ts` style narrowing is still impossible
- exclusions such as `**/*.generated.ts` or `**/dist/**` still require path-prefix workarounds or shell fallback
- the next highest-value improvement is path-pattern filtering, not more match-semantics knobs

## Decision: Add One Include Glob And One Exclude Glob

Recommendation:

- add `glob: Option<String>`
- add `exclude_glob: Option<String>`

Why:

- this is the smallest additive contract that covers a large share of real grep workflows
- singular include/exclude fields keep the first shell small and easy to test
- widening later to multiple patterns remains backward-compatible

Rejected for this slice:

- arrays of globs
- separate include/exclude path-prefix families
- shell-style brace expansion or richer mini-language

## Path Semantics

Recommendation:

- evaluate globs against normalized repo-relative paths using `/`
- apply both `path_prefix` and `glob` if both are present
- apply `exclude_glob` last as a hard exclusion

This yields a deterministic rule:

1. path scope must match
2. include glob, if present, must match
3. exclude glob, if present, must not match
4. language/scope/noise checks still apply

## Matching Engine

Recommendation:

- use `globset`

Why:

- it gives predictable gitignore-like path matching on normalized paths
- it is more robust than hand-rolled wildcard matching
- it keeps the contract small without locking the implementation into ad-hoc parsing

## Explicit Deferrals

Defer for a later slice:

- `case_sensitive`
- `whole_word`
- multi-glob arrays
- mixed text-lane search

Why:

- none of those are required to land the first path-pattern shell
- adding them together would make it harder to attribute regressions

## Recommended Next Implementation Slice

- extend `SearchTextInput` with `glob` and `exclude_glob`
- extend `TextSearchOptions` with raw include/exclude glob fields
- compile and validate glob patterns in the query/tool path
- keep output formatting unchanged
- add focused tests for:
  - include glob narrowing
  - exclude glob suppression
  - combined `path_prefix` + `glob`
  - invalid glob error reporting

Expected touch points:

- `Cargo.toml`
- `src/protocol/tools.rs`
- `src/live_index/search.rs`
- `src/protocol/format.rs`

## Carry Forward

- keep the first glob slice additive to the current `search_text` shell
- preserve code-lane behavior; do not couple this to text-lane introduction
- keep singular `glob` and `exclude_glob` fields for now
