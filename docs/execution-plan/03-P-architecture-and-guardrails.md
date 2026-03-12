# Architecture And Guardrails

Derived from `TOKENIZOR_CONTEXT_AND_EXECUTION_PLAN.md` on 2026-03-11.
Source coverage: lines 1284-1678.

## Architecture Refactors Required

If speed, robustness, and reliability are the top priorities, the query surface cannot keep growing by only adding optional fields to formatter calls.

The system needs a small, intentional refactor.

This is a refactor-in-place plan, not a "start over" plan.

That is the default posture, not a hard law.

If a measured architectural replacement is genuinely superior, do it.

The rule is:

- prefer incremental change by default
- allow replacement when the evidence is strong
- reject both blind rewrites and blind preservation

Every architectural change should satisfy all of the following:

- preserves or improves current hot-path latency
- preserves or improves determinism
- preserves existing tool behavior unless explicitly upgraded
- ships with focused regression tests
- is measurable with before/after benchmarks

## Refactor 1: Build a Real Query Layer

Create a shared internal query layer between protocol tools and formatting.

Current risk:

- tool handlers are thin, which is good
- but behavior is still spread across protocol input parsing, formatting, sidecar helpers, and live-index internals

Target:

- one query module that resolves scope, ranking, filtering, and exact identity
- formatting becomes presentation only

Recommended files:

- add `src/live_index/search.rs` or `src/live_index/query_engine.rs`
- keep `src/protocol/format.rs` focused on rendering
- keep `src/protocol/tools.rs` focused on input validation and dispatch

Why this matters:

- faster feature growth without duplicating logic
- easier correctness testing
- lower chance of tool/resource/hook divergence

## Refactor 2: Add Secondary In-Memory Indices

The current index is already fast, but the replacement goal needs more than raw file and reverse symbol maps.

Add:

- `files_by_basename`
- `files_by_dir_component`
- path trigram or token index
- `symbols_by_id`
- `symbols_by_qualified_name`
- file classification metadata
- a lightweight non-binary text file lane or registry, separate from the semantic hot path where practical

Why this matters:

- path search and exact symbol follow-up become O(1) or close to it
- ranking becomes deterministic and cheap
- filters stop being a formatting concern
- non-code text reading does not need to pollute the semantic code index

Likely files:

- `src/live_index/store.rs`
- `src/live_index/query.rs`
- `src/live_index/trigram.rs`

## Refactor 3: Stable Symbol Identity

This is the most important structural change after path search.

Current problem:

- public reference lookup is name-driven
- common names become noisy and unreliable

Target:

- every indexed symbol gets a stable symbol id for the life of the loaded snapshot
- exact-symbol tools can use the id directly
- search results expose enough identity for exact follow-up calls

Why this matters:

- precision
- repeatability
- dramatically better behavior on common names

## Refactor 4: Snapshot-Based Read Path

Prioritize read stability over cleverness.

Recommended rule:

- query execution should work from an immutable snapshot of the current index state
- formatting should happen after the minimum needed data has been copied out

Why this matters:

- reduced lock hold times
- fewer edge cases during watcher-driven mutation
- clearer performance behavior under concurrent tool calls

This is especially important once path search and richer filters get added.

## Refactor 5: Deterministic Result Contracts

For reliability, the same query on the same snapshot should produce the same order every time.

Standardize:

- ranking tie-breakers
- truncation rules
- overflow wording
- path display format

Why this matters:

- agents reason better on stable outputs
- tests become simpler and more trustworthy

## Refactor 6: File Classification At Index Time

Do not detect generated/test/vendor noise on every query.

Instead:

- classify files when indexing or reindexing
- store lightweight booleans or tags on `IndexedFile`

Examples:

- `is_generated`
- `is_test`
- `is_vendor`
- `is_code`
- `is_text`
- `is_binary`

Why this matters:

- faster query path
- simpler filter logic
- more reliable default ranking
- explicit separation of semantic files from plain-text files

## Refactor 8: Dual-Lane Retrieval Model

If full `Get-Content` replacement is a real goal, the architecture should likely split retrieval into two lanes.

### Lane 1: Semantic code lane

Applies to:

- source files in supported languages

Features:

- parsing
- symbols
- references
- semantic ranking
- `search_symbols`
- `find_references`
- `get_context_bundle`

### Lane 2: Plain-text content lane

Applies to:

- non-binary text files that are not semantically parsed

Features:

- path discovery
- plain-text search
- file content reading
- chunking and line-numbered excerpts
- lightweight metadata or caching rather than mandatory semantic-style full indexing

Examples:

- JSON
- TOML
- YAML
- Markdown
- shell scripts
- logs
- config files

Why this is likely the correct model:

- it preserves code-first intelligence
- it enables true `Get-Content` replacement
- it avoids flooding semantic navigation with non-code noise
- it keeps performance and reliability easier to reason about

Implementation preference:

