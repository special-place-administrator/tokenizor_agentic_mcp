# Research: P1 Find References Exact Selector Contract

Related plan:

- [04-P-phase-plan.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [05-P-validation-and-backlog.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)
- [02-P-workstreams-and-tool-surface.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/02-P-workstreams-and-tool-surface.md)
- [84-R-p1-search-symbols-scope-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/84-R-p1-search-symbols-scope-contract-research.md)
- [87-T-p1-search-symbols-noise-defaults-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/87-T-p1-search-symbols-noise-defaults-shell.md)

Goal:

- choose the smallest exact-selector contract that lets current `search_symbols` output drive a materially less noisy `find_references` flow without first introducing stable symbol ids

## Current Code Reality

Current public `find_references` exposes only:

- `name`
- `kind` for reference-kind filtering

Current public `search_symbols` already exposes enough follow-up context to do better:

- exact repo-relative `path`
- symbol `kind`
- symbol `line`

But the current xref substrate does **not** store a resolved target symbol identity on each reference. Reference records currently carry:

- simple `name`
- optional `qualified_name`
- reference `kind`
- the file/location where the reference occurs
- the enclosing symbol where the reference occurs

That means the first exact-selector shell cannot honestly promise full symbol-id precision across all languages and scopes.

## Candidate Approaches

### Option 1: stable symbol id now

- strongest end state
- aligns with the long-term phase plan
- too large for the next P1 slice because it touches symbol storage, search outputs, follow-up tools, and migration behavior together

### Option 2: `{path,name,kind}` only

- small public contract
- chains from existing `get_symbol`
- still ambiguous for same-file duplicates, overloads, and repeated nested names

### Option 3: exact selector with line disambiguation

- keep current `name`
- add `path`
- add `symbol_kind`
- add `symbol_line`
- preserve `kind` as the reference-kind filter

This chains directly from the current `search_symbols` output and gives the first shell a deterministic way to resolve same-file ambiguity without inventing a new identity model.

## Decision: Add `path`, `symbol_kind`, And `symbol_line`

Recommendation:

- keep `name` required
- keep `kind` meaning reference-kind filter only
- add optional `path: Option<String>`
- add optional `symbol_kind: Option<String>`
- add optional `symbol_line: Option<u32>`

Interpretation:

- if `path` is absent, preserve the current global name-only behavior exactly
- if `path` is present, switch to exact-selector mode
- exact-selector mode resolves a symbol inside `path` using:
  - `name`
  - optional `symbol_kind`
  - optional `symbol_line`

If selector resolution finds:

- zero matches: return a stable symbol-not-found error for that file
- more than one match and no `symbol_line`: return a stable ambiguity error listing candidate lines
- one exact match: continue with the exact-selector reference query

## Why `symbol_line` Belongs In The First Shell

Recommendation:

- include `symbol_line` in the first public contract rather than deferring it

Why:

- current `search_symbols` output already prints the line number
- same-file duplicates are the most obvious failure mode for `{path,name,kind}` alone
- adding line now is smaller than inventing a second selector revision immediately after the first shell lands

The contract should define `symbol_line` as:

- the same line number shown by `search_symbols`

## Query Semantics Recommendation

Recommendation:

- make exact-selector mode exact about **which symbol was selected**
- keep the first implementation dependency-scoped and explicit about its heuristic ceiling

Smallest useful first-shell algorithm:

1. resolve the target symbol in the selected file
2. collect same-file references that match the selected symbol name
3. collect cross-file references from files that already depend on the selected file via `find_dependents_for_file(path)`
4. filter those dependent-file hits down to the selected symbol name and optional reference-kind filter
5. prefer qualified/module-path matches when available, but do not redesign output in this slice

Why:

- it materially reduces global homonym floods such as `new`
- it reuses existing dependency and import-matching substrate
- it avoids pretending we already have fully resolved target identities when we do not

## Honesty Boundary

This first shell should be documented and tested as:

- exact symbol **selection**
- dependency-scoped reference **matching**

Not yet:

- stable target-id exactness across every language and nesting pattern

That stronger guarantee belongs to the later stable-symbol-id phase.

## Output Contract

Recommendation:

- keep the successful `find_references` formatter output unchanged in the first shell
- use stable user-facing error strings for:
  - file not found
  - symbol not found in file
  - ambiguous selector without `symbol_line`

Why:

- the value of this slice is query precision, not output redesign
- keeping the grouped-by-file formatter unchanged keeps regression risk bounded

## Recommended Next Implementation Slice

- extend `FindReferencesInput` with `path`, `symbol_kind`, and `symbol_line`
- add an exact-selector capture/query helper in `src/live_index/query.rs`
- preserve current name-only behavior when `path` is omitted
- add focused tests for:
  - exact-selector mode excludes unrelated same-name references
  - ambiguous same-file selector errors without `symbol_line`
  - `symbol_line` resolves ambiguity deterministically
  - current name-only mode remains unchanged

Expected touch points:

- `src/protocol/tools.rs`
- `src/live_index/query.rs`
- likely `src/protocol/format.rs` only for stable error wording if helpers are added there

## Carry Forward

- keep this slice separate from stable symbol-id work
- keep this slice separate from `get_context_bundle` exactness follow-up
- preserve current name-only `find_references` behavior when no exact selector is provided
