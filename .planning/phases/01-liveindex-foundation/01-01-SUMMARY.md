---
phase: 01-liveindex-foundation
plan: 01
subsystem: infra
tags: [rust, cargo, tree-sitter, parsing, sha256, domain-types]

# Dependency graph
requires: []
provides:
  - "Compiling v2 codebase skeleton: src/domain/, src/parsing/, src/hash.rs, src/error.rs, src/lib.rs"
  - "Empty stubs for src/live_index/ and src/discovery/"
  - "~20,000 lines of v1 infrastructure removed (application, indexing, protocol, storage)"
  - "Domain types (LanguageId, SymbolRecord, SymbolKind, FileOutcome, FileProcessingResult) with minimal derives"
  - "Cargo.toml cleaned: rayon added, fs2/spacetimedb-sdk/schemars/num_cpus/tokio-util removed"
affects: [02-liveindex-foundation, 03-liveindex-foundation, all-subsequent-phases]

# Tech tracking
tech-stack:
  added: ["rayon = 1.10"]
  patterns:
    - "Domain types use minimal derives (Clone, Debug, PartialEq, Eq) — no serde/JsonSchema until needed"
    - "SHA-256 implementation lives in src/hash.rs as pub(crate) — no external crypto dep"
    - "Module skeleton in lib.rs declares all top-level v2 modules including empty stubs"

key-files:
  created:
    - src/hash.rs
    - src/live_index/mod.rs
    - src/discovery/mod.rs
  modified:
    - src/domain/index.rs
    - src/domain/mod.rs
    - src/error.rs
    - src/lib.rs
    - src/main.rs
    - src/parsing/mod.rs
    - Cargo.toml

key-decisions:
  - "Stub main.rs immediately (v1 code deleted, Plan 03 will rewrite) — not deferring compilation errors"
  - "Keep content_hash field in FileProcessingResult — parsing still computes it, LiveIndex will use it"
  - "digest_hex relocated to src/hash.rs as pub(crate) — identical implementation, no behavioral change"

patterns-established:
  - "Auto-stub blocking files with TODO comment rather than deferring compilation failures"

requirements-completed: [RELY-04]

# Metrics
duration: 15min
completed: 2026-03-10
---

# Phase 1 Plan 01: LiveIndex Foundation — Teardown and Skeleton Summary

**~20,000 lines of v1 infrastructure deleted; v2 compiling skeleton with minimal-derive domain types, rayon dep, and empty live_index/discovery stubs passes cargo check and 18 parsing tests**

## Performance

- **Duration:** ~15 min
- **Started:** 2026-03-10T14:08:00Z
- **Completed:** 2026-03-10T14:23:08Z
- **Tasks:** 2
- **Files modified:** 10 (7 modified, 3 created, 37 deleted)

## Accomplishments

- Deleted all v1 modules: `src/application/`, `src/indexing/`, `src/protocol/`, `src/storage/`, `src/config.rs`, and 9 domain submodules — ~34,700 lines removed across 37 files
- Relocated `digest_hex` + SHA-256 helpers verbatim from `src/storage/sha256.rs` to new `src/hash.rs` (pub(crate)), fixing the parsing module's broken import
- Rewrote `src/domain/index.rs` stripping all serde/JsonSchema derives and removing 14+ v1-only types; kept `LanguageId`, `SupportTier`, `FileProcessingResult`, `FileOutcome`, `SymbolRecord`, `SymbolKind` with all impl blocks intact
- Rewrote `src/error.rs` with 5 v2 variants (Io, Parse, Discovery, CircuitBreaker, Config); dropped 6 v1 variants and added `From<io::Error>`
- Rewrote `src/lib.rs` declaring 7 v2 modules; created empty stubs for `src/live_index/mod.rs` and `src/discovery/mod.rs`
- Cleaned `Cargo.toml`: removed `fs2`, `spacetimedb-sdk`, `schemars`, `num_cpus`, `tokio-util`; added `rayon = "1.10"`
- `cargo check` passes with zero errors; `cargo test --lib parsing` passes 18/18 tests

## Task Commits

Each task was committed atomically:

1. **Task 1: Delete v1 modules, clean Cargo.toml, fix parsing import** - `24140e3` (chore)
2. **Task 2: Rewrite domain/mod.rs, domain/index.rs, error.rs, lib.rs** - `3aa5d92` (feat)

**Plan metadata:** (docs commit below)

## Files Created/Modified

- `src/hash.rs` - Pure SHA-256 implementation (pub(crate) digest_hex, digest, to_hex)
- `src/live_index/mod.rs` - Empty stub, implemented in Plan 02
- `src/discovery/mod.rs` - Empty stub, implemented in Plan 02
- `src/domain/index.rs` - Stripped to 5 kept types + LanguageId impls, ~1,800 lines removed
- `src/domain/mod.rs` - Slim re-export of kept types only
- `src/error.rs` - 5 v2 variants, no is_systemic(), From<io::Error> added
- `src/lib.rs` - 7-module v2 skeleton
- `src/main.rs` - Minimal stub (v1 removed, full rewrite in Plan 03)
- `src/parsing/mod.rs` - Import fixed: crate::storage -> crate::hash
- `Cargo.toml` - rayon added, 5 v1 deps removed

## Decisions Made

- **Stub main.rs immediately:** main.rs referenced v1 types (ApplicationContext, ServerConfig, TokenizorServer, DeploymentReport) that were deleted. Rather than deferring compilation errors to Plan 03, replaced with a 5-line stub so the baseline `cargo check` is clean from this plan forward.
- **Keep content_hash in FileProcessingResult:** The parsing module already computes it; LiveIndex will use it for cache invalidation in Plan 02. No reason to remove and re-add.
- **digest_hex stays pub(crate):** No external crate needs it directly. pub(crate) enforces the "single source of truth" boundary.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Stubbed main.rs to unblock cargo check**
- **Found during:** Task 2 (cargo check after module rewrites)
- **Issue:** src/main.rs referenced v1 types (ApplicationContext, ServerConfig, TokenizorServer, DeploymentReport, ComponentHealth, HealthIssueCategory) that were all deleted in Task 1. cargo check failed with 3 errors.
- **Fix:** Replaced main.rs with a 5-line stub calling observability::init_tracing() and logging a placeholder message. Added TODO comment pointing to Plan 03.
- **Files modified:** src/main.rs
- **Verification:** cargo check passes after stub; plan explicitly states main.rs will be rewritten in Plan 03.
- **Committed in:** 3aa5d92 (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (Rule 3 — blocking)
**Impact on plan:** The fix was necessary to satisfy the plan's primary success criterion (`cargo check` passes). It is exactly what Plan 03 expects — main.rs is a stub awaiting v2 rewrite. No scope creep.

## Issues Encountered

None beyond the auto-fixed main.rs stub.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- v2 compiling skeleton is established — Plans 02 and 03 can proceed
- `src/live_index/` and `src/discovery/` stubs are in place for Plan 02 to fill
- `src/parsing/` is fully operational (18 tests passing, hash import fixed)
- No blockers

---
*Phase: 01-liveindex-foundation*
*Completed: 2026-03-10*
