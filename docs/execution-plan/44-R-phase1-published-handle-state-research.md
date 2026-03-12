# Research: Phase 1 Published Handle State Shell

Related plan:

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [26-R-phase1-live-state-snapshot-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/26-R-phase1-live-state-snapshot-research.md)
- [42-R-phase1-shared-index-handle-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/42-R-phase1-shared-index-handle-research.md)
- [43-T-phase1-shared-index-handle-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/43-T-phase1-shared-index-handle-shell.md)

Goal:

- choose the smallest publication step that makes `SharedIndexHandle` an active state container instead of a passive lock wrapper

## Current Code Reality

- `SharedIndexHandle` now gives the project one named shared container
- query-side read breadth has already been narrowed, so publication work does not need to fight formatter-held read locks first
- production mutations still happen through raw live-lock access in several places:
- watcher reindex/remove paths
- background snapshot verify updates
- `index_folder` reload
- daemon project reload
- sidecar file reparse/update paths

This means the next useful boundary is not “replace all readers with immutable snapshots immediately.” It is “make real mutation paths publish authoritative handle state.”

## What The Next Step Should Achieve

- publish a stable handle-level snapshot whenever production write paths mutate the live index
- give that snapshot a monotonic generation and key state metadata
- centralize mutating entry points on `SharedIndexHandle` instead of ad hoc `write()` usage in production code
- avoid duplicating the full repository content in a second immutable structure yet

## Candidate A: Full Published Query Snapshot Now

### Shape

- add a second full query-facing immutable snapshot beside the live `RwLock<LiveIndex>`
- republish it on every write path
- begin switching readers to it immediately

### Advantages

- closest to the end-state architecture

### Weaknesses

- duplicates bytes and query indices before the publication plumbing is even proven
- larger memory and churn risk
- touches readers, writers, and publication in one slice

### Verdict

- too large for the next move

## Candidate B: Published Handle-State Snapshot Only

### Shape

- add a lightweight published state snapshot to `SharedIndexHandle`
- include generation and core operational metadata such as file count, symbol count, provenance, verify state, and last load/mutation wall-clock time
- add mutation helper methods on the handle that both mutate the live index and republish the lightweight snapshot
- keep query readers on the live index for now

### Advantages

- small code churn
- exercises real publication and generation flow on production mutation paths
- avoids whole-repo duplication
- gives later snapshot-reader work a proven publication seam

### Weaknesses

- does not yet give queries immutable snapshot reads
- production code must migrate from direct `write()` usage to handle mutation helpers

### Verdict

- best immediate implementation slice

## Candidate C: Generation Counter Only, No Published Snapshot

### Shape

- add an atomic generation counter to the handle
- bump it from write helpers
- do not publish any structured state snapshot yet

### Advantages

- even smaller than Candidate B

### Weaknesses

- too little observable structure
- still leaves later code without a published handle snapshot to consume

### Verdict

- not enough value for the next slice

## Preferred Approach

- choose Candidate B now
- defer full immutable query snapshot publication until the handle publication flow is proven

## Recommended Next Implementation Slice

- add `PublishedIndexState` or equivalent lightweight state snapshot to `SharedIndexHandle`
- include at least:
- generation
- file count
- symbol count
- load provenance
- snapshot verify state
- loaded/mutated wall-clock time
- add handle mutation helpers for:
- reload
- update_file
- add_file
- remove_file
- mark snapshot verify running/completed
- migrate production mutators to those helpers so publication stays current

## Carry Forward

- immediate next slice: published lightweight handle state plus mutation-helper migration
- deferred on purpose:
- full duplicated immutable query snapshot
- reader migration onto published snapshots
- richer state-machine phases beyond the current provenance/verify model
