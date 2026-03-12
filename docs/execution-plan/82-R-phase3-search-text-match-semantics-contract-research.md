# Research: Phase 3 Search Text Match Semantics Contract

Related plan:

- [04-P-phase-plan.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [05-P-validation-and-backlog.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)
- [79-T-phase3-search-text-context-window-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/79-T-phase3-search-text-context-window-shell.md)
- [81-T-phase3-search-text-glob-filter-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/81-T-phase3-search-text-glob-filter-shell.md)
- [82-T-phase3-search-text-match-semantics-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/82-T-phase3-search-text-match-semantics-contract-research.md)

Goal:

- choose the smallest `search_text` match-semantics extension that lands `case_sensitive` and `whole_word` without destabilizing the current scoped shell

## Current Code Reality

After tasks 77, 79, and 81, public `search_text` already supports:

- `path_prefix`
- `language`
- `limit`
- `max_per_file`
- `include_generated`
- `include_tests`
- `context`
- `glob`
- `exclude_glob`

Current literal matching behavior is still:

- case-insensitive by default
- substring-based
- OR semantics across `terms`

Current regex behavior is still:

- enabled by `regex=true`
- compiled directly with Rust `regex`
- case-sensitive by default because no case-insensitive flag is applied

The current candidate prefiltering path also matters:

- literal searches use the existing case-insensitive trigram index as a cheap candidate-path prefilter
- final line matching already happens in the query layer, so stricter match semantics can be added there without redesigning the formatter

## Decision: Add `case_sensitive` And `whole_word` Together

Recommendation:

- add `case_sensitive: Option<bool>`
- add `whole_word: Option<bool>`

Why:

- both knobs change literal match semantics in the same narrow code path
- they are explicitly called for by Phase 3
- landing them together avoids two nearly identical matcher rewrites and keeps the current shell coherent

## Default Semantics

Recommendation:

- literal search remains case-insensitive by default
- regex search remains case-sensitive by default to preserve current behavior

This means `case_sensitive` should be interpreted after `regex` mode is known:

- literal mode:
  - `case_sensitive = None` behaves like `false`
- regex mode:
  - `case_sensitive = None` behaves like `true`

Why:

- this preserves today's public behavior in both literal and regex modes
- callers can still opt into case-sensitive literal search or case-insensitive regex search explicitly

## Whole-Word Scope

Recommendation:

- support `whole_word` only for literal search in the first shell
- reject `regex=true` plus `whole_word=true` with a stable user-facing error

Why:

- whole-word literal matching is straightforward and testable
- Rust `regex` does not support the look-around machinery needed for a faithful code-oriented whole-word wrapper around arbitrary regex patterns
- wrapping arbitrary regex patterns in `\b` would be lossy and surprising for punctuation-heavy patterns

## Literal Whole-Word Semantics

Recommendation:

- use identifier-style boundaries for literal whole-word matching
- treat a word character as `char::is_alphanumeric()` or `_`
- a match is whole-word only when the preceding and following characters are not word characters

Why:

- this aligns better with code-search expectations than raw whitespace tokenization
- it prevents `foo` from matching inside `foobar` or `foo_bar`
- it still allows identifier-like searches in Rust, TypeScript, and similar languages

## Prefilter Guidance

Recommendation:

- keep the current trigram prefilter unchanged for this slice
- enforce `case_sensitive` and `whole_word` only in final line matching

Why:

- the existing trigram search is already a safe superset prefilter for stricter literal semantics
- this keeps the slice small and avoids widening the hot-path indexing contract mid-phase

## Output Contract

Recommendation:

- keep output formatting unchanged
- keep `limit` and `max_per_file` semantics unchanged
- keep context-window rendering behavior unchanged when `context` is present

Why:

- this slice is about match semantics, not rendering
- the current formatter and cap behavior were just stabilized in tasks 77, 79, and 81

## Recommended Next Implementation Slice

- extend `SearchTextInput` with `case_sensitive` and `whole_word`
- extend `TextSearchOptions` with effective match-semantics flags
- add a small matcher layer for:
  - case-sensitive literal search
  - case-insensitive literal search
  - whole-word literal search
  - optional case-insensitive regex search
- reject `regex=true` plus `whole_word=true`
- add focused tests for:
  - case-sensitive literal matching
  - default case-insensitive literal matching
  - whole-word boundaries against identifier-like text
  - whole-word with multiple `terms`
  - case-insensitive regex via `case_sensitive=false`
  - stable error text for `regex + whole_word`

Expected touch points:

- `src/protocol/tools.rs`
- `src/live_index/search.rs`
- `src/protocol/format.rs`

## Carry Forward

- keep this slice additive to the existing scoped `search_text` shell
- do not redesign the trigram substrate in the same task
- defer regex whole-word semantics until there is evidence that a broader contract is worth the complexity
