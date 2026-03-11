# Tokenizor Context And Execution Plan

Status: research-backed execution document
Date: 2026-03-11
Audience: AI coding agents and maintainers working on `tokenizor_agentic_mcp`

## Purpose

This document turns product intent into an implementation-grade backlog.

It is intended to serve two roles at the same time:

- context document: explains what Tokenizor is trying to become, what constraints matter, and what tradeoffs are acceptable
- planning document: gives a dependency-ordered execution plan that an AI coding agent can follow

The target is not "make Tokenizor somewhat nicer."

The target is:

- make Tokenizor the default code exploration surface for AI agents
- make it good enough to replace most `rg` usage for code search
- make it good enough to replace most `Get-Content` usage for code reading
- stay code-first for semantic intelligence while adding a bounded non-binary text reading/search lane where it materially improves usability

This is intentionally opinionated. It is based on:

- direct usage of the MCP tools on a real Rust workspace
- direct usage of the MCP tools on the Tokenizor repo itself
- review of the current README and planning docs
- inspection of the current implementation in the query, protocol, sidecar, and hook layers

## How To Use This Document

An implementation agent should read this document in two passes.

### Pass 1: Context pass

Read these sections first:

- Purpose
- Primary Success Principle
- Success Hierarchy
- Code-First, Not Code-Only
- Core Product Stance
- What Is Already Strong
- Use The Foundation Pragmatically
- Performance Guardrails
- Reliability Guardrails
- The Four Strategic Goals

This establishes the product intent and the constraints that should govern technical decisions.

### Pass 2: Execution pass

Then read these sections:

- Recommended Final Tool Set
- Tool Routing Guidance
- Recommended Decision Flow
- Bespoke Phase Plan
- Test Strategy
- Suggested Data Model Additions
- Recommended Output Standards

This establishes what to build, in what order, and how to verify it.

### Working rule

If the plan and the context ever seem to conflict:

- trust the success hierarchy
- protect the current baseline
- optimize for the four strategic goals
- choose the path that produces the better overall system

## Primary Success Principle

The main goal is to make Tokenizor better than it is today without breaking what already works.

That means:

- preserve existing strengths
- ship additive improvements
- accept partial wins that make the product more useful
- avoid large risky rewrites unless they are necessary and proven

Full replacement of `rg` and `Get-Content` remains the long-term direction.

But a change is still successful if it:

- makes exploration faster
- makes results more precise
- reduces shell escapes
- preserves speed, robustness, and reliability

This principle should govern prioritization and implementation decisions.

Important nuance:

The current foundation is not sacred.

It should be preserved when it is still the best base for progress.

It should be replaced when a new design is demonstrably better overall on the metrics that matter:

- speed
- robustness
- reliability
- usefulness
- maintainability

Do not preserve architecture out of loyalty.
Preserve it only when it is still the best option.

## Success Hierarchy

Use this priority order for all decisions.

### Tier 1: Preserve current baseline

Any serious implementation effort should preserve current useful behavior and current performance characteristics unless there is strong measured evidence that a trade is worth it.

This means protecting:

- current working tool behavior
- current read-path speed
- current query determinism
- current watcher-driven freshness
- current daemon/session stability

The baseline should be treated as the floor.

### Tier 2: Achieve the four strategic capability goals

These are the main product goals regardless of how they are implemented:

1. first-class path discovery so agents can find the right file without shell help
2. scoped search so agents can search code precisely instead of globally flooding context
3. exact-symbol reference navigation so common-name queries stop collapsing into noise
4. read-surface parity so agents can inspect the right code slice without falling back to `Get-Content`

Those four matter more than whether the implementation is a pure extension of the current architecture or a measured replacement of part of it.

### Tier 3: Fallback to net improvement if full target is unrealistic

If there is no realistic path to full shell-replacement behavior in the near term, the fallback is not failure.

The fallback is:

- improve the current app
- reduce shell escapes where possible
- make the most common exploration workflows faster and more reliable

That fallback is acceptable as long as the product clearly becomes better than it is now.

## North Star

An agent should be able to answer these questions without reaching for shell tools:

