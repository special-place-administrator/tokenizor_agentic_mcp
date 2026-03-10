# Roadmap: Tokenizor v2

## Overview

Seven phases in strict dependency order, derived from the component graph: LiveIndex data model must exist before tools can query it, tools must be queryable before watcher freshness can be validated end-to-end, xref extraction must be complete before hooks can deliver impact analysis, the HTTP sidecar must be running before hook scripts can reach the LiveIndex, and hooks must be wired before token savings can be measured. Phases 1-6 deliver the core system. Phase 7 adds persistence, advanced search ranking, and additional languages on top of a validated working system.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [x] **Phase 1: LiveIndex Foundation** - In-memory store with concurrent access, symbol extraction, circuit breaker (completed 2026-03-10)
- [x] **Phase 2: MCP Tools v1 Parity** - Wire all core tools to LiveIndex, compact responses, stdout purity (completed 2026-03-10)
- [x] **Phase 3: File Watcher + Freshness** - notify crate integration, incremental reparse, staleness eliminated (completed 2026-03-10)
- [x] **Phase 4: Cross-Reference Extraction** - tree-sitter xref queries for all 6 languages, find_references tools (completed 2026-03-10)
- [ ] **Phase 5: HTTP Sidecar + Hook Infrastructure** - axum sidecar on ephemeral port, tokenizor init
- [ ] **Phase 6: Hook Enrichment Integration** - PostToolUse hooks for Read/Edit/Write/Grep, SessionStart, token tracking
- [ ] **Phase 7: Polish and Persistence** - LiveIndex serialization, trigram search, scored ranking, additional languages

## Phase Details

### Phase 1: LiveIndex Foundation
**Goal**: A functional in-memory index that loads all project source files on startup, stores symbols with O(1) lookup, and never panics on bad input
**Depends on**: Nothing (first phase)
**Requirements**: LIDX-01, LIDX-02, LIDX-03, LIDX-04, RELY-01, RELY-02, RELY-04
**Success Criteria** (what must be TRUE):
  1. Running `cargo test` passes with symbols from a real repo queryable from RAM with no disk I/O on the read path
  2. A file with malformed syntax produces a logged warning and keeps its previous symbol set — the server does not crash or corrupt other files
  3. If more than 20% of files fail parsing, the server aborts further indexing and reports the circuit breaker trigger rather than serving partial data silently
  4. The running MCP binary produces zero non-JSON bytes on stdout — piping its output through `jq` succeeds
**Plans:** 3/3 plans complete

Plans:
- [x] 01-01-PLAN.md — Tear down v1 modules, strip domain types, establish v2 module skeleton
- [x] 01-02-PLAN.md — Implement LiveIndex store, discovery, circuit breaker, and query methods
- [x] 01-03-PLAN.md — Wire main.rs, create integration tests, validate stdout purity gate

### Phase 2: MCP Tools v1 Parity
**Goal**: A shippable MCP server where all core retrieval tools query the LiveIndex and return compact, human-readable responses
**Depends on**: Phase 1
**Requirements**: LIDX-02, LIDX-05, TOOL-01, TOOL-02, TOOL-03, TOOL-04, TOOL-05, TOOL-06, TOOL-07, TOOL-08, TOOL-12, TOOL-13, INFR-02, INFR-03, INFR-05
**Success Criteria** (what must be TRUE):
  1. Initial index load completes in under 500ms for a 70-file repo and under 3 seconds for a 1,000-file repo
  2. `get_file_outline`, `get_symbol`, `search_symbols`, and `search_text` all return results in under 1ms from memory
  3. Server auto-indexes on startup when `.git` is present, without any explicit tool call
  4. All v1 over-infrastructure tools (`cancel_index_run`, `checkpoint_now`, `resume_index_run`, and 7 others) are absent from the tool list — the server does not expose them
  5. Tool responses are compact human-readable text matching the style of Claude's native Read output, not verbose JSON envelopes
**Plans:** 3/3 plans complete

Plans:
- [x] 02-01-PLAN.md — LiveIndex extensions (empty, reload, SystemTime) + response formatter module
- [x] 02-02-PLAN.md — MCP server struct + all 10 tool handlers with rmcp macros
- [x] 02-03-PLAN.md — main.rs rewrite (auto-index + serve) + integration tests

### Phase 3: File Watcher + Freshness
**Goal**: The LiveIndex always reflects current disk state — queries never return stale symbols after any file change
**Depends on**: Phase 2
**Requirements**: FRSH-01, FRSH-02, FRSH-03, FRSH-04, FRSH-05, FRSH-06, RELY-03
**Success Criteria** (what must be TRUE):
  1. Saving a file in an editor produces exactly one re-index event (not 3-6 from raw OS events) — verified by watching re-index log entries
  2. After editing a function name and saving, querying that symbol within 300ms returns the updated name, not the old one
  3. Creating a new source file causes it to appear in `get_repo_outline` within 200ms, with no manual reload
  4. Deleting a source file removes its symbols from the index within 200ms without crashing the server
**Plans:** 3/3 plans complete

Plans:
- [x] 03-01-PLAN.md — LiveIndex mutation methods, watcher types, extended HealthStats, Cargo.toml deps
- [x] 03-02-PLAN.md — Watcher core: notify-debouncer-full, path normalization, content hash skip, event loop
- [x] 03-03-PLAN.md — Wire watcher into main.rs + tools, integration tests for all FRSH requirements

