# Research: Phase 1 Next Published Query Family

Related plan:

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [50-R-phase1-first-immutable-query-snapshot-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/50-R-phase1-first-immutable-query-snapshot-research.md)
- [51-T-phase1-repo-outline-published-query-snapshot-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/51-T-phase1-repo-outline-published-query-snapshot-shell.md)

Goal:

- choose the correct next published query family after the repo-outline/file-tree snapshot without introducing hidden repo-wide memory duplication

## Current Code Reality

- the first published query-facing snapshot is now `RepoOutlineView`
- that snapshot was safe because it contains only:
- file path
- language
- symbol count
- total file / symbol counts
- the next already-owned view candidates are:
- `WhatChangedTimestampView`
- `FileOutlineView`
- `FileContentView`
- `SymbolDetailView`
- `FileContentView` owns `Vec<u8>`
- `SymbolDetailView` owns `Vec<u8>` plus `Vec<SymbolRecord>`
- `FileOutlineView` owns `Vec<SymbolRecord>`

That means the next useful family is no longer “tiny metadata only” by default.

## Candidate A: Publish `WhatChangedTimestampView`

### Shape

- publish the timestamp-mode `what_changed` view
- migrate only timestamp-mode `what_changed` to it

### Advantages

- smallest next immutable publication from a pure memory-safety standpoint
- duplicates only sorted path strings and a timestamp
- low behavior risk

### Weaknesses

- only serves one smaller operational/query hybrid tool
- does not materially advance the higher-value file-local read flows
- weak answer to the project’s larger in-memory state goal

### Verdict

- acceptable fallback, but strategically weak

## Candidate B: Publish Current File-Local Views Directly

### Shape

- publish the existing `FileOutlineView`, `FileContentView`, or `SymbolDetailView` repo-wide
- migrate the corresponding tools to read those published snapshots directly

### Advantages

- higher user-visible leverage than `what_changed`
- file-local read tools are central to the product

### Weaknesses

- current view shapes are capture-time clones, not cheap shared snapshots
- publishing `FileContentView` repo-wide would duplicate raw file bytes already stored in `LiveIndex`
- publishing `SymbolDetailView` repo-wide would duplicate file bytes and symbol vectors
- that is precisely the kind of hidden memory blow-up Phase 1 should avoid

### Verdict

- reject as the next direct implementation step

## Candidate C: Treat File-Local Reads As The Next Real Family, But Add A Shared File Substrate First

### Shape

- explicitly name file-local reads as the next high-value published family
- do not publish the current clone-heavy views directly
- first define a shared immutable file unit that later published readers can reuse cheaply

### Advantages

- aligns the next work with the highest-value read flows:
- `get_file_outline`
- `get_symbol`
- `get_symbols`
- `get_file_content`
- addresses the real risk instead of routing around it with a low-value metadata-only slice
- keeps publication aligned with the project’s in-memory-authoritative-state direction

### Weaknesses

- requires one more research/design step before implementation
- delays another immediately visible published-reader migration

### Verdict

- preferred

## Preferred Approach

- choose Candidate C
- make file-local reads the next real published family
- explicitly gate them on a shared file-read substrate research step
- defer `what_changed` publication unless a later roadmap needs one more ultra-small metadata-only slice

## Why

- after repo-outline/file-tree, the best remaining leverage is in file-local reads, not timestamp-only metadata
- the current owned view shapes for those reads are not safe to publish repo-wide as-is
- spending a task on `what_changed` now would optimize for easy motion rather than the correct substrate

## Recommended Next Implementation Sequence

1. research the shared file-read substrate that can support future published readers without duplicating bytes
2. implement that substrate
3. only then choose the first file-local published consumer family

## Carry Forward

- immediate next slice: shared file-read substrate research
- explicitly rejected for now:
- direct publication of current `FileContentView` / `SymbolDetailView`
- using `what_changed` timestamp publication as the main next architectural move