1. Which file is the one I actually want?
2. Show me the exact chunk I need from that file.
3. Find all code matches, but only in the relevant slice of the repo.
4. Show me the real symbol I mean, not every homonym in the repo.
5. Show me callers, callees, and dependents for that exact symbol.
6. Hide generated and irrelevant noise unless I explicitly ask for it.
7. Give me the result in a format that is immediately usable by a model.

If Tokenizor cannot do those seven things smoothly, agents will continue falling back to `rg` and `Get-Content`.

## Code-First, Not Code-Only

For `rg` replacement, code-first is sufficient.

For complete `Get-Content` replacement, code-only is not sufficient.

The correct target is:

- code files get semantic parsing, symbols, references, and code-first ranking
- non-binary text files get path discovery, plain-text search, and reliable file reading through a lighter secondary lane
- binary files remain out of scope or explicitly unsupported

This keeps Tokenizor code-centric where it matters while still allowing true shell replacement for reading text files.

It does not require treating every text file as a first-class semantic or always-hot in-memory object.

## Core Product Stance

Keep these as explicit non-negotiables:

- Code-centric indexing is correct. Do not add markdown/docs/config noise to the primary retrieval plane.
- Human-readable plain text output is correct. Do not regress into JSON-heavy responses.
- Shared daemon-backed state is correct. Keep project state warm and query latency low.
- Cross-reference-aware retrieval is the differentiator. Lean into it.

Do not optimize for "support everything."
Optimize for "be the fastest trustworthy path to the next code decision."

## What Is Already Strong

These parts are already genuinely useful:

- `get_context_bundle` is high value. It is one of the first tools I reach for once I know the symbol.
- `get_file_context` is a good code-reader primitive for agent workflows.
- `find_dependents` and language-aware reference extraction already go beyond bare grep.
- `search_text` already supports regex and OR-style term search.
- `search_symbols` already supports a kind filter and reasonably readable ranking.
- The daemon/session/runtime direction is correct.
- The code-centric index is fast enough to feel interactive.

Do not throw this away in pursuit of a larger rewrite.

## Use The Foundation Pragmatically

This project already has the right core shape.

Do not destabilize it with a broad rewrite unless benchmarks prove the current shape cannot support the required capabilities.

The following should be treated as current strengths, not untouchable sacred structures:

- the in-memory live index model
- incremental watcher-driven freshness
- the daemon/session runtime model
- the plain-text model-friendly formatter direction
- the existing cross-reference extraction base
- `get_context_bundle`, `get_file_context`, and `find_dependents` as proven value surfaces

The correct strategy is:

- keep the foundation
- add missing indices
- add a cleaner query layer
- enrich the public tools
- benchmark every step

The also-correct strategy, when warranted, is:

- replace a working subsystem if a measured redesign is clearly better overall
- migrate in stages
- keep compatibility or comparability during the transition
- switch only after benchmarks and regression tests prove the improvement

The mandatory constraint is:

- do not lose current functionality or performance casually on the way to the three strategic goals

The incorrect strategy is:

- rewrite major subsystems first
- proliferate tools without strengthening the core query path
- regress determinism or latency while chasing features

If a change makes Tokenizor materially more useful while preserving or improving reliability, it is a good change even if it does not fully achieve the final replacement vision yet.

If a structural replacement produces a better overall system, that replacement is valid and should not be blocked just because the old foundation was "working."

## Why I Still Reach For Shell Tools

These are the highest-value blockers between the current product and the stated replacement goal.

### Blocker 1: No file/path search primitive

I still need `rg --files` because Tokenizor does not give me a first-class way to resolve paths.

Typical agent questions:

- find `resources.rs`
- find all files under `src/protocol` containing `resource`
- resolve whether I want `mod.rs` in `src/protocol` or `src/parsing`
- find files with `hook` in the name, excluding tests

Without `search_files` or `resolve_path`, I still need shell.

### Blocker 2: Search scope is too broad

Current `search_text` and `search_symbols` are useful, but still too global.

Missing filters that matter in real work:

- `path_prefix`
- `glob`
- `language`
- `exclude_glob`
- `include_generated`
- `include_tests`
- `limit`
- `max_per_file`
- `case_sensitive`
- `whole_word`

If I cannot scope search, I will keep using `rg`.

