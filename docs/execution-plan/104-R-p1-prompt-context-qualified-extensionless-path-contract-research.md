# Research: P1 Prompt Context Qualified Extensionless Path Contract

Related plan:

- [05-P-validation-and-backlog.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)
- [102-R-p1-prompt-context-extensionless-line-hint-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/102-R-p1-prompt-context-extensionless-line-hint-contract-research.md)
- [103-T-p1-prompt-context-extensionless-line-hint-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/103-T-p1-prompt-context-extensionless-line-hint-shell.md)

Goal:

- decide the smallest prompt-context follow-up after `db:2` that can disambiguate repeated stems without introducing language-aware module parsing

## Current Code Reality

After task 103, prompt-context accepts:

1. `line N`
2. exact `<resolved-path>:<line>`
3. unique basename `<file.rs>:<line>`
4. unique extensionless stem `<stem>:<line>`

That still leaves one adjacent prompt shape unsupported:

- `src/db:2 connect`

This is useful when a repo has multiple `db` files and a user wants to stay path-oriented while omitting the file extension.

## Candidate Approaches

### Option 1: stop after bare stems

- simplest current boundary
- leaves repeated-stem prompts without a compact disambiguation path

### Option 2: accept repo-relative extensionless paths like `src/db:2`

- stays file-oriented and deterministic
- builds directly on the existing exact-path and stem logic
- resolves repeated stems without introducing language-specific module rules

### Option 3: accept module-style aliases like `crate::db:2`

- potentially ergonomic in some languages
- requires language-aware interpretation and broader ambiguity rules
- too large for the next prompt-context slice

## Decision: Add A Repo-Relative Extensionless Path Bridge

Recommendation:

- accept repo-relative extensionless path hints like `src/db:2`
- treat them as file-path aliases, not module identifiers
- only activate when the alias maps uniquely to an indexed file

Keep current behavior intact:

- exact-path, basename-derived, and bare stem `:line` support stay
- explicit `line N` stays
- ambiguous or partial path-like aliases still fall back to the current non-exact path

## Why This Is The Smallest Useful Slice

- it solves real repeated-stem ambiguity with a path-shaped hint the system already understands
- it avoids dragging prompt-context into per-language module resolution
- it extends the exact-selector lane without changing the public tool contracts

## Recommended Next Implementation Slice

- extend prompt-context file-hint matching to accept unique repo-relative extensionless paths that are immediately followed by `:<line>`
- continue feeding the selected file into the existing `symbol_line` exact-selector path
- add focused tests for:
  - `src/db:2 connect` disambiguates repeated `db` stems
  - ambiguous or partial extensionless paths do not activate exact selection
  - existing exact-path, basename, and bare stem behavior remains intact

Expected touch points:

- `src/sidecar/handlers.rs`
- `tests/sidecar_integration.rs`

## Carry Forward

- keep the accepted syntax repo-relative and slash-separated
- preserve the current fallback when no exact file alias is available
- defer true module-style aliases unless later usage shows a clear need
