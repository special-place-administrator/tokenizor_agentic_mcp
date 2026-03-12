# Workstreams And Tool Surface

Derived from `TOKENIZOR_CONTEXT_AND_EXECUTION_PLAN.md` on 2026-03-11.
Source coverage: lines 402-1283.

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

