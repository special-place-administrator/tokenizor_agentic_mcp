# Overview And Principles

Derived from `TOKENIZOR_CONTEXT_AND_EXECUTION_PLAN.md` on 2026-03-11.
Source coverage: lines 1-401.

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

