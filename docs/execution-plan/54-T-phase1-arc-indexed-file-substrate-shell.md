---
doc_type: task
task_id: 54
title: Phase 1 arc indexed file substrate shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 53-T-phase1-shared-file-read-substrate-research.md
next_task: 55-T-phase1-first-file-local-shared-consumer-research.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 54: Phase 1 Arc Indexed File Substrate Shell

## Objective

- introduce the smallest shared immutable file unit chosen by task 53 so future published file-local read families can reuse file bytes and symbols without repo-wide deep clones

## Why This Exists

- after task 51, the next high-value published readers are file-local rather than metadata-only
- those readers need a shared byte/symbol substrate before direct published snapshots are safe

## Read Before Work

- [53-R-phase1-shared-file-read-substrate-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/53-R-phase1-shared-file-read-substrate-research.md)

## Expected Touch Points

- `src/live_index/store.rs`
- `src/live_index/query.rs`
- `src/live_index/trigram.rs`
- `src/live_index/persist.rs`

## Deliverable

- the first storage-layer shell for shared immutable file entries, with focused coverage

## Done When

- live storage can share immutable file units with later published read families
- current query behavior remains unchanged
- focused tests cover the substrate migration

## Completion Notes

- changed `LiveIndex.files` to `HashMap<String, Arc<IndexedFile>>` so whole-file entries become the shared immutable unit for later file-local readers
- kept current query behavior stable by adapting borrowed accessors in `src/live_index/query.rs` and making trigram builders/search helpers generic over file containers that expose `AsRef<IndexedFile>`
- updated snapshot restore/build paths plus direct test/helper constructors in persistence, formatting, tool, search, resource, and sidecar test surfaces so the Arc-backed storage shape compiles cleanly end to end
- focused verification passed:
- `cargo test --no-run`
- `cargo test live_index::persist::tests:: -- --nocapture`
- `cargo test trigram -- --nocapture`
- `cargo test live_index_reload -- --nocapture`
- `cargo test repo_outline -- --nocapture`
- `cargo test file_tree -- --nocapture`
- `cargo test file_content -- --nocapture`

## Carry Forward To Next Task

Next task:

- `55-T-phase1-first-file-local-shared-consumer-research.md`

Carry forward:

- the shared immutable file substrate is now in place without changing current read behavior, so later file-local readers can reuse bytes, symbols, references, and alias maps through cheap `Arc<IndexedFile>` clones
- the next decision should be about the first consumer family and shape on top of that substrate, not more storage churn

Open points:

- OPEN: decide which first file-local read family should consume the shared file substrate first, and whether that should be a published repo-wide snapshot or a narrow Arc-capture path
