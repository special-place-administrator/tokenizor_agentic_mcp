# Project Retrospective

*A living document updated after each milestone. Lessons feed forward into future planning.*

## Milestone: v1.0 — Tokenizor v2 Rewrite

**Shipped:** 2026-03-11
**Phases:** 7 | **Plans:** 22 | **Sessions:** ~10

### What Was Built
- Complete v2 rewrite: in-memory LiveIndex with 13-language tree-sitter parsing
- File watcher for real-time freshness (200ms latency, content-hash dedup)
- Cross-reference extraction (call sites, imports, type usages) across all languages
- HTTP sidecar + PostToolUse hooks enriching Read/Edit/Write/Grep automatically
- Trigram text search, scored symbol ranking, file tree navigation
- Postcard-serialized persistence for instant restart

### What Worked
- **2-task-per-plan granularity**: Every plan had exactly 2 tasks (implement + test). Simple, predictable, fast to execute.
- **Strict phase dependencies**: No phase started before its predecessor was proven with integration tests. Zero rework from dependency gaps.
- **Content-hash skip in watcher**: Prevented redundant reparse on editor save events that don't change content. Huge reliability win.
- **ABI-14 grammar pinning**: Identified tree-sitter ABI incompatibility early in Phase 7 and pinned grammars rather than upgrading. Shipped 13 languages without blocked on upstream.
- **Integration tests as phase gates**: Each phase ended with end-to-end tests proving requirements. Caught wiring bugs immediately.

### What Was Inefficient
- **REQUIREMENTS.md checkboxes not auto-updated**: PLSH-04/PLSH-05 were implemented but not checked off. Manual sync between code commits and requirement status is error-prone.
- **STATE.md accumulated stale data**: Performance metrics table never properly populated. The structured frontmatter was duplicated (two YAML blocks). Better automation needed.
- **Phase 7 plan numbering**: Plan 07-04 (additional languages) was added after 07-03 (persistence) but executed before it. Numbering doesn't match execution order.
- **3 pre-existing init_integration.rs failures**: Discovered in Phase 6, deferred. Should have been caught and fixed during Phase 5.

### Patterns Established
- **loading_guard! macro**: Eliminates 6-line IndexState match boilerplate across all tool handlers. Reuse in any future MCP tool.
- **SidecarState bundle**: Axum state bundles index + token_stats + symbol_cache. Pattern for any future sidecar extension.
- **Fail-open hooks**: Hook errors produce empty output (not crashes). Model continues without enrichment. Critical for production reliability.
- **Single stdin-routed PostToolUse entry**: One hook entry routes by tool_name from stdin JSON, replacing the original 3-entry format. Simpler config.

### Key Lessons
1. **Tree-sitter ABI compatibility is the real constraint**, not language support. Any grammar crate must match the host's max ABI. Pin grammar versions to host ABI.
2. **Parasitic integration (hooks) delivers value that standalone MCP tools don't**. Models prefer native tools. Don't fight it — enrich them.
3. **Content-hash gating is essential for file watchers**. OS events fire multiple times per save. Without hash skip, the index thrashes.
4. **Postcard is the right serialization choice for Rust**: Fast, safe (no RUSTSEC advisories), community-recommended, and forward-compatible.
5. **42K lines in 2 days is achievable with AI-driven development** when the architecture is clear, phases are well-scoped, and tests gate each phase.

### Cost Observations
- Model mix: ~80% opus, ~20% sonnet (opus for all phase execution, sonnet for research agents)
- Sessions: ~10 across 2 days
- Notable: The 2-task-per-plan structure kept each session focused and under context limits. No session exceeded 60% context.

---

## Cross-Milestone Trends

### Process Evolution

| Milestone | Sessions | Phases | Key Change |
|-----------|----------|--------|------------|
| v1.0 | ~10 | 7 | Established 2-task-per-plan, integration-test-as-gate, parasitic hook architecture |

### Cumulative Quality

| Milestone | Tests | Coverage | Languages |
|-----------|-------|----------|-----------|
| v1.0 | 385+ | All requirements covered | 13 |

### Top Lessons (Verified Across Milestones)

1. Strict phase dependencies with integration test gates prevent rework
2. Fail-open patterns (hooks, circuit breaker) are essential for production reliability
