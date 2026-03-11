---
phase: 02-mcp-tools-v1-parity
plan: 02
subsystem: api
tags: [rmcp, mcp-server, tool-handlers, schemars, rust, live-index]

requires:
  - phase: 02-01
    provides: "format.rs pure formatter functions, SharedIndex type, IndexState variants"
  - phase: 01-liveindex-foundation
    provides: "LiveIndex, SharedIndex, reload(), empty(), index_state(), health_stats()"

provides:
  - "TokenizorServer struct with SharedIndex + ToolRouter fields"
  - "ServerHandler impl via #[tool_handler] macro, get_info() with tools capability"
  - "10 MCP tool handlers: get_file_outline, get_symbol, get_symbols, get_repo_outline, search_symbols, search_text, health, index_folder, what_changed, get_file_content"
  - "Input param structs for all 10 tools with Deserialize + JsonSchema derives"
  - "Loading guard pattern blocking all tools except health when index not ready"

affects:
  - "03-file-watcher"
  - "main.rs rewrite (Phase 04 or binary wiring plan)"
  - "integration tests (tool list, INFR-05)"

tech-stack:
  added:
    - "schemars 1.x (upgraded from 0.8 to match rmcp 1.1.0's transitive schemars 1.2.1)"
  patterns:
    - "#[tool_router(vis = pub(crate))] — makes generated tool_router() fn accessible across module boundary"
    - "loading_guard! macro — DRY pattern for IndexState check at top of every non-health handler"
    - "Extract-then-drop — all handlers acquire lock, extract owned values, drop guard before returning"

key-files:
  created:
    - "src/protocol/tools.rs — #[tool_router] impl with all 10 handlers + input param structs + 16 unit tests"
  modified:
    - "src/protocol/mod.rs — TokenizorServer struct + ServerHandler impl (was stub with just format mod)"
    - "Cargo.toml — schemars upgraded from 0.8 to 1.x"

key-decisions:
  - "schemars 1.x instead of 0.8 — rmcp 1.1.0 uses schemars 1.2.1 transitively; using 0.8 caused trait bound mismatch on Parameters<T>"
  - "#[tool_router(vis = pub(crate))] — splits struct definition (mod.rs) from tool impl (tools.rs) while preserving module encapsulation"
  - "loading_guard! macro instead of inline match — eliminates 6-line boilerplate repeated 9 times, error-proof"
  - "index_folder uses write-lock + guard.reload() directly — consistent with Plan 01 AD: reload() takes &mut self on the lock guard"

patterns-established:
  - "Pattern: Loading guard macro — apply `loading_guard!(guard);` as first statement in every non-health tool handler"
  - "Pattern: Extract-then-drop — never hold RwLockReadGuard across statement boundaries; extract String/usize owned values, drop explicitly"
  - "Pattern: pub(crate) vis on tool_router — required when #[tool_router] impl and struct new() are in different files"

requirements-completed: [TOOL-01, TOOL-02, TOOL-03, TOOL-04, TOOL-05, TOOL-06, TOOL-07, TOOL-08, TOOL-12, TOOL-13, INFR-05]

duration: 15min
completed: 2026-03-10
---

# Phase 2 Plan 02: MCP Tools v1 Parity — Server Wiring Summary

**TokenizorServer with 10 MCP tools wired via rmcp #[tool_router] macros, loading guard pattern, and write-lock index_folder**

## Performance

- **Duration:** ~15 min
- **Started:** 2026-03-10T15:35:00Z
- **Completed:** 2026-03-10T15:43:54Z
- **Tasks:** 2
- **Files modified:** 3 (mod.rs, tools.rs, Cargo.toml)

## Accomplishments

- TokenizorServer struct with SharedIndex, ToolRouter, and project_name fields, wired as rmcp ServerHandler with tools capability
- All 10 MCP tool handlers implemented as thin wrappers: acquire lock, check guard, call format:: function, return String
- Loading guard (IndexState::Empty / Loading / CircuitBreakerTripped) blocks 9 of 10 tools; health always responds
- index_folder acquires write lock and calls guard.reload() in-place — no Arc replacement needed
- get_symbols supports both symbol-name lookup and byte-range code slice in a single batch request
- 16 unit tests covering guard states, all tool delegates, INFR-05 v1-absent assertion, and exact tool count

## Task Commits

1. **Task 1: TokenizorServer struct + ServerHandler impl + pub mod protocol** - `8325190` (feat)
2. **Task 2: All 10 tool handler methods + input param structs** - `aded9b1` (feat)

## Files Created/Modified

- `src/protocol/mod.rs` — TokenizorServer struct, ServerHandler impl, pub mod tools declaration
- `src/protocol/tools.rs` — #[tool_router] impl with all 10 handlers, input param structs, 16 unit tests
- `Cargo.toml` — schemars upgraded from "0.8" to "1" to match rmcp 1.1.0

## Decisions Made

- **schemars 1.x over 0.8**: rmcp 1.1.0 uses schemars 1.2.1 transitively; keeping 0.8 caused "two versions of crate schemars" trait mismatch on `Parameters<T>`. Upgrade was mandatory for compilation.
- **`#[tool_router(vis = "pub(crate)")]`**: Without this, the generated `tool_router()` fn is private to `tools.rs` but `mod.rs::new()` needs to call `Self::tool_router()`. The `vis` attribute makes it accessible while preserving encapsulation from external crates.
- **`loading_guard!` macro**: 9 of 10 handlers need the same 6-line IndexState match block. A declarative macro eliminates copy-paste drift and makes the guard pattern explicit and auditable.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] schemars version mismatch — upgraded to 1.x**
- **Found during:** Task 2 (tool handler compilation)
- **Issue:** Cargo.toml had `schemars = "0.8"` but rmcp 1.1.0 depends on schemars 1.2.1 transitively. Rust resolved both, creating two separate `JsonSchema` traits — the derive on our structs used 0.8's trait, but rmcp's `Parameters<T>` bound required 1.x's trait.
- **Fix:** Changed `schemars = "0.8"` to `schemars = "1"` in Cargo.toml
- **Files modified:** Cargo.toml
- **Verification:** `cargo check` clean, all 118 tests pass
- **Committed in:** aded9b1 (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (Rule 1 - bug / version mismatch)
**Impact on plan:** Required fix for compilation — no scope creep.

## Issues Encountered

- The research note about schemars being "LOW confidence — verify during implementation" correctly predicted this issue. The explicit dep in Cargo.toml had to be bumped to 1.x to unify the two-version split.

## Next Phase Readiness

- TokenizorServer is a complete, compilable rmcp ServerHandler with 10 tools
- Missing: main.rs wiring (`serve_server(server, transport::stdio()).await`) — this is Phase 04 work
- All 10 tool handlers are callable via MCP protocol once main.rs is wired
- Phase 3 (file watcher) can be built independently — LiveIndex already has the reload() interface

---
*Phase: 02-mcp-tools-v1-parity*
*Completed: 2026-03-10*
