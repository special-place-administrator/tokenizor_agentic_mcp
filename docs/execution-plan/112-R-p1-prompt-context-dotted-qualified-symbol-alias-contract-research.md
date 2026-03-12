# Research: P1 Prompt Context Dotted Qualified Symbol Alias Contract

Related plan:

- [05-P-validation-and-backlog.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)
- [111-T-p1-prompt-context-qualified-symbol-alias-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/111-T-p1-prompt-context-qualified-symbol-alias-shell.md)
- [106-R-p1-prompt-context-qualified-module-alias-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/106-R-p1-prompt-context-qualified-module-alias-contract-research.md)

Goal:

- decide whether prompt-context should explicitly support exact dotted qualified symbol aliases such as `pkg.db.connect`

## Current Code Reality

After task 111, prompt-context can derive an exact qualified-symbol route from:

1. a language-specific module alias
2. a symbol name in the matched file
3. an exact alias boundary match in the prompt

That means dotted aliases are adjacent to the current contract wherever module-alias derivation already exists. Today that is most concrete for Python-style module paths.

## Candidate Approaches

### Option 1: leave dotted aliases implicit

- smallest documentation change
- keeps useful behavior uncontracted and untested

### Option 2: bless exact dotted qualified aliases backed by module derivation

- deterministic because the file lane is still driven by derived module aliases
- keeps the contract narrow to exact dotted names like `pkg.db.connect`
- fits the same exact-selector routing used for Rust `::` aliases

### Option 3: broaden to generic dotted property chains

- would treat arbitrary member access as a prompt-context symbol hint
- too risky because it blurs file/module hints with normal prose and code snippets

## Decision: Add An Explicit Exact Dotted-Alias Contract

Recommendation:

- explicitly support exact dotted qualified symbol aliases only for languages that already derive dotted module aliases
- start with Python-style paths such as `pkg.db.connect`
- keep boundary checks strict so `pkg.db.connect.more` and partial dotted prefixes do not activate exact selection

## Why This Is The Smallest Useful Slice

- it turns already-adjacent behavior into an explicit, tested contract
- it keeps prompt-context language-aware without opening a fuzzy dotted-search lane
- it composes with the existing exact file+symbol selector path

## Recommended Next Implementation Slice

- add focused prompt-context coverage for exact dotted qualified symbol aliases and their boundary guardrails
- verify exact dotted aliases route into the exact selector lane
- verify continued dotted chains stay on the fallback path

Expected touch points:

- `src/sidecar/handlers.rs`
- `tests/sidecar_integration.rs`

## Carry Forward

- keep dotted alias matching exact and explicitly rooted in current module-alias derivation
- preserve the Rust `::` route and existing file/module fallbacks
- defer generic dotted member-access parsing unless later usage shows clear value
