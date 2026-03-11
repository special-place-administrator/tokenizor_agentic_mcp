---
phase: 06-hook-enrichment-integration
plan: 03
subsystem: testing
tags: [integration-tests, tokio, axum, sidecar, token-savings, health-tool]

# Dependency graph
requires:
  - phase: 06-01
    provides: SidecarState, TokenStats, StatsSnapshot, all 5 enriched HTTP handlers
  - phase: 06-02
    provides: run_hook, HookSubcommand, stdin JSON routing

provides:
  - 12-test integration suite covering HOOK-04 through HOOK-09 and INFR-04
  - format_token_savings() formatter for token savings display
  - MCP health tool with live token savings section via Arc<TokenStats>
  - SidecarHandle.token_stats field exposing TokenStats Arc from spawn_sidecar

affects:
  - phase-07 (final phase — health tool enriched, integration test baseline established)

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "CWD_LOCK(Mutex) pattern for serializing cwd-mutating integration tests (multi_thread flavor)"
    - "raw_http_get_with_status: returns (status_line, body) for 4xx assertion without panic"
    - "SidecarHandle exposes token_stats Arc — caller passes to MCP server, no HTTP round-trip"
    - "format_token_savings returns empty string on all-zeros — fail-open, callers append unconditionally"

key-files:
  created:
    - tests/hook_enrichment_integration.rs
    - .planning/phases/06-hook-enrichment-integration/06-03-SUMMARY.md
  modified:
    - src/protocol/format.rs
    - src/protocol/tools.rs
    - src/protocol/mod.rs
    - src/sidecar/mod.rs
    - src/sidecar/server.rs
    - src/main.rs

key-decisions:
  - "SidecarHandle exposes token_stats Arc: spawn_sidecar retains clone and returns in handle, avoids HTTP round-trip in health tool"
  - "format_token_savings omits zero-fire hook types: only show rows with at least 1 fire, total still sums all"
  - "init_integration.rs failures confirmed pre-existing: verified by stashing plan changes, deferred to deferred-items.md"

patterns-established:
  - "raw_http_get_with_status helper: returns (status_line, body) to assert 4xx without error at transport level"
  - "Integration tests write files to TempDir before spawning sidecar: handlers read from cwd via cwd.join(path)"

requirements-completed: [HOOK-04, HOOK-05, HOOK-06, HOOK-07, HOOK-08, HOOK-09, INFR-04]

# Metrics
duration: 7min
completed: 2026-03-10
---

# Phase 6 Plan 3: Hook Enrichment Integration Summary

**12 end-to-end integration tests proving all 5 hook types (Read/Edit/Write/Grep/SessionStart) plus token savings tracking, and MCP health tool enriched with live hook savings via Arc<TokenStats> direct-share pattern**

## Performance

- **Duration:** 7 min
- **Started:** 2026-03-10T21:57:42Z
- **Completed:** 2026-03-10T22:05:00Z
- **Tasks:** 2
- **Files modified:** 7 (1 created for tests, 6 modified for token savings wiring)

## Accomplishments

- 12 integration tests covering every hook type end-to-end: plain text assertions, budget enforcement, token savings recording, 404 for missing files, grep cap-at-10
- `format_token_savings(snap)` in `format.rs`: formats "Token Savings (this session)" section from `StatsSnapshot`, omits zero-fire hook types, returns empty string when no hooks have fired (fail-open)
- MCP health tool now appends token savings section when sidecar is running; wired via `Arc<TokenStats>` direct-share (no HTTP round-trip) through `SidecarHandle.token_stats`
- 6 unit tests for `format_token_savings` covering all permutations

## Task Commits

1. **Task 1: Integration tests for all 5 hook types, budget enforcement, and token savings** - `c198f14` (test)
2. **Task 2: Wire token savings from sidecar /stats into MCP health tool** - `b8827f8` (feat)

**Plan metadata:** (pending final docs commit)

## Files Created/Modified

- `tests/hook_enrichment_integration.rs` - 12 integration tests (719 lines) covering HOOK-04 through HOOK-09 and INFR-04
- `src/protocol/format.rs` - Added `format_token_savings(snap)` function + 6 unit tests
- `src/protocol/tools.rs` - Updated `health()` handler to append token savings; updated `make_server` helper in tests
- `src/protocol/mod.rs` - Added `token_stats: Option<Arc<TokenStats>>` field to `TokenizorServer`; updated `new()` signature to 5 params
- `src/sidecar/mod.rs` - Added `token_stats: Arc<TokenStats>` field to `SidecarHandle`
- `src/sidecar/server.rs` - Updated `spawn_sidecar` to retain Arc clone of TokenStats and return in handle
- `src/main.rs` - Updated to extract `sidecar_handle.token_stats` and pass to `TokenizorServer::new()`

## Decisions Made

- **SidecarHandle exposes token_stats Arc:** The simpler approach from the plan was chosen — `spawn_sidecar` retains an Arc clone of the `TokenStats` it creates and returns it in `SidecarHandle`. `main.rs` passes this Arc directly to `TokenizorServer::new()`. This avoids an HTTP round-trip in the health tool and prevents cross-process synchronization issues.
- **format_token_savings omits zero-fire hook types:** Only rows with at least 1 fire are displayed. This keeps the savings section clean when only some hook types have been used in the session.
- **init_integration.rs failures confirmed pre-existing:** Verified by stashing all plan changes and running `cargo test --test init_integration` independently — same 3 failures. Deferred to `deferred-items.md`, not caused by Plan 06-03 work.

## Deviations from Plan

None - plan executed exactly as written. The "simpler approach" (direct Arc<TokenStats> share) was explicitly offered as the preferred path in the plan, and it was used.

## Issues Encountered

- `init_integration.rs` has 3 pre-existing failing tests (hook registration expects 3 entries per PostToolUse event, current code registers 1). Confirmed pre-existing via stash test. Logged to `deferred-items.md`.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Phase 06 is now complete: all plans (01, 02, 03) executed
- Phase 07 (if defined) has a full integration test baseline for all hook types
- Token savings are live in both the sidecar `/stats` endpoint and the MCP `health` tool
- All 302 lib tests pass; all 12 hook enrichment integration tests pass; sidecar integration tests pass

---
*Phase: 06-hook-enrichment-integration*
*Completed: 2026-03-10*