### Blocker 3: `find_references` is too name-driven

`find_references(name="new")` on the Tokenizor repo produced massive noisy output.

That is expected. A name-only query cannot replace structural navigation for common symbols.

Tokenizor needs exact-symbol reference lookup, not just bare-name lookup.

### Blocker 4: `get_file_content` is too raw

Current `get_file_content` is effectively a memory-backed raw slice.

That is necessary but not sufficient to replace `Get-Content`.

It needs:

- optional line numbers
- explicit echoed path and selected range
- `around_line`
- `around_match`
- `around_symbol`
- chunking support
- helpful "next chunk" workflows for large files

### Blocker 5: Result formatting is still missing grep/read ergonomics

The output is readable, but not always immediately decisive.

Examples:

- `repo_outline` uses filenames instead of path-rich labels, which becomes ambiguous with repeated `mod.rs` and `lib.rs`
- `search_text` does not support before/after context lines
- `search_symbols` cannot easily de-duplicate or collapse noisy repeated hits
- generated code can dominate some searches

### Blocker 6: Missing path-to-symbol workflow

The ideal agent loop is:

1. resolve file
2. inspect file chunk
3. inspect enclosing symbol
4. inspect exact references

Tokenizor has most of the pieces, but not the shortest route through them.

## Current Code Observations

These observations are directly based on the current codebase.

### Query/input surface

Relevant files:

- `src/protocol/tools.rs`
- `src/protocol/format.rs`
- `src/live_index/query.rs`

Notable facts:

- `SearchSymbolsInput` currently exposes only `query` and optional `kind`.
- `SearchTextInput` currently exposes only `query`, `terms`, and `regex`.
- `FindReferencesInput` currently exposes only `name` and optional `kind`.
- `GetFileContentInput` currently exposes only `path`, `start_line`, and `end_line`.

This is the main reason the tool layer cannot yet replace shell ergonomics.

### Formatting layer

Relevant file:

- `src/protocol/format.rs`

Notable facts:

- `search_symbols_result_with_kind` caps results at 50 but lacks path/language filters.
- `search_text_result_with_options` supports regex and multi-term OR, but no context lines and no scope filters.
- `file_content` returns raw text or line-ranged text, but no line numbers or header.
- `repo_outline` displays filenames only, which creates ambiguity.
- `context_bundle_result` is already strong and should be preserved and extended, not replaced.

### Sidecar/hook layer

Relevant files:

- `src/sidecar/handlers.rs`
- `src/cli/hook.rs`

Notable facts:

- `outline_text` is well-suited for hook-time lightweight enrichment.
- hook rendering is intentionally budgeted and compact.
- hook-derived intelligence is useful, but Codex still depends mainly on explicit tools/resources/prompts.

This means standard MCP tools must become good enough on their own, not just as sidecar helpers.

### Reference/query core

Relevant file:

- `src/live_index/query.rs`

Notable facts:

- cross-reference handling already has substantial language-specific logic
- `find_dependents_for_file` is meaningfully smarter than import-only matching
- `find_references_for_name` is still fundamentally name-oriented

This is a strong base, but exact symbol identity needs to be brought to the public tool surface.

## Strategic Workstreams

The work should be broken into seven workstreams.

## Workstream A: Path Discovery Parity

Goal: eliminate the need for `rg --files` in normal code exploration.

### Deliverables

- add `search_files`
- add `resolve_path`
- add `path_prefix` support to relevant tools

### Proposed tool additions

#### `search_files`

Purpose:

- fuzzy or substring path lookup across indexed code files first, with optional support for non-binary text files

Suggested input:

- `query: String`
- `path_prefix: Option<String>`
- `glob: Option<String>`
- `language: Option<String>`
- `limit: Option<u32>`
- `include_generated: Option<bool>`
- `include_tests: Option<bool>`

Suggested behavior:

- rank exact filename match first
- then exact basename prefix match
- then full relative path prefix match
- then substring/fuzzy path match

### Acceptance criteria

- I can find `resources.rs` without shell
- I can narrow to `src/protocol`
- repeated names like `mod.rs` are returned with enough path context to disambiguate
- tests/generated results are suppressed by default unless requested
- non-code text files can still be resolved when the user is trying to read rather than navigate semantically

