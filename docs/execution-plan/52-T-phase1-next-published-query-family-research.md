---
doc_type: task
task_id: 52
title: Phase 1 next published query family research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 51-T-phase1-repo-outline-published-query-snapshot-shell.md
next_task: 53-T-phase1-shared-file-read-substrate-research.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 52: Phase 1 Next Published Query Family Research

## Objective

- choose the next published query family after the repo-outline/file-tree snapshot, with explicit attention to memory duplication risk

## Why This Exists

- task 51 proved the first query-facing immutable published snapshot on a safe metadata-only family
- the next obvious candidates touch owned file bytes and symbols, so a wrong choice here could silently duplicate repository content in memory

## Read Before Work

- [50-R-phase1-first-immutable-query-snapshot-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/50-R-phase1-first-immutable-query-snapshot-research.md)
- [51-T-phase1-repo-outline-published-query-snapshot-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/51-T-phase1-repo-outline-published-query-snapshot-shell.md)

## Expected Touch Points

- `src/live_index/query.rs`
- `src/live_index/store.rs`
- `src/protocol/tools.rs`
- `docs/execution-plan/`

## Deliverable

- a short research note naming the correct next query family or prerequisite, with explicit rejection of unsafe publication shapes

## Done When

- the note compares the real next candidates
- it states whether the next move should be another metadata-only snapshot or a prerequisite for richer file-local reads
- memory and correctness risks are captured

## Completion Notes

- the next real published query family should be file-local reads, not another metadata-only tool
- direct publication of the current file-local views was rejected because `FileContentView` and `SymbolDetailView` currently own cloned file bytes and symbols, which would risk repo-wide memory duplication
- `what_changed` timestamp publication remains a safe fallback, but it is too low-leverage to be the main next move now that repo-outline/file-tree publication is done
- the correct immediate next step is a shared file-read substrate research slice before any richer published reader shell

## Carry Forward To Next Task

Next task:

- `53-T-phase1-shared-file-read-substrate-research.md`

Carry forward:

- the next published family should target `get_file_outline`, `get_symbol`, `get_symbols`, and `get_file_content`, but only after the storage substrate can share immutable file data cheaply
- do not publish the current clone-heavy file-local views repo-wide as a shortcut

Open points:

- OPEN: choose the exact shared file unit that can support published file-local readers without deep-cloning repository content
