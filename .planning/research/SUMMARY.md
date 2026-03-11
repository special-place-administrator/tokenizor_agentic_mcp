# Project Research Summary

**Project:** Tokenizor v2 — In-memory Code Intelligence MCP Server
**Domain:** Rust-native MCP server, parasitic hook integration, tree-sitter symbol extraction
**Researched:** 2026-03-10
**Confidence:** HIGH

## Executive Summary

Tokenizor v2 is a focused rewrite of a Rust-native MCP server that provides code intelligence to AI models (Claude Code, Cursor) via two mechanisms: standard MCP tool calls and PostToolUse hook enrichment. The central architectural bet is "parasitic integration" — hooks that fire automatically after the model's native Read, Edit, and Grep tool calls to inject symbol outlines, cross-reference context, and impact analysis without requiring the model to change its behavior. All symbol data lives in an in-process HashMap-based LiveIndex that answers queries in <1ms from RAM, eliminating the SQLite round-trip overhead that constrains every competing tool. The rewrite removes ~20,000 lines of over-engineered infrastructure (RunManager, checkpoint/resume, SpacetimeDB scaffolding) that accumulated in v1 while delivering no measurable user value.

The recommended approach is to build in strict dependency order: LiveIndex data model first, tree-sitter parsing layer second (kept from v1), then MCP tool handlers, then the file watcher and axum HTTP sidecar in parallel, then hook scripts last. The file watcher (notify + debouncer) is a correctness requirement, not an enhancement — shipping LiveIndex without it recreates jCodeMunch's fatal staleness problem. The axum sidecar on an ephemeral port is the required IPC bridge between hook scripts (Python, external processes) and the in-process LiveIndex.

The primary risks are: (1) infrastructure accumulation repeating v1's mistake — enforce that every new struct or module must trace directly to a concrete performance target; (2) hook latency degrading user experience — all sidecar endpoints must respond <50ms or users will disable hooks before experiencing the value; (3) symbol false positives from naive tree-sitter queries flooding cross-reference results — built-in type filtering and import alias resolution must be designed alongside the query patterns, not retrofitted. The tree-sitter grammar version lock (core at 0.24, not 0.25+) is a non-obvious constraint that must be respected or three grammar crates break simultaneously.

---

## Key Findings

### Recommended Stack

The v2 stack makes targeted additions on top of the existing v1 foundation. All current crates (rmcp 1.1.1, tokio 1.x, tree-sitter 0.24, tracing, serde, anyhow, thiserror, ignore) are kept as-is. Five new crates are required: `dashmap 6` for the sharded-lock concurrent HashMap that powers LiveIndex, `notify 8` + `notify-debouncer-full 0.7` for event-driven file watching (the debouncer is mandatory — raw notify events produce 3-6 duplicate events per editor save), `axum 0.8` for the HTTP sidecar (shares the tokio runtime, zero ceremony), and `bincode 2` for fast binary index persistence on shutdown. The `spacetimedb-sdk`, `fs2`, `num_cpus`, and `tokio-util` crates are all removed. The tree-sitter grammar version constraint is the sharpest operational risk: python, javascript, and go grammar crates have already moved to 0.25.x (requiring `^0.25.8`), while rust and typescript are still at 0.24.x. Upgrading the core requires coordinating all six grammars simultaneously — do not bump any individual grammar crate until a coordinated upgrade plan is ready.

**Core technologies:**
- `rmcp 1.1.1`: MCP stdio protocol — bare JSON lines, no Content-Length framing. Already in use, unchanged.
- `tree-sitter 0.24` (pinned): Symbol + xref extraction for 6 languages. Pin at 0.24 until all grammar crates coordinate upgrade to 0.25+.
- `dashmap 6`: Sharded concurrent HashMap for LiveIndex — 8-16x lower contention than `Arc<RwLock<HashMap>>` under overlapping read/write workloads.
- `notify 8` + `notify-debouncer-full 0.7`: Event-driven file watching with 200ms debounce window — one re-index per save, not 3-6.
- `axum 0.8`: HTTP sidecar on ephemeral port (OS-assigned, written to `.tokenizor/sidecar.port`). Shares tokio runtime with MCP server.
- `bincode 2`: Binary serialization for LiveIndex persistence on shutdown. New API vs. 1.x — use `encode_to_vec`/`decode_from_slice`.

### Expected Features