### Likely files to modify

- `src/protocol/tools.rs`
- `src/protocol/format.rs`
- `src/live_index/query.rs`
- `src/live_index/store.rs`

## Workstream B: Grep Parity

Goal: make `search_text` good enough that agents prefer it over `rg` for indexed code.

Longer-term extension:

- plain-text search should also work for non-binary text files even if semantic symbol search remains code-only

### Deliverables

- scoped search
- context lines
- better limits
- better noise suppression

### Extend `search_text`

Add:

- `path_prefix`
- `glob`
- `exclude_glob`
- `language`
- `case_sensitive`
- `whole_word`
- `before`
- `after`
- `context`
- `limit`
- `max_per_file`
- `include_generated`
- `include_tests`

### Ranking/output changes

- group by file, but allow match-level ranking when query is selective
- show context blocks instead of bare lines when requested
- optionally collapse repeated adjacent hits into one block
- show relative paths consistently

### Acceptance criteria

- I can search within `src/protocol`
- I can request 2 lines of context around each match
- I can exclude generated code and tests by default
- I can cap output to avoid flooding context
- I do not need `rg -n -C 2` for normal indexed code search

### Likely files to modify

- `src/protocol/tools.rs`
- `src/protocol/format.rs`
- `src/live_index/query.rs`
- `src/live_index/trigram.rs`

## Workstream C: Read Parity

Goal: make `get_file_content` good enough that agents do not need `Get-Content`.

To fully satisfy this goal, `get_file_content` must work for non-binary text files, not just indexed source files.

This does not imply that all non-code text should be fully indexed and kept hot in memory like semantic code data.

The preferred approach is:

- keep the semantic code lane as the main always-hot retrieval surface
- add a lighter text-content lane for non-binary files
- allow lazy loading, bounded caching, or a separate lightweight text registry if benchmarks support it

### Extend `get_file_content`

Add:

- `show_line_numbers`
- `header`
- `around_line`
- `around_match`
- `around_symbol`
- `context`
- `max_lines`
- `chunk_index`
- `chunk_count_hint`

### Recommended behavior

The tool should support at least four modes:

1. raw full file
2. explicit line range
3. contextual view around a line or match
4. chunked file view for very large files

And it should support two content classes:

1. semantic code files
2. non-binary plain-text files handled through a lightweight retrieval path

### Output requirements

- always identify the file path
- identify the range being shown
- optionally include 1-based line numbers
- keep the formatting stable enough for downstream reasoning

### Acceptance criteria

- I can ask for lines 120-180 with line numbers
- I can ask for 20 lines around line 530
- I can ask for the chunk containing a symbol or text match
- large files can be read progressively without shell
- plain text files such as config, JSON, TOML, YAML, Markdown, logs, and scripts can be read without shell fallback

### Likely files to modify

- `src/protocol/tools.rs`
- `src/protocol/format.rs`

## Workstream D: Exact Symbol Identity

Goal: make reference navigation precise enough that common names do not collapse into noise.

### Deliverables

- exact symbol lookup inputs
- stable symbol identity
- symbol-qualified reference queries

### Replace or extend `find_references`

The current public query is too weak for common names.

Add one of these models:

#### Option 1: richer lookup input

- `path`
- `name`
- `kind`
- `kind_filter_for_refs`

#### Option 2: stable symbol id

- `symbol_id`
- optional fallback `{path,name,kind}`

I strongly prefer stable symbol IDs in the internal model and a path/name/kind fallback in the public surface.

### Related changes

- allow `get_symbol` and `get_context_bundle` to return the symbol id
- allow `search_symbols` results to expose enough identity to feed exact follow-up tools
- rank exact in-file or same-module hits above global homonyms

### Acceptance criteria

- `find_references` on `new` is not a flood
- I can trace references for an exact constructor/function/class without guessing
- follow-up tool calls can chain without shell disambiguation

### Likely files to modify

- `src/domain/index.rs`
- `src/live_index/store.rs`
- `src/live_index/query.rs`
- `src/protocol/tools.rs`
- `src/protocol/format.rs`

## Workstream E: Noise Suppression and Ranking

Goal: make results feel curated rather than merely comprehensive.

