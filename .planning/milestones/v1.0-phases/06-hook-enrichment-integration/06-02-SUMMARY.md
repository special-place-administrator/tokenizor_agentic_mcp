---
phase: 06-hook-enrichment-integration
plan: 02
subsystem: cli-hook
tags: [rust, stdin-routing, tdd, hooks, clap, serde_json]

# Dependency graph
requires:
  - phase: 06-hook-enrichment-integration
    plan: 01
    provides: "Enriched sidecar handlers with formatted text responses for /outline /impact /symbol-context /repo-map"
provides:
  - "HookInput/HookToolInput serde structs for Claude Code PostToolUse stdin payload"
  - "parse_stdin_input() reading stdin, unwrap_or_default fail-open"
  - "relative_path() stripping cwd prefix and normalizing backslashes to forward slashes"
  - "HookSubcommand::Write variant routing to /impact?new_file=true"
  - "run_hook taking Option<&HookSubcommand> — None triggers stdin-routing mode"
  - "Single PostToolUse init entry with Read|Edit|Write|Grep matcher and no subcommand suffix"
  - "Auto-migration of old 3-entry format (hook read/edit/grep) to single entry on merge"
affects:
  - "All future hook invocations now read stdin JSON for routing (no env vars)"
  - "tokenizor init now writes 2 entries total: 1 PostToolUse + 1 SessionStart"

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Stdin-routing pattern: run_hook reads stdin before deciding which sidecar endpoint to call"
    - "Fail-open stdin parsing: serde_json::from_str unwrap_or_default returns empty HookInput on any failure"
    - "Option<HookSubcommand> in clap command: None triggers stdin-routing, Some triggers backward-compat explicit routing"
    - "Auto-migration via is_tokenizor_entry predicate: 'tokenizor hook' substring matches both old and new commands"

key-files:
  created: []
  modified:
    - src/cli/hook.rs
    - src/cli/mod.rs
    - src/cli/init.rs
    - src/main.rs

key-decisions:
  - "HookInput/HookToolInput are pub(crate) not pub — eliminates private_interfaces warnings, not needed outside crate"
  - "resolve_subcommand_from_input maps tool_name string to HookSubcommand variant — clean separation between parsing and routing"
  - "Unknown tool_name routes to /health as fail-open endpoint — returns health JSON harmlessly, Claude Code sees non-empty additionalContext which is acceptable"
  - "endpoint_for signature changed to Option<&HookSubcommand> + &HookInput — both stdin context and explicit subcommand available at routing time"
  - "Auto-migration requires zero extra code: is_tokenizor_entry('tokenizor hook') substring already matches 'tokenizor hook read', 'tokenizor hook edit', and 'tokenizor hook' (new format)"
  - "SessionStart entry unchanged: '{binary} hook session-start' — no stdin routing needed for session events"

# Metrics
duration: 5min
completed: 2026-03-10
---

# Phase 6 Plan 2: Hook Enrichment Integration - Stdin JSON Routing Summary

**Replaced Phase 5 env-var shim in hook binary with stdin JSON parsing that routes by tool_name, converts absolute paths to relative, and supports Write tool; updated tokenizor init to write single stdin-routed PostToolUse entry with auto-migration from old 3-entry format.**

## Performance

- **Duration:** ~5 min
- **Started:** 2026-03-10T21:47:44Z
- **Completed:** 2026-03-10T21:52:44Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments

- `HookInput` struct (`tool_name`, `tool_input`, `cwd`) and `HookToolInput` struct (`file_path`, `pattern`, `path`) with `serde::Deserialize + Default` — fail-open on any parse error
- `parse_stdin_input()` reads all stdin lines, concatenates, deserializes with `unwrap_or_default` — no panic on empty or malformed input
- `relative_path(absolute, cwd)` using `Path::strip_prefix` for cross-platform correctness, normalizing result to forward slashes via `.replace('\\', "/")`
- `HookSubcommand::Write` variant added — routes to `/impact?path=<rel>&new_file=true`
- `Hook { subcommand: Option<HookSubcommand> }` — None triggers stdin-routing path in `run_hook`
- `run_hook(Option<&HookSubcommand>)` always reads stdin first; explicit subcommand takes priority over stdin tool_name
- `endpoint_for(Option<&HookSubcommand>, &HookInput)` replaces env var reads with stdin-derived file paths and patterns
- `build_post_tool_use_entries` reduced from 3 entries to 1 with matcher `"Read|Edit|Write|Grep"` and command `"{binary} hook"` (no subcommand suffix)
- Auto-migration: `is_tokenizor_entry("tokenizor hook")` substring predicate already matches old `"tokenizor hook read/edit/grep"` commands — no extra migration logic needed
- 37 CLI unit tests (27 hook + 10 init); 296 total lib tests pass

## Task Commits

Each task was committed atomically:

1. **Task 1: Stdin JSON parsing, tool_name routing, Write subcommand, path conversion** - `bb2d083` (feat)
2. **Task 2: Update init to single stdin-routed entry with auto-migration** - `c7502b2` (feat)

**Plan metadata:** (docs commit below)

## Files Created/Modified

- `src/cli/hook.rs` — Complete rewrite: HookInput/HookToolInput structs, parse_stdin_input, relative_path, resolve_subcommand_from_input, updated run_hook + endpoint_for signatures, 27 unit tests
- `src/cli/mod.rs` — Added Write variant to HookSubcommand; changed Hook.subcommand to Option<HookSubcommand>
- `src/main.rs` — Updated Hook match arm: passes subcommand.as_ref() to run_hook
- `src/cli/init.rs` — build_post_tool_use_entries returns single entry; updated 3 tests + added 2 new tests

## Decisions Made

- `HookInput` structs are `pub(crate)` — eliminates `private_interfaces` compiler warnings; no external access needed
- `endpoint_for` now takes `Option<&HookSubcommand>` and `&HookInput` — both pieces of context available at call site, explicit subcommand wins
- Unknown `tool_name` (None subcommand with no recognized tool) routes to `/health` as fail-open — sidecar returns health JSON, Claude Code processes it gracefully
- Auto-migration is a free side-effect of the existing `is_tokenizor_entry` predicate — no dedicated migration code path needed
- `relative_path` uses `Path::strip_prefix` not string split — handles edge cases like repeated path components, Windows vs Unix separators correctly at OS level

## Deviations from Plan

None - plan executed exactly as written.

The only minor refinement was making helper functions `pub(crate)` instead of `pub` to eliminate `private_interfaces` warnings that appeared when `HookInput` was `pub(crate)` but `endpoint_for` was `pub`. This is a visibility correction, not a behavioral change.

## Issues Encountered

None — all tests passed green on first compile run.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- Hook binary now reads Claude Code's native PostToolUse JSON from stdin — no env var configuration needed
- Write tool fully supported — new files are indexed immediately via `/impact?new_file=true`
- `tokenizor init` writes minimal 2-entry hook config that works with stdin routing
- Old subcommands still work for manual testing (`tokenizor hook read`, `tokenizor hook edit`, etc.)
- Phase 06-03 can extend /stats endpoint integration into the health MCP tool

## Self-Check: PASSED

Files verified:
- `src/cli/hook.rs`: FOUND
- `src/cli/mod.rs`: FOUND
- `src/cli/init.rs`: FOUND
- `src/main.rs`: FOUND

Commits verified:
- `bb2d083`: FOUND (Task 1 — stdin routing)
- `c7502b2`: FOUND (Task 2 — init single entry)

---
*Phase: 06-hook-enrichment-integration*
*Completed: 2026-03-10*