v1 already ships 18 MCP tools and 749 tests. v2 delivers parity plus the hook enrichment layer that v1 lacked entirely. The parasitic hook model is the sole competitive differentiator — no competing tool (jCodeMunch, Code-Index-MCP, SymDex, codebase-memory-mcp) enriches native Read/Edit/Grep results automatically.

**Must have (table stakes):**
- Symbol search, file outline, repo outline — users assume these exist. Already in v1, must be kept.
- Multi-language support (Rust, Python, JS, TS, Go, Java) — minimum credible set for a code intelligence tool.
- Index freshness after file changes — correctness requirement. Stale index is worse than no tool. Killed jCodeMunch.
- `.gitignore` respect — indexing `node_modules/` or `target/` is broken behavior.
- Sub-second query responses — >1s query latency is perceived as broken.
- `tokenizor init` hook installer — hook features are useless without one-command activation.
- Circuit breaker on parse failures — per-file graceful degradation, not full-index crash.

**Should have (competitive):**
- PostToolUse hook enrichment (Read outline, Edit impact, Grep symbol context) — the defining v2 feature. No competitor does this.
- Cross-reference extraction (`find_references`, `find_dependents`) — syntactic via tree-sitter queries, ~85% coverage, no LSP dependency.
- `get_context_bundle` — pre-change impact summary combining symbol, callers, callees in one <100ms query.
- SessionStart repo map injection — sets session context at session open, reduces wasteful model exploration.
- In-process LiveIndex (<1ms queries) — all competitors use SQLite (5-50ms). This headroom matters for hook latency budgets.
- File watcher with <200ms freshness — event-driven (notify crate), not polling. Eliminates the staleness problem.
- Token savings tracking — makes invisible value visible; measurable evidence for users.

**Defer (v3+):**
- Semantic vector search — adds API key dependency, breaks single-binary promise.
- LSP protocol support — heavyweight, language-server-per-process, explicitly out of scope.
- Multi-repo federation — requires index federation and cross-repo reference resolution.
- External database (SpacetimeDB) — justified only for repos >100K files.

### Architecture Approach

The v2 architecture is a single Rust process containing three concurrent services sharing one `Arc<RwLock<LiveIndex>>`: the rmcp stdio MCP handler, the axum HTTP sidecar, and the file watcher event consumer. All three are independent `tokio::spawn` tasks wired at startup with Arc clones. No global state, no dependency injection framework. The parse-outside-lock pattern is the most important discipline: tree-sitter parsing (10-100ms per file) runs without holding any lock; the write lock is acquired only for the <1ms HashMap insert operations. Hook scripts are Python processes that query the sidecar via HTTP; they cannot share in-process memory with Rust. The sidecar port written to `.tokenizor/sidecar.port` is the discovery mechanism.

**Major components:**
1. **LiveIndex** (`src/index/`) — central in-memory store (HashMap fields for symbols, refs, trigram index). All reads and writes flow through this single struct. Nothing outside this module mutates it directly.
2. **Indexer Pipeline** (`src/indexing/`) — startup parallel load + incremental single-file reparse. Startup uses bounded `tokio::spawn` parallelism. Incremental reparse must complete in <50ms regardless of repo size.
3. **Tree-sitter Parser + Xref Extractor** (`src/parsing/`, `src/parsing/xref/`) — stateless functions: `parse_file() -> (Vec<SymbolEntry>, Vec<ReferenceRecord>)`. Xref extraction uses `.scm` query files embedded via `include_str!`.
4. **File Watcher** (`src/watcher.rs`) — notify + debouncer sends changed paths to a `tokio::mpsc` channel. Background consumer triggers incremental re-index. Decouples event receipt from index work.
5. **Axum HTTP Sidecar** (`src/sidecar.rs`) — localhost-only, ephemeral port, endpoints: `/outline`, `/refs`, `/impact`, `/reindex`, `/health`. Hook scripts are the sole callers.
6. **Hook Scripts** (`hooks/`) — thin Python: read stdin JSON → HTTP GET sidecar → emit `additionalContext` JSON to stdout. All debug output to stderr only.

### Critical Pitfalls

1. **Infrastructure accumulation (v1 trap)** — every new struct must trace to a concrete performance target. If a proposed module is "just coordination," delete it. Warning signs: new lifecycle manager structs, test setup longer than the test, 5+ files touched per behavior change. Lock the architecture surface area in Phase 1.

2. **Staleness by design (jCodeMunch trap)** — file watcher is a correctness requirement, not an enhancement. Wire it in the same milestone as LiveIndex. Test: edit a file while the server is running, query within 300ms, verify updated symbol appears.

