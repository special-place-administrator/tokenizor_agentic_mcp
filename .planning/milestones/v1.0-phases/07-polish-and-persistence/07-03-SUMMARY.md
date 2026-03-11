---
phase: 07-polish-and-persistence
plan: 03
subsystem: persistence
tags: [postcard, serialization, persistence, snapshot, background-verify, shutdown-hook]

# Dependency graph
requires:
  - phase: 07-02
    provides: TrigramIndex field added to LiveIndex; 13 language grammars working

provides:
  - LiveIndex persistence: serialize on shutdown, load in <100ms on startup
  - IndexSnapshot / IndexedFileSnapshot postcard-serializable types
  - serialize_index / load_snapshot / snapshot_to_live_index / stat_check_files / spot_verify_sample
  - background_verify async task: reconciles stale index after loading snapshot
  - Ctrl+C / stdin-EOF shutdown hook serializes index to .tokenizor/index.bin
  - 16 persist unit tests + 3 integration tests

affects: [future phases, MCP server startup performance]

# Tech tracking
tech-stack:
  added:
    - postcard 1.1 (use-std) — compact binary serialization, RUSTSEC-safe bincode replacement
  patterns:
    - Atomic write pattern: write to .tmp then rename to prevent half-written index on crash
    - Snapshot struct pattern: non-serializable LiveIndex fields (Instant, AtomicUsize) excluded; rebuilt on load
    - Background verification: stat-check (mtime/size) + 10% spot hash-verify after snapshot load
    - Signal handling: tokio::select! on service.waiting() | ctrl_c() for clean shutdown

key-files:
  created:
    - src/live_index/persist.rs (IndexSnapshot, serialize_index, load_snapshot, snapshot_to_live_index, stat_check_files, spot_verify_sample, background_verify)
  modified:
    - src/main.rs (startup persistence path, shutdown hook, Ctrl+C signal handling)
    - src/live_index/mod.rs (added pub mod persist)
    - src/domain/index.rs (serde Serialize/Deserialize on LanguageId, SymbolRecord, SymbolKind, ReferenceRecord, ReferenceKind)
    - src/live_index/store.rs (serde Serialize/Deserialize on ParseStatus)
    - Cargo.toml (postcard 1.1, tokio signal feature, postcard dev-dep)
    - tests/live_index_integration.rs (3 new persistence integration tests)

key-decisions:
  - "postcard 1.1 used over bincode: RUSTSEC-2025-0141 advisory on bincode; postcard is community-recommended drop-in"
  - "IndexedFileSnapshot stores mtime_secs (i64 epoch seconds) for stat_check comparison; LiveIndex itself does not store mtime"
  - "background_verify is async fn in persist.rs, not main.rs: keeps main.rs thin, easier to test in isolation"
  - "Snapshot does NOT include trigram_index or reverse_index: both are cheaply rebuilt from files on load (~0ms)"
  - "serialize_index takes &LiveIndex (caller holds lock): no double-locking, caller manages concurrency"

patterns-established:
  - "Snapshot pattern: create owned snapshot data from LiveIndex fields, serialize, write atomically"
  - "Load-or-reindex: startup always tries snapshot first; falls back to full re-index on None (missing/corrupt/version mismatch)"

requirements-completed: [PLSH-04, PLSH-05]

# Metrics
duration: 25min
completed: 2026-03-11
---

# Phase 7 Plan 03: LiveIndex Persistence Summary

**Postcard-serialized index snapshot with atomic write, version-gated load, background stat+hash verification, and Ctrl+C shutdown hook — cold-start re-parsing eliminated**

## Performance

- **Duration:** ~25 min
- **Started:** 2026-03-11
- **Completed:** 2026-03-11
- **Tasks:** 2 completed
- **Files modified:** 7

## Accomplishments

- Created `src/live_index/persist.rs` (300+ lines) with full snapshot lifecycle: serialize, load, convert, stat-check, spot-verify, background reconciliation
- Wired persistence into `main.rs`: startup loads from `.tokenizor/index.bin` if available (under 100ms vs full parse), falls back to full re-index on missing/corrupt/version-mismatch
- Added `background_verify` async task: after loading snapshot, stat-checks all files (mtime+size) and spot-verifies 10% by content hash, re-parsing any stale files without blocking queries
- Added shutdown hook: `tokio::select!` on `service.waiting() | ctrl_c()` followed by `persist::serialize_index` to `.tokenizor/index.bin` (atomic write via tmp+rename)
- Added `serde::Serialize/Deserialize` derives to 5 domain types: `LanguageId`, `SymbolRecord`, `SymbolKind`, `ReferenceRecord`, `ReferenceKind`, `ParseStatus`
- 19 new tests: 16 persist unit tests + 3 integration tests — all pass; 401 total lib tests pass

## Task Commits

Each task was committed atomically:

1. **Task 1: Create persistence module with snapshot types and serialize/deserialize** - `2fe7168` (feat)
2. **Task 2: Wire persistence into main.rs with shutdown hook and startup load path** - `4c07981` (feat)

**Plan metadata:** (docs commit follows)

## Files Created/Modified

- `src/live_index/persist.rs` - New: IndexSnapshot, IndexedFileSnapshot, serialize_index, load_snapshot, snapshot_to_live_index, stat_check_files, spot_verify_sample, background_verify; 16 unit tests
- `src/main.rs` - Modified: startup tries persist::load_snapshot first; shutdown serializes on signal/EOF
- `src/live_index/mod.rs` - Modified: added pub mod persist
- `src/domain/index.rs` - Modified: serde derives on LanguageId, SymbolRecord, SymbolKind, ReferenceRecord, ReferenceKind
- `src/live_index/store.rs` - Modified: serde derives on ParseStatus
- `Cargo.toml` - Modified: postcard 1.1, tokio signal feature, postcard dev-dep
- `tests/live_index_integration.rs` - Modified: 3 persistence integration tests added

## Decisions Made

- **postcard over bincode**: RUSTSEC-2025-0141 marks bincode as unmaintained; postcard is the community-recommended replacement with near-identical API and active maintenance
- **mtime_secs in IndexedFileSnapshot**: LiveIndex doesn't store mtime at runtime; snapshot adds this field only for stat-check purposes without polluting the live type
- **background_verify in persist.rs**: keeps concerns together; main.rs stays thin (just orchestrates)
- **Rebuilt trigram + reverse indices on load**: both are O(files) to rebuild from content and references; storing them would balloon snapshot size with no benefit
- **tokio signal feature added to tokio dep**: required for `tokio::signal::ctrl_c()` in shutdown path

## Deviations from Plan

None — plan executed exactly as written. One minor structural choice: `background_verify` placed in `persist.rs` rather than as a local function in `main.rs` (cleaner separation of concerns, no behavioral difference).

## Issues Encountered

None — postcard serialization of all types worked on first attempt after adding serde derives. The `mtime_secs` field (plan item 3 note) was added to `IndexedFileSnapshot` as specified.

## User Setup Required

None — no external service configuration required. `.tokenizor/index.bin` is created automatically on first clean shutdown.

## Next Phase Readiness

- Phase 7 complete: all 3 plans (01, 02, 03, 04) done
- LiveIndex now has: trigram search (07-02), 13 language grammars (07-04), persistence (07-03)
- No blockers

---
*Phase: 07-polish-and-persistence*
*Completed: 2026-03-11*
