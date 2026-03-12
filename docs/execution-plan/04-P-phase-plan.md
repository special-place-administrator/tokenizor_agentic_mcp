# Phase Plan

Derived from `TOKENIZOR_CONTEXT_AND_EXECUTION_PLAN.md` on 2026-03-11.
Source coverage: lines 1679-2059.

## Bespoke Phase Plan

This is the recommended execution sequence.

Each phase is intentionally dependency-ordered.
Do not skip ahead unless the required substrate already exists and is benchmarked.

## Research Gate Policy

Before coding begins for any phase that changes:

- query semantics
- ranking behavior
- index structure
- memory profile
- watcher behavior
- public tool contracts

the implementation agent should do a short explicit research pass first.

The research pass should answer:

- what exact user workflow is being improved
- which existing files/functions are the real edit points
- which design options exist
- which option best preserves speed, robustness, and reliability
- what the benchmark and regression risks are

Expected research output:

- a short written note or plan section
- candidate approaches considered
- chosen approach and why
- any risks or open questions

Do not skip research when the phase introduces real architectural or performance tradeoffs.

### Phase 0: Baseline, Safety, and Benchmark Harness

Goal:

- establish the current behavior and performance floor before changing the query surface

Depends on:

- nothing

Tasks:

- define benchmark scenarios for path lookup, text search, symbol lookup, reference lookup, and file reading
- capture baseline latency and output snapshots for the current tools
- add regression fixtures for repeated filenames, noisy common symbols, generated files, and mixed code/text repos
- define compatibility expectations for current high-value tools such as `get_context_bundle` and `get_file_context`

Outputs:

- reproducible benchmark harness
- baseline output fixtures
- explicit pass/fail thresholds for future phases

Acceptance:

- future phases can prove whether they preserved or improved the current floor

### Phase 1: Query Substrate and File Classification

Goal:

- create the internal substrate needed for fast path search, scoped filtering, and dual-lane retrieval

Depends on:

- Phase 0

Research tasks:

- inspect the current split between `src/protocol/tools.rs`, `src/protocol/format.rs`, `src/live_index/query.rs`, and any sidecar helpers to identify the real semantic duplication points
- evaluate candidate internal query-layer shapes and pick the smallest one that can support the coming phases
- evaluate whether path lookup should be backed by basename maps, path token indices, path trigrams, or a hybrid
- evaluate whether the non-code text lane should use lazy reads, bounded cache, or a lightweight side registry
- define what file classification can be done cheaply at index-time versus query-time

Tasks:

- add a shared internal query layer separate from formatting
- add file classification metadata such as `is_code`, `is_text`, `is_binary`, `is_generated`, `is_test`, and `is_vendor`
- add secondary indices for path/basename lookup
- define internal query option structs such as `PathScope`, `SearchScope`, `ResultLimit`, `ContentContext`, and `NoisePolicy`
- define the dual-lane retrieval boundary:
  semantic code lane vs lightweight plain-text lane

Outputs:

- shared query engine or search module
- file metadata and path lookup indices
- internal option types reused by tools

Acceptance:

- no public behavior regressions
- path and filter lookups have a fast internal representation
- non-code text support has a clear lightweight lane, not an accidental semantic expansion

### Phase 2: Path Discovery Tools

Goal:

- eliminate the biggest current shell escape hatch: file/path discovery

Depends on:

- Phase 1

Research tasks:

- collect real ambiguous-path cases from the repo and mixed-fixture repos
- compare ranking heuristics for basename-first, path-prefix-first, and fuzzy path matching
- determine what output shape gives enough disambiguation without bloating tokens

Tasks:

- implement `search_files`
- implement `resolve_path`
- upgrade `repo_outline` to use path-rich labels instead of ambiguous basenames
- add path-aware ranking rules
- ensure code-first ranking while still allowing non-binary text resolution for read workflows

Outputs:

- `search_files`
- `resolve_path`
- improved `repo_outline`

Acceptance:

- common file discovery tasks no longer require `rg --files`
- repeated `mod.rs` and `lib.rs` cases are handled cleanly
- output is deterministic and bounded

### Phase 3: Scoped Text Search

Goal:

- make `search_text` good enough to replace a large share of `rg`

Depends on:

- Phase 1
- benefits strongly from Phase 2, but does not require full completion of it

Research tasks:

- determine the minimum viable scope/filter contract that covers most real workflows without overcomplicating the public API
- compare candidate context rendering formats against token cost and readability
- determine whether the current prefiltering approach is sufficient once filters and context are added
- define how non-code text files should participate in scoped search without harming code-first defaults

Tasks:

- extend `search_text` with `path_prefix`, `glob`, `exclude_glob`, `language`, `limit`, `max_per_file`, `case_sensitive`, `whole_word`, `include_generated`, and `include_tests`
- add grep-style `before`, `after`, or `context`
- support both code files and lightweight non-binary text files
- standardize truncation and match grouping behavior
- ensure prefiltering stays cheap and does not regress hot-path speed

Outputs:

- upgraded `search_text`
- deterministic scoped search formatting

Acceptance:

- targeted search with scope and context can replace many `rg -n -C` workflows
- non-binary text search works through the text lane without polluting semantic ranking

### Phase 4: Read Surface Parity

Goal:

- make `get_file_content` and adjacent read tools capable enough to replace a large share of `Get-Content`

Depends on:

