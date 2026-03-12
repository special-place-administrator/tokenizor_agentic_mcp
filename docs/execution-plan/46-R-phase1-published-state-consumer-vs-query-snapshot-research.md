# Research: Phase 1 Published State Consumer Vs Query Snapshot

Related plan:

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [26-R-phase1-live-state-snapshot-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/26-R-phase1-live-state-snapshot-research.md)
- [44-R-phase1-published-handle-state-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/44-R-phase1-published-handle-state-research.md)
- [45-T-phase1-published-handle-state-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/45-T-phase1-published-handle-state-shell.md)

Goal:

- choose the next smallest architectural slice after lightweight handle-state publication

## Current Code Reality

- `SharedIndexHandle` now publishes generation and lightweight authoritative state on real mutation paths
- query readers still borrow from the live `LiveIndex`
- heavier query paths already capture owned views under short read locks, so read-lock breadth is much better than before
- no production reader currently consumes `PublishedIndexState`
- current operational surfaces still reacquire the live lock for state/count reporting:
- `health` in `src/protocol/tools.rs`
- startup readiness logging in `src/main.rs`
- sidecar `GET /health` in `src/sidecar/handlers.rs`
- daemon project health in `src/daemon.rs`

That creates two plausible next directions:

- make one real consumer read the lightweight published state
- or start publishing the first fuller immutable query snapshot for one targeted reader path

## Candidate A: First Consumer Of `PublishedIndexState`

### Shape

- switch one reader or reporting surface to prefer `PublishedIndexState`
- likely candidates:
- `health`
- startup readiness logging
- sidecar `GET /health`
- daemon project health

### Advantages

- proves the new published state is actually useful
- smallest churn
- no duplicated query data

### Weaknesses

- mostly operational value, not query-architecture value
- does not advance immutable query reads meaningfully
- `health` cannot move fully as-is yet because current published state does not include:
- parsed / partial / failed counts
- load duration

### Best fit

- `health` is still the best first consumer, but only if `PublishedIndexState` grows slightly to cover the existing report payload
- startup logging, sidecar health, and daemon project health become near-free follow-on consumers once that state exists

## Candidate B: First Immutable Query Snapshot For One Reader Path

### Shape

- publish one targeted immutable read snapshot beside the lightweight state
- migrate one query/read path to consume it

### Candidate reader shapes

- `repo_outline`/`get_repo_outline`
- `health` counts and status
- `what_changed` timestamp mode

### Advantages

- directly advances the long-term snapshot-read architecture
- gives a proven pattern for later reader migration

### Weaknesses

- more structural churn than consuming lightweight state
- risks mixing “what should be in a full query snapshot” with “which reader needs it first”
- lower immediate payoff than earlier in the phase because the clean candidates already avoid broad read-lock hold times via owned capture

### Best fit

- if this path is chosen, `repo_outline` is the cleanest first candidate because it is whole-index, deterministic, and already has an owned view shape

## Candidate C: Do Both Immediately

### Shape

- wire `health` to `PublishedIndexState`
- also add a first fuller immutable query snapshot

### Advantages

- faster visible progress

### Weaknesses

- blurs the measurement of what each change bought
- larger slice than needed right now

### Verdict

- avoid unless a single shared implementation clearly forces both together

## Preferred Approach

- take Candidate A first
- make `health` the first real consumer of `PublishedIndexState`
- expand `PublishedIndexState` just enough to cover the current health payload
- defer the first fuller immutable query snapshot to the next research/implementation step after that

## Why

- `health` is the highest-value operational surface that can prove end-to-end published-state usage
- extending the published schema with parse counts and load duration is still materially smaller than publishing a first fuller immutable query snapshot
- this keeps the next slice small and behavior-preserving while turning the new publication shell into a real used interface
- startup logging, sidecar health, and daemon project health can reuse the same expanded state with minimal extra design work
- once one consumer exists, a later immutable query-snapshot slice can focus on query semantics instead of proving basic publication value

## Recommended Next Implementation Slice

- extend `PublishedIndexState` with:
- parsed / partial / failed counts
- load duration
- enough state to derive the health status string without a live read
- make `health` prefer `PublishedIndexState` for index status and counts
- keep watcher info and token-savings append behavior unchanged
- optionally move startup logging and sidecar / daemon health summaries in the same slice if the change remains mechanical

## Carry Forward

- immediate next slice: first `PublishedIndexState` consumer, centered on `health`
- deferred on purpose:
- first fuller immutable query snapshot
- reader migration for outline/search/content tools
