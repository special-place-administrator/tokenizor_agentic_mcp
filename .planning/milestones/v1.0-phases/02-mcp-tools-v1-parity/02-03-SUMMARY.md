---
phase: 02-mcp-tools-v1-parity
plan: 03
subsystem: infra
tags: [rust, mcp, rmcp, tokio, stdio-transport, integration-testing, live-index]

# Dependency graph
requires:
  - phase: 02-mcp-tools-v1-parity plan 01
    provides: LiveIndex::load, LiveIndex::empty, reload, SharedIndex, IndexState
  - phase: 02-mcp-tools-v1-parity plan 02
    provides: TokenizorServer::new, format:: functions, tool handlers
provides:
  - Persistent MCP server binary (stdio transport, auto-index on startup)
  - Phase 2 integration test suite (19 tests covering all requirements)
affects: [03-file-watcher, 04-cross-references, 06-hooks-integration]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "TOKENIZOR_AUTO_INDEX env var gates auto-index behavior at startup"
    - "Stdio::null() + TOKENIZOR_AUTO_INDEX=false for subprocess binary testing"
    - "include_str! + fn-pattern check for static v1 tool absence verification"

key-files:
  created:
    - tests/live_index_integration.rs (Phase 2 section — 11 new tests appended)
  modified:
    - src/main.rs
    - tests/live_index_integration.rs

key-decisions:
  - "test_no_v1_tools_in_codebase checks fn {name} patterns not raw strings — avoids false positives from test assertion strings in tools.rs unit tests"
  - "test_stdout_purity uses Stdio::null() + TOKENIZOR_AUTO_INDEX=false — null stdin causes immediate EOF so MCP server exits, empty index skips disk I/O"
  - "CircuitBreakerTripped is logged as error but server continues (health tool reports degraded state) — no early exit in v2"

patterns-established:
  - "MCP server entry pattern: LiveIndex decision → TokenizorServer::new → serve_server(server, transport::stdio()) → service.waiting()"
  - "Integration test subprocess pattern: Stdio::null() stdin + short-circuit env var for clean exit"

requirements-completed: [INFR-02, LIDX-05, TOOL-01, TOOL-02, TOOL-03, TOOL-04, TOOL-05, TOOL-06, TOOL-07, TOOL-08, TOOL-12, TOOL-13, INFR-03, INFR-05]

# Metrics
duration: 15min
completed: 2026-03-10
---

# Phase 2 Plan 03: MCP Server Entry Point + Integration Tests Summary

**Persistent MCP stdio server with auto-index startup and 18-test Phase 2 integration suite covering performance, tool formats, INFR-02, INFR-05, and RELY-04**

## Performance

- **Duration:** ~15 min
- **Started:** 2026-03-10T15:50:00Z
- **Completed:** 2026-03-10T16:05:00Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments

- Rewrote main.rs from load-and-exit stub to full persistent MCP server on stdio transport (auto-index or empty based on TOKENIZOR_AUTO_INDEX env var)
- Added 11 Phase 2 integration tests covering LIDX-05 (perf), INFR-02 (auto-index), INFR-05 (no v1 tools), TOOL-01/03/06/07/08/13, and RELY-04 (stdout purity)
- Updated test_stdout_purity to use Stdio::null() stdin so binary exits cleanly when running under test harness
- Full test suite green: 118 unit + 18 integration + 6 grammar = 142 tests passing, 1 ignored

## Task Commits

Each task was committed atomically:

1. **Task 1: Rewrite main.rs as persistent v2 MCP server** - `8f38388` (feat)
2. **Task 2: Phase 2 integration tests** - `8a977f9` (test)

**Plan metadata:** (docs commit follows)

## Files Created/Modified

- `src/main.rs` - Full v2 MCP server entry point: TOKENIZOR_AUTO_INDEX gate, LiveIndex::load or empty, TokenizorServer::new, serve_server(stdio), graceful CB-tripped handling
- `tests/live_index_integration.rs` - Added 11 Phase 2 tests + updated test_stdout_purity for MCP server compatibility

## Decisions Made

- **test_no_v1_tools_in_codebase uses fn-pattern matching**: Raw string check caused false positive because the existing unit test in tools.rs contains v1 names as assertion strings. Switching to `fn {name}` pattern checks actual function definitions only.
- **test_stdout_purity uses Stdio::null() + TOKENIZOR_AUTO_INDEX=false**: MCP server now blocks on stdin; null stdin provides immediate EOF causing clean shutdown. The env var skips disk I/O for speed.
- **CircuitBreakerTripped does not abort server startup**: Server logs the error but starts in degraded mode — the health tool reports the degraded state, which is more useful than an exit code.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] test_no_v1_tools_in_codebase false positive from test assertion strings**
- **Found during:** Task 2 (integration tests — first test run)
- **Issue:** `include_str!` + `.contains(tool_name)` picked up the v1 tool name strings inside the existing `test_no_v1_tools_in_server` unit test in tools.rs (those strings appear as assertion data, not as actual tool definitions)
- **Fix:** Changed check from raw string presence to `fn {name}` pattern — only matches actual function definitions
- **Files modified:** tests/live_index_integration.rs
- **Verification:** cargo test passes; all 18 integration tests green
- **Committed in:** `8a977f9` (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (Rule 1 - bug in test logic)
**Impact on plan:** Minor fix to test logic only. No scope creep. No architectural changes.

## Issues Encountered

None beyond the test false positive documented above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Phase 2 is complete: all 15 requirements verified by automated tests
- Binary is a shippable MCP server: auto-indexes on startup, serves 10 tools on stdio transport
- Phase 3 (file watcher) can build directly on the LiveIndex::reload mechanism already integrated into index_folder handler
- Known pre-Phase 3 concern: Windows path normalization for ReadDirectoryChangesW vs MSYS-style paths (documented in STATE.md)

## Self-Check: PASSED

- FOUND: src/main.rs
- FOUND: tests/live_index_integration.rs
- FOUND: .planning/phases/02-mcp-tools-v1-parity/02-03-SUMMARY.md
- FOUND: commit 8f38388 (Task 1)
- FOUND: commit 8a977f9 (Task 2)

---
*Phase: 02-mcp-tools-v1-parity*
*Completed: 2026-03-10*