3. **Hook latency blocking the model** — sidecar must respond <50ms. All queries are O(1) HashMap lookups from memory — no scanning, no re-parsing. Set explicit 2-3s timeouts on all hooks. Hooks that update the index (Write, Edit reindex) should use `async: true` since they don't need to block the model. Hooks that inject enrichment context (Read, Grep) must be synchronous.

4. **stdout pollution corrupting MCP protocol** — rmcp uses bare JSON lines on stdout. A single `println!` anywhere in the server binary silently corrupts the JSON-RPC stream. Use `tracing` routed to stderr exclusively. Add a CI test: spawn binary → send `initialize` → pipe stdout to `jq` → fail on non-JSON.

5. **Symbol false positives from naive tree-sitter queries** — `(type_identifier) @ref.type` in TypeScript captures `string`, `number`, `boolean`, and single-letter generics. Build per-language built-in blocklists and per-file import alias maps (`use X as Y` → `find_references(X)` must also search for `Y`) alongside the query patterns, not as a retrofit.

---

## Implications for Roadmap

Based on research, the architecture mandates a strict dependency order. The component graph has a clear critical path (types → LiveIndex → parser → loader → MCP tools) with three independent branches that extend from it (file watcher, HTTP sidecar, hook scripts). This maps naturally to four milestones.

### Phase 1: LiveIndex Foundation

**Rationale:** Everything else depends on this. No MCP tools, no hooks, no watcher without a functional in-memory store. Lock the architecture surface area here to prevent v1-style accumulation.
**Delivers:** `src/index/` module with all HashMap fields, `src/indexing/loader.rs` for startup parallel load, `src/indexing/incremental.rs` for single-file reparse, circuit breaker on parse failures. End state: `cargo test` passes, symbols queryable from RAM.
**Addresses:** In-memory LiveIndex, parallel initial load, `.gitignore` exclusion, file count cap (add here, not later), circuit breaker.
**Avoids:** Infrastructure accumulation — measure lines-of-infrastructure vs. lines-of-domain-logic before this phase closes. Avoids unbounded index growth — gitignore exclusion and hard file count cap are specified in this phase.
**Research flag:** NONE — architecture is fully specified, patterns are standard.

### Phase 2: MCP Tools (v1 Parity)

**Rationale:** With LiveIndex functional, wire the rmcp handlers to query it. This replaces v1's core tools on the new foundation and gives a testable, shippable server. Validates that the LiveIndex API surface is correct before building hooks on top.
**Delivers:** All core MCP tools querying LiveIndex (`get_symbol`, `get_file_outline`, `get_repo_outline`, `search_symbols`, `search_text`). Trigram index for text search. Compact human-readable response formatter. stdout-purity CI test.
**Addresses:** Symbol search, file outline, repo outline, text search, sub-second query responses, stdout safety.
**Avoids:** stdout pollution — CI test added here. Verbose JSON responses — compact formatter defined here.
**Research flag:** NONE — well-documented patterns, all crates already in use.

### Phase 3: File Watcher + Freshness

**Rationale:** File watcher is a correctness requirement, not an optional enhancement. It must ship before hooks, because hooks that enrich stale data are worse than no hooks. Shipping the file watcher before the hook layer ensures hooks are built on top of a fresh index from day one.
**Delivers:** `src/watcher.rs` (notify + debouncer), `src/indexing/incremental.rs` integrated with watcher channel. Verified: exactly one re-index per editor save, symbol updated within 300ms of file save.
**Addresses:** Index freshness (<200ms), debounced re-index (prevents 3-6x storm per save).
**Avoids:** Staleness by design — verified with "edit then query" test sequence. Avoids re-index storm — debouncer is mandatory, tested against real editor behavior.
**Research flag:** NONE — notify + debouncer-full pattern is well-specified in research.

### Phase 4: Cross-Reference Extraction

**Rationale:** Cross-references are required before any hook enrichment involving impact analysis. The tree-sitter xref queries must be written alongside their built-in filters and import alias maps — not retrofitted after. This phase is separate from hook wiring because xref quality validation (false positive rates, alias coverage) needs its own testing pass.
**Delivers:** `src/parsing/xref/` with `.scm` query files for all 6 languages, per-language built-in blocklist, per-file import alias map, `find_references`/`find_dependents` MCP tools, `get_context_bundle` tool.
**Addresses:** Cross-reference extraction, find_references/find_dependents, get_context_bundle.
**Avoids:** Symbol false positives — built-in filters and alias maps designed here, not retrofitted. Verify with TypeScript repo: `find_references("string")` must return <10 results.
**Research flag:** NEEDS RESEARCH — tree-sitter `.scm` query patterns per language need verification. The xref extraction research (`docs/summaries/research-xref-extraction-and-file-watching.md`) covers this but per-language query completeness should be verified during phase planning.

