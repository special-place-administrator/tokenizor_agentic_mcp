# Research: Phase 1 Published Degraded State

Related plan:

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [46-R-phase1-published-state-consumer-vs-query-snapshot-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/46-R-phase1-published-state-consumer-vs-query-snapshot-research.md)
- [47-T-phase1-first-published-state-consumer-or-query-snapshot-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/47-T-phase1-first-published-state-consumer-or-query-snapshot-shell.md)

Goal:

- choose the next smallest post-`47` slice while keeping Phase 1 aligned with an explicit in-memory state substrate

## Current Code Reality

- `SharedIndexHandle` now republishes lightweight authoritative state on both helper-based mutations and compatibility-mode direct `write()` mutations
- `PublishedIndexState` already carries:
- generation
- compact status
- file / parse / symbol counts
- load timing
- provenance
- snapshot verify state
- `health`, sidecar `GET /health`, and daemon project health already consume the published state
- `main.rs` startup logging still reads live `IndexState` because the degraded path currently needs the circuit-breaker summary string

That leaves one remaining operational seam: the published substrate can say `Degraded`, but it cannot yet say why.

## Candidate A: Add Compact Degraded Detail To Published State

### Shape

- keep `PublishedIndexStatus` as the primary compact state enum
- add one optional degraded-detail field, most likely the current circuit-breaker summary string
- migrate `main.rs` startup readiness / degraded logging to use `PublishedIndexState`

### Advantages

- smallest possible follow-on slice after task 47
- improves the handle as a real in-memory state machine without duplicating query data
- closes the last obvious operational read that still depends on live `IndexState`
- keeps the next immutable query snapshot decision separate from startup/logging mechanics

### Weaknesses

- does not advance immutable query reads directly
- introduces one more published field that must remain behavior-compatible

### Best fit

- very strong; the current code only needs the degraded summary string, not a full query snapshot

## Candidate B: Publish Full `IndexState` Instead Of A Compact Summary Field

### Shape

- replace or duplicate the compact published status with a fuller published `IndexState`-like enum
- migrate operational consumers to the fuller enum

### Advantages

- closer to the live type shape
- more future room if additional degraded/loading variants appear

### Weaknesses

- larger and noisier than needed right now
- partially duplicates information already encoded by `PublishedIndexStatus`
- weakens the “small authoritative operational payload” goal

### Best fit

- not necessary yet; over-structured for the single remaining consumer gap

## Candidate C: Pivot Immediately To First Immutable Query Snapshot

### Shape

- leave startup degraded logging on a live read
- use the next slice on a query-facing immutable publication candidate such as `repo_outline`

### Advantages

- resumes the long-term reader-migration path sooner

### Weaknesses

- leaves the state-machine story visibly incomplete right after proving published-state consumers
- mixes two concerns: operational-state completeness and immutable query publication
- lower immediate leverage for the user concern that the system should keep an authoritative in-memory state view

### Best fit

- better as the step after operational state is complete enough, not before

## Preferred Approach

- take Candidate A now
- add one compact degraded-summary field to `PublishedIndexState`
- migrate startup readiness / degraded logging in `main.rs` to published state

## Why

- this is the smallest remaining step that strengthens the handle as the authoritative in-memory operational state
- it directly addresses the current gap identified in task 47 without introducing a fuller duplicated query structure too early
- once startup logging is publication-backed, the next immutable query snapshot slice can focus on query/read benefits rather than unfinished operational state

## Recommended Next Implementation Slice

- extend `PublishedIndexState` with an optional degraded summary string derived from the circuit breaker
- keep the compact `PublishedIndexStatus` enum as the primary published status label
- migrate `main.rs` startup readiness / degraded logging to `published_state()`
- add focused tests for degraded summary capture and the migrated startup-log decision path

## Carry Forward

- immediate next slice: published degraded-state shell for startup logging
- deferred on purpose:
- first fuller immutable query snapshot publication
- broader publication of query-owned data structures
