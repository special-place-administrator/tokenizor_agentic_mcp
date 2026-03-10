---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: milestone
status: completed
stopped_at: Completed 04-cross-reference-extraction/04-03-PLAN.md
last_updated: "2026-03-10T19:30:25.705Z"
last_activity: "2026-03-10 — Phase 03 Plan 03 complete: watcher wired into MCP server, 8 integration tests prove all FRSH/RELY-03 reqs"
progress:
  total_phases: 7
  completed_phases: 4
  total_plans: 12
  completed_plans: 12
  percent: 30
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-10)

**Core value:** Measurable token savings (80%+) on multi-file code exploration — automatically via hooks, zero model behavior change required
**Current focus:** Phase 3 — File Watcher + Freshness

## Current Position

Phase: 3 of 7 (File Watcher + Freshness)
Plan: 3 of 3 completed in current phase (PHASE COMPLETE)
Status: Phase 03 complete — Phase 04 (Cross-References) is next
Last activity: 2026-03-10 — Phase 03 Plan 03 complete: watcher wired into MCP server, 8 integration tests prove all FRSH/RELY-03 reqs

Progress: [███░░░░░░░] 30%

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
| Phase 02-mcp-tools-v1-parity PP02 | 15 | 2 tasks | 3 files |
| Phase 02-mcp-tools-v1-parity P03 | 15 | 2 tasks | 2 files |
| Phase 03-file-watcher-freshness P01 | 5 | 2 tasks | 6 files |
| Phase 03-file-watcher-freshness P02 | 4 | 2 tasks | 1 files |
| Phase 03-file-watcher-freshness P03 | 18 | 2 tasks | 6 files |
| Phase 04-cross-reference-extraction P01 | 13 | 2 tasks | 10 files |
| Phase 04-cross-reference-extraction P02 | 6 | 2 tasks | 2 files |
| Phase 04-cross-reference-extraction P03 | 7 | 2 tasks | 3 files |

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
- [Phase 02-mcp-tools-v1-parity]: schemars 1.x mandatory over 0.8 — rmcp 1.1.0 uses schemars 1.2.1 transitively; two-version split causes trait mismatch on Parameters<T>
- [Phase 02-mcp-tools-v1-parity]: #[tool_router(vis = pub(crate))] splits struct (mod.rs) from tool impl (tools.rs) while allowing Self::tool_router() cross-module call
- [Phase 02-mcp-tools-v1-parity]: loading_guard! macro eliminates 6-line IndexState match boilerplate repeated 9 times across tool handlers
- [Phase 02-mcp-tools-v1-parity]: test_no_v1_tools_in_codebase uses fn-pattern matching not raw strings — avoids false positives from test assertion strings in tools.rs unit tests
- [Phase 02-mcp-tools-v1-parity]: test_stdout_purity uses Stdio::null() + TOKENIZOR_AUTO_INDEX=false — null stdin causes immediate EOF so MCP server exits cleanly under test harness
- [Phase 02-mcp-tools-v1-parity]: CircuitBreakerTripped is logged as error but server continues in degraded mode — health tool reports state, no early exit in v2
- [Phase 03-01]: WatcherState is a separate enum in src/watcher/mod.rs — Plan 02 can import it without the health module
- [Phase 03-01]: health_stats() always returns Off defaults — health_report remains correct without an active watcher
- [Phase 03-01]: health_stats_with_watcher() is additive, not a replacement — callers choose which variant to use at the call site
- [Phase 03-01]: remove_file only updates loaded_at_system if path was present — prevents spurious timestamp churn on phantom events
- [Phase Phase 03-02]: ReindexResult is a local enum — caller can match outcomes without unwrapping bool; enables per-outcome telemetry
- [Phase Phase 03-02]: std::sync::mpsc (not tokio) for notify callback — notify's debouncer thread is a native OS thread, not a tokio task
- [Phase Phase 03-02]: normalize_event_path tries original then \?\-stripped root on strip_prefix failure — handles mixed UNC scenarios
- [Phase 03-03]: recv_timeout(50ms) + yield_now() replaces blocking recv() in run_watcher — blocking std::sync::mpsc::recv() on a tokio worker starves the executor; recv_timeout releases the thread every 50ms
- [Phase 03-03]: health_report_with_watcher is additive alongside health_report — unit tests use Off-defaults variant; production health tool always uses live-watcher variant
- [Phase 03-03]: Integration tests use multi_thread tokio flavor — run_watcher must run on a separate worker thread; single-thread would deadlock
- [Phase Phase 04-01]: streaming-iterator added as direct dep: tree-sitter 0.24 QueryCursor uses StreamingIterator not Iterator — advance()/get() pattern required
- [Phase Phase 04-01]: use_as_clause has path/alias fields (not path/name) per tree-sitter-rust grammar.js
- [Phase Phase 04-01]: FileProcessingResult must be destructured before consuming references via into_iter() to satisfy borrow checker (symbols borrowed in closure)
- [Phase Phase 04-01]: Definition-site filter in from_parse_result: skip references whose byte_range exactly matches a SymbolRecord's byte_range (prevents self-reference noise)
- [Phase Phase 04-02]: Qualified queries use full-scan not reverse_index: reverse_index is keyed by simple name; qualified names like Vec::new must scan files and match qualified_name field
- [Phase Phase 04-02]: collect_refs_for_key as private method: closures capturing self and mut results hit E0521; private method with explicit lifetime annotation solves this
- [Phase Phase 04-02]: is_filtered_name checks all language lists unconditionally: avoids per-file language detection at query time; cross-language repos handled uniformly
- [Phase 04-cross-reference-extraction]: Annotation inline on reference line: is_ref_line check in context loop appends annotation string with padding
- [Phase 04-cross-reference-extraction]: XREF-08 test uses reload not update_file: maybe_reindex is pub(crate); public reload API used in integration tests
- [Phase 04-cross-reference-extraction]: format_ref_section extracted as private helper: eliminates cap-at-20 duplication across Callers/Callees/Type usages sections

### Pending Todos

None yet.

### Blockers/Concerns

- **[Pre-Phase 4]** tree-sitter grammar version split: Python/JS/Go already at ^0.25.8, Rust/TS still at 0.24.x. Coordinated upgrade required before any grammar crate can be individually bumped. Track but not a v2 blocker.
- **[Pre-Phase 6]** `additionalContext` JSON schema path varies across Claude Code releases. Must verify against live hooks spec before Phase 6 implementation begins.
- **[Pre-Phase 3]** Windows path normalization: `ReadDirectoryChangesW` returns `C:\` paths while index may key on MSYS-style `/c/` paths. Needs explicit handling and Windows-specific test in Phase 3.

## Session Continuity

Last session: 2026-03-10T19:30:25.702Z
Stopped at: Completed 04-cross-reference-extraction/04-03-PLAN.md
Resume file: None
