---
doc_type: task
task_id: 65
title: Phase 1 file classification metadata shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 64-T-phase1-file-classification-heuristics-research.md
next_task: 66-T-phase1-shared-query-option-struct-shell.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 65: Phase 1 File Classification Metadata Shell

## Objective

- add the first real file classification substrate for the current semantic lane without broadening discovery or watcher behavior to non-code text files yet

## Why This Exists

- task 64 concludes that Phase 1 needs a real classification model now, but the first shell must stay bounded
- the current file metadata path still has no classification for code-vs-text-vs-binary or generated/test/vendor noise

## Read Before Work

- [03-P-architecture-and-guardrails.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/03-P-architecture-and-guardrails.md)
- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [23-R-phase1-text-lane-boundary-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/23-R-phase1-text-lane-boundary-research.md)
- [64-R-phase1-file-classification-heuristics-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/64-R-phase1-file-classification-heuristics-research.md)

## Expected Touch Points

- `src/domain/index.rs`
- `src/discovery/mod.rs`
- `src/live_index/store.rs`
- `src/live_index/persist.rs`
- `src/watcher/mod.rs`

## Deliverable

- file classification metadata carried through discovery, parsing, live index storage, and snapshot persistence for current semantic-lane files

## Done When

- current indexed code files have a deterministic classification model plus generated/test/vendor tags
- watcher updates preserve the same classification semantics as initial load
- snapshot round-trip preserves classification metadata
- non-code text discovery remains deferred

## Completion Notes

- added a first bounded file classification substrate in code using a mutually exclusive `FileClass` plus orthogonal generated/test/vendor tags
- threaded classification through:
- `src/domain/index.rs`
- `src/discovery/mod.rs`
- `src/parsing/mod.rs`
- `src/live_index/store.rs`
- `src/live_index/persist.rs`
- `src/watcher/mod.rs`
- preserved current code-only discovery and watcher behavior; `Text` and `Binary` remain deferred for the future lightweight text registry
- bumped snapshot schema version and preserved classification metadata across snapshot round-trip
- updated affected test helpers and manual file constructors so the new metadata exists everywhere the semantic lane constructs `IndexedFile`
- verification passed:
- `cargo test --no-run`
- `cargo test file_classification -- --nocapture`
- `cargo test discover_files_assigns_classification_tags_from_path -- --nocapture`
- `cargo test round_trip_preserves_files_symbols_references_content -- --nocapture`
- `cargo test maybe_reindex_updates_reverse_index_on_change -- --nocapture`

## Carry Forward To Next Task

Next task:

- `TBD`

Carry forward:

- use the new classification substrate as the basis for shared internal query option structs and future text-lane work
- keep generated-file detection on strong path/filename heuristics for now; do not mix banner-based heuristics into the next slice unless benchmarks or real misses justify it

Open points:

- OPEN: shared query option structs are the next likely Phase 1 substrate slice, but the first adoption boundary should stay narrow
