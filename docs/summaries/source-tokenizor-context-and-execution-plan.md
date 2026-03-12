# Source Summary: Tokenizor Context And Execution Plan

Processed: 2026-03-11
Source: `TOKENIZOR_CONTEXT_AND_EXECUTION_PLAN.md`
Source size: 63,048 bytes
Status: active authoritative project plan retained in place
Archive status: not archived because this is still the working source-of-truth document

Authority order:

1. `TOKENIZOR_CONTEXT_AND_EXECUTION_PLAN.md`
2. `docs/execution-plan/*.md`
3. this summary

Use rule:

- use this summary for orientation only
- use the split docs or the original source for actual implementation decisions
- when summary wording and source wording differ, defer to the source

## Document Role

- Combined context document and execution plan for the Tokenizor upgrade
- Intended audience is AI coding agents plus maintainers working inside this repository
- Primary goal is to make Tokenizor the default exploration surface for agents, replacing most `rg` usage for code search and most `Get-Content` usage for file reading without regressing current strengths

## Core Decision Rules

- Preserve the current useful baseline unless a measured replacement is clearly better
- Optimize for four strategic goals: path discovery, scoped search, exact reference navigation, and read-surface parity
- Keep Tokenizor code-first, but not code-only
- Support non-binary text reading/search through a lightweight secondary lane rather than expanding the semantic code index into an all-files engine
- Prefer additive, benchmarked, regression-tested slices over broad rewrites

## What The Plan Says Is Already Strong

- `get_context_bundle`
- `get_file_context`
- `find_dependents`
- current `search_text` and `search_symbols` base behavior
- daemon/session/runtime direction
- fast code-centric in-memory index

## Main Capability Gaps

- No first-class path discovery tool, so agents still need `rg --files`
- Search tools lack practical scope controls such as `path_prefix`, `glob`, `exclude_glob`, `language`, and context-line support
- `find_references` is too name-driven for common symbols
- `get_file_content` is too raw to replace normal read workflows
- Output formatting is not consistently path-rich or follow-up friendly
- There is no short path from file resolution to local read to exact symbol trace

## Target Tool Surface

Add or formalize:

- `search_files`
- `resolve_path`
- `inspect_match`
- `trace_symbol`
- optionally `inspect_file`

Enrich instead of replace:

- `search_text`
- `search_symbols`
- `get_file_content`
- `find_references`
- preserve `get_context_bundle`

## Required Architecture Direction

- Build a shared internal query layer separate from rendering
- Add secondary in-memory indices for basename/path lookup and exact symbol follow-up
- Introduce stable symbol identity for exact reference navigation
- Use snapshot-based reads for deterministic, concurrent-safe query behavior
- Classify files at index time (`is_generated`, `is_test`, `is_vendor`, `is_code`, `is_text`, `is_binary`)
- Use a dual-lane retrieval model:
- semantic code lane for parsed source
- lightweight plain-text lane for non-binary text files

## Ordered Phase Plan

1. Phase 0: baseline, fixtures, benchmarks, and compatibility thresholds
2. Phase 1: shared query substrate, file classification, path indices, and dual-lane boundary
3. Phase 2: path discovery tools (`search_files`, `resolve_path`, better `repo_outline`)
4. Phase 3: scoped `search_text` with filters, limits, and context lines
5. Phase 4: read-surface parity for `get_file_content` plus optional `inspect_match`
6. Phase 5: stable symbol identity and exact `find_references`
7. Phase 6: noise suppression and ranking quality
8. Phase 7: one-call exploration tools (`trace_symbol`, `inspect_match`, optional `inspect_file`)
9. Phase 8: prompt/resource/README/client guidance alignment
10. Phase 9: evidence-based decision on any deeper subsystem replacement

## Recommended First Implementation Slice

- Phase 1
- Phase 2
- OPEN: the body of the plan ties line-numbered reading work to Phase 4 (`get_file_content` parity), while the final execution guidance line says "the line-numbered subset of Phase 3"
- until resolved, treat the source wording as authoritative and flag the ambiguity explicitly during execution planning rather than silently normalizing it

## Concrete Priority Backlog

P0:

- add `search_files`
- add `resolve_path`
- extend `search_text` with path/language/glob filters and context lines
- extend `get_file_content` with line numbers and `around_line`
- make `repo_outline` path-rich

P1:

- extend `search_symbols` with path/language/limit filters
- add generated/test suppression metadata and defaults
- extend `get_file_content` with `around_match` and chunking
- add exact-symbol reference query flow

P2:

- add `trace_symbol`
- add `inspect_match`
- add prompt/resource support for the new flows

P3:

- refine ranking with module/path locality
- tune formatting and result caps from usage

## Validation Requirements

- Add benchmark scenarios before major query-surface changes
- Regression coverage must include repeated basenames, noisy common symbols like `new`, generated/test suppression, context-line rendering, line-numbered reads, and tool/resource/prompt parity where public behavior changes
- Output contracts must stay path-rich, bounded, deterministic, and explicit about ambiguity

## Open Questions Captured By The Plan

- Best internal shape for the shared query layer
- Best backing structure for path lookup: basename maps, path tokens, trigrams, or hybrid
- Best implementation for the non-code text lane: lazy reads, bounded cache, or lightweight registry
- Public exact-symbol contract: stable `symbol_id` vs `{path,name,kind}` fallback semantics
- Whether `inspect_match` should be its own tool or a `get_file_content` mode
- Whether `trace_symbol` coexists with or eventually supersedes parts of `get_context_bundle`

## Reading Strategy For Future Sessions

- Prefer the split docs in `docs/execution-plan/`
- Reopen the monolith only when exact wording, provenance, or full cross-section context is required
- Treat this summary as the quick re-entry point before implementation work