### Phase 4: Cross-Reference Extraction
**Goal**: The index tracks call sites, imports, and type usages across all 6 languages so `find_references` returns accurate results with low false-positive rates
**Depends on**: Phase 3
**Requirements**: XREF-01, XREF-02, XREF-03, XREF-04, XREF-05, XREF-06, XREF-07, XREF-08, TOOL-09, TOOL-10, TOOL-11
**Success Criteria** (what must be TRUE):
  1. `find_references("MyStruct")` returns all call sites, type usages, and import locations across the repo, with each result annotated with the enclosing function that contains it
  2. `find_references("string")` on a TypeScript repo returns fewer than 10 results — built-in type filtering is working
  3. `find_dependents("src/foo.rs")` returns only files that actually import or use symbols from that file
  4. `get_context_bundle` for any symbol returns its definition, callers, callees, and type usages in a single response under 100ms
  5. After editing a file, its cross-references update incrementally within the watcher's 200ms window
**Plans:** 3/3 plans complete

Plans:
- [x] 04-01-PLAN.md — Domain types (ReferenceRecord, ReferenceKind), tree-sitter xref extraction for 6 languages, LiveIndex storage extensions with reverse index
- [x] 04-02-PLAN.md — Query methods (find_references_for_name, find_dependents_for_file) with built-in/generic filters, alias resolution, watcher integration
- [x] 04-03-PLAN.md — MCP tool handlers (find_references, find_dependents, get_context_bundle), formatters, integration tests

### Phase 5: HTTP Sidecar + Hook Infrastructure
**Goal**: The axum HTTP sidecar is running on an ephemeral port, reachable by external hook scripts, and `tokenizor init` installs hooks into the Claude Code config in one command
**Depends on**: Phase 4
**Requirements**: HOOK-01, HOOK-02, HOOK-03, HOOK-10
**Success Criteria** (what must be TRUE):
  1. The sidecar starts on a port assigned by the OS, writes that port to `.tokenizor/sidecar.port`, and all sidecar endpoints respond within 50ms for any query
  2. A Python script that reads `.tokenizor/sidecar.port` and calls the `/outline` endpoint gets valid data from the LiveIndex without any in-process memory access
  3. `tokenizor init` writes valid PostToolUse entries into `.claude/hooks.json` — running it twice produces the same result (idempotent)
  4. All sidecar responses are valid JSON with no debug output — running sidecar output through `jq` succeeds
**Plans:** 3 plans

Plans:
- [ ] 05-01-PLAN.md — Cargo.toml deps (axum, clap, dirs), sidecar module (router, handlers, port/PID management, spawn)
- [ ] 05-02-PLAN.md — CLI module (clap dispatch, hook subcommand with fail-open, tokenizor init settings.json merge)
- [ ] 05-03-PLAN.md — Wire main.rs (CLI dispatch + sidecar spawn), integration tests for HOOK-01/02/03/10

### Phase 6: Hook Enrichment Integration
**Goal**: After every native Read, Edit, Grep, and Write call, the model automatically receives symbol context injected by hook scripts without changing its behavior
**Depends on**: Phase 5
**Requirements**: HOOK-04, HOOK-05, HOOK-06, HOOK-07, HOOK-08, HOOK-09, INFR-01, INFR-04
**Success Criteria** (what must be TRUE):
  1. After Claude reads a source file, the response includes a symbol outline and key references injected by the Read hook, within 200 tokens of additional context
  2. After Claude edits a file, the impact hook fires and injects a list of callers that may need review — within 150 tokens and within the 100ms total hook latency budget
  3. After Claude runs Grep, matched lines with associated symbol context appear in the response, within 100 tokens
  4. At session start, the model receives a compact repo map (under 500 tokens) without any tool call
  5. Token savings are tracked per session and accessible — the model can report how many tokens the hooks have saved this session
**Plans**: TBD

### Phase 7: Polish and Persistence
**Goal**: The server restarts in under 100ms by loading a serialized index, search returns ranked results, and additional languages are supported
**Depends on**: Phase 6
**Requirements**: PLSH-01, PLSH-02, PLSH-03, PLSH-04, PLSH-05, LANG-01, LANG-02, LANG-03, LANG-04, LANG-05, LANG-06, LANG-07
**Success Criteria** (what must be TRUE):
  1. After stopping and restarting the server, the index is fully loaded from disk in under 100ms without re-parsing any source files
  2. If the serialized index is corrupted or outdated, the server falls back to full re-index without crashing
  3. `search_symbols "parse"` returns exact matches before prefix matches before substring matches — relevance ranking is observable
  4. C and C++ source files appear in `get_repo_outline` and return symbols from `get_file_outline`, with the same quality as the original 6 languages
**Plans**: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 1 -> 2 -> 3 -> 4 -> 5 -> 6 -> 7

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. LiveIndex Foundation | 3/3 | Complete   | 2026-03-10 |
| 2. MCP Tools v1 Parity | 3/3 | Complete   | 2026-03-10 |
| 3. File Watcher + Freshness | 3/3 | Complete   | 2026-03-10 |
| 4. Cross-Reference Extraction | 3/3 | Complete   | 2026-03-10 |
| 5. HTTP Sidecar + Hook Infrastructure | 0/3 | Not started | - |
| 6. Hook Enrichment Integration | 0/? | Not started | - |
| 7. Polish and Persistence | 0/? | Not started | - |

---
*Roadmap created: 2026-03-10*
*Requirements coverage: 63/63 v1 requirements mapped*
