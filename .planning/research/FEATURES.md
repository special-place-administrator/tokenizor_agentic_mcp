# Feature Research

**Domain:** Code intelligence MCP server (Rust-native, parasitic hook integration, tree-sitter)
**Researched:** 2026-03-10
**Confidence:** HIGH

---

## Feature Landscape

### Table Stakes (Users Expect These)

Features users assume exist. Missing these = product feels incomplete or broken.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Symbol search by name | Every code intelligence tool since ctags. "Find where `MyStruct` is defined" is day-one use case. | LOW | Already in v1 via `search_symbols`. Keep. |
| File outline (list symbols in file) | Users need to know what's in a file without reading it whole. Expected from IDEs and LSP since 2016. | LOW | Already in v1 via `get_file_outline`. Keep. |
| Repo-level symbol overview | Understand the full codebase shape before diving in. Expected from any code indexer. | MEDIUM | Already in v1 via `get_repo_outline`. Keep. |
| Multi-language support (6+ languages) | Projects mix languages. Rust+Python, TS+Go are common. Single-language tools feel narrow. | MEDIUM | v1 has 6 languages (Rust, Python, JS, TS, Go, Java). This is the minimum credible set. |
| `.gitignore` respect | Indexing `node_modules/` or `.git/` is broken behavior. Every code tool respects gitignore. | LOW | Already in v1 via `ignore` crate. Keep. |
| Text search across codebase | "Find all occurrences of `deprecated_fn`" is a core workflow. Grep-level capability is expected. | LOW | v1 has this. Trigram index in v2 makes it faster. |
| Sub-second query responses | Users tolerate slow indexing but not slow queries. >1s query = broken in perception. | MEDIUM | v2 target: all queries <1ms from memory. |
| Index freshness after file changes | Stale index produces wrong answers, which is worse than no tool. This is a hard correctness requirement. | HIGH | The failure mode that killed jCodeMunch. File watcher (notify crate) is the answer. |
| MCP stdio protocol compliance | Must work with Claude Code, Cursor, and other MCP clients out of the box. Non-negotiable. | LOW | Already in v1 via `rmcp` crate. |
| npm distribution | Developers install via `npx` or `npm install -g`. Requiring Rust toolchain is a barrier. | LOW | Already in v1. Keep. |
| Circuit breaker on parse failures | A bad file should not crash the server or corrupt the index. Graceful degradation is expected at this maturity level. | LOW | Already in v1. Keep in v2. |
| `tokenizor init` installer | Hook installation must be one command. Manual hook JSON editing is an unacceptable setup experience. | LOW | Writes PostToolUse entries into `.claude/hooks.json` idempotently. |

### Differentiators (Competitive Advantage)

Features that set this product apart. These map directly to Tokenizor v2's core value proposition.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| PostToolUse hook enrichment (parasitic integration) | Enriches the model's native Read/Edit/Grep results automatically — zero behavior change required from the model. No competitor does this. jCodeMunch, Code-Index-MCP, SymDex all require explicit MCP tool calls. | HIGH | The defining architectural bet of v2. Hooks inject `additionalContext` into Read (file outline), Edit (impact analysis), Grep (symbol matches with context). Model gets intelligence without ever calling a tool. |
| Cross-reference extraction (find_references / find_dependents) | Answers "who calls this?" and "what does this call?" without semantic type inference. Every competitor either skips xrefs or requires a language server. Syntactic xrefs via tree-sitter queries cover ~85% of real-world cases at <10% the implementation cost. | HIGH | Core v2 differentiator. Call sites, imports, type usages extracted via tree-sitter queries. No LSP dependency. |
| `get_context_bundle` — pre-change impact summary | Before an Edit fires, return the symbol at point, its callers, its callees, and any files that import it. Compresses a 30-minute exploration task to a single <100ms query. CodeMCP and Axon have blast radius analysis but require explicit tool calls; this fires automatically via PreToolUse hook. | HIGH | Requires cross-reference extraction as a foundation. High complexity, high value. |
| SessionStart repo map injection | Inject a compact repo map at session start — file tree, symbol counts, key entry points. Sets context for the entire session without the model needing to ask. Reduces session-opening token waste by 20-30% on typical tasks. | MEDIUM | SessionStart hook fires once per session. Compact format (<500 tokens for typical repos) is critical. |
| In-process LiveIndex (<1ms query latency) | All queries answer from RAM. No SQLite round-trip, no file seek, no IPC. Competitors use SQLite (Code-Index-MCP, SymDex, codebase-memory-mcp) which adds 5-50ms per query. At hook latency targets of <100ms, this headroom matters. | HIGH | HashMap-based store, all files + symbols + references in memory. Repos <10K files fit easily. |
| File watcher with <200ms freshness | Continuous freshness by construction — not "please re-run index." Notify crate debounces filesystem events, triggers incremental single-file re-index in <50ms. jCodeMunch's fatal flaw was stale data. Freshness is a trust feature. | HIGH | Eliminates the staleness problem that makes users abandon tools. Most competitors use background polling (Code-Index-MCP uses Watchdog with polling intervals), not event-driven watchers. |
| Token savings tracking per session | Show users measurable evidence of value: "Saved ~42K tokens this session." Converts an invisible feature into a visible, shareable win. jCodeMunch published TOKEN_SAVINGS.md as a marketing asset; it worked. | LOW | Counter increments on each hook enrichment. Session summary on SessionEnd. Lightweight to implement. |
| Compact human-readable hook output | Match the format models already understand from Read/Grep. No JSON envelopes, no field metadata, no verbosity. Every wasted token in a hook response is direct cost to the user. | LOW | AD-6 from PROJECT.md. Competitors tend toward structured JSON. This is a discipline/format choice, not a feature gap, but it compounds across every interaction. |
| Scored relevance ranking on symbol search | Return the most relevant matches first when searching `parse`. Trigram index + BM25 scoring beats raw substring match for developer ergonomics. SymDex claims 97% token reduction per lookup; ranking determines whether the first result is the right one. | MEDIUM | Trigram text search index in v2. Scoring based on name similarity + usage frequency. |
| Fast restart via serialized LiveIndex | Cold start on a large repo from scratch is expensive (parsing 10K files). Serialize the index to disk on shutdown, deserialize on startup. Eliminates the "wait for indexing" friction on restarts. | MEDIUM | Store in `.tokenizor/index.bin`. Invalidate on git HEAD change. |

