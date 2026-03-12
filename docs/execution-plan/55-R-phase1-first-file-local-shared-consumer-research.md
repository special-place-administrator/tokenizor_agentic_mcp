# Research: Phase 1 First File-Local Shared Consumer

Related plan:

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [52-R-phase1-next-published-query-family-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/52-R-phase1-next-published-query-family-research.md)
- [53-R-phase1-shared-file-read-substrate-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/53-R-phase1-shared-file-read-substrate-research.md)
- [54-T-phase1-arc-indexed-file-substrate-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/54-T-phase1-arc-indexed-file-substrate-shell.md)

Goal:

- choose the first file-local reader family and the right read shape on top of `Arc<IndexedFile>`

## Current Code Reality

- `LiveIndex.files` now stores `HashMap<String, Arc<IndexedFile>>`
- current file-local read capture methods in `src/live_index/query.rs` still deep-clone owned views:
- `capture_file_outline_view`
- `capture_symbol_detail_view`
- `capture_file_content_view`
- current tool consumers in `src/protocol/tools.rs` are:
- `get_file_outline`
- `get_symbol`
- `get_symbols`
- `get_file_content`
- current pure render helpers in `src/protocol/format.rs` already separate formatting from lock timing, but they still accept clone-based owned view structs
- `SharedIndexHandle` currently publishes lightweight state plus `RepoOutlineView`; it does not publish a repo-wide file-entry snapshot

That means task 54 solved the storage substrate, but not yet the first reader shape.

## Candidate A: `get_file_outline` First

### Advantages

- simplest symbol-only reader
- avoids raw-byte handling on the first shared-file consumer
- naturally close to the earlier file-outline capture work

### Weaknesses

- smaller win than content-oriented readers because it only avoids cloning `Vec<SymbolRecord>`
- does not prove the main value of the new substrate: reusing hot raw file bytes directly
- if this goes first, the most expensive clone-heavy path still remains on `get_file_content`

### Verdict

- reasonable follow-on, but not the first proof

## Candidate B: `get_file_content` First

### Advantages

- removes the heaviest current clone in the simplest reader: `capture_file_content_view` clones whole `Vec<u8>`
- semantics are narrow and stable: one path lookup plus optional line slicing
- sidecar/resource surfaces reuse the same tool path, so one migration reaches more than a single leaf
- proves that later file-local readers can capture `Arc<IndexedFile>` under lock and format from shared bytes after the lock is released

### Weaknesses

- does not yet exercise symbol reuse
- by itself it is a narrow consumer, not the full single-file reader family

### Verdict

- preferred first implementation slice

## Candidate C: `get_symbol` / Symbol-Lookup Path First

### Advantages

- highest reuse leverage after the substrate because it avoids cloning both raw bytes and symbol vectors
- naturally helps both `get_symbol` and the symbol-lookup branch of `get_symbols`

### Weaknesses

- more behavioral surface than `get_file_content`:
- symbol-name lookup
- optional kind filtering
- batch mixed-mode handling in `get_symbols`
- easier to mix substrate work with semantic behavior changes
- a poorer first proof when the project still needs a clean read-shape pattern

### Verdict

- high-value second or third consumer, but too broad for the first step

## Candidate D: Publish A Repo-Wide File Snapshot First

### Advantages

- would extend the published immutable snapshot model beyond repo-outline metadata
- could serve many future file-local readers from one publication layer

### Weaknesses

- for keyed single-file reads, this publishes far more than the immediate consumer needs
- every mutation would need to republish an O(repo-file-count) path -> file-entry map, even though only one path is looked up per request
- task 54 already created the cheaper alternative: clone a single `Arc<IndexedFile>` under lock and do the rest after releasing it

### Verdict

- reject as the first consumer shape

## Preferred Approach

- make the first shared-file consumer the single-file direct-reader path, starting with `get_file_content`
- use narrow `Arc<IndexedFile>` capture under the read lock
- keep repo-wide published file snapshots deferred

## Why

- it gives the clearest win for the least behavioral risk
- it demonstrates the actual value of `Arc<IndexedFile>` immediately: no deep clone of whole file bytes just to render one file
- it creates the reusable pattern for later readers:
- capture one `Arc<IndexedFile>` under lock
- release the lock
- render from `&IndexedFile` via pure formatting helpers

## Recommended Next Implementation Slice

- add a query/helper path that clones one `Arc<IndexedFile>` by relative path
- add a pure file-content formatting helper that renders directly from shared file bytes instead of `FileContentView`
- migrate `get_file_content` to the shared-file capture path
- keep `FileContentView` compatibility only where tests or older helpers still need it, if any
- defer file-outline and symbol/detail migration to later tasks

## Carry Forward

- immediate next slice: `56-T-phase1-file-content-shared-file-capture-shell.md`
- next likely follow-on after that: `get_file_outline`
- explicitly deferred for now:
- repo-wide published path -> shared-file snapshot
- symbol/detail family as the first consumer
