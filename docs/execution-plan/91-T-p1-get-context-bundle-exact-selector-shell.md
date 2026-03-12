---
doc_type: task
task_id: 91
title: P1 get_context_bundle exact selector shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 05-P-validation-and-backlog.md
prev_task: 90-T-p1-get-context-bundle-exact-selector-contract-research.md
next_task: 92-T-p1-get-symbol-context-exact-selector-contract-research.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 91: P1 Get Context Bundle Exact Selector Shell

## Objective

- add the first exact-selector `get_context_bundle` shell that chains from current `search_symbols` output while preserving current successful bundle formatting

## Why This Exists

- task 90 fixes the smallest safe contract: keep current `path` and `kind`, add `symbol_line`, and make callers/type usages exact after a single symbol is resolved
- `get_context_bundle` is a current strength and should stay compact and reliable even on common names and duplicate definitions

## Read Before Work

- [90-R-p1-get-context-bundle-exact-selector-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/90-R-p1-get-context-bundle-exact-selector-contract-research.md)
- [90-T-p1-get-context-bundle-exact-selector-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/90-T-p1-get-context-bundle-exact-selector-contract-research.md)
- [41-T-phase1-context-bundle-read-view-capture.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/41-T-phase1-context-bundle-read-view-capture.md)
- [89-T-p1-find-references-exact-selector-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/89-T-p1-find-references-exact-selector-shell.md)

## Expected Touch Points

- `src/protocol/tools.rs`
- `src/live_index/query.rs`
- `src/protocol/format.rs`

## Deliverable

- a first exact-selector `get_context_bundle` shell that accepts `symbol_line`, resolves duplicate same-name symbols deterministically, and keeps successful bundle formatting unchanged

## Done When

- current `get_context_bundle` behavior remains intact for unambiguous lookups
- `symbol_line` can disambiguate duplicate same-name symbols in the same file
- ambiguous duplicate selectors fail deterministically with a stable message
- caller and type-usage sections exclude unrelated same-name hits once a symbol is resolved exactly
- callee extraction remains tied to the selected symbol index
- focused tests cover ambiguity, exact selection, and backward-compatible formatting

## Completion Notes

- extended `get_context_bundle` with optional `symbol_line` so current `search_symbols` output can disambiguate duplicate same-name symbols in one file
- added a stable `AmbiguousSymbol` owned-view variant and formatter path so ambiguity errors stay aligned across the tool and compatibility wrapper
- refactored exact symbol resolution in `src/live_index/query.rs` so `find_references` and `get_context_bundle` share the same selector rules
- switched caller and type-usage bundle sections to the exact-selector dependency-scoped reference flow after a symbol is resolved
- preserved successful bundle formatting, section caps, and the existing under-100ms integration guard
- verification run for this task:
  - `cargo test context_bundle -- --nocapture`
  - `cargo test`

## Carry Forward To Next Task

Next task:

- `92-T-p1-get-symbol-context-exact-selector-contract-research.md`

Carry forward:

- keep this slice separate from stable symbol-id work
- preserve the existing section caps and order
- reuse the exact-selector helper path for the next xref follow-up instead of growing a third selector contract

Resolved point:

- extend exact selection into `get_symbol_context` next, because it still groups by global name and shares the same current ambiguity problem
