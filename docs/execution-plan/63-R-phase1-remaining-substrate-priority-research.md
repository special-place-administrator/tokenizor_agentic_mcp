# Research: Phase 1 Remaining Substrate Priority

Related plan:

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [20-D-phase1-query-duplication-discovery.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/20-D-phase1-query-duplication-discovery.md)
- [21-R-phase1-query-layer-shape-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/21-R-phase1-query-layer-shape-research.md)
- [23-R-phase1-text-lane-boundary-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/23-R-phase1-text-lane-boundary-research.md)
- [52-R-phase1-next-published-query-family-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/52-R-phase1-next-published-query-family-research.md)
- [53-R-phase1-shared-file-read-substrate-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/53-R-phase1-shared-file-read-substrate-research.md)
- [61-R-phase1-file-local-view-compatibility-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/61-R-phase1-file-local-view-compatibility-research.md)

Goal:

- choose the next highest-value Phase 1 move after the shared-file migration wave without inventing work that the source plan does not actually prioritize

## Current Code Reality

- the shared query seam now exists:
- `src/live_index/search.rs` owns shared symbol and text search semantics
- `src/live_index/store.rs` now publishes lightweight handle state and a repo-outline snapshot
- the hot file-local readers already use shared `Arc<IndexedFile>` capture under short locks:
- `get_file_content`
- `get_file_outline`
- `get_symbol`
- `get_symbols`
- clone-based file-local views were intentionally retained only as compatibility/test scaffolding

That means the earlier Phase 1 read-path and publication work is no longer the main structural gap.

## Remaining Plan-Level Gaps

- the Phase 1 plan still explicitly calls for:
- file classification metadata:
- `is_code`
- `is_text`
- `is_binary`
- `is_generated`
- `is_test`
- `is_vendor`
- reusable internal option structs:
- `PathScope`
- `SearchScope`
- `ResultLimit`
- `ContentContext`
- `NoisePolicy`
- repo inspection confirms those names do not currently exist in `src/` or `tests/`
- current file metadata shapes remain minimal:
- `DiscoveredFile` has path plus language only
- `FileProcessingResult` has parse output plus bytes/hash/references only
- `IndexedFile` has language, content, symbols, refs, and alias map, but no classification flags
- current public input structs are still ad hoc and narrow:
- `SearchSymbolsInput` only has `query` and `kind`
- `SearchTextInput` only has `query`, `terms`, and `regex`
- `GetFileContentInput` only has `path`, `start_line`, and `end_line`

## Candidate A: Continue Published Immutable Query Snapshot Work

### Advantages

- extends the in-memory authoritative-state direction
- keeps building on the recent snapshot/publication work

### Weaknesses

- the first published snapshot already covers the small metadata-only case that was clearly worth doing
- the file-local hot paths already shortened read-lock lifetime through shared-file capture, so another snapshot increment is no longer closing the biggest visible Phase 1 gap
- it does not complete the still-missing source-plan substrate for classification, scoping, or noise suppression

### Verdict

- reject as the immediate next priority

## Candidate B: Remove Compatibility Scaffolding Next

### Advantages

- reduces duplicate shapes in `query.rs` and `format.rs`
- makes the shared-file path even more visually dominant

### Weaknesses

- almost entirely internal cleanup
- low product leverage compared with still-missing Phase 1 substrate
- risks churn before the remaining classification and query-option work settles

### Verdict

- defer

## Candidate C: Add Shared Query Option Structs First

### Advantages

- would reduce ad hoc validation drift across tool handlers
- lines up with the Phase 1 plan and Refactor 7 in the architecture notes

### Weaknesses

- many of the most important future options need real file classification beneath them
- `NoisePolicy` is not meaningful without generated/test/vendor metadata
- `SearchScope` and future text-lane routing are clearer once `is_code` / `is_text` / `is_binary` boundaries are defined
- adding option wrappers before the classification substrate risks a second redesign when the filters become real

### Verdict

- right direction, wrong first step

## Candidate D: Return To The Remaining Phase 1 Substrate, Starting With File Classification

### Advantages

- directly addresses explicit Phase 1 plan work that is still absent in code
- unblocks later query-option structs with real semantics instead of placeholders
- aligns with the earlier text-lane research, which already identified classification as unresolved
- gives future scoped search, path tools, noise suppression, and dual-lane retrieval a common factual substrate

### Weaknesses

- needs one more research pass to define cheap, deterministic classification rules before coding
- could expand too broadly if it tries to implement text-lane storage at the same time

### Verdict

- preferred

## Preferred Approach

- choose Candidate D
- do not spend the next slice on more published snapshots or cleanup
- first resolve the exact file-classification heuristics and ownership boundary
- after classification metadata exists, add shared internal query option structs on top of it

## Why Classification Comes Before Query Options

- the plan’s reusable options are supposed to encode real scope and noise semantics, not just rename current tool parameters
- the most valuable future option types depend on classification:
- `NoisePolicy` needs generated/test/vendor metadata
- `SearchScope` needs a stable code-lane vs text-lane boundary
- `PathScope` and `ContentContext` become more useful once file classes determine which lane and ranking defaults apply
- current tool inputs are still small enough that delaying internal option structs briefly does not create urgent maintenance risk

## Recommended Next Sequence

1. research cheap deterministic file-classification heuristics and decide what is index-time vs deferred
2. implement the first file-classification metadata shell across discovery/domain/live-index storage
3. then add shared internal query option structs that consume the new classification substrate

## Carry Forward

- the next best Phase 1 move is not another snapshot increment
- the remaining highest-value gap is file classification metadata, with shared option structs immediately after it
- immediate next task: [64-T-phase1-file-classification-heuristics-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/64-T-phase1-file-classification-heuristics-research.md)
