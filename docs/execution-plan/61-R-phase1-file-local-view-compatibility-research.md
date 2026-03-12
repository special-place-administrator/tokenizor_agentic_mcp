# Research: Phase 1 File-Local View Compatibility

Related plan:

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [55-R-phase1-first-file-local-shared-consumer-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/55-R-phase1-first-file-local-shared-consumer-research.md)
- [56-T-phase1-file-content-shared-file-capture-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/56-T-phase1-file-content-shared-file-capture-shell.md)
- [57-T-phase1-file-outline-shared-file-capture-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/57-T-phase1-file-outline-shared-file-capture-shell.md)
- [58-T-phase1-symbol-detail-shared-file-capture-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/58-T-phase1-symbol-detail-shared-file-capture-shell.md)
- [60-T-phase1-get-symbols-code-slice-shared-file-capture-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/60-T-phase1-get-symbols-code-slice-shared-file-capture-shell.md)

Goal:

- decide the fate of `FileOutlineView`, `FileContentView`, and `SymbolDetailView` after the shared-file migrations

## Current Code Reality

- the main tool paths now use shared `Arc<IndexedFile>` capture for:
- `get_file_content`
- `get_file_outline`
- `get_symbol`
- `get_symbols`
- the clone-based file-local view types still exist in `src/live_index/query.rs`:
- `FileOutlineView`
- `FileContentView`
- `SymbolDetailView`
- their current usage is narrow:
- compatibility wrappers in `src/protocol/format.rs`
- parity-focused tests in `src/live_index/query.rs` and `src/protocol/format.rs`
- type re-exports in `src/live_index/mod.rs`

They are no longer the main read-path substrate.

## Candidate A: Remove The View Types Immediately

### Advantages

- reduces duplicate patterns quickly
- makes the shared-file path the only obvious file-local shape

### Weaknesses

- forces immediate cleanup of parity tests and compatibility wrappers that still document output equivalence
- removes a simple owned-shape option that could still be useful for future narrow published snapshots or isolated formatting checks
- adds churn right after a successful migration wave without strong product payoff

### Verdict

- reject for now

## Candidate B: Keep The View Types Quietly As-Is

### Advantages

- zero churn
- preserves all current tests and compatibility helpers

### Weaknesses

- leaves the codebase with two equally visible patterns and no explanation
- future contributors could accidentally keep using clone-heavy views on new hot paths

### Verdict

- better than premature removal, but too ambiguous

## Candidate C: Keep Them Explicitly As Compatibility/Test Shapes

### Advantages

- preserves parity helpers and isolated formatting tests
- keeps a small owned-shape surface available while the shared-file approach settles
- makes the architectural direction explicit: shared-file capture for hot paths, clone-based views only for compatibility or tests

### Weaknesses

- leaves some duplicate structures in the tree for longer
- requires light labeling or commentary to avoid confusion

### Verdict

- preferred

## Preferred Approach

- keep `FileOutlineView`, `FileContentView`, and `SymbolDetailView` for now
- treat them as compatibility/test shapes, not the preferred main-path substrate
- do not remove them until either:
- parity coverage has been rewritten around shared-file helpers, or
- a later published snapshot design clearly no longer wants the owned shapes

## Why

- the migration wave just proved the shared-file pattern on production read paths
- immediate deletion of the owned views creates cleanup churn without a user-visible win
- explicitly retaining them as compatibility scaffolding avoids both premature removal and silent architectural drift

## Recommended Next Step

- if cleanup is desired, make one tiny labeling/documentation slice rather than a broad removal slice
- otherwise defer removal until a later phase that intentionally revisits test helpers and published snapshot shapes

## Carry Forward

- current recommendation: retain the clone-based file-local views as compatibility/test scaffolding
- explicitly avoid using them for new hot-path reader work
