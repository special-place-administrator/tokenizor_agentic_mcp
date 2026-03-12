# Research: P1 Get Context Bundle Exact Selector Contract

Related plan:

- [05-P-validation-and-backlog.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)
- [41-T-phase1-context-bundle-read-view-capture.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/41-T-phase1-context-bundle-read-view-capture.md)
- [88-R-p1-find-references-exact-selector-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/88-R-p1-find-references-exact-selector-contract-research.md)
- [89-T-p1-find-references-exact-selector-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/89-T-p1-find-references-exact-selector-shell.md)

Goal:

- choose the smallest exact-selector contract that lets current `search_symbols` output drive a materially less noisy `get_context_bundle` flow without first introducing stable symbol ids

## Current Code Reality

Current public `get_context_bundle` already exposes:

- `path`
- `name`
- `kind` as the symbol-kind filter

Current weaknesses are narrower than `find_references`:

- same-file duplicate names still resolve to the first matching symbol
- caller and type-usage sections still use global name-driven matching
- the callee section is already based on the selected symbol index and is therefore more precise

That means the next shell does not need a new path selector or a new symbol-kind field. The missing public selector piece is the line number already shown by `search_symbols`.

## Candidate Approaches

### Option 1: stable symbol id now

- strongest end state
- still too large for the next P1 slice because it touches symbol storage, outputs, and all follow-up consumers together

### Option 2: reuse current `{path,name,kind}` only

- no public surface growth
- still ambiguous for duplicate local names, overload-like patterns, and nested repeated names inside the same file

### Option 3: additive `symbol_line` on top of current input

- keep current `path`
- keep current `name`
- keep current `kind` meaning symbol kind
- add optional `symbol_line`

This chains directly from current `search_symbols` output and solves the most obvious exact-selection failure mode without redesigning the tool.

## Decision: Add `symbol_line`

Recommendation:

- preserve the existing `path`, `name`, and `kind` contract
- add `symbol_line: Option<u32>`

Interpretation:

- resolve the target symbol inside `path` using `name`, optional `kind`, and optional `symbol_line`
- if resolution finds one match, proceed
- if resolution finds more than one match and no `symbol_line`, return a stable ambiguity result
- if resolution finds zero matches, keep the current not-found behavior but make the selected line part of the lookup criteria

## Query Semantics Recommendation

Recommendation:

- make the first shell exact about symbol selection
- reuse dependency-scoped exact matching for the caller and type-usage sections once a single symbol is resolved

Smallest useful first-shell algorithm:

1. resolve the symbol inside `path` by `name`, optional `kind`, and optional `symbol_line`
2. reuse the selected symbol index for the body and callee section
3. build the caller section from exact-selector `call` references for the resolved symbol
4. build the type-usage section from exact-selector `type_usage` references for the resolved symbol
5. keep section caps, ordering, and successful formatting unchanged

Why:

- it preserves the current strengths of `get_context_bundle`
- it eliminates the most obvious same-name caller and type-usage floods
- it reuses the exact-selector groundwork from task 89 instead of inventing a second heuristic family

## Output Contract

Recommendation:

- keep successful `get_context_bundle` formatter output unchanged
- extend the owned view with a stable ambiguity variant rather than returning a raw string directly

Why:

- `get_context_bundle` already has an owned result enum and a compatibility formatter wrapper
- an explicit ambiguity variant keeps the tool path and the formatter wrapper aligned
- it keeps regression risk smaller than changing the tool to bypass formatter rendering on errors

Recommended stable ambiguity wording:

- `Ambiguous symbol selector for {name} in {path}; pass \`symbol_line\` to disambiguate. Candidates: {lines}`

## Recommended Next Implementation Slice

- extend `GetContextBundleInput` with `symbol_line`
- add a shared symbol-resolution helper in `src/live_index/query.rs` for `path` + `name` + optional `kind` + optional `symbol_line`
- add an ambiguity variant to `ContextBundleView`
- switch caller and type-usage sections to the exact-selector reference flow after successful resolution
- add focused tests for:
  - ambiguity without `symbol_line`
  - `symbol_line` selecting the intended duplicate symbol
  - caller/type-usage sections excluding unrelated same-name hits
  - backward-compatible successful output for unambiguous cases

Expected touch points:

- `src/protocol/tools.rs`
- `src/live_index/query.rs`
- `src/protocol/format.rs`
- likely `tests/xref_integration.rs` only if a higher-level exactness case is needed

## Carry Forward

- keep this slice separate from stable symbol-id work
- preserve section ordering, caps, and the existing performance guardrail
- keep `kind` meaning symbol kind for `get_context_bundle`
