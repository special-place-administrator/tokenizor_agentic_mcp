# Research: Phase 1 Shared File Read Substrate

Related plan:

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [52-R-phase1-next-published-query-family-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/52-R-phase1-next-published-query-family-research.md)
- [31-T-phase1-file-outline-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/31-T-phase1-file-outline-read-view-capture.md)
- [34-T-phase1-symbol-detail-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/34-T-phase1-symbol-detail-read-view-capture.md)
- [35-T-phase1-batch-symbol-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/35-T-phase1-batch-symbol-read-view-capture.md)
- [36-T-phase1-file-content-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/36-T-phase1-file-content-read-view-capture.md)

Goal:

- choose the shared file unit that future published file-local readers should sit on

## Current Code Reality

- `LiveIndex.files` is currently `HashMap<String, IndexedFile>`
- `IndexedFile` owns:
- `content: Vec<u8>`
- `symbols: Vec<SymbolRecord>`
- `references: Vec<ReferenceRecord>`
- `alias_map: HashMap<String, String>`
- current file-local owned views are clone-based capture types:
- `FileOutlineView` clones symbols
- `FileContentView` clones bytes
- `SymbolDetailView` clones bytes and symbols
- current mutation model is whole-file replacement:
- `update_file` inserts a new `IndexedFile`
- watcher and sidecar reparses replace the file entry
- no code path mutates `IndexedFile.content` or `IndexedFile.symbols` in place

That last point matters: the natural unit of sharing is the whole immutable file entry.

## Real Edit Points

- `src/live_index/store.rs`
- `LiveIndex.files`
- `LiveIndex::load`, `reload`, `update_file`, `add_file`, `remove_file`
- `LiveIndex::get_file`, `symbols_for_file`, `all_files`
- `src/live_index/trigram.rs`
- `build_from_files`
- `search`
- `linear_scan`
- `src/live_index/persist.rs`
- `snapshot_to_live_index`
- `build_snapshot`
- tests and helper builders that currently construct `HashMap<String, IndexedFile>` directly

## Candidate A: Publish Current File Views Directly

### Shape

- keep live storage unchanged
- publish repo-wide `FileContentView` / `SymbolDetailView` maps

### Advantages

- smallest code churn

### Weaknesses

- deep-clones repo bytes and symbol vectors into the published layer
- directly conflicts with the goal of authoritative in-memory state without accidental duplication

### Verdict

- reject

## Candidate B: Arc-Back Individual Fields Inside `IndexedFile`

### Shape

- change `IndexedFile` fields to arc-backed storage such as:
- `Arc<[u8]>`
- `Arc<[SymbolRecord]>`
- `Arc<[ReferenceRecord]>`
- `Arc<HashMap<String, String>>`
- keep `LiveIndex.files` as `HashMap<String, IndexedFile>`

### Advantages

- future clones of `IndexedFile` become cheap
- very flexible for later per-field sharing

### Weaknesses

- broad field-level API churn everywhere that expects `Vec` behavior
- more invasive than necessary for the first shared file unit
- solves a finer-grained problem than the code currently has, because mutation is already whole-file replacement

### Verdict

- plausible long-term direction, but too granular for the next move

## Candidate C: Store `Arc<IndexedFile>` In `LiveIndex.files`

### Shape

- change `LiveIndex.files` to `HashMap<String, Arc<IndexedFile>>`
- keep `IndexedFile` itself structurally the same
- adapt accessors so readers still mostly consume `&IndexedFile`
- future published file-local readers clone `Arc<IndexedFile>` cheaply instead of deep-cloning bytes/symbols

### Advantages

- matches the current mutation model: whole-file replacement
- shares raw file bytes, symbols, references, and alias map together as one immutable unit
- smaller conceptual change than arc-backing every field
- `get_file` and `all_files` can still expose borrowed `&IndexedFile` with adapter helpers
- later published read families can derive:
- file outline
- file content
- symbol detail
- batched symbol/code-slice paths
  from the same shared immutable file entry

### Weaknesses

- moderate signature churn where code currently takes `HashMap<String, IndexedFile>` directly
- trigram helpers need adaptation because they currently accept concrete `HashMap<String, IndexedFile>`
- persistence helpers and many test builders need mechanical updates

### Verdict

- preferred

## Candidate D: Introduce A Separate Published File Entry Type Without Changing Live Storage

### Shape

- keep live storage unchanged
- build a second repo-wide published file-entry map with shared-oriented types

### Advantages

- avoids immediate changes to live-index callers

### Weaknesses

- unless it can borrow/share from live storage, it still duplicates repo content
- if it cannot share, it is structurally the same problem in a different wrapper
- if it does share, it ends up needing a live-storage substrate change anyway

### Verdict

- not a real simplification

## Preferred Approach

- choose Candidate C: `Arc<IndexedFile>` as the shared immutable file unit

## Why

- it fits the current replace-whole-file mutation semantics exactly
- it avoids premature fine-grained field refactors
- it gives later published file-local readers a cheap, obvious unit to share
- it keeps the first file-read substrate step focused on storage semantics rather than formatter or tool behavior

## Recommended Next Implementation Slice

- change `LiveIndex.files` to `HashMap<String, Arc<IndexedFile>>`
- adapt `get_file`, `symbols_for_file`, and `all_files` to return borrowed `&IndexedFile` views from `Arc`
- adjust trigram helpers and persistence conversion points to work with the shared file unit
- keep public query behavior unchanged
- do not publish file-local reader snapshots yet in the same slice

## Carry Forward

- immediate next slice: `Arc<IndexedFile>` substrate shell
- explicitly deferred:
- repo-wide publication of file-local read families
- field-level arc conversion inside `IndexedFile`
