# Research: Phase 1 Live State And Snapshot Model

Related plan:

- [03-P-architecture-and-guardrails.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/03-P-architecture-and-guardrails.md)
- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [23-R-phase1-text-lane-boundary-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/23-R-phase1-text-lane-boundary-research.md)
- [26-T-phase1-live-state-snapshot-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/26-T-phase1-live-state-snapshot-research.md)

Goal:

- choose the smallest safe model for always-hot in-memory state, real-time disk-change ingestion, and durable snapshot recovery without freezing the current mutable lock design in place forever

## Current Code Reality

- `src/live_index/store.rs` owns one mutable `LiveIndex`
- shared access is `Arc<RwLock<LiveIndex>>`
- queries borrow directly from that live structure
- watcher updates re-read bytes from disk, parse outside the lock, then acquire a write lock and mutate the live structure in place
- startup already prefers persisted snapshot restore:
- `src/main.rs` loads `.tokenizor/index.bin`
- `src/live_index/persist.rs` converts that snapshot into a live `LiveIndex`
- `background_verify` then reconciles changed, deleted, and new files against disk

This means the project already has three critical ingredients:

- whole-file bytes resident in memory for the semantic code lane
- incremental watcher-driven refresh from disk bytes rather than from diff text
- durable snapshot recovery on startup

The missing piece is an explicit coordination model for those ingredients.

## What The Model Must Guarantee

- query tools read from one authoritative in-memory view
- edits on disk become visible quickly without full reindex
- startup from persisted state is fast but never silently stale forever
- partial failures degrade one file or one verify pass, not the whole index
- later non-code text support can participate without forcing all text files into the semantic hot path

## Eventualities This Model Must Handle

- repeated edit bursts on the same file
- delete and recreate races
- parse failures after a change
- stale snapshot restore after the process was down
- watcher updates arriving while background snapshot verification is still reconciling
- future tool-originated writes that should be reflected even if they do not go through an external editor
- non-code text files later joining read/search flows through a separate lane

## Candidate A: Keep The Current Mutable Lock Model As The Long-Term Design

### Shape

- keep `Arc<RwLock<LiveIndex>>`
- continue mutating `LiveIndex` in place for watcher updates, verify updates, and reloads
- add more metadata and indices directly onto the same mutable structure

### Advantages

- smallest code churn
- matches the current implementation
- no new container type

### Weaknesses

- readers still depend on borrowing from a mutable structure
- many tool paths hold a read guard across formatting work, so read lock hold time remains broader than necessary
- any future richer state machine becomes intertwined with the mutable query container rather than published as a stable snapshot
- it is harder to reason about “what exact snapshot did this query see” once more background repair and dual-lane work arrive

### Verdict

- acceptable as a transitional implementation substrate
- not a good end-state design

## Candidate B: Immediate Full Immutable Published Snapshot

### Shape

- define a read-oriented immutable snapshot struct that contains all query-facing indices and metadata
- mutations build a replacement snapshot or patched clone
- publish the new snapshot atomically
- query paths read only from published snapshots, not from a mutating `LiveIndex`

### Advantages

- best read consistency model
- easier lock reasoning
- directly matches the architecture guardrail that query execution should work from immutable snapshots
- strongest foundation for later dual-lane reads, ranking, and repair flows

### Weaknesses

- broader refactor right now
- current format and query helpers borrow deeply from `LiveIndex`, so the migration surface is larger than one small slice
- doing this immediately would mix container replacement with the still-in-flight Phase 1 substrate work

### Verdict

- preferred end state
- too large for the next slice

## Candidate C: Staged Migration

### Shape

- keep the current `Arc<RwLock<LiveIndex>>` container for now
- make snapshot provenance and verification state explicit immediately
- keep watcher and background verify byte-driven
- after the metadata and state surface are explicit, introduce a published immutable read snapshot behind the same public tool surface

### Advantages

- smallest additive move that still points toward the right architecture
- lets the project answer basic operational questions now:
- was this index freshly loaded or restored from snapshot
- is snapshot reconciliation still pending or complete
- is the current state ready or degraded
- reduces the risk of a broad refactor before the state semantics are even named
- preserves the current watcher path, which already does the important thing correctly: re-read disk bytes and parse outside the lock

### Weaknesses

- read-lock breadth remains for one more slice
- not yet the final published-snapshot architecture

### Verdict

- best immediate choice

## Preferred Model

- choose Candidate C now
- target Candidate B next

In other words:

- the near-term implementation should make state explicit
- the medium-term refactor should make reads snapshot-based

## Why This Best Matches The Project Goal

The user requirement behind this research is effectively:

- keep repository content hot in memory
- reflect disk edits in near real time
- let all tools depend on that state
- survive restarts without starting from zero

The current repo already satisfies the basic mechanics for the semantic code lane.

What it lacks is a first-class statement of:

- where the state came from
- whether it is still reconciling with disk
- when it is truly ready

That is the smallest missing layer. Adding it now strengthens correctness without pretending the mutable lock model is the final answer.

## Recommended Next Implementation Slice

- add explicit internal metadata for:
- load provenance
- snapshot verify state
- optional generation/progress hooks only if they stay local to the persistence path
- set that metadata in:
- fresh `LiveIndex::load`
- `empty`
- snapshot restore
- background verify start and completion
- keep public tool behavior unchanged for now

This is exactly what task 27 should implement.

## Read-Publication Direction After That

Once provenance and verify state exist, the next structural step should be:

- introduce an immutable read snapshot type for query-facing state
- make mutations publish a replacement snapshot after rebuild
- keep parsing and mutation assembly off the read path

The first published-snapshot implementation does not need to be perfect:

- a standard-library-only version using `RwLock<Arc<...>>` is acceptable as a transitional publish layer
- a dedicated atomic-swap dependency should be justified only if benchmarks show the extra lock on snapshot acquisition matters

## Important Boundary Rules

- diff text is not the source of truth; on-disk bytes are
- watcher and repair flows should continue to rebuild from actual file bytes
- semantic code files stay fully hot in memory
- non-code text should still follow the separate registry-and-cache lane chosen in task 23

## Risks And Gaps To Carry Forward

- current persistence metadata capture appears to derive file mtimes from relative paths during snapshot build, which is only reliable if path resolution is rooted correctly; this should be reviewed in the next implementation slice or immediately after it
- background verify currently mutates silently; once provenance and verify state exist, that path should become explicit and testable
- public `IndexState` does not need to expand immediately if internal metadata is enough for the first step

## Carry-Forward

- chosen immediate path: staged migration
- chosen end state: immutable published read snapshots
- next implementation slice: explicit snapshot provenance and verify state
- deferred on purpose:
- full read-snapshot publication
- dual-lane text registry implementation
- public `IndexState` expansion unless internal metadata proves insufficient
