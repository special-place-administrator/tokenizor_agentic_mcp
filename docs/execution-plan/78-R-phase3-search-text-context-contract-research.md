# Research: Phase 3 Search Text Context Contract

Related plan:

- [04-P-phase-plan.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [02-P-workstreams-and-tool-surface.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/02-P-workstreams-and-tool-surface.md)
- [05-P-validation-and-backlog.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)
- [76-R-phase3-scoped-search-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/76-R-phase3-scoped-search-contract-research.md)
- [77-T-phase3-search-text-scope-filter-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/77-T-phase3-search-text-scope-filter-shell.md)
- [78-T-phase3-search-text-context-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/78-T-phase3-search-text-context-contract-research.md)

Goal:

- choose the smallest public `search_text` context contract that materially replaces common `rg -n -C` workflows without forcing a broader read-surface redesign

## Current Code Reality

After task 77, public `search_text` now exposes:

- `query`
- `terms`
- `regex`
- `path_prefix`
- `language`
- `limit`
- `max_per_file`
- `include_generated`
- `include_tests`

The internal substrate currently carries:

- `TextSearchOptions` for candidate selection and caps
- `TextSearchResult` as an owned query result
- `TextFileMatches` grouped by file
- `TextLineMatch` entries that currently hold only the matched line number and line text

Current formatter behavior in `search_text_result_view()` is intentionally simple:

- one header line
- one path header per file
- one rendered line per match in sorted line-number order

That means the current search layer already owns result capture, but it does not yet own context-window materialization.

## Decision: Start With Symmetric `context`

Recommendation:

- add `context: Option<u32>` first
- do not add separate `before` and `after` fields in the first context slice

Why:

- it directly covers the most common `rg -C N` workflow
- it is the smallest additive contract after task 77
- it avoids three overlapping public knobs (`before`, `after`, `context`) before real usage evidence exists
- asymmetric `before` and `after` can still be added later without breaking the first shell

## Output Contract Recommendation

Keep the existing file-grouped output shape.

When `context` is omitted:

- preserve the current exact output contract

When `context` is present:

- keep the same top-level header and file grouping
- render merged line windows per file
- mark matched lines distinctly from surrounding context lines
- separate disjoint windows inside a file with an explicit ellipsis line

Recommended line rendering:

- context line: `  {line_number}: {line}`
- matched line: `> {line_number}: {line}`
- disjoint window separator: `  ...`

Why this shape:

- it stays visually close to the existing formatter
- it is deterministic and cheap to parse by both humans and models
- it avoids a larger block-based redesign before Phase 4 read-surface work

## Match And Cap Semantics

Recommendation:

- `limit` and `max_per_file` should continue to count matches, not rendered context lines
- context expansion should happen only after the match set is finalized
- overlapping windows should merge deterministically within a file
- file ordering and line ordering should stay sorted as they are today

Why:

- task 77 already established deterministic match caps
- counting rendered lines instead would make result size depend on file layout rather than on the query contract
- merged windows prevent repeated duplicate context lines around nearby hits

## Query Layer Guidance

Recommendation:

- keep context-window expansion in the search/query layer, not in the formatter

Why:

- the project already moved `search_text` and `search_symbols` toward capture-then-format behavior
- having the formatter re-read file content would work against that separation
- owned window lines can be tested independently of textual rendering

This suggests the next shell should extend the owned text-search result model with enough information to render:

- context lines
- match lines
- window separators

without re-entering the live index during formatting.

## Explicit Deferrals

Defer for a later slice:

- `before`
- `after`
- `glob`
- `exclude_glob`
- `case_sensitive`
- `whole_word`
- mixed code/text lane search

Why:

- those are real features, but they are not required to land the first context-capable shell
- adding them together would blur whether any regression came from scope selection, match semantics, or context rendering

## Recommended Next Implementation Slice

- extend `SearchTextInput` with `context: Option<u32>`
- extend `TextSearchOptions` with a symmetric context field
- materialize merged context windows in the search layer after match selection
- preserve old output when `context` is absent
- add focused tests for:
  - exact legacy output when no context is requested
  - one match with surrounding context
  - overlapping windows merging cleanly
  - disjoint windows separated by `...`
  - caps still counting matches rather than rendered lines

Expected touch points:

- `src/protocol/tools.rs`
- `src/live_index/search.rs`
- `src/protocol/format.rs`

## Carry Forward

- keep the first context slice additive to the task 77 shell rather than redesigning `search_text`
- preserve current no-context output byte-for-byte when `context` is not provided
- stay on the semantic code lane until a real text-lane registry exists
- defer asymmetric `before` and `after` fields until there is evidence they are needed
