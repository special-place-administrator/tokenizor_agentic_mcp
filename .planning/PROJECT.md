# Tokenizor v2

## What This Is

A Rust-native MCP server that keeps an entire project live in memory, parasitically integrates with the coding CLI's native Read/Edit/Grep tools via PostToolUse hooks, and delivers cross-reference-powered retrieval that saves 80-95% of tokens on typical code exploration tasks. This is a full rewrite of tokenizor v1 on a new branch (`v2-rewrite`), stripping 20,000 lines of over-engineered infrastructure and replacing it with ~6,000 lines of focused code intelligence.

## Core Value

Measurable token savings (80%+) on multi-file code exploration — the model gets the same understanding with a fraction of the context, and it happens automatically via hooks with zero behavior change required from the model.

## Requirements

### Validated

- ✓ Tree-sitter parsing for 6 languages (Rust, Python, JS, TS, Go, Java) — existing, battle-tested
- ✓ Symbol extraction (functions, structs, enums, classes, methods, constants) — existing
- ✓ File discovery with .gitignore respect (ignore crate) — existing
- ✓ MCP protocol over stdio (rmcp crate) — existing
- ✓ npm distribution packaging — existing

### Active

- [ ] In-memory LiveIndex (HashMap-based, all files + symbols + references in RAM)
- [ ] File watcher (notify crate) keeps index fresh within 200ms of any change
- [ ] Incremental single-file re-index in <50ms
- [ ] Cross-reference extraction (call sites, imports, type usages) via tree-sitter queries
- [ ] find_references / find_dependents / get_context_bundle MCP tools
- [ ] PostToolUse hooks enrich Read (outline), Edit (impact analysis), Write (index), Grep (symbol context)
- [ ] SessionStart hook injects compact repo map
- [ ] HTTP sidecar (axum) for hook ↔ LiveIndex communication
- [ ] Compact human-readable responses matching Read/Grep format
- [ ] Token savings tracking per session
- [ ] Trigram text search index for instant search
- [ ] Scored symbol search with relevance ranking
- [ ] File tree navigation tool
- [ ] Persistence for fast restart (serialized LiveIndex on shutdown)
- [ ] Additional language support (C, C++, C#, Ruby, PHP, Swift, Dart)
- [ ] Circuit breaker + graceful degradation on parse failures

### Out of Scope

- SpacetimeDB integration — deferred to v3+ if cold start on very large repos becomes a problem
- Full semantic analysis (type inference, trait dispatch, cross-crate resolution) — tree-sitter syntactic matching at ~85% coverage is the deliberate trade-off
- Multi-repo support — one LiveIndex per MCP server process; multi-repo is v3
- Mobile/web UI — this is a CLI tool, no GUI
- Language server protocol (LSP) — we use tree-sitter directly, not rust-analyzer/pyright
- Replacing the model's native tools — we enrich them via hooks, never replace

## Context

- **Prior art**: jCodeMunch (Python MCP server) inspired this. Its approach (tree-sitter → byte offsets → seek+read) is correct but its implementation is unreliable (indexing gets stuck, data always stale). Tokenizor v1 solved robustness but over-engineered the indexing infrastructure.
- **Key insight**: Models prefer their native Read/Grep/Edit tools. MCP tools are second-class. PostToolUse hooks bypass this by enriching native tool results deterministically.
- **v1 state**: 18 MCP tools, 749 tests, ~27,000 lines. Works but doesn't deliver value — no cross-references, no freshness, verbose JSON responses, model doesn't use the tools voluntarily.
- **Branch**: `v2-rewrite` off `main`. v1 tagged as `v1-final`.

## Constraints

- **Tech stack**: Rust, tree-sitter, rmcp, tokio, axum — all already in use or lightweight additions
- **Performance**: All queries <1ms from memory, hook responses <100ms, initial load <500ms for 70 files
- **Compatibility**: Must work as stdio MCP server for Claude Code. npm distribution.
- **Platform**: Windows (MSYS2/Git Bash) primary dev environment, must also work on Linux/macOS
- **Hook format**: Claude Code PostToolUse hooks — stdin JSON, stdout JSON with `additionalContext`

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| AD-1: In-process LiveIndex over external DB | Repos <10K files fit in RAM. No IPC overhead. One process. | — Pending |
| AD-2: Parasitic hooks over tool replacement | Models drift from CLAUDE.md instructions. Hooks are deterministic. | — Pending |
| AD-3: Syntactic xrefs only (tree-sitter) | 85% coverage in weeks vs 100% requiring months + language servers | — Pending |
| AD-4: File watcher (notify crate) | Staleness killed jCodeMunch. Watcher eliminates it by construction. | — Pending |
| AD-5: Keep circuit breaker, remove run lifecycle | jCodeMunch gets stuck without circuit breaker. But run lifecycle is overkill for seconds-long indexing. | — Pending |
| AD-6: Compact responses, not JSON envelopes | Match model's Read/Grep expectations. Every JSON field is wasted tokens. | — Pending |
| Fresh test suite for v2 | Old 749 tests test old architecture. Port grammar + retrieval conformance, rewrite the rest. | — Pending |
| HTTP sidecar for hooks | axum on localhost:0, port in .tokenizor/sidecar.port. ~50-80ms hook latency. | — Pending |
| Single repo per process | Simplicity. Multi-repo is v3. | — Pending |
| `tokenizor init` for hook installation | Writes PostToolUse entries into .claude/hooks.json. Idempotent. | — Pending |

---
*Last updated: 2026-03-10 after initialization*
