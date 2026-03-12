---
doc_type: task
task_id: 37
title: Phase 1 what changed timestamp view capture
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 36-T-phase1-file-content-read-view-capture.md
next_task: 
created: 2026-03-11
updated: 2026-03-11
---
# Task 37: Phase 1 What Changed Timestamp View Capture

## Objective

- move `what_changed` timestamp mode to the capture-then-format pattern so timestamp comparison and path rendering happen after the live-index read guard is released

## Why This Exists

- timestamp mode still formats from a borrowed `&LiveIndex`
- unlike the xref/context family, this path only needs loaded-at metadata and a sorted path list, so it is a clean low-risk operational slice
- git-based `what_changed` modes already drop the read lock before running external commands, so this closes the remaining `what_changed` lock-breadth gap

## Read Before Work

- [29-T-phase1-persistence-lock-narrowing.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/29-T-phase1-persistence-lock-narrowing.md)
- [36-T-phase1-file-content-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/36-T-phase1-file-content-read-view-capture.md)

## Expected Touch Points

- `src/protocol/tools.rs`
- `src/protocol/format.rs`
- `src/live_index/`

## Deliverable

- an owned timestamp/path view or equivalent capture path for `what_changed` timestamp mode, with focused regression coverage

## Done When

- timestamp mode in `what_changed` captures the metadata it needs under a short read lock and formats after the guard is released
- current public output remains unchanged
- focused tests cover parity or the migrated tool path

## Completion Notes

- migrated `what_changed` timestamp mode off borrowed `&LiveIndex` formatting by adding `LiveIndex::capture_what_changed_timestamp_view()`
- added `WhatChangedTimestampView` in `src/live_index/query.rs` so timestamp mode captures loaded-at seconds and sorted paths under the read lock
- updated `src/protocol/tools.rs` so the timestamp branch captures the owned view under the guard and then calls `format::what_changed_timestamp_view()` after the guard is released
- kept `format::what_changed_result()` as a compatibility wrapper that delegates through the same owned-view path, preserving current public output
- added focused helper and parity coverage, then reran `cargo test what_changed`

## Carry Forward To Next Task

Next task:

- `TBD`

Carry forward:

- the remaining low-risk operational formatter path is `health`
- after `health`, the remaining borrowed formatter-held read paths are mostly the xref/context family and will need dedicated owned result structures

Open points:

- OPEN: decide whether to finish the operational lane with `health` before starting the heavier xref/context view work
