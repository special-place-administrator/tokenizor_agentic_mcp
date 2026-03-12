# Validation And Backlog

Derived from `TOKENIZOR_CONTEXT_AND_EXECUTION_PLAN.md` on 2026-03-11.
Source coverage: lines 2060-2214.

## Test Strategy

Treat tests as part of the product, not as cleanup.

### Must-add test coverage

- path search exact/prefix/substring/fuzzy ranking
- repeated basename disambiguation
- scoped search behavior across path/language/glob filters
- generated/test suppression defaults
- line-context rendering for text search
- line-numbered file content rendering
- around-line and around-match content extraction
- exact-symbol reference disambiguation
- regression tests for common-name floods like `new`
- hook/resource/prompt parity where the public surface changes

### Existing likely test homes

- `src/protocol/tools.rs`
- `src/protocol/format.rs`
- `src/live_index/query.rs`
- `tests/xref_integration.rs`
- `tests/hook_enrichment_integration.rs`
- `tests/live_index_integration.rs`
- `tests/retrieval_conformance.rs`

## Suggested Data Model Additions

These are likely worth adding to indexed file/symbol metadata.

### File metadata

- `basename`
- `dir_path`
- `module_path`
- `is_generated`
- `is_test`
- `is_vendor`

### Symbol metadata

- stable `symbol_id`
- parent or container identity
- display signature
- module-qualified display name

These additions support ranking, filtering, and exact follow-up queries.

## Recommended Output Standards

Every high-traffic tool should follow these rules.

### Path-rich

Never rely on basename-only output when collisions are plausible.

### Follow-up friendly

Outputs should make the next tool call obvious:

- exact path
- exact line
- exact symbol identity when available

### Read-like

When returning code or excerpts:

- line numbers should be easy to enable
- range headers should be explicit
- formatting should be stable

### Bounded

Every query tool should have sane limits and predictable truncation behavior.

### Honest

If a result is heuristic or partially ambiguous, say so.

## Concrete Backlog

This is the short, practical backlog I would hand to an execution agent.

### P0

- add `search_files`
- add `resolve_path`
- extend `search_text` with path/language/glob filters and context lines
- extend `get_file_content` with line numbers and `around_line`
- change `repo_outline` to include path context, not basename only

### P1

- extend `search_symbols` with path/language/limit filters
- add generated/test suppression metadata and defaults
- extend `get_file_content` with `around_match` and chunking
- add exact-symbol reference query flow

### P2

- add `trace_symbol`
- add `inspect_match`
- add prompt/resource support for the new exploration workflows

### P3

- refine ranking with module/path locality
- tune formatting and result caps from real usage telemetry

## Things Not To Do

- do not broaden the primary index to arbitrary text files just to chase parity
- do not turn the semantic code engine into a generic all-files in-memory indexer
- do not replace readable outputs with structured JSON-heavy envelopes
- do not introduce brittle client-specific magic instead of improving the core tool surface
- do not try to solve full semantic language-server equivalence before fixing path/search/read ergonomics

## Definition Of Success

Tokenizor is succeeding when the normal agent loop becomes:

1. `search_files`
2. `search_text` or `search_symbols` with scope
3. `get_file_content` or `inspect_match`
4. `trace_symbol`

And the agent does not feel the need to use shell tools except for:

- non-indexed assets
- git plumbing
- external system commands

That is the bar.

## Execution Guidance For AI Coding Agents

If you are implementing this backlog:

1. Do not tackle everything in one pass.
2. Start with path search, scoped text search, and better file content ergonomics.
3. Preserve current strengths, especially `get_context_bundle`.
4. Add tests before and during each capability expansion.
5. Keep outputs compact and model-friendly.
6. Prefer additive changes to the public surface before destructive renames.
7. Benchmark every new hot-path query.

Recommended first implementation slice:

- Phase 1
- Phase 2
- the line-numbered subset of Phase 3

That slice alone would materially change how often Tokenizor gets chosen over shell.