### Anti-Features (Commonly Requested, Often Problematic)

Features that seem good but create problems specific to this project's architecture and goals.

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| Semantic search (vector embeddings) | "Find functions that handle authentication" feels powerful. Code-Index-MCP, SymDex, and CodeIndexer (Zilliz) all offer it. | Requires external embedding model or API key (privacy, cost, latency). Adds a deployment dependency that breaks the "single binary, no API keys" promise. For code navigation, syntactic search + good ranking already covers 90% of use cases. | Scored trigram search with BM25 ranking covers the practical use cases. Semantic search can be a v3 opt-in that requires explicit configuration. |
| LSP protocol support | LSP gives access to rust-analyzer, pyright, typescript-language-server — full type inference and cross-crate resolution. | LSP is a heavyweight dependency: each language server is a separate process, startup time is seconds, stability varies by language. PROJECT.md explicitly scopes this out. The 85% syntactic coverage from tree-sitter is the deliberate trade-off. | Tree-sitter syntactic xrefs for 85% coverage. Flag the 15% gap honestly rather than adding LSP complexity. |
| Multi-repo / monorepo federation | Large orgs have many repos. Code-Index-MCP and SymDex advertise cross-repo search. | One LiveIndex per process is a deliberate simplicity decision. Multi-repo requires index federation, namespace collisions, cross-repo reference resolution — each is a significant problem. PROJECT.md scopes this to v3+. | Document that `tokenizor` can be run once per repo. Shell alias or MCP config handles the "which repo" routing at the client level. |
| Web UI / visualization dashboard | Dependency graphs, call graphs, architecture maps look impressive in demos. | This is a CLI tool. A web UI adds a server, a port, a browser dependency, and zero value to a model-in-the-loop workflow where the model is the consumer. CodeMCP has architecture visualization; nobody uses it during coding sessions. | Provide structured text output that a human could pipe to graphviz if they want visualization. Don't maintain a web server. |
| External database (SpacetimeDB, PostgreSQL) | Persistent storage, queryable from external tools, survives process crashes. | For repos <10K files, an in-process HashMap is faster, simpler, and eliminates all IPC overhead. The only justification is very large repos — that's v3 scope. PROJECT.md explicitly defers this. | Serialized LiveIndex (`.tokenizor/index.bin`) provides persistence. External DB is an escape hatch for v3 if repos exceed the RAM ceiling. |
| Run lifecycle / checkpoint/resume | Resumable indexing for very long jobs, repair modes, operational history. | v1 had this (6,149 lines of RunManager). It added complexity without shipping value — indexing a 10K file repo takes ~500ms. Checkpoint/resume is engineering theater for a task that completes in under a second. | Circuit breaker with graceful degradation handles the actual failure mode (a bad file). No lifecycle needed. |
| Replacing native tools (custom Read/Edit/Grep) | Full control over what the model sees. Some MCP servers replace Read with a smarter version. | Models prefer their native tools. Instructions drift. Replacement tools require the model to actively choose to use them — which it won't reliably do. The entire parasitic architecture exists to avoid this dependency. | PostToolUse enrichment: let the model use its native tools, then inject intelligence into the result. This is the core AD-2 decision. |
| Secret detection / credential scanning | Code-Index-MCP and CodeMCP include this. Seems valuable in a code analysis tool. | Scope creep with legal and accuracy risk. False negatives create false confidence. False positives create noise. A dedicated tool (truffleHog, gitleaks) does this better. | Out of scope. Document the boundary clearly. |

