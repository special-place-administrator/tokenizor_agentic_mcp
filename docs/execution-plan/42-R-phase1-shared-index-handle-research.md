# Research: Phase 1 Shared Index Handle Shape

Related plan:

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [26-R-phase1-live-state-snapshot-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/26-R-phase1-live-state-snapshot-research.md)
- [41-T-phase1-context-bundle-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/41-T-phase1-context-bundle-read-view-capture.md)

Goal:

- choose the smallest shared-container change that points toward a long-lived in-memory state machine without mixing that work with a full immutable-snapshot migration

## Current Code Reality

- `src/live_index/store.rs` defines `pub type SharedIndex = Arc<RwLock<LiveIndex>>`
- `src/protocol/mod.rs` repeats the same alias independently
- watchers, protocol handlers, and sidecar helpers all assume the shared container exposes raw `.read()` and `.write()` directly
- query-side lock breadth is much smaller now because heavy formatter-backed tool paths capture owned views under short reads first

This means the biggest remaining structural issue is not broad formatting under locks. It is that the shared state container still has no identity beyond “an `Arc` around a lock around `LiveIndex`”.

## What The Next Step Should Achieve

- give the shared in-memory state a single authoritative handle type
- stop duplicating the raw alias across modules
- preserve current watcher/tool call sites as much as possible
- leave room to add published immutable read snapshots later without another repo-wide alias hunt

## Candidate A: Keep The Raw Alias Until Full Snapshot Publication

### Shape

- leave `Arc<RwLock<LiveIndex>>` in place everywhere
- defer all container changes until the eventual immutable snapshot implementation

### Advantages

- zero immediate churn

### Weaknesses

- keeps the central state model implicit
- duplicates the alias seam across modules
- future snapshot publication still starts with mechanical alias cleanup before the real architecture change

### Verdict

- too passive now that the query layer is already cleaner

## Candidate B: Introduce A Shared Handle Wrapper, But Keep Only The Live `RwLock`

### Shape

- replace the raw alias with a central shared handle type such as `SharedIndexHandle`
- the handle initially stores the existing `RwLock<LiveIndex>`
- expose `.read()` and `.write()` pass-through methods so most consumers do not need behavioral changes
- move all shared-index aliases to reuse this one type instead of redeclaring `Arc<RwLock<LiveIndex>>`

### Advantages

- small code churn
- creates a named home for later metadata or published snapshots
- keeps watcher and protocol code mechanically stable

### Weaknesses

- does not yet publish immutable snapshots
- still leaves the live `RwLock` as the only real state field for one more slice

### Verdict

- best immediate implementation slice

## Candidate C: Add Handle Wrapper And Published Read Snapshot Immediately

### Shape

- introduce a shared handle with both live mutable state and a published read snapshot
- update load, reload, watcher writes, and restore paths to republish snapshots
- begin switching query readers to the published snapshot path now

### Advantages

- gets closer to the desired end state immediately

### Weaknesses

- broader behavioral change touching watchers, persistence, and readers at once
- too large for the next slice after just completing the query read-view migration sequence

### Verdict

- right medium-term direction, wrong next slice

## Preferred Approach

- choose Candidate B now
- use it to prepare for Candidate C next

## Recommended Next Implementation Slice

- introduce a central shared handle type around the current `RwLock<LiveIndex>`
- remove duplicated raw shared-index aliases
- keep `.read()` / `.write()` compatibility methods so watchers and tool handlers remain simple
- update constructors such as `LiveIndex::load()` and `LiveIndex::empty()` to return the new shared handle

This preserves behavior while creating a concrete place to attach:

- published read snapshots
- generation counters
- future state-machine metadata

## Carry Forward

- immediate next slice: shared-index handle shell only
- deferred on purpose:
- published immutable read snapshots
- generation-based read identity
- watcher/write-path publication hooks
