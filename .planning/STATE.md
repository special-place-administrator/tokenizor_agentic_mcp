---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: milestone
status: planning
stopped_at: Completed 02-mcp-tools-v1-parity-02-01-PLAN.md
last_updated: "2026-03-10T15:35:02.209Z"
last_activity: 2026-03-10 — Roadmap created, requirements mapped, STATE initialized
progress:
  total_phases: 7
  completed_phases: 1
  total_plans: 6
  completed_plans: 4
  percent: 0
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-10)

**Core value:** Measurable token savings (80%+) on multi-file code exploration — automatically via hooks, zero model behavior change required
**Current focus:** Phase 1 — LiveIndex Foundation

## Current Position

Phase: 1 of 7 (LiveIndex Foundation)
Plan: 0 of ? in current phase
Status: Ready to plan
Last activity: 2026-03-10 — Roadmap created, requirements mapped, STATE initialized

Progress: [░░░░░░░░░░] 0%

## Performance Metrics

**Velocity:**
- Total plans completed: 0
- Average duration: -
- Total execution time: 0 hours

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| - | - | - | - |

**Recent Trend:**
- Last 5 plans: -
- Trend: -

*Updated after each plan completion*
| Phase 01-liveindex-foundation P01 | 15 | 2 tasks | 10 files |
| Phase 01-liveindex-foundation P02 | 5 | 2 tasks | 4 files |
| Phase 01-liveindex-foundation P03 | 15 | 2 tasks | 4 files |
| Phase 02-mcp-tools-v1-parity P01 | 7 | 2 tasks | 7 files |

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- AD-1: In-process LiveIndex (Arc<DashMap>) is primary store — no external DB
- AD-2: Parasitic hooks, not tool replacement — PostToolUse enriches Read/Edit/Grep
- AD-3: Syntactic xrefs only via tree-sitter (~85% coverage, no LSP dependency)
- AD-4: File watcher (notify + debouncer) — must ship with Phase 3, not after
- AD-5: Keep circuit breaker, remove run lifecycle (~20,000 lines removed)
- AD-6: Compact human-readable responses, not JSON envelopes
- [Phase 01-01]: Stub main.rs immediately when v1 types deleted — keeps cargo check green from Plan 01 forward, Plan 03 will fully rewrite
- [Phase 01-01]: Keep content_hash in FileProcessingResult — parsing already computes it, LiveIndex will use it for cache invalidation
- [Phase 01-01]: digest_hex relocated to src/hash.rs as pub(crate) — single source of truth, no external crate access needed
- [Phase 01-02]: Query methods take &LiveIndex not &SharedIndex — prevents re-entrant RwLock deadlocks, enforced by type system
- [Phase 01-02]: CircuitBreakerState::new(threshold) for testability; from_env() reads TOKENIZOR_CB_THRESHOLD env var
- [Phase 01-02]: Content bytes stored for all files including failed-parse files (LIDX-03) — zero disk I/O on read path
- [Phase 01-02]: LiveIndex::load is sync (runs before tokio runtime) — Rayon handles internal parallelism
- [Phase 01-03]: Gate retrieval_conformance.rs with #![cfg(feature = v1)] inner attribute — v1 types removed, file preserved for history, v2 conformance tests follow in Phase 2
- [Phase 01-03]: Use CircuitBreakerState::new(threshold) directly in threshold tests — env vars are process-global and flaky in parallel test runs
- [Phase 01-03]: Stdout purity RELY-04 CI gate implemented as test_stdout_purity: spawns binary as subprocess, asserts stdout is empty
- [Phase 02-mcp-tools-v1-parity]: IndexState::Empty is a first-class variant checked before Ready/CB — empty() is structurally distinct from loading
- [Phase 02-mcp-tools-v1-parity]: reload() validates path exists before discover_files — ignore crate silently returns empty on invalid paths
- [Phase 02-mcp-tools-v1-parity]: format.rs functions accept &LiveIndex directly — no intermediate DTOs, maximal composability for tool handlers
- [Phase 02-mcp-tools-v1-parity]: repo_outline accepts project_name parameter — caller provides context, formatter stays pure

### Pending Todos

None yet.

### Blockers/Concerns

- **[Pre-Phase 4]** tree-sitter grammar version split: Python/JS/Go already at ^0.25.8, Rust/TS still at 0.24.x. Coordinated upgrade required before any grammar crate can be individually bumped. Track but not a v2 blocker.
- **[Pre-Phase 6]** `additionalContext` JSON schema path varies across Claude Code releases. Must verify against live hooks spec before Phase 6 implementation begins.
- **[Pre-Phase 3]** Windows path normalization: `ReadDirectoryChangesW` returns `C:\` paths while index may key on MSYS-style `/c/` paths. Needs explicit handling and Windows-specific test in Phase 3.

## Session Continuity

Last session: 2026-03-10T15:35:02.207Z
Stopped at: Completed 02-mcp-tools-v1-parity-02-01-PLAN.md
Resume file: None