---

## Feature Dependencies

```
[In-memory LiveIndex]
    ├──required by──> [Symbol Search]
    ├──required by──> [File Outline]
    ├──required by──> [Cross-Reference Extraction]
    ├──required by──> [Text Search (trigram index)]
    └──required by──> [Token Savings Tracking]

[Cross-Reference Extraction]
    ├──required by──> [find_references / find_dependents tools]
    ├──required by──> [get_context_bundle]
    └──required by──> [Edit hook impact analysis]

[File Watcher]
    └──required by──> [Index Freshness (<200ms)]
                           └──enables──> [Reliable hook enrichment]

[PostToolUse Hook Infrastructure]
    ├──required by──> [Read hook enrichment (outline injection)]
    ├──required by──> [Edit hook enrichment (impact analysis)]
    ├──required by──> [Write hook enrichment (index update)]
    └──required by──> [Grep hook enrichment (symbol context)]

[HTTP Sidecar (axum)]
    └──required by──> [PostToolUse Hook Infrastructure]
                           (hooks are shell processes, need IPC to reach LiveIndex)

[SessionStart Hook]
    └──required by──> [Repo map injection]

[tokenizor init]
    └──installs──> [PostToolUse Hook Infrastructure]
    └──installs──> [SessionStart Hook]

[Serialized LiveIndex]
    └──enables──> [Fast restart (avoid cold-index on startup)]
    └──depends on──> [In-memory LiveIndex]
```

### Dependency Notes

- **LiveIndex is the foundation**: Symbol search, xref extraction, text search, and all hook enrichments read from it. It must be stable before any hook work begins.
- **Cross-references require LiveIndex + new tree-sitter queries**: The parsing layer (src/parsing/) handles symbol extraction. Xref extraction is new query work on top of it — same framework, new queries per language.
- **HTTP Sidecar is a deployment detail, not a feature**: Hooks are shell processes; they cannot call Rust functions directly. The axum sidecar on `localhost:0` (port written to `.tokenizor/sidecar.port`) is the IPC bridge. It is infrastructure that enables hook features, not a feature itself.
- **get_context_bundle is a composition feature**: It calls `find_references`, `find_dependents`, and `get_file_outline` internally. All three must exist before the bundle tool is useful.
- **tokenizor init enables adoption**: The hook infrastructure is useless if users cannot install it in one command. `tokenizor init` is the activation feature — without it, all hook features remain theoretical for most users.

---

## MVP Definition

Context: v2 is a rewrite on `v2-rewrite` branch. v1 already ships (18 tools, 749 tests). MVP for v2 is the minimum needed to deliver measurable token savings and retire v1.

### Launch With (v2 milestone 1 — Core LiveIndex)

Minimum to replace v1 with a leaner, fresher foundation.

- [ ] In-memory LiveIndex — without it nothing else works
- [ ] File watcher (<200ms freshness) — correctness requirement, not an enhancement
- [ ] Symbol search + file outline + repo outline — parity with v1's core tools
- [ ] Text search with trigram index — parity with v1
- [ ] `tokenizor init` hook installer — required to activate hook-based value
- [ ] Circuit breaker on parse failures — carry forward from v1

### Add After Foundation (v2 milestone 2 — Parasitic Hooks)

The differentiated value. Requires stable LiveIndex from milestone 1.

- [ ] HTTP sidecar (axum) — IPC bridge for hooks
- [ ] Read hook enrichment (inject outline into Read result) — most frequent hook, highest token impact
- [ ] Grep hook enrichment (inject symbol context into Grep result)
- [ ] SessionStart repo map injection — sets session context cheaply

### Add After Hook Validation (v2 milestone 3 — Cross-References)

Unlocks impact analysis and the full xref feature set.

- [ ] Cross-reference extraction (call sites, imports, type usages) — tree-sitter queries per language
- [ ] `find_references` / `find_dependents` MCP tools
- [ ] Edit hook enrichment (impact analysis via `get_context_bundle`)
- [ ] `get_context_bundle` tool
- [ ] Token savings tracking per session

### Future Consideration (v2 milestone 4 — Polish)

Deferred until core is validated.