### Deliverables

- generated-code suppression
- test/vendor suppression by default where sensible
- better path-aware ranking

### Required ranking rules

- exact path or filename matches beat generic substring hits
- non-generated source beats generated source
- main code beats tests by default
- path-local hits beat distant global hits when a path scope exists

### Suggested implementation

Maintain lightweight file metadata in the index:

- `is_generated`
- `is_test`
- `is_vendor`
- `module_path`
- `basename`

The metadata does not need to be perfect to be useful.

### Acceptance criteria

- common-name queries do not start with generated code
- repeated generated artifacts do not drown the signal
- path-aware narrowing visibly improves ranking

### Likely files to modify

- `src/live_index/store.rs`
- `src/discovery/mod.rs`
- `src/protocol/format.rs`
- `src/live_index/query.rs`

## Workstream F: One-Call Exploration Primitives

Goal: reduce multi-call friction for common agent workflows.

### Add `trace_symbol`

Purpose:

- one-call exact symbol exploration

Suggested output:

- symbol signature/body
- enclosing path/module
- callers
- callees
- type usages
- dependents
- nearby sibling symbols

This can be built on top of existing `get_context_bundle`.

### Add `inspect_match`

Purpose:

- convert a search hit into a read-ready local context block

Suggested input:

- `path`
- `line`
- optional `context`

Suggested output:

- file path
- line-numbered excerpt
- enclosing symbol if available

### Add `inspect_file`

Purpose:

- file-outline-plus-readable-chunks surface for large-file exploration

This is effectively a stronger, tool-grade evolution of `get_file_context`.

### Acceptance criteria

- an agent can go from "search result" to "actionable local context" in one follow-up call
- the tool chain is shorter than `rg` plus `Get-Content`

### Likely files to modify

- `src/protocol/tools.rs`
- `src/protocol/format.rs`
- `src/live_index/query.rs`
- `src/sidecar/handlers.rs`

## Workstream G: Agent Adoption and Client Maximization

Goal: make agents actually choose Tokenizor more often.

### Deliverables

- stronger Codex-facing MCP ergonomics
- prompt/resource surfaces that drive the new path/search/read primitives
- explicit AGENTS guidance that pushes exact flows

### Recommendations

- add prompts specifically for:
  - "locate file"
  - "trace symbol"
  - "inspect hotspot"
- expose path/search/read improvements through resources where helpful
- keep Codex integration honest: no hidden hook magic assumptions

### Acceptance criteria

- the recommended prompt flow for exploration uses Tokenizor tools first
- new tools are discoverable from MCP prompts/resources, not only from README text

### Likely files to modify

- `src/protocol/prompts.rs`
- `src/protocol/resources.rs`
- `src/cli/init.rs`
- `README.md`

## Recommended New Public Tools

Do not add tools casually.

Every new tool should remove a real shell escape hatch or shorten a common agent workflow.

I would add only the following new tools at first.

## Tool Surface Philosophy

The goal is not to accumulate lots of overlapping tools.

The goal is to expose a small set of reliable primitives plus clear routing guidance so an agent can choose the right tool quickly and consistently.

Preferred shape:

- keep the core semantic tools small and sharp
- expand existing tools where that preserves clarity
- add new tools only when they remove a real multi-step shell workflow
- document when each tool should be used

Bad shape:

- many overlapping tools with vague differences
- tool choice depending on hidden implementation details
- shell parity achieved through complexity instead of a clean retrieval model

### `search_files`

Reason:

- replaces `rg --files` style path discovery for indexed code

Why it matters:

- this is the single biggest current gap

### `resolve_path`

Reason:

- converts an ambiguous file hint into one exact indexed path

Why it matters:

- agents frequently know part of a path, not the full path
- this tool can be low-output and deterministic

### `inspect_match`

Reason:

- converts a search hit into a read-ready excerpt with line numbers and enclosing symbol

Why it matters:

- it replaces the common `rg` then `Get-Content` follow-up loop

### `trace_symbol`

Reason:

- upgrades the exact-symbol workflow into one call

Why it matters:

- it is the structural navigation equivalent of a great IDE "peek graph" action

### `inspect_file`

Reason:

