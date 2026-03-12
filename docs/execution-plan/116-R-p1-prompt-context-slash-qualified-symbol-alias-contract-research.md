# Research: P1 Prompt Context Slash Qualified Symbol Alias Contract

Related plan:

- [05-P-validation-and-backlog.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)
- [115-T-p1-prompt-context-dotted-qualified-symbol-line-hint-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/115-T-p1-prompt-context-dotted-qualified-symbol-line-hint-shell.md)
- [111-T-p1-prompt-context-qualified-symbol-alias-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/111-T-p1-prompt-context-qualified-symbol-alias-shell.md)

Goal:

- decide whether prompt-context should explicitly support exact slash-qualified symbol aliases derived from normalized module paths

## Current Code Reality

Prompt-context now has explicit exact qualified-symbol coverage for:

1. Rust `::` aliases
2. dotted aliases backed by Python-style module derivation
3. direct `:line` hints on both of those exact alias families

The remaining adjacent namespace separator already used elsewhere in the query layer is `/`, primarily for JavaScript and TypeScript module paths.

## Candidate Approaches

### Option 1: stop at current alias families

- simplest boundary
- leaves slash-based module-path conventions unhandled even where the query layer already understands them

### Option 2: accept exact slash-qualified symbol aliases

- deterministic if the alias must exactly match a derived slash module path plus one trailing symbol name
- keeps the lane aligned with the exact-selector machinery already in place
- requires clear boundaries so ordinary path/file hints do not get misclassified

### Option 3: parse arbitrary import-like strings

- more permissive
- too risky because it blurs path discovery, file hints, and symbol routing into one fuzzy lane

## Decision: Explore An Exact Slash-Alias Shell

Recommendation:

- add a narrow shell slice for exact slash-qualified symbol aliases only when the module portion is derived from normalized module-path conventions
- keep exact file/path hints higher-value and intact
- require an exact trailing symbol segment so a slash-qualified symbol alias remains distinct from a plain file hint

## Why This Is The Smallest Useful Slice

- it reuses the same exact selector pattern as the current namespaced alias routes
- it addresses the next concrete language family without jumping to semantic parsing
- it forces a precise boundary between path hints and slash-qualified symbol aliases

## Recommended Next Implementation Slice

- add the first exact slash-qualified symbol alias route and focused tests
- verify exact slash aliases route into the exact selector lane
- verify plain path hints and partial slash aliases still fall back cleanly

Expected touch points:

- `src/sidecar/handlers.rs`
- `tests/sidecar_integration.rs`

## Carry Forward

- keep slash-qualified symbol matching exact and tied to normalized module-path derivation
- preserve existing file hints and previously implemented `::` and dotted routes
- defer fuzzy import/path parsing unless later usage shows a clear need
