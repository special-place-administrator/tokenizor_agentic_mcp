# Requirements: Tokenizor v2

**Defined:** 2026-03-10
**Core Value:** Measurable token savings (80%+) on multi-file code exploration — the model gets the same understanding with a fraction of the context, automatically via hooks.

## v1 Requirements

Requirements for the v2 rewrite release. Each maps to roadmap phases.

### LiveIndex

- [x] **LIDX-01**: All discovered source files loaded into in-memory HashMap on startup
- [x] **LIDX-02**: All tree-sitter extracted symbols stored with O(1) lookup by name, file, and ID
- [x] **LIDX-03**: File content bytes stored in memory — zero disk I/O on read path
- [x] **LIDX-04**: Concurrent access via shared ownership (Arc + concurrent map) — many readers, exclusive writer
- [x] **LIDX-05**: Initial load completes in <500ms for 70 files, <3s for 1,000 files

### Freshness

- [x] **FRSH-01**: File watcher (notify crate) detects file changes within 200ms (debounced)
- [x] **FRSH-02**: Single-file incremental reparse completes in <50ms
- [x] **FRSH-03**: LiveIndex always reflects current disk state — queries never serve stale data
- [x] **FRSH-04**: File creation detected and indexed automatically
- [x] **FRSH-05**: File deletion detected and removed from LiveIndex automatically
- [x] **FRSH-06**: Real-time synchronization — index syncs in milliseconds on any file change, always current and available to the model

### Reliability

- [x] **RELY-01**: Circuit breaker aborts indexing if >20% of files fail parsing
- [x] **RELY-02**: Partial parse on syntax errors — keep previous symbols, log warning
- [x] **RELY-03**: File deletion during edit handled gracefully (no panic/crash)
- [x] **RELY-04**: MCP server stdout purity — zero non-JSON output on stdout (CI gate)

### Cross-References

- [x] **XREF-01**: Call site extraction for all 6 languages (Rust, Python, JS, TS, Go, Java)
- [x] **XREF-02**: Import/dependency tracking — which files import what
- [x] **XREF-03**: Type usage extraction — struct/class/enum references across files
- [x] **XREF-04**: Built-in type filters (string, number, bool) prevent false positives
- [x] **XREF-05**: Alias map support (use X as Y — references via alias are tracked)
- [x] **XREF-06**: Single-letter generic filters (T, K, V) prevent noise
- [x] **XREF-07**: Enclosing symbol tracked for each reference (which function contains the call)
- [x] **XREF-08**: References updated incrementally when a file is re-parsed

### MCP Tools

- [x] **TOOL-01**: get_symbol — lookup by (file, name, kind_filter) from LiveIndex
- [x] **TOOL-02**: get_symbols — batch lookup, supports symbol and code_slice targets
- [x] **TOOL-03**: get_file_outline — ordered symbol list for a file
- [x] **TOOL-04**: get_repo_outline — file list with coverage stats
- [x] **TOOL-05**: search_symbols — substring matching with relevance scoring
- [x] **TOOL-06**: search_text — text search across all indexed files
- [x] **TOOL-07**: health — report LiveIndex stats (files, symbols, watcher status, last update)
- [x] **TOOL-08**: index_folder — trigger full reload of LiveIndex
- [x] **TOOL-09**: find_references — all call sites for a symbol with context snippets
- [x] **TOOL-10**: find_dependents — files that import a given file
- [x] **TOOL-11**: get_context_bundle — one-call full context (symbol + callers + callees + types + imports)
- [x] **TOOL-12**: what_changed — files and symbols modified since timestamp
- [x] **TOOL-13**: get_file_content — serve file content from memory with optional line range

### Hook Integration

