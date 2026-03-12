# Research: P1 Get File Content Around Line Contract

Related plan:

- [05-P-validation-and-backlog.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)
- [01-P-overview-and-principles.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/01-P-overview-and-principles.md)
- [02-P-workstreams-and-tool-surface.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/02-P-workstreams-and-tool-surface.md)

Goal:

- decide the first explicit `get_file_content` contract for centered reads around one line without breaking today’s raw read modes

## Current Code Reality

`get_file_content` currently accepts `path`, `start_line`, and `end_line`. It can return the full file or a raw explicit line slice, but it does not offer a centered excerpt around one anchor line and does not yet expose a dedicated `around_line` API.

## Candidate Approaches

### Option 1: only add `around_line`

- smallest surface
- too rigid because callers still need a way to control how much context surrounds the anchor

### Option 2: add `around_line` plus symmetric `context_lines`

- still compact
- matches how agents usually think about “show me N lines around line X”
- avoids immediate complexity around separate before/after counts

### Option 3: add `around_line`, `before`, and `after`

- more flexible
- larger API and more formatting/validation cases than needed for the first slice

## Decision: Add `around_line` Plus Symmetric `context_lines`

Recommendation:

- add optional `around_line` and `context_lines` inputs to `get_file_content`
- require exact `path` as today
- treat `around_line` as mutually exclusive with explicit `start_line` / `end_line` to keep the first contract deterministic

## Output Contract

- when `around_line` is present, render only the bounded excerpt around the anchor line
- include line numbers in this excerpt so the centered read is self-locating
- keep the existing raw full-file and raw explicit line-range behavior unchanged for now

## Why This Is The Smallest Useful Slice

- it solves the most common “show me the area around line X” need without opening chunking or search-driven reads
- it preserves today’s behavior for existing callers
- it fits the existing exact-path read surface and shared content-context plumbing

## Recommended Next Implementation Slice

- extend `GetFileContentInput` with `around_line` and `context_lines`
- add validation that rejects mixing `around_line` with explicit `start_line` / `end_line`
- render centered excerpts with line numbers for `around_line`
- add focused tool, formatter, and integration coverage

Expected touch points:

- `src/protocol/tools.rs`
- `src/protocol/format.rs`
- `src/live_index/search.rs`
- `tests/live_index_integration.rs`

## Carry Forward

- keep the first `around_line` contract exact-path only
- preserve current full-file and explicit range output for compatibility
- defer `around_match`, chunking, and non-code file reads to later slices
