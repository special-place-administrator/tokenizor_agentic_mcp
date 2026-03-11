# Tokenizor v2

## What This Is

A Rust-native MCP server that keeps an entire project live in memory, parasitically integrates with Claude Code's native Read/Edit/Write/Grep tools via PostToolUse hooks, and delivers cross-reference-powered retrieval that saves 80-95% of tokens on typical code exploration tasks. Supports 13 languages, persists index to disk for instant restart, and tracks token savings per session.

## Core Value

Measurable token savings (80%+) on multi-file code exploration — the model gets the same understanding with a fraction of the context, and it happens automatically via hooks with zero behavior change required from the model.

## Requirements

### Validated

- ✓ In-memory LiveIndex (HashMap-based, all files + symbols + references in RAM) — v1.0
- ✓ Tree-sitter parsing for 13 languages (Rust, Python, JS, TS, Go, Java, C, C++, C#, Ruby, Kotlin, Dart, Elixir) — v1.0
- ✓ Symbol extraction (functions, structs, enums, classes, methods, constants) — v1.0
- ✓ File discovery with .gitignore respect (ignore crate) — v1.0
- ✓ MCP protocol over stdio (rmcp crate) — v1.0
- ✓ npm distribution packaging — v1.0
- ✓ File watcher (notify crate) keeps index fresh within 200ms — v1.0
- ✓ Incremental single-file re-index in <50ms — v1.0
- ✓ Cross-reference extraction (call sites, imports, type usages) via tree-sitter — v1.0
- ✓ find_references / find_dependents / get_context_bundle MCP tools — v1.0
- ✓ PostToolUse hooks enrich Read (outline), Edit (impact), Write (index), Grep (symbol context) — v1.0
- ✓ SessionStart hook injects compact repo map — v1.0
- ✓ HTTP sidecar (axum) for hook ↔ LiveIndex communication — v1.0
- ✓ Compact human-readable responses matching Read/Grep format — v1.0
- ✓ Token savings tracking per session — v1.0
- ✓ Trigram text search index for instant search — v1.0
- ✓ Scored symbol search with relevance ranking (Exact > Prefix > Substring) — v1.0
- ✓ File tree navigation tool — v1.0
- ✓ Persistence for fast restart (postcard-serialized LiveIndex) — v1.0
- ✓ Circuit breaker + graceful degradation on parse failures — v1.0
- ✓ Stdout purity — zero non-JSON output on stdout (CI gate) — v1.0

### Active

(None — fresh for next milestone)

### Out of Scope

- SpacetimeDB integration — deferred to v3+ if cold start on very large repos becomes a problem
- Full semantic analysis (type inference, trait dispatch, cross-crate resolution) — tree-sitter syntactic matching at ~85% coverage is the deliberate trade-off
- Multi-repo support — one LiveIndex per MCP server process; multi-repo is v3
- Mobile/web UI — this is a CLI tool, no GUI
- Language server protocol (LSP) — we use tree-sitter directly, not rust-analyzer/pyright
- Replacing the model's native tools — we enrich them via hooks, never replace
- Vector/semantic search — complexity without proven value for code navigation
- PHP/Swift/Perl language support — ABI 15 grammars incompatible with tree-sitter 0.24 host; revisit after tree-sitter upgrade

## Context

Shipped v1.0 with 42,334 LOC Rust across 118 files.
Tech stack: Rust, tree-sitter (0.24), rmcp, tokio, axum, notify-debouncer-full, postcard, dashmap.
Branch: `v2-rewrite` off `main`. v1 tagged as `v1-final`.

**Prior art**: jCodeMunch (Python MCP server) inspired this. Tokenizor v1 solved robustness but over-engineered the indexing infrastructure with RunManager, checkpoint/resume, multi-tier trust (~20,000 lines removed in v2).

**Key insight validated**: Models prefer their native Read/Grep/Edit tools. PostToolUse hooks bypass this by enriching native tool results deterministically — the model doesn't need to remember to use custom MCP tools.

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| AD-1: In-process LiveIndex over external DB | Repos <10K files fit in RAM. No IPC overhead. One process. | ✓ Good — O(1) lookups, no serialization overhead on queries |
| AD-2: Parasitic hooks over tool replacement | Models drift from CLAUDE.md instructions. Hooks are deterministic. | ✓ Good — zero model behavior change required |
| AD-3: Syntactic xrefs only (tree-sitter) | 85% coverage in weeks vs 100% requiring months + language servers | ✓ Good — shipped in 1 phase with all 13 languages |
| AD-4: File watcher (notify crate) | Staleness killed jCodeMunch. Watcher eliminates it by construction. | ✓ Good — 200ms freshness, content-hash skip prevents redundant reparse |
| AD-5: Keep circuit breaker, remove run lifecycle | jCodeMunch gets stuck without circuit breaker. Run lifecycle is overkill. | ✓ Good — 20K lines removed, circuit breaker catches bad repos |
| AD-6: Compact responses, not JSON envelopes | Match model's Read/Grep expectations. Every JSON field is wasted tokens. | ✓ Good — responses indistinguishable from native Read output |
| Fresh test suite for v2 | Old 749 tests test old architecture. Port grammar + retrieval conformance. | ✓ Good — 385+ focused tests, no dead test maintenance |
| HTTP sidecar for hooks | axum on localhost:0, port in .tokenizor/sidecar.port. | ✓ Good — <100ms hook latency, shared Arc<LiveIndex> |
| Single repo per process | Simplicity. Multi-repo is v3. | ✓ Good — clean architecture, no multiplexing complexity |
| `tokenizor init` for hook installation | Writes PostToolUse entries into settings.json. Idempotent. | ✓ Good — single-entry format with auto-migration |
| Postcard over bincode for persistence | RUSTSEC-2025-0141 advisory on bincode. Postcard is community-recommended. | ✓ Good — safe, fast, well-maintained |
| ABI-14 grammar pinning | tree-sitter 0.24 host max ABI is 14. Newer grammars (ABI 15) crash. | ✓ Good — 13 languages work, 3 deferred cleanly |

---
*Last updated: 2026-03-11 after v1.0 milestone*