### Phase 5: HTTP Sidecar + Hook Infrastructure

**Rationale:** The axum sidecar is the IPC bridge between hook scripts (external Python processes) and the in-process LiveIndex. It depends on LiveIndex (Phase 1), MCP tools (Phase 2), watcher (Phase 3), and cross-references (Phase 4) all being stable before hooks can deliver correct enrichment. Sidecar latency budget (<50ms) must be validated as a pass/fail criterion before hook scripts are written.
**Delivers:** `src/sidecar.rs` (axum on ephemeral port, endpoints: `/outline`, `/refs`, `/impact`, `/reindex`, `/health`), port discovery via `.tokenizor/sidecar.port`, `tokenizor init` hook installer.
**Addresses:** HTTP sidecar IPC, `tokenizor init`, hook infrastructure.
**Avoids:** Hook latency blocking the model — end-to-end hook round-trip measured at <100ms before hook scripts are connected. Port hardcoding — ephemeral port pattern.
**Research flag:** NONE — axum sidecar pattern is fully specified in ARCHITECTURE.md.

### Phase 6: Hook Enrichment Integration

**Rationale:** Hook scripts are the last layer, dependent on everything below. This phase wires Python hook scripts to sidecar endpoints and validates token budgets. Read and Grep hooks are highest-value (fire most frequently). Edit hook impact analysis depends on cross-references from Phase 4. SessionStart repo map injection is simpler and lower-dependency.
**Delivers:** `hooks/post_read.py` (outline injection, <200 tokens), `hooks/post_grep.py` (symbol context, <100 tokens), `hooks/post_edit.py` (impact analysis, <150 tokens), `hooks/session_start.py` (repo map). Token savings counter per session.
**Addresses:** PostToolUse Read enrichment, Grep enrichment, Edit impact enrichment, SessionStart repo map, token savings tracking.
**Avoids:** Hook enrichment token inflation — token budget enforced as acceptance criteria. `additionalContext` duplication — hooks must not repeat what the native tool already returned.
**Research flag:** NONE for Read/Grep hooks. NEEDS RESEARCH for `additionalContext` exact JSON schema path (nesting varies per hook event type — verify against hooks spec before implementation).

### Phase 7: Polish and Persistence

**Rationale:** Fast restart via serialized LiveIndex and scored relevance ranking are valuable but not blocking. Defer until the core hook value is validated in production.
**Delivers:** `bincode 2` serialization on shutdown/deserialization on startup, git HEAD change invalidation, scored trigram ranking (BM25), `post_write.py` hook, additional language support prep.
**Addresses:** Serialized LiveIndex (fast restart), scored relevance ranking, Write hook enrichment.
**Avoids:** Persistence corruption — deserialization validates; corrupted file falls back to full re-index, not crash.
**Research flag:** NONE — bincode 2 API is fully specified in STACK.md.

### Phase Ordering Rationale

- **Phases 1-3 are non-negotiable in order.** LiveIndex must exist before tools can run, tools must run before watcher freshness can be validated end-to-end. The watcher must ship before hooks to prevent hooks enriching stale data.
- **Phase 4 (xrefs) before Phase 5 (sidecar) because** `get_context_bundle` and Edit impact analysis call xref functions. Wiring the sidecar before xrefs are complete would require the sidecar to partially degrade on its highest-value endpoint.
- **Phase 5 before Phase 6 because** hook scripts have no target without the sidecar. This ordering prevents spending time on Python scripts that cannot be tested.
- **Phase 7 is always last.** Persistence and ranking are quality improvements over a working system, not enablers.
- **The architecture explicitly mandates this order** via the build dependency graph in ARCHITECTURE.md: types → LiveIndex → parser → loader → MCP tools → [watcher | sidecar | hooks].

### Research Flags

Phases likely needing `/gsd:research-phase` during planning:
- **Phase 4 (Cross-Reference Extraction):** Per-language tree-sitter `.scm` query patterns for call sites, imports, and type usages need per-language verification. The general approach is proven but query completeness and false positive rates vary by language. Existing research in `docs/summaries/research-xref-extraction-and-file-watching.md` is a strong start but per-language query review is warranted.
- **Phase 6 (Hook Integration):** The `additionalContext` JSON schema path differs between hook event types. PostToolUse nesting vs. top-level field needs verification against the current hooks spec before implementation. The hooks spec has changed in recent Claude Code releases.