- [ ] Serialized LiveIndex (fast restart) — nice to have, not required for value delivery
- [ ] Scored relevance ranking beyond basic BM25 — refinement, not launch blocker
- [ ] Additional language support (C, C++, C#, Ruby) — expand after core 6 languages proven
- [ ] Write hook enrichment (index new files on Write) — lower value than Read/Edit/Grep hooks

---

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| In-memory LiveIndex | HIGH | HIGH | P1 |
| File watcher (<200ms freshness) | HIGH | MEDIUM | P1 |
| Symbol search / file outline / repo outline | HIGH | LOW | P1 |
| `tokenizor init` hook installer | HIGH | LOW | P1 |
| PostToolUse Read hook enrichment | HIGH | MEDIUM | P1 |
| Cross-reference extraction | HIGH | HIGH | P1 |
| `find_references` / `find_dependents` | HIGH | MEDIUM | P1 |
| `get_context_bundle` (pre-edit impact) | HIGH | MEDIUM | P1 |
| HTTP sidecar (axum IPC) | HIGH | MEDIUM | P1 (enables all hooks) |
| SessionStart repo map injection | MEDIUM | LOW | P2 |
| Grep hook enrichment | MEDIUM | LOW | P2 |
| Edit hook enrichment | MEDIUM | MEDIUM | P2 |
| Token savings tracking | MEDIUM | LOW | P2 |
| Serialized LiveIndex (fast restart) | MEDIUM | MEDIUM | P2 |
| Scored relevance ranking (BM25) | LOW | MEDIUM | P3 |
| Additional languages (C, C++, C#, Ruby) | LOW | MEDIUM | P3 |
| Write hook enrichment | LOW | LOW | P3 |
| Semantic search (vector embeddings) | LOW | HIGH | Out of scope |
| LSP integration | LOW | HIGH | Out of scope |
| Multi-repo federation | LOW | HIGH | Out of scope (v3) |

**Priority key:**
- P1: Must have for v2 launch — these deliver the core value proposition
- P2: Should have — these complete the experience
- P3: Nice to have — deferred to v2.x or v3

---

## Competitor Feature Analysis

| Feature | jCodeMunch | Code-Index-MCP (ViperJuice) | SymDex | codebase-memory-mcp | Tokenizor v2 |
|---------|------------|----------------------------|--------|---------------------|--------------|
| Symbol search | Yes (tree-sitter, byte offsets) | Yes (BM25, SQLite) | Yes (tree-sitter, 14 languages) | Yes (Go, 64 languages) | Yes (in-memory, <1ms) |
| Cross-references / call graph | No | Partial (type inference) | Yes (call graph) | Yes (call graph, HTTP routes) | Yes (syntactic only, tree-sitter) |
| File watcher / freshness | No (staleness is the fatal flaw) | Yes (Watchdog polling) | Yes (SHA-256 change detection) | Yes (background polling) | Yes (notify crate, event-driven, <200ms) |
| Hook enrichment (parasitic) | No | No | No | No | Yes — the defining feature |
| In-memory store | No (SQLite) | No (SQLite + FTS5) | No (SQLite) | No (SQLite) | Yes (HashMap, all in RAM) |
| Impact analysis / blast radius | No | No | No | Yes (git diff + graph) | Yes (via get_context_bundle) |
| Token savings measurement | Yes (TOKEN_SAVINGS.md) | No | Yes (97% claim) | Yes (99% claim) | Yes (session counter) |
| npm distribution | Yes | No (pip) | No (pip) | No (Go binary) | Yes |
| Single binary, no external deps | No (Python) | No (Python + optional Voyage AI) | No (Python) | Yes (Go + SQLite) | Yes (Rust) |
| Semantic / vector search | No | Yes (optional, Voyage AI) | Yes (local embeddings) | No | No (deliberate) |
| SessionStart repo map | No | No | No | No | Yes (hook-injected) |

---

## Sources

- jCodeMunch GitHub: https://github.com/jgravelle/jcodemunch-mcp (tree-sitter byte-offset retrieval, token savings claims)
- Code-Index-MCP GitHub: https://github.com/ViperJuice/Code-Index-MCP (BM25, SQLite+FTS5, Watchdog, Voyage AI semantic search)
- SymDex GitHub: https://github.com/husnainpk/SymDex (97% token reduction, 14 languages, call graph, SHA-256 change detection)
- codebase-memory-mcp GitHub: https://github.com/DeusData/codebase-memory-mcp (Go binary, 64 languages, 99% reduction claim, git diff impact)
- CodeMCP (CKB) GitHub: https://github.com/SimplyLiz/CodeMCP (blast radius, CODEOWNERS, dead code detection, compound operations)
- Claude Code Hooks Reference: https://code.claude.com/docs/en/hooks (PostToolUse, SessionStart, additionalContext schema)
- Claude Code LSP article: https://byteiota.com/claude-code-adds-lsp-support-ai-ides-close-the-feature-gap/ (LSP added Dec 2025, v2.0.74)
- MCP ecosystem overview: https://github.com/punkpeye/awesome-mcp-servers

---
*Feature research for: Code intelligence MCP server (parasitic hook integration, Rust-native)*
*Researched: 2026-03-10*