- do not force all text files into the same fully-hot in-memory treatment as code
- prefer bounded caching, lazy reads, or a separate lightweight text registry when that preserves speed and token-saving behavior better

## Refactor 7: Shared Query Options Structs

As the query surface grows, ad hoc per-tool option structs will drift.

Introduce reusable internal option types such as:

- `PathScope`
- `SearchScope`
- `ResultLimit`
- `ContentContext`
- `NoisePolicy`

Why this matters:

- less duplicated validation
- easier expansion across tools
- more consistent user-facing behavior

## Performance Guardrails

These should be treated as design constraints, not aspirations.

- no disk I/O on the hot read path
- no shelling out for normal code search or code reading
- no regex scan across every file when a cheaper prefilter is available
- no repeated recomputation of generated/test classification on queries
- no large lock hold during formatting
- no unstable ranking rules
- no speculative rewrite without baseline and post-change benchmark evidence
- no acceptance of meaningful speed regressions unless they unlock clearly superior overall capability and the trade is explicitly intentional
- no blanket expansion that forces all non-binary text into the same memory and watcher cost profile as semantic code files

## Reliability Guardrails

These should be explicit in the implementation plan.

- fail clearly when a query is ambiguous
- expose exact identity when ambiguity matters
- keep output bounded and predictable
- preserve deterministic behavior under watcher updates
- do not let hook/resource/tool implementations drift apart semantically
- keep tests for noisy-symbol regressions
- preserve proven current behaviors unless an intentional compatibility note says otherwise
- preserve current working behavior unless the replacement behavior is clearly better and validated

## Recommended Architectural Priority Order

If the goal is speed, robustness, and reliability above all else, the order should be:

1. secondary in-memory indices for path and exact symbol lookup
2. a shared query layer
3. exact symbol identity
4. scoped search and better read ergonomics
5. one-call exploration tools

Do not invert that order.

Fancy tools built on a weak query core will feel impressive for a week and flaky forever.

## The Four Strategic Goals

If an implementation agent has to optimize for only a few things, optimize for these:

### Goal 1: Path Discovery

Tokenizor must be able to replace most `rg --files` style path lookup for relevant project text files, while still ranking code first for semantic workflows.

Minimum useful outcome:

- `search_files`
- `resolve_path`
- path-rich output labels

### Goal 2: Scoped Search

Tokenizor must be able to replace a large share of targeted `rg` queries for code, and eventually plain-text project search for non-binary files.

Minimum useful outcome:

- `path_prefix`
- `glob`
- `language`
- result limits
- context lines

### Goal 3: Exact Reference Navigation

Tokenizor must stop relying on name-only reference lookup for ambiguous symbols.

Minimum useful outcome:

- stable symbol identity or equivalent exact-symbol input
- exact-symbol `find_references`
- chainable outputs for follow-up navigation

### Goal 4: Read Surface Parity

Tokenizor must be able to replace a large share of `Get-Content` style reading, which means it cannot stay source-only forever.

Minimum useful outcome:

- line-numbered `get_file_content`
- `around_line`
- `around_match`
- chunking or equivalent progressive file reading
- `inspect_match` or equivalent read-ready local context tool
- support for non-binary text files, not only source files

Important distinction:

- semantic features do not need to apply to all text files
- raw reading and plain text search should apply to all non-binary text files
- non-code text support should be implemented in the lightest reliable way that does not undermine the original in-memory semantic code mission

## Safe Execution Strategy

This is the recommended implementation discipline for any AI coding agent working this plan.

### Rule 1: Prefer additive slices

Add new indices and query helpers behind the current tool surface first.

Do not replace working behavior until the replacement is benchmarked and tested.

An improvement does not need to complete the whole vision to be worth shipping.

### Rule 2: Keep old and new behavior comparable during migration

For major query changes:

- keep the old implementation reachable during development
- compare outputs on the same fixture repos
- switch only after accuracy and speed are verified

### Rule 3: Measure before claiming improvement

Before merging a substantial search or read-path change, capture:

- query latency
- result count
- result ordering
- memory impact if meaningful
- baseline compatibility with current useful behavior

### Rule 4: Protect the read path

Do not make query-time formatting or filtering depend on expensive recomputation.

Push cheap metadata and indices into index-time or reindex-time work where possible.

### Rule 5: Keep the mental model simple

The end state should still be easy to explain:

- in-memory code index
- incremental updates
- shared query layer
- thin tool/resource/prompt wrappers

If the design gets harder to explain than that, it is probably drifting.

### Rule 6: Prefer usefulness over theoretical completeness

Do the highest-value improvements first.

Examples:

- path search that solves most real cases is worth shipping
- line-numbered file content without full chunk orchestration is worth shipping
- scoped search without perfect fuzzy ranking is worth shipping

The product should become better in steady steps, not wait for a perfect end state.

### Rule 7: Protect the baseline while aiming at the four goals

The correct decision rule is:

- first, do not casually regress what already works
- second, push toward the four strategic goals
- third, if full realization is unrealistic, still ship the best net improvement available

