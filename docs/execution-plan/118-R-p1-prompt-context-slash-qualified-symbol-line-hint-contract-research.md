# Research: P1 Prompt Context Slash Qualified Symbol Line Hint Contract

Related plan:

- [05-P-validation-and-backlog.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)
- [117-T-p1-prompt-context-slash-qualified-symbol-alias-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/117-T-p1-prompt-context-slash-qualified-symbol-alias-shell.md)
- [115-T-p1-prompt-context-dotted-qualified-symbol-line-hint-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/115-T-p1-prompt-context-dotted-qualified-symbol-line-hint-shell.md)

Goal:

- decide whether prompt-context should explicitly support exact slash-qualified symbol aliases with direct `:line` suffixes such as `src/utils/connect:2`

## Current Code Reality

After task 117, prompt-context can consume exact slash-qualified symbol aliases derived from normalized JS/TS module paths, but the route is only covered for the no-line form. The alias-attached line-hint path already exists for the Rust and dotted families through the shared exact selector machinery.

## Candidate Approaches

### Option 1: rely on generic `line N` only

- keeps the contract smaller
- leaves the slash alias `:line` form implicit even though it fits the same exact-selector model

### Option 2: bless exact slash alias `:line` suffixes

- consistent with the explicit behavior already documented for other qualified-alias families
- keeps the syntax narrow because the number must be attached to the full slash alias
- avoids conflating unrelated path-like colon numbers with selector lines

### Option 3: infer unlabeled numbers near slash aliases

- more permissive
- too risky because slash-heavy prompts already resemble ordinary paths and import strings

## Decision: Add An Explicit Exact Slash `:line` Contract

Recommendation:

- explicitly support exact full slash-qualified aliases with direct `:line` suffixes, starting with normalized JS/TS forms like `src/utils/connect:2`
- keep the current `line N` fallback intact
- reject unrelated colon numbers that are not attached to the exact slash alias

## Why This Is The Smallest Useful Slice

- it only extends the explicit slash contract to the already-adjacent `:line` form
- it keeps the selector lane exact and easy to reason about
- it can be validated with focused tests and little or no implementation churn

## Recommended Next Implementation Slice

- add focused prompt-context coverage for exact slash-qualified symbol aliases with `:line`
- verify the slash `:line` form disambiguates duplicate same-name symbols in one matched file
- verify unrelated colon numbers still do not activate slash exact selection

Expected touch points:

- `src/sidecar/handlers.rs`
- `tests/sidecar_integration.rs`

## Carry Forward

- keep slash alias line matching exact and attached to the full alias
- preserve the existing slash no-line route, earlier alias families, and `line N` fallback
- defer broader path-number inference unless later usage shows a strong need