- [ ] **HOOK-01**: HTTP sidecar (axum) on localhost:0, port written to .tokenizor/sidecar.port
- [ ] **HOOK-02**: Sidecar shares Arc<LiveIndex> with MCP tools — zero data duplication
- [ ] **HOOK-03**: Hook response latency <100ms total (Python spawn + HTTP + query)
- [ ] **HOOK-04**: PostToolUse(Read) — inject symbol outline + key references for indexed files
- [ ] **HOOK-05**: PostToolUse(Edit) — trigger re-index + inject impact analysis (callers to review)
- [ ] **HOOK-06**: PostToolUse(Write) — trigger index of new file + confirmation
- [ ] **HOOK-07**: PostToolUse(Grep) — inject symbol context for matched lines
- [ ] **HOOK-08**: SessionStart — inject compact repo map (~500 tokens)
- [ ] **HOOK-09**: Hook output token budget enforced (<200 tokens for Read, <100 for Grep)
- [ ] **HOOK-10**: Hook stdout is valid JSON only — no debug output corruption

### Infrastructure

- [ ] **INFR-01**: tokenizor init command writes PostToolUse hooks into .claude/hooks.json (idempotent)
- [x] **INFR-02**: Auto-index on startup if .git exists (configurable via TOKENIZOR_AUTO_INDEX)
- [x] **INFR-03**: Compact response formatter — human-readable output matching Read/Grep style
- [ ] **INFR-04**: Token savings calculation and tracking per session
- [x] **INFR-05**: Removed tools: cancel_index_run, checkpoint_now, resume_index_run, get_index_run, list_index_runs, invalidate_indexed_state, repair_index, inspect_repository_health, get_operational_history, reindex_repository

### Polish

- [ ] **PLSH-01**: Trigram text search index — <10ms for any query on 10,000-file repo
- [ ] **PLSH-02**: Scored symbol search — exact > prefix > substring > word overlap ranking
- [ ] **PLSH-03**: File tree navigation tool — get_file_tree with directory browsing + symbol counts
- [ ] **PLSH-04**: Persistence — serialize LiveIndex to disk on shutdown, load on startup (<100ms)
- [ ] **PLSH-05**: Background hash verification after loading serialized index

### Languages

- [ ] **LANG-01**: Tree-sitter parsing for C (high priority)
- [ ] **LANG-02**: Tree-sitter parsing for C++ (high priority)
- [ ] **LANG-03**: Tree-sitter parsing for C# (medium priority)
- [ ] **LANG-04**: Tree-sitter parsing for Ruby (medium priority)
- [ ] **LANG-05**: Tree-sitter parsing for PHP (medium priority)
- [ ] **LANG-06**: Tree-sitter parsing for Swift (lower priority)
- [ ] **LANG-07**: Tree-sitter parsing for Dart (lower priority)

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### Advanced Intelligence

- **ADVN-01**: Predictive context — "you'll need these files next" based on import graph analysis
- **ADVN-02**: Session awareness — track model's working set, avoid re-sending already-seen context
- **ADVN-03**: Semantic cross-references via language servers (rust-analyzer, pyright)

### Distribution

- **DIST-01**: Multi-repo support — multiple LiveIndex instances per server process
- **DIST-02**: SpacetimeDB backing store for durability + multi-session coordination

## Out of Scope