- Phase 1
- should follow Phase 2 and Phase 3 so path resolution and text hits can feed it cleanly

Research tasks:

- compare read-path options for non-code text support: lazy disk read, bounded cache, or lightweight in-memory registry
- determine the smallest stable content API that can cover range reads, around-line, around-match, and chunking
- identify what output format best balances token cost with readability for models
- verify whether `inspect_match` should be a separate tool or a mode of `get_file_content`

Tasks:

- extend `get_file_content` with line numbers, headers, `around_line`, `around_match`, `around_symbol`, `max_lines`, and chunking
- ensure reads work for both code files and non-binary text files
- add a lightweight text-lane content lookup path if needed
- define stable formatting for line-ranged and chunked reads
- add `inspect_match` if the upgraded `get_file_content` alone is not sufficient

Outputs:

- upgraded `get_file_content`
- optional `inspect_match`

Acceptance:

- agents can inspect code and non-code text files without shell fallback in common workflows
- the read path remains fast and bounded

### Phase 5: Exact Symbol Identity and Reference Precision

Goal:

- fix the biggest semantic precision gap: name-only reference lookup

Depends on:

- Phase 1

Research tasks:

- determine the best symbol identity strategy for the current index model
- define identity lifetime semantics and how follow-up tools should consume them
- compare stable symbol id vs `{path,name,kind}` as the public disambiguation contract
- identify migration risks for existing tool/resource/prompt consumers

Tasks:

- introduce stable symbol identity or equivalent exact-symbol addressing
- extend `search_symbols` and `get_symbol` outputs so follow-up calls can stay exact
- upgrade `find_references` to accept exact symbol identity or `{path,name,kind}` disambiguation
- ensure common-name queries like `new` no longer flood the user with irrelevant results

Outputs:

- stable symbol identity model
- upgraded `find_references`
- chainable exact-symbol outputs

Acceptance:

- exact reference navigation works reliably on ambiguous names
- current reference behavior is preserved where it is still useful, but precision is materially better

### Phase 6: Noise Suppression and Ranking Quality

Goal:

- keep results compact, trustworthy, and code-first

Depends on:

- Phase 1
- should follow Phases 2 through 5 so the ranking can be tuned against the richer tool surface

Research tasks:

- identify reliable generated/test/vendor heuristics that are cheap and language-agnostic enough
- gather noisy-result cases from current behavior to tune against
- compare whether suppression should be hard-hidden, demoted, or user-toggle-driven by tool

Tasks:

- apply generated/test/vendor suppression defaults where appropriate
- refine path-local and module-local ranking
- demote noisy generated artifacts in search and file discovery
- standardize ranking tie-breakers and overflow wording

Outputs:

- tuned ranking behavior
- trustworthy default suppression policy

Acceptance:

- common-name and common-path queries produce visibly higher-signal results
- result ordering is deterministic and testable

### Phase 7: One-Call Exploration Tools

Goal:

- shorten common multi-call agent workflows once the substrate is strong enough

Depends on:

- Phase 4
- Phase 5
- strongly benefits from Phase 6

Research tasks:

- compare `trace_symbol` against the current `get_context_bundle` to decide whether it wraps, replaces, or coexists
- determine whether `inspect_file` is truly needed after upgraded `get_file_content` and `get_file_context`
- define which compound workflows actually save calls instead of just creating overlapping tools

Tasks:

- implement `trace_symbol` as the preferred one-call semantic investigation surface
- implement or finalize `inspect_match`
- implement `inspect_file` if large-file code exploration still feels too manual
- ensure these tools reuse the same query substrate rather than inventing parallel logic

Outputs:

- `trace_symbol`
- `inspect_match`
- `inspect_file`

Acceptance:

- the common loop of resolve -> read -> trace becomes shorter than the equivalent shell workflow
- the new tools are clearly better than chaining older primitives manually

### Phase 8: Prompt, Guidance, and Client Routing Polish

Goal:

- ensure models actually use the improved capabilities correctly

Depends on:

- at least Phase 2 through Phase 5
- ideally Phase 7

Research tasks:

- identify which client guidance surfaces are actually documented and safe to rely on
- validate that the routing guidance matches the final implemented tool surface rather than the intended one
- compare example flows for Codex and Claude so guidance stays realistic

Tasks:

- update MCP prompts and resources to reflect the routing logic
- add AGENTS/init guidance for tool selection flows
- update README examples to demonstrate the intended path-first, text-search, symbol, and read flows
- keep Codex/Claude guidance aligned with the actual tool surface

Outputs:

- updated prompts/resources
- updated client guidance
- updated README

Acceptance:

- a future agent can infer the right tool flow without rediscovering it from scratch
- documentation matches the real tool behavior

### Phase 9: Evaluate Whether Further Structural Replacement Is Justified

Goal:

- decide, based on evidence, whether any deeper architectural replacement is needed

Depends on:

- enough earlier phases implemented to judge the current architecture fairly

Research tasks:

- review all benchmark deltas and regression outcomes across implemented phases
- identify any remaining bottlenecks that cannot be solved incrementally
- compare targeted replacement options only where the data shows the current structure is limiting

Tasks:

- review benchmark deltas across the completed phases
- identify any remaining bottlenecks caused by the current foundation
- decide whether targeted subsystem replacement is warranted

Outputs:

- explicit keep/refactor/replace decision for any remaining weak subsystem

Acceptance:

- architectural replacement, if chosen, is justified by evidence rather than intuition

