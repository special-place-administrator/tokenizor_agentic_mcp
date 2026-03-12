# Research: P1 Prompt Context Slash Module Alias Line Hint Contract

Related plan:

- [05-P-validation-and-backlog.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)
- [121-T-p1-prompt-context-slash-module-alias-file-hint-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/121-T-p1-prompt-context-slash-module-alias-file-hint-shell.md)
- [119-T-p1-prompt-context-slash-qualified-symbol-line-hint-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/119-T-p1-prompt-context-slash-qualified-symbol-line-hint-shell.md)

Goal:

- decide whether prompt-context should explicitly support exact normalized slash module aliases with direct `:line` suffixes such as `src/utils:3 connect`

## Current Code Reality

After task 121, prompt-context can consume exact normalized slash module aliases like `src/utils` as file hints. Because that file-hint lane records `line_hint_alias`, the shared line-hint parser may already accept `src/utils:3`, but that behavior is still implicit and unpinned by focused tests.

## Candidate Approaches

### Option 1: rely on generic `line N` only

- keeps the contract smaller
- leaves the slash module-alias `:line` form undocumented even though it fits the existing exact-selector model

### Option 2: bless exact slash module-alias `:line` suffixes

- consistent with the explicit behavior already documented for other module-alias and slash-qualified selector families
- keeps the syntax narrow because the number must be attached to the full module alias
- avoids conflating unrelated colon numbers with selector lines

### Option 3: infer unlabeled numbers near slash module aliases

- more permissive
- too risky because slash-heavy prompts already resemble ordinary paths and import strings

## Decision: Add An Explicit Exact Slash Module-Alias `:line` Contract

Recommendation:

- explicitly support exact full normalized slash module aliases with direct `:line` suffixes, starting with prompts like `src/utils:3 connect`
- keep the no-line slash module alias route and generic `line N` fallback intact
- reject unrelated colon numbers that are not attached to the exact slash module alias

## Why This Is The Smallest Useful Slice

- it only extends the already-adjacent slash module-alias contract to the next exact disambiguation form
- it keeps selector routing exact and easy to reason about
- it can be validated with focused tests and little or no implementation churn

## Recommended Next Implementation Slice

- add focused prompt-context coverage for exact slash module aliases with direct `:line`
- verify `src/utils:3 connect` disambiguates duplicate same-name symbols in one matched file
- verify unrelated colon numbers still do not activate slash module-alias line selection

Expected touch points:

- `src/sidecar/handlers.rs`
- `tests/sidecar_integration.rs`

## Carry Forward

- keep slash module alias line matching exact and attached to the full alias
- preserve the existing no-line slash module alias route, slash-qualified symbol priority, and `line N` fallback
- defer broader slash-path number inference unless later usage shows a strong need
