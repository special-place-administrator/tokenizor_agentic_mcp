# Research: P1 Prompt Context Qualified Module Alias Contract

Related plan:

- [05-P-validation-and-backlog.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)
- [104-R-p1-prompt-context-qualified-extensionless-path-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/104-R-p1-prompt-context-qualified-extensionless-path-contract-research.md)
- [105-T-p1-prompt-context-qualified-extensionless-path-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/105-T-p1-prompt-context-qualified-extensionless-path-shell.md)

Goal:

- decide whether prompt-context should add an exact logical-module alias after finishing path-shaped line hints

## Current Code Reality

After task 105, prompt-context accepts:

1. `line N`
2. exact `<resolved-path>:<line>`
3. unique basename `<file.rs>:<line>`
4. unique extensionless stem `<stem>:<line>`
5. unique repo-relative extensionless path `<path-without-ext>:<line>`

That still leaves one adjacent prompt shape unsupported:

- `crate::db:2 connect`

This is useful when a user thinks in language modules rather than file paths, but it should not be allowed to degrade into fuzzy module guessing.

## Candidate Approaches

### Option 1: stop at path-shaped aliases

- safest current boundary
- leaves exact module-oriented prompts unsupported

### Option 2: accept exact qualified module aliases with an explicit namespace separator

- stays deterministic if the alias must equal the file's resolved module path
- keeps the feature distinct from existing path and stem routes
- can reuse existing module-path derivation logic

### Option 3: accept fuzzy bare module aliases

- more permissive
- too risky because it overlaps with current stem and symbol parsing

## Decision: Add An Exact Qualified Module-Alias Bridge

Recommendation:

- accept module aliases only when they exactly match a language-derived module path
- require an explicit namespace separator such as `::` or `.` so the alias is clearly module-shaped
- continue requiring `:<line>` so the prompt still feeds the exact-selector lane

Keep current behavior intact:

- path-shaped aliases stay the preferred lane for file-oriented prompts
- ambiguous or partial module aliases still fall back to the current non-exact path
- bare tokens like `db:2` remain handled only by the stem-based route from task 103

## Why This Is The Smallest Useful Slice

- it supports a real prompt shape without adding fuzzy language-level guessing
- it reuses module-path logic the index already computes
- it keeps module aliases clearly separated from existing path aliases

## Recommended Next Implementation Slice

- extend prompt-context file-hint matching to accept exact qualified module aliases like `crate::db:2`
- only activate when the alias exactly equals the resolved module path for one indexed file and contains an explicit namespace separator
- add focused tests for:
  - exact Rust-style module alias `crate::db:2` disambiguates combined prompts
  - partial or fuzzy module aliases do not activate exact selection
  - existing path-shaped aliases keep working

Expected touch points:

- `src/sidecar/handlers.rs`
- `tests/sidecar_integration.rs`

## Carry Forward

- keep the accepted module syntax exact and explicitly qualified
- preserve current path/stem behavior and fallback semantics
- defer broader module guessing unless later usage justifies it
