# Research: P1 Prompt Context Slash Module Alias File Hint Contract

Related plan:

- [05-P-validation-and-backlog.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)
- [119-T-p1-prompt-context-slash-qualified-symbol-line-hint-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/119-T-p1-prompt-context-slash-qualified-symbol-line-hint-shell.md)
- [109-T-p1-prompt-context-module-alias-file-hint-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/109-T-p1-prompt-context-module-alias-file-hint-shell.md)

Goal:

- decide whether prompt-context should explicitly support exact normalized slash module aliases like `src/utils` as file hints without `:line`

## Current Code Reality

After task 119, prompt-context can consume exact slash-qualified symbol aliases like `src/utils/connect` and `src/utils/connect:3`, but the file-hint lane still does not treat the bare module alias `src/utils` as a resolved file hint. That leaves JS and TS index-style modules less ergonomic than earlier Rust and Python module aliases.

## Candidate Approaches

### Option 1: rely on extensionless path hints only

- keeps the contract smaller
- misses normalized module aliases that intentionally collapse `index.ts` and `index.js` to `src/utils`

### Option 2: bless exact normalized slash module aliases as no-line file hints

- consistent with the earlier module-alias file-hint work for Rust and Python
- matches the import strings users already write in JS and TS code
- keeps the syntax narrow because the alias must match one full normalized module boundary

### Option 3: infer partial slash segments as module guesses

- more permissive
- too risky because slash-heavy prompts already overlap with ordinary paths and path fragments

## Decision: Add An Explicit Exact Slash Module-Alias File Hint Contract

Recommendation:

- explicitly support exact normalized slash module aliases like `src/utils` as file hints without `:line`
- keep exact slash-qualified symbol aliases like `src/utils/connect` higher priority
- reject partial and continued slash aliases such as `src/utilsx` and `src/utils/more`

## Why This Is The Smallest Useful Slice

- it only extends the already-established slash alias family to the adjacent file-hint lane
- it keeps matching exact and easy to reason about
- it can be validated with focused handler and endpoint tests

## Recommended Next Implementation Slice

- extend the prompt-context file-hint matcher to recognize normalized JS and TS slash module aliases
- verify exact no-line slash module aliases activate file hints for both file-only and combined file+symbol prompts
- verify partial and continued slash aliases do not activate the exact file-hint lane

Expected touch points:

- `src/sidecar/handlers.rs`
- `tests/sidecar_integration.rs`

## Carry Forward

- keep slash module alias matching exact and boundary-aware
- preserve slash-qualified symbol alias priority over file-hint routing
- defer slash module-alias `:line` behavior unless a later slice explicitly blesses it
