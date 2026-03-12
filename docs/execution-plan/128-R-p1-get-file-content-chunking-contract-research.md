# Research: P1 Get File Content Chunking Contract

Related plan:

- [05-P-validation-and-backlog.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)
- [01-P-overview-and-principles.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/01-P-overview-and-principles.md)
- [02-P-workstreams-and-tool-surface.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/02-P-workstreams-and-tool-surface.md)

Goal:

- decide the first explicit `get_file_content` contract for progressive chunked reads without widening into byte-exact paging or richer navigation hints

## Current Code Reality

`get_file_content` now supports exact-path full-file reads, explicit line ranges, `around_line`, and first-match `around_match`. It still lacks a way to page through large files in stable, bounded chunks.

## Candidate Approaches

### Option 1: fixed server-side chunk size with only `chunk_index`

- smallest input surface
- too rigid because callers cannot tune chunk size to their token budget

### Option 2: `chunk_index` plus `max_lines`

- still compact
- keeps chunking line-oriented and easy to reason about
- lets agents trade off chunk size against output budget deterministically

### Option 3: byte-range chunking

- more exact for raw storage
- less ergonomic for reasoning because callers must translate byte offsets back to lines

## Decision: Start With Exact-Path Line-Oriented Chunking

Recommendation:

- add optional `chunk_index` and `max_lines` inputs to `get_file_content`
- treat `chunk_index` as 1-based to match existing line-numbered ergonomics
- use `max_lines` as the chunk size for the first slice
- reject mixing chunking with `start_line`, `end_line`, `around_line`, or `around_match`

## Output Contract

- when `chunk_index` is present, render one numbered line chunk from the selected file
- include a small stable header identifying the file path, selected chunk number, total chunks, and covered line range
- return a deterministic out-of-range message when the requested chunk does not exist
- keep existing full-file, explicit range, `around_line`, and `around_match` behavior unchanged

## Why This Is The Smallest Useful Slice

- it solves the main progressive-read gap without opening byte math or navigation state
- it keeps the first chunking contract line-based and compatible with today’s reasoning flows
- it aligns with the backlog’s `max_lines` and chunk-index direction while deferring richer continuation helpers

## Recommended Next Implementation Slice

- extend `GetFileContentInput` with `chunk_index`
- reuse `max_lines` as the first chunk-size control
- add validation for chunking exclusivity against the other read modes
- render a numbered chunk plus stable chunk header
- add focused tool, formatter, and integration coverage

Expected touch points:

- `src/protocol/tools.rs`
- `src/protocol/format.rs`
- `src/live_index/search.rs`
- `tests/live_index_integration.rs`

## Carry Forward

- keep the first chunking contract exact-path only
- defer caller-provided `chunk_count_hint`, byte-range chunking, and symbol-anchored chunk selection
- preserve current full-file, explicit range, `around_line`, and `around_match` compatibility