| Feature | Reason |
|---------|--------|
| SpacetimeDB integration | Deferred to v3+ — in-process LiveIndex sufficient for single-session |
| Full semantic analysis | Type inference, trait dispatch, cross-crate resolution adds months of complexity for 15% coverage gain |
| Language Server Protocol | We use tree-sitter directly — LSP adds external process management |
| Web/mobile UI | CLI-only tool, no GUI needed |
| Replacing native tools | We enrich Read/Edit/Grep via hooks, never replace them |
| Vector/semantic search | Complexity without proven value for code navigation |
| Multi-session coordination | Single MCP server process per session for v2 |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| LIDX-01 | Phase 1 | Complete |
| LIDX-02 | Phase 1 | Complete |
| LIDX-03 | Phase 1 | Complete |
| LIDX-04 | Phase 1 | Complete |
| LIDX-05 | Phase 2 | Complete |
| FRSH-01 | Phase 3 | Complete |
| FRSH-02 | Phase 3 | Complete |
| FRSH-03 | Phase 3 | Complete |
| FRSH-04 | Phase 3 | Complete |
| FRSH-05 | Phase 3 | Complete |
| FRSH-06 | Phase 3 | Complete |
| RELY-01 | Phase 1 | Complete |
| RELY-02 | Phase 1 | Complete |
| RELY-03 | Phase 3 | Complete |
| RELY-04 | Phase 1 | Complete |
| XREF-01 | Phase 4 | Complete |
| XREF-02 | Phase 4 | Complete |
| XREF-03 | Phase 4 | Complete |
| XREF-04 | Phase 4 | Complete |
| XREF-05 | Phase 4 | Complete |
| XREF-06 | Phase 4 | Complete |
| XREF-07 | Phase 4 | Complete |
| XREF-08 | Phase 4 | Complete |
| TOOL-01 | Phase 2 | Complete |
| TOOL-02 | Phase 2 | Complete |
| TOOL-03 | Phase 2 | Complete |
| TOOL-04 | Phase 2 | Complete |
| TOOL-05 | Phase 2 | Complete |
| TOOL-06 | Phase 2 | Complete |
| TOOL-07 | Phase 2 | Complete |
| TOOL-08 | Phase 2 | Complete |
| TOOL-09 | Phase 4 | Complete |
| TOOL-10 | Phase 4 | Complete |
| TOOL-11 | Phase 4 | Complete |
| TOOL-12 | Phase 2 | Complete |
| TOOL-13 | Phase 2 | Complete |
| HOOK-01 | Phase 5 | Pending |
| HOOK-02 | Phase 5 | Pending |
| HOOK-03 | Phase 5 | Pending |
| HOOK-04 | Phase 6 | Pending |
| HOOK-05 | Phase 6 | Pending |
| HOOK-06 | Phase 6 | Pending |
| HOOK-07 | Phase 6 | Pending |
| HOOK-08 | Phase 6 | Pending |
| HOOK-09 | Phase 6 | Pending |
| HOOK-10 | Phase 5 | Pending |
| INFR-01 | Phase 6 | Pending |
| INFR-02 | Phase 2 | Complete |
| INFR-03 | Phase 2 | Complete |
| INFR-04 | Phase 6 | Pending |
| INFR-05 | Phase 2 | Complete |
| PLSH-01 | Phase 7 | Pending |
| PLSH-02 | Phase 7 | Pending |
| PLSH-03 | Phase 7 | Pending |
| PLSH-04 | Phase 7 | Pending |
| PLSH-05 | Phase 7 | Pending |
| LANG-01 | Phase 7 | Pending |
| LANG-02 | Phase 7 | Pending |
| LANG-03 | Phase 7 | Pending |
| LANG-04 | Phase 7 | Pending |
| LANG-05 | Phase 7 | Pending |
| LANG-06 | Phase 7 | Pending |
| LANG-07 | Phase 7 | Pending |

**Coverage:**
- v1 requirements: 63 total
- Mapped to phases: 63
- Unmapped: 0 ✓

**Phase breakdown:**
- Phase 1 (LiveIndex Foundation): LIDX-01..04, RELY-01, RELY-02, RELY-04 — 7 requirements
- Phase 2 (MCP Tools v1 Parity): LIDX-05, TOOL-01..08, TOOL-12, TOOL-13, INFR-02, INFR-03, INFR-05 — 14 requirements
- Phase 3 (File Watcher + Freshness): FRSH-01..06, RELY-03 — 7 requirements
- Phase 4 (Cross-Reference Extraction): XREF-01..08, TOOL-09, TOOL-10, TOOL-11 — 11 requirements
- Phase 5 (HTTP Sidecar + Hook Infrastructure): HOOK-01, HOOK-02, HOOK-03, HOOK-10 — 4 requirements
- Phase 6 (Hook Enrichment Integration): HOOK-04..09, INFR-01, INFR-04 — 8 requirements
- Phase 7 (Polish and Persistence): PLSH-01..05, LANG-01..07 — 12 requirements

---
*Requirements defined: 2026-03-10*
*Last updated: 2026-03-10 — traceability updated after roadmap creation. RELY-01/02 moved to Phase 1 (belong with foundation), INFR-03 moved to Phase 2 (formatter needed for tool responses), INFR-04 moved to Phase 6 (token tracking only measurable after hooks are live). Total corrected to 63.*
