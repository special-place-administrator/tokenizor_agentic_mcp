# Research: Phase 1 Dual-Lane Option Defaults

Related plan:

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [23-R-phase1-text-lane-boundary-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/23-R-phase1-text-lane-boundary-research.md)
- [63-R-phase1-remaining-substrate-priority-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/63-R-phase1-remaining-substrate-priority-research.md)
- [64-R-phase1-file-classification-heuristics-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/64-R-phase1-file-classification-heuristics-research.md)
- [66-T-phase1-shared-query-option-struct-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/66-T-phase1-shared-query-option-struct-shell.md)
- [67-T-phase1-dual-lane-option-defaults-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/67-T-phase1-dual-lane-option-defaults-research.md)

Goal:

- decide how the new internal option types should default across current tool families without accidentally collapsing the code lane and future text lane into one behavior

## Current State

- task 66 added the first internal option vocabulary:
  - `PathScope`
  - `SearchScope`
  - `ResultLimit`
  - `ContentContext`
  - `NoisePolicy`
- file classification substrate now exists, but the current indexed set is still semantic-lane only:
  - current discovered/indexed files are `FileClass::Code`
  - generated/test/vendor tags are available
  - `Text` and `Binary` entries are intentionally deferred
- preferred text-lane architecture is still separate from `LiveIndex.files`:
  - lightweight text registry
  - bounded cache
  - lane-aware query dispatch

That means the option defaults chosen now should preserve current code-first behavior and avoid pretending the text lane already exists.

## Decision: Defaults By Tool Family

### 1. Current Semantic Search Tools

Applies to:

- `search_symbols`
- current public `search_text`

Recommended defaults:

- `SearchScope::Code`
- `NoisePolicy::permissive()`

Why:

- current public behavior is code-lane only
- widening current searches to `All` would be a silent semantic change once the text lane exists
- Phase 3 explicitly plans richer search filters later, including generated/test toggles and broader path scoping
- Phase 6 explicitly reserves suppression-default tuning for later ranking work

Conclusion:

- keep current public search adapters code-only and noise-permissive for now
- do not infer text-lane participation or suppression behavior implicitly from the existence of `FileClassification`

### 2. Exact File-Local Reads

Applies to:

- `get_file_content`
- internal file-content render helpers

Recommended defaults:

- explicit `PathScope::Exact`
- `ContentContext` from requested line range
- no `NoisePolicy` suppression
- no synthetic `SearchScope` filtering on explicit path reads

Why:

- explicit path reads are not ranking or discovery operations
- a caller asking for one exact file should not lose access because that file is generated, test, or vendor
- once the text lane exists, lane selection for explicit reads should be:
  1. semantic code lane exact match
  2. text-lane registry exact match
  3. not found

Conclusion:

- exact path reads should stay unsuppressed and lane-aware by membership, not by search defaults

### 3. Structural Symbol/Outline/Xref Reads

Applies to:

- `get_file_outline`
- `get_symbol`
- `get_symbols`
- `find_references`
- `find_dependents`
- `get_context_bundle`

Recommended defaults:

- inherently semantic code lane only
- do not force these onto generic `SearchScope` / `NoisePolicy` adapters yet

Why:

- these tools depend on symbols, references, or semantic parse artifacts
- the future text lane will not produce equivalent symbol/xref structures
- adding generic option defaults here would blur a boundary that should remain explicit

Conclusion:

- keep these tools structurally code-lane by design
- only adopt shared option defaults where they encode real semantics instead of ornamental abstraction

## Why `search_text` Should Not Default To `All` Yet

- current product posture is coding-first
- current public `search_text` behavior implicitly means “search indexed code files”
- once text files exist, flipping to `All` would pull docs/config noise into existing workflows without user intent
- Phase 3 already plans the public search contract that can safely expose broader scope controls

The smallest safe path is:

- keep current public `search_text` mapped to `Code`
- later add explicit scope/filter inputs when text-lane participation becomes real

## Why Suppression Defaults Should Not Tighten Yet

- Phase 6 explicitly reserves generated/test/vendor suppression tuning for later ranking work
- current public inputs do not yet expose the planned `include_generated` / `include_tests` style controls
- turning on hidden suppression now would create behavior drift before the public API can explain or override it

That means `NoisePolicy` should exist now, but current tool-family defaults should remain permissive until ranking and public filter work arrive.

## Recommended Next Implementation Slice

- add named constructors or adapter helpers for current tool-family defaults instead of relying on raw `Default::default()` calls

Examples:

- semantic symbol search helper:
  - code lane
  - permissive noise
  - explicit result limit
- semantic text search helper:
  - code lane
  - permissive noise
- exact file-content helper:
  - exact path scope
  - explicit content context

This is the smallest useful follow-on because it:

- preserves current behavior
- makes the current default mapping explicit in code
- avoids baking accidental semantics into generic `Default` usage
- leaves room for future text-lane adapters without changing current public MCP contracts

## Carry Forward

- current public search defaults should remain `Code` + permissive noise
- explicit file reads should remain unsuppressed and lane-aware by exact membership
- structural symbol/xref tools remain semantic-lane only by design
- the next small implementation should encode these defaults explicitly in named adapters rather than implicit `Default` construction
