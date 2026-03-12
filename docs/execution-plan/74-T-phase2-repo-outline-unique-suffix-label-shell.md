---
doc_type: task
task_id: 74
title: Phase 2 repo_outline unique suffix label shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 73-T-phase2-repo-outline-path-rich-label-research.md
next_task: 75-T-phase2-text-lane-bridge-timing-research.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 74: Phase 2 Repo Outline Unique Suffix Label Shell

## Objective

- implement collision-aware shortest unique path suffix labels for `repo_outline` while keeping the published repo-outline snapshot and `file_tree` behavior unchanged

## Why This Exists

- task 73 concluded that basename-only repo-outline output is still ambiguous, especially for repeated `mod.rs`
- the existing published `RepoOutlineView` snapshot already carries enough data to derive unambiguous labels without changing storage or publication
- Phase 2 needs the remaining major outline ambiguity fixed without turning `repo_outline` into a full-path dump

## Read Before Work

- [73-R-phase2-repo-outline-path-rich-label-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/73-R-phase2-repo-outline-path-rich-label-research.md)
- [51-T-phase1-repo-outline-published-query-snapshot-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/51-T-phase1-repo-outline-published-query-snapshot-shell.md)
- [72-T-phase2-search-files-code-lane-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/72-T-phase2-search-files-code-lane-shell.md)

## Expected Touch Points

- `src/protocol/format.rs`

## Deliverable

- `repo_outline` output that stays compact for unique files and becomes path-rich only when basename collisions require it

## Done When

- unique files still render compactly
- repeated basenames render with deterministic shortest unique suffix labels
- `file_tree` output is unchanged
- focused tests cover the ambiguity cases

## Completion Notes

- upgraded `repo_outline` formatting to derive collision-aware labels from the existing published `RepoOutlineView`
- implementation stayed formatter-local in `src/protocol/format.rs`
- behavior:
  - unique files still render basename-only labels
  - repeated basenames expand to the shortest unique path suffix
  - deeper collisions expand beyond one parent only when required
  - `file_tree` remains hierarchical and unchanged because it already carries directory context
- focused verification passed:
  - `cargo test repo_outline -- --nocapture`
  - `cargo test file_tree -- --nocapture`

## Carry Forward To Next Task

Next task:

- `75-T-phase2-text-lane-bridge-timing-research.md`

Carry forward:

- preserve the current published repo-outline snapshot seam and keep `file_tree` on the same underlying view

Open points:

- OPEN: whether Phase 2 should continue with a text-lane path bridge now, or hand that work off to the later text-registry phases first