- gives a stronger file exploration surface than raw content or a minimal outline

Why it matters:

- large files need a guided entry point, not just a blob of text

## Tools To Enrich Instead Of Replacing

Not everything needs a new name.

These existing tools should be upgraded rather than duplicated:

### `search_text`

Keep the name. Add filters, context lines, and limits.

### `search_symbols`

Keep the name. Add scope filters, ranking improvements, and exact follow-up identity.

### `get_file_content`

Keep the name. Add line numbers, context modes, and chunking.

### `find_references`

Keep the name, but add exact-symbol identity support.

This reduces churn for existing clients while still materially improving capability.

## Tool Routing Guidance

If Tokenizor is meant to replace shell exploration, an agent should not have to invent its own tool-selection strategy every session.

The product should define the optimal routing rules.

### Use `search_files` or `resolve_path` when:

- the user knows part of a filename or path
- the agent needs to find the right file before reading
- multiple same-named files need disambiguation

### Use `search_text` when:

- the user is searching for raw text
- the target may be code or non-code text
- scope and context matter more than symbol identity

### Use `search_symbols` when:

- the user is searching for a definition-like entity
- semantic/code-first ranking is desirable
- the next step is likely exact symbol navigation

### Use `get_file_content` when:

- the agent already knows the target file
- the need is raw reading, line ranges, or chunked viewing
- the file may be non-code text

### Use `get_file_context` or `inspect_file` when:

- the file is code
- the agent wants structure plus important references
- a better entry point than raw content is needed

### Use `find_references` when:

- the symbol is already disambiguated
- the task is caller/import/type-usage navigation

### Use `trace_symbol` or `get_context_bundle` when:

- the agent wants the fastest route from symbol to surrounding semantic context
- definition, callers, callees, and dependents are likely all relevant

### Use `inspect_match` when:

- a text hit has already been found
- the next action is to inspect the local excerpt, not the whole file

## Recommended Decision Flow

This is the intended high-level decision tree for future agent guidance.

### Path-first flow

Use when the location is unclear.

1. `search_files`
2. `resolve_path`
3. `get_file_content` or `get_file_context`

### Text-search flow

Use when the user provides a string, phrase, config key, or error text.

1. `search_text`
2. `inspect_match` or `get_file_content`
3. optional semantic follow-up if the hit is inside code

### Symbol flow

Use when the user names a function, class, method, type, or module.

1. `search_symbols`
2. exact symbol disambiguation
3. `trace_symbol` or `find_references`

### Read flow

Use when the file is already known.

1. `get_file_content`
2. `around_line` or `around_match` style view
3. optional `get_file_context` if semantic structure matters

### Investigation flow

Use when the task is "understand what this does" or "find the impact."

1. resolve the file or symbol
2. read the local excerpt
3. `trace_symbol`
4. use scoped search only if more breadth is needed

## Prompt And Guidance Implications

These routing rules should not live only in this document.

They should eventually inform:

- MCP prompts
- AGENTS/CLAUDE guidance blocks
- README usage examples
- any client initialization guidance

The model should be gently steered toward the optimal flow rather than forced to rediscover it every time.

## Recommended Final Tool Set

This section defines the intended steady-state tool surface.

It is not necessarily the exact first implementation slice.
It is the target shape the product should converge toward.

### `search_files`

Lane:

- shared discovery lane

Purpose:

- resolve filenames, partial paths, and ambiguous path hints

Use when:

- the path is not known yet
- the user names a file, folder, or partial path
- multiple same-named files may exist

Do not use when:

- the exact file path is already known
- the user is searching for text content, not a path

Likely follow-up:

- `resolve_path`
- `get_file_content`
- `get_file_context`

### `resolve_path`

Lane:

- shared discovery lane

Purpose:

- turn an ambiguous path hint into one exact project path

Use when:

- `search_files` returned a small candidate set
- the next step needs one deterministic path

Do not use when:

- the path is already exact
- the ambiguity is semantic rather than path-based

Likely follow-up:

- `get_file_content`
- `get_file_context`
- `inspect_file`

### `search_text`

Lane:

- text lane with code-aware scoping

Purpose:

- raw text search across code and non-binary text files

Use when:

