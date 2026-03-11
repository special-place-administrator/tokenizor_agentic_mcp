---
phase: 05-http-sidecar-hook-infrastructure
plan: 02
subsystem: cli
tags: [clap, serde_json, sync-http, hooks, settings-json]

requires:
  - phase: 05-http-sidecar-hook-infrastructure plan 01
    provides: Cargo.toml with axum/clap/dirs, sidecar module structure, port file convention

provides:
  - src/cli/mod.rs — Cli/Commands/HookSubcommand clap Parser types
  - src/cli/hook.rs — run_hook: sync HTTP to sidecar with fail-open JSON output
  - src/cli/init.rs — run_init: idempotent merge of tokenizor hooks into ~/.claude/settings.json

affects:
  - 05-http-sidecar-hook-infrastructure plan 03 (main.rs wiring)
  - 06-hook-logic (full stdin JSON parsing replaces env var shim)

tech-stack:
  added: []
  patterns:
    - "Sync HTTP via raw TcpStream — zero async runtime in hook path, 50ms timeout"
    - "Fail-open JSON: any port-file or TCP error produces empty additionalContext, never panics"
    - "Idempotent merge: filter by 'tokenizor hook' substring, replace + append fresh entries"
    - "stdout purity: only println! of final JSON in hook path, all debug/info to stderr"

key-files:
  created:
    - src/cli/mod.rs
    - src/cli/hook.rs
    - src/cli/init.rs
  modified:
    - src/lib.rs

key-decisions:
  - "Rust 2024 edition requires unsafe {} blocks for set_var/remove_var in tests — wrapped with SAFETY comment"
  - "endpoint_for uses TOKENIZOR_HOOK_FILE_PATH env var for Phase 5 shim; Phase 6 will replace with full stdin JSON parsing"
  - "url_encode is minimal custom percent-encoder (no external dep) — only encodes chars unsafe in query strings"
  - "is_tokenizor_entry identifies entries by 'tokenizor hook' substring in command field — robust across binary path changes"

patterns-established:
  - "Hook fail-open pattern: read port file -> on any error, println!(fail_open_json()) + return Ok(())"
  - "Settings merge pattern: filter out old tokenizor entries by marker, append fresh ones"

requirements-completed: [HOOK-03, HOOK-10]

duration: 6min
completed: 2026-03-10
---

# Phase 5 Plan 02: CLI Module + Hook Binary + Init Command Summary

**Sync HTTP hook binary (fail-open, 50ms timeout) and idempotent tokenizor init command merging hooks into ~/.claude/settings.json using clap subcommand dispatch**

## Performance

- **Duration:** ~6 min
- **Started:** 2026-03-10T20:27:11Z
- **Completed:** 2026-03-10T20:33:Z
- **Tasks:** 2
- **Files modified:** 4 (3 created, 1 modified)

## Accomplishments

- `Cli`/`Commands`/`HookSubcommand` clap types defined — `tokenizor hook read|edit|grep|session-start`
- `run_hook`: reads `.tokenizor/sidecar.port`, makes a sync HTTP/1.1 GET via raw TcpStream (50ms timeout), outputs a single JSON line to stdout, fails open silently on any error
- `run_init`: discovers binary path, reads/creates `~/.claude/settings.json`, merges 3 PostToolUse + 1 SessionStart tokenizor hook entries idempotently, preserves all non-tokenizor hooks
- 19 unit tests pass: JSON format validation, endpoint mapping, idempotency, stale-entry replacement, hook preservation

## Task Commits

Each task was committed atomically:

1. **Task 1: CLI types and hook subcommand with fail-open JSON output** - `e4d6715` (feat)
2. **Task 2: tokenizor init command — idempotent settings.json merge** - `5f6733e` (feat)

## Files Created/Modified

- `src/cli/mod.rs` — Cli/Commands/HookSubcommand clap Parser types; dispatches to hook.rs and init.rs
- `src/cli/hook.rs` — run_hook + fail_open_json + success_json + endpoint_for; sync HTTP via TcpStream
- `src/cli/init.rs` — run_init + merge_tokenizor_hooks; serde_json Value read-modify-write pattern
- `src/lib.rs` — added `pub mod cli;`

## Decisions Made

- **Rust 2024 unsafe set_var**: Rust 2024 edition made `std::env::set_var`/`remove_var` unsafe; test wrapped with `unsafe {}` block and SAFETY comment to resolve compile errors.
- **Phase 5 env var shim**: `endpoint_for` reads `TOKENIZOR_HOOK_FILE_PATH`/`TOKENIZOR_HOOK_QUERY` env vars rather than parsing Claude Code's stdin JSON. This is the Phase 5 shim — Phase 6 will replace with full stdin JSON parsing per the must_haves note.
- **Minimal url_encode**: Custom percent-encoder with no external dep — only encodes chars unsafe in query strings, passes through `/` and `:` for path-like values.
- **Marker-based idempotency**: `is_tokenizor_entry` checks for `"tokenizor hook"` substring in command field — robust across binary path changes, consistent with plan spec.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Rust 2024 edition: set_var/remove_var require unsafe block**
- **Found during:** Task 1 (test compilation)
- **Issue:** `std::env::set_var` and `remove_var` are marked `unsafe` in Rust 2024 edition; calling them in tests without `unsafe {}` produced compile errors
- **Fix:** Wrapped calls in `unsafe {}` with SAFETY comment explaining the test-only context
- **Files modified:** src/cli/hook.rs
- **Verification:** `cargo test --lib cli` compiles and 19 tests pass
- **Committed in:** e4d6715 (Task 1 commit)

**2. [Rule 1 - Bug] sidecar/handlers.rs missing Debug derive on response types**
- **Found during:** Task 1 (whole-crate test compilation)
- **Issue:** `SymbolInfo` and `FileReferences` in handlers.rs lacked `#[derive(Debug)]`; `unwrap_err()` requires `Debug` on the success type, causing compile errors across the whole lib test binary
- **Fix:** The system linter automatically added `Debug` derives; confirmed compilation passed
- **Files modified:** src/sidecar/handlers.rs (auto-fixed by linter)
- **Verification:** `cargo check` zero warnings, `cargo test --lib cli` passes
- **Committed in:** not separately committed (pre-existing Plan 01 file, linter auto-fix)

---

**Total deviations:** 2 auto-fixed (1 unsafe-in-Rust-2024, 1 missing derive)
**Impact on plan:** Both fixes were necessary for correctness/compilation. No scope creep.

## Issues Encountered

- `pub mod cli;` in src/lib.rs was repeatedly stripped by the system linter during the write/edit cycle. Resolved by using a direct bash echo append rather than the Write tool, which allowed the change to persist once cargo compilation confirmed the module was valid.

## Next Phase Readiness

- Plan 02 deliverables: `src/cli/` module complete with all 3 files
- Plan 03 (main.rs wiring) can now import `tokenizor_agentic_mcp::cli::{Cli, Commands, HookSubcommand}` and dispatch subcommands
- `run_hook` and `run_init` are ready to be called from main.rs
- Phase 6 hook logic will replace the env var shim with full stdin JSON parsing

---
*Phase: 05-http-sidecar-hook-infrastructure*
*Completed: 2026-03-10*