Phases with standard patterns (skip research-phase):
- **Phase 1 (LiveIndex Foundation):** HashMap-based in-memory store with parking_lot RwLock is a standard Rust pattern. dashmap API is well-documented.
- **Phase 2 (MCP Tools):** rmcp tool handler pattern is unchanged from v1. Parsing layer is kept from v1. No new patterns.
- **Phase 3 (File Watcher):** notify + debouncer-full pattern is fully specified in STACK.md and prior research.
- **Phase 5 (HTTP Sidecar):** axum Arc<AppState> pattern with ephemeral port is fully specified in STACK.md and ARCHITECTURE.md.
- **Phase 7 (Persistence):** bincode 2 API is documented and straightforward.

---

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | All versions verified against docs.rs/crates.io. Grammar version constraints confirmed via direct crate inspection. The tree-sitter 0.24 pin is the only non-obvious constraint, well-documented in research. |
| Features | HIGH | Derived from direct competitor analysis (4 tools), Claude Code hooks spec, and v1 post-mortem. Feature dependency graph is explicit and verified. |
| Architecture | HIGH | Based on finalized ROADMAP-v2.md plus prior xref/watcher research. All patterns reference existing code (`src/parsing/` kept from v1, `src/protocol/mcp.rs` partially kept). No speculative patterns. |
| Pitfalls | HIGH | Three pitfalls are derived from actual project history (v1 over-engineering, jCodeMunch staleness). Remaining pitfalls are from current-year sources on the specific failure modes. |

**Overall confidence:** HIGH

### Gaps to Address

- **Tree-sitter grammar upgrade path:** Python, JS, and Go grammar crates already require `^0.25.8`. The current strategy (pin at 0.24 for Rust and TypeScript) means a split dependency. This will need a coordinated all-six-language upgrade before any grammar crate can be updated. Track as a known technical debt item — not a v2 blocker but needs a resolution plan.
- **`additionalContext` nesting schema:** The exact JSON field path for PostToolUse hooks has varied across Claude Code releases. PITFALLS.md calls this out explicitly. Before Phase 6, verify the current schema from `https://code.claude.com/docs/en/hooks` — do not assume the path from any cached source.
- **Hook async support:** PITFALLS.md references `async: true` for hooks that do not need to block the model. This feature should be verified against the current hooks spec during Phase 5 planning — availability and semantics may differ from what the research documents.
- **Windows path normalization:** `ReadDirectoryChangesW` returns `C:\` paths while the index may key on MSYS-style `/c/` paths. This is called out in PITFALLS.md as a known gotcha. Requires explicit handling in the watcher event processor and a Windows-specific test.

---

## Sources

### Primary (HIGH confidence)
- `docs/ROADMAP-v2.md` — finalized architecture decisions, phase breakdown, removed components
- `docs/summaries/research-xref-extraction-and-file-watching.md` — tree-sitter node types, notify crate API, watcher gotchas
- https://docs.rs/dashmap, https://docs.rs/notify, https://docs.rs/notify-debouncer-full, https://docs.rs/axum — verified current versions
- https://docs.rs/tree-sitter-rust, https://docs.rs/tree-sitter-typescript, https://docs.rs/tree-sitter-python — grammar version constraints confirmed
- https://code.claude.com/docs/en/hooks — PostToolUse schema, `additionalContext`, exit code behavior, timeout defaults
- Existing v1 source: `src/parsing/`, `src/protocol/mcp.rs` — confirmed what is kept vs. replaced

### Secondary (MEDIUM confidence)
- rust-analyzer architecture docs (https://rust-analyzer.github.io/book/contributing/architecture.html) — parse-outside-lock pattern
- jCodeMunch GitHub, Code-Index-MCP, SymDex, codebase-memory-mcp — competitor feature analysis
- https://www.shuttle.dev/blog/2025/07/18/how-to-build-a-stdio-mcp-server-in-rust — rmcp stdio pattern confirmation

### Tertiary (LOW confidence)
- https://github.com/tree-sitter/tree-sitter/issues/5013 — 0.26.x release checklist (used to validate 0.25 grammar migration risk, in-progress state)
- https://github.com/modelcontextprotocol/modelcontextprotocol/issues/1576 — token bloat in MCP responses

---
*Research completed: 2026-03-10*
*Ready for roadmap: yes*