- the query is a phrase, key, literal, error message, config entry, or snippet
- context lines and scoping matter
- the target may be non-code text

Do not use when:

- the user is clearly asking for a symbol/definition
- the next step depends on exact semantic identity first

Likely follow-up:

- `inspect_match`
- `get_file_content`
- `trace_symbol` if the hit is inside code

### `search_symbols`

Lane:

- semantic code lane

Purpose:

- find definitions and code entities by name

Use when:

- the user names a function, class, method, struct, enum, module, or similar symbol
- semantic ranking is more important than raw text matching

Do not use when:

- the target may be in non-code text
- the query is mostly a literal or prose string

Likely follow-up:

- exact symbol disambiguation
- `trace_symbol`
- `find_references`

### `get_file_content`

Lane:

- text lane

Purpose:

- line-ranged, chunked, or contextual reading of file content

Use when:

- the exact path is known
- the task is reading, not semantic graph navigation
- the file may be code or non-code text

Do not use when:

- the real need is symbol graph navigation
- the file is large code and a structured entry point would be better

Likely follow-up:

- `inspect_match`
- `get_file_context`
- `trace_symbol`

### `get_file_context`

Lane:

- semantic code lane

Purpose:

- give a structured file overview plus important references

Use when:

- the file is code
- the agent needs an entry point richer than raw content
- likely hotspots or external references matter

Do not use when:

- the file is non-code text
- exact line-ranged reading is the immediate need

Likely follow-up:

- `get_file_content`
- `trace_symbol`
- `find_references`

### `inspect_file`

Lane:

- semantic code lane built on top of text reading

Purpose:

- a guided exploration surface for large or complex code files

Use when:

- the file is code and large enough that raw reading is inefficient
- the agent wants structure, chunks, and likely hotspots together

Do not use when:

- the file is small and direct reading is faster
- the file is non-code text

Likely follow-up:

- `get_file_content`
- `trace_symbol`

### `inspect_match`

Lane:

- bridge tool between text and semantic lanes

Purpose:

- turn a text hit into a local excerpt with line numbers and enclosing structure

Use when:

- `search_text` already found a useful hit
- the next step is local inspection rather than whole-file reading

Do not use when:

- the path and line are already being read directly with `get_file_content`
- the real task is global symbol graph analysis

Likely follow-up:

- `get_file_content`
- `trace_symbol`

### `find_references`

Lane:

- semantic code lane

Purpose:

- find callers, imports, or type usages for an exact symbol

Use when:

- the symbol is already disambiguated
- the task is impact analysis or navigation through usage sites

Do not use when:

- only a bare common name is known and ambiguity is unresolved
- the need is raw content search

Likely follow-up:

- `get_file_content`
- `inspect_match`
- `trace_symbol`

### `trace_symbol`

Lane:

- semantic code lane

Purpose:

- one-call semantic investigation for an exact symbol

Use when:

- the agent wants the fastest route from definition to callers, callees, and nearby semantic context
- the symbol identity is already known or can be made exact

Do not use when:

- the file/path is not known at all
- the user is searching for raw text rather than code structure

Likely follow-up:

- `find_references`
- `get_file_content`
- `get_file_context`

### `get_context_bundle`

Lane:

- semantic code lane

Purpose:

- preserve the current high-value bundled symbol context tool

Use when:

- a precise symbol bundle is needed
- the product has not yet fully converged on `trace_symbol`

Do not use when:

- the task is path discovery or raw text reading

Likely follow-up:

- `find_references`
- `get_file_content`

## Tool Surface Rules

Use these to prevent the tool set from drifting into a messy overlap.

### Rule A: One primary purpose per tool

Each tool should answer one kind of question well.

### Rule B: Shared internals, distinct public intent

Tools may share internal query code, but their user-facing purpose should still be distinct.

### Rule C: Text tools and semantic tools must stay conceptually separate

They can interoperate, but they should not blur into one ambiguous mega-tool.

### Rule D: Follow-up paths should be obvious

Tool outputs should make the next likely tool easy to choose.

### Rule E: Preserve the current high-value tools if the replacement is not clearly better yet

Do not remove `get_context_bundle` or similar tools just because a cleaner future naming scheme exists.

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
