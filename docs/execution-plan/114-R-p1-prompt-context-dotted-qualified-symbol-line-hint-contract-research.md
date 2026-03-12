# Research: P1 Prompt Context Dotted Qualified Symbol Line Hint Contract

Related plan:

- [05-P-validation-and-backlog.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)
- [113-T-p1-prompt-context-dotted-qualified-symbol-alias-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/113-T-p1-prompt-context-dotted-qualified-symbol-alias-shell.md)
- [111-T-p1-prompt-context-qualified-symbol-alias-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/111-T-p1-prompt-context-qualified-symbol-alias-shell.md)

Goal:

- decide whether prompt-context should explicitly support exact dotted qualified symbol aliases with direct `:line` suffixes such as `pkg.db.connect:2`

## Current Code Reality

After task 111, prompt-context can already pass a full qualified alias into the line-hint parser when the exact qualified-symbol lane is selected. Task 113 made the dotted no-line route explicit, but did not pin the dotted `:line` form with focused coverage.

## Candidate Approaches

### Option 1: rely on generic `line N` only

- keeps the contract smaller
- leaves the dotted `:line` form implicit even though it fits the exact-alias model

### Option 2: bless exact dotted alias `:line` suffixes

- consistent with the exact Rust qualified-symbol behavior already covered by task 111
- keeps the syntax narrow because the line number must be attached to the full dotted alias
- avoids introducing fuzzy number parsing

### Option 3: infer unlabeled numbers near dotted aliases

- more permissive
- too risky because it blurs exact selector cues with ordinary prose and snippets

## Decision: Add An Explicit Exact Dotted `:line` Contract

Recommendation:

- explicitly support exact full dotted qualified aliases with direct `:line` suffixes, starting with Python-style forms like `pkg.db.connect:2`
- keep the current `line N` fallback intact
- reject unrelated colon numbers that are not attached to the exact dotted alias

## Why This Is The Smallest Useful Slice

- it only extends the explicit dotted contract to the already-adjacent `:line` form
- it keeps the selector lane exact and easy to reason about
- it can be validated with focused tests and little or no implementation churn

## Recommended Next Implementation Slice

- add focused prompt-context coverage for exact dotted qualified symbol aliases with `:line`
- verify the dotted `:line` form disambiguates duplicate same-name symbols in one matched file
- verify unrelated colon numbers still do not activate dotted exact selection

Expected touch points:

- `src/sidecar/handlers.rs`
- `tests/sidecar_integration.rs`

## Carry Forward

- keep dotted alias line matching exact and attached to the full alias
- preserve the existing dotted no-line route, Rust `::` behavior, and `line N` fallback
- defer broader numeric inference unless later usage shows a strong need
