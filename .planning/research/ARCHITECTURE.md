# Architecture Research

**Domain:** In-memory code intelligence MCP server (Rust, tree-sitter, async)
**Researched:** 2026-03-10
**Confidence:** HIGH — based on existing codebase, finalized ROADMAP-v2.md, prior research in docs/summaries/, and verified patterns

---

## Standard Architecture

### System Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                     External Processes                               │
│  ┌──────────────────────┐      ┌───────────────────────────────┐    │
│  │  Claude Code (model) │      │  Filesystem (editor saves)    │    │
│  │  - Read/Edit/Grep    │      │  - .rs, .py, .ts, .go, .java  │    │
│  └──────┬───────────────┘      └────────────────┬──────────────┘    │
│         │ stdio JSON-RPC                         │ FS events         │
└─────────┼───────────────────────────────────────┼───────────────────┘
          │                                        │
┌─────────┼───────────────────────────────────────┼───────────────────┐
│         │      MCP Server Process (single PID)   │                   │
│  ┌──────▼────────────┐    ┌────────────────┐    │                   │
│  │  MCP stdio Layer  │    │  Axum Sidecar  │    │                   │
│  │  (rmcp crate)     │    │  :0 (random)   │◄───┘ FileWatcher       │
│  │  - Tool dispatch  │    │  - /outline    │      (notify crate)     │
│  │  - Schema valid.  │    │  - /refs       │                         │
│  └──────┬────────────┘    │  - /impact     │                         │
│         │                 └───────┬────────┘                         │
│         │                         │                                   │
│         │        Arc<RwLock<LiveIndex>>                               │
│         └─────────────────┬───────┘                                  │
│                           │                                           │
│  ┌────────────────────────▼──────────────────────────────────────┐   │
│  │                      LiveIndex                                 │   │
│  │  files: HashMap<RelPath, FileEntry>                            │   │
│  │  symbols: HashMap<SymbolId, SymbolEntry>                       │   │
│  │  symbols_by_file: HashMap<RelPath, Vec<SymbolId>>              │   │
│  │  symbols_by_name: HashMap<String, Vec<SymbolId>>               │   │
│  │  refs_to: HashMap<String, Vec<ReferenceRecord>>                │   │
│  │  refs_from: HashMap<SymbolId, Vec<ReferenceRecord>>            │   │
│  │  imports: HashMap<RelPath, Vec<ImportRecord>>                  │   │
│  │  trigram_idx: HashMap<[u8;3], Vec<(FileId, LineNum)>>          │   │
│  └───────────────────────────────────────────────────────────────┘   │
│                           │                                           │
│  ┌────────────────────────▼──────────────────────────────────────┐   │
│  │                  Indexer Pipeline                               │   │
│  │  Discovery → Parse (tree-sitter) → Extract (symbols+refs)     │   │
│  │  → Populate LiveIndex                                          │   │
│  └───────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────┘
          │
┌─────────▼─────────────────────────────────────────────────────────┐
│                      Hook Scripts (Python)                          │
│  post_read.py → HTTP GET :port/outline?file=...                     │
│  post_edit.py → HTTP GET :port/impact?file=...                      │
│  post_grep.py → HTTP GET :port/refs?name=...                        │
│  session_start.py → reads .tokenizor/derived/repo-outline.json      │
└─────────────────────────────────────────────────────────────────────┘
```

### Component Responsibilities

| Component | Responsibility | Implementation |
|-----------|----------------|----------------|
| **MCP stdio layer** | JSON-RPC protocol, tool registration, input validation, response dispatch | `rmcp` crate + `src/protocol/mcp.rs` |
| **LiveIndex** | Central in-memory store. All reads from here, no disk I/O on query path | `Arc<RwLock<LiveIndex>>`, `src/index/live_index.rs` (new) |
| **Indexer Pipeline** | Startup load + incremental file re-parse. Populates/updates LiveIndex | `src/indexing/` (rewritten), uses `src/parsing/` |
| **Tree-sitter Parser** | Converts bytes → symbols + references for one file. Stateless. | `src/parsing/` (kept), extended with xref queries |
| **Cross-reference Extractor** | Extracts call sites, imports, type usages via tree-sitter queries per language | New `src/parsing/xref/` per language |
| **File Watcher** | Detects filesystem changes, triggers incremental re-index | `notify` + `notify-debouncer-full`, `src/watcher.rs` (new) |
| **Axum Sidecar** | HTTP server on localhost for hook ↔ LiveIndex communication. Shares Arc | `axum` + `tokio::spawn`, `src/sidecar.rs` (new) |
| **Compact Formatter** | Converts LiveIndex query results → Read/Grep-style text. No JSON envelopes | `src/format.rs` (new) |
| **Trigram Index** | In-memory inverted index for sub-10ms text search across all files | Inside LiveIndex, `src/index/trigram.rs` (new) |
| **Hook Scripts** | Python scripts installed in `.claude/hooks.json`. Query sidecar, emit `additionalContext` | `hooks/` directory, bundled with npm package |

---

## Recommended Project Structure

```
src/
├── index/                  # In-memory data model (new)
│   ├── mod.rs              # LiveIndex struct + Arc<RwLock<LiveIndex>> alias
│   ├── live_index.rs       # HashMap fields, insert/update/remove operations
│   ├── trigram.rs          # Trigram posting list index
│   └── types.rs            # FileEntry, SymbolEntry, ReferenceRecord, ImportRecord
├── parsing/                # Tree-sitter extraction (kept, extended)
│   ├── mod.rs              # process_file() — symbols + refs for one file
│   ├── languages/          # Per-language symbol extraction (kept)
│   │   ├── rust.rs
│   │   ├── python.rs
│   │   ├── javascript.rs
│   │   ├── typescript.rs
│   │   ├── go.rs
│   │   └── java.rs
│   └── xref/               # Cross-reference extraction (new)
│       ├── mod.rs           # extract_refs(root, source, lang) → Vec<ReferenceRecord>
│       ├── queries/         # .scm query files per language
│       │   ├── rust.scm
│       │   ├── python.scm
│       │   ├── javascript.scm
│       │   ├── typescript.scm
│       │   ├── go.scm
│       │   └── java.scm
│       └── resolver.rs      # Name-to-SymbolId resolution (simple name matching)
├── indexing/                # Load + incremental update orchestration (rewritten)
│   ├── mod.rs
│   ├── discovery.rs         # File discovery with .gitignore (kept)
│   ├── loader.rs            # Parallel initial load → LiveIndex
│   └── incremental.rs       # Single-file reparse + LiveIndex patch
├── watcher.rs               # notify + debouncer, sends paths to incremental indexer
├── sidecar.rs               # axum HTTP server, /outline /refs /impact /health
├── format.rs                # Compact human-readable response formatter
├── protocol/
│   ├── mod.rs
│   └── mcp.rs               # rmcp tool handlers (rewritten to query LiveIndex)
├── domain/
│   ├── mod.rs
│   ├── index.rs             # LanguageId, SymbolKind, SymbolRecord (kept)
│   └── retrieval.rs         # API contract types (kept, pruned)
├── error.rs                 # TokenizorError (kept)
├── config.rs                # TokenizorConfig (simplified)
└── main.rs                  # Startup: index → spawn watcher + sidecar → serve MCP

hooks/
├── post_read.py             # PostToolUse(Read): inject file outline
├── post_edit.py             # PostToolUse(Edit): inject impact analysis
├── post_write.py            # PostToolUse(Write): inject index status
├── post_grep.py             # PostToolUse(Grep): inject symbol context
└── session_start.py         # SessionStart: inject repo map

npm/                         # Distribution packaging (kept)
tests/
├── tree_sitter_grammars.rs  # Grammar coverage tests (kept)
├── retrieval_conformance.rs # Data model tests (kept)
├── live_index.rs            # LiveIndex unit tests (new)
├── xref_extraction.rs       # Per-language xref tests (new)
└── hook_integration.rs      # Sidecar + hook response tests (new)
```

### Structure Rationale

- **`src/index/`:** Central data model gets its own module. Nothing outside this module writes to LiveIndex directly — all mutations flow through `live_index.rs` methods. This enforces consistency (trigram index updates, reference invalidation) as a single responsibility.
- **`src/parsing/xref/`:** Reference extraction lives alongside symbol extraction but in a subdirectory. The `.scm` query files are data, not code — keeping them in-tree avoids runtime file path issues and enables compile-time embedding via `include_str!`.
- **`src/indexing/`:** Loader and incremental updater are separate files. The loader uses bounded `tokio::spawn` parallelism. The incremental updater is a much simpler single-file path that must complete in <50ms.
- **`watcher.rs` and `sidecar.rs` at top level:** Both are independent async tasks spawned at startup. Keeping them flat avoids false module groupings — they're both just "background services".
- **`hooks/` as Python:** Claude Code hooks must be Python or shell scripts. Python is preferable for readable JSON handling. Hook scripts are thin: read stdin, make HTTP request to sidecar, print `additionalContext`, exit.

---

## Architectural Patterns

### Pattern 1: Shared-Nothing Query Path

**What:** All MCP tool handlers and all sidecar HTTP handlers share a single `Arc<RwLock<LiveIndex>>`. Read locks are acquired per-request and dropped immediately after the query returns. Writers (file watcher, incremental indexer) hold write lock for microseconds during a single HashMap update.

**When to use:** Always. This is the core data access contract for the entire system.

**Trade-offs:** `parking_lot::RwLock` over `std::sync::RwLock` for write-biased fairness and lower overhead under contention. Consider `arc-swap` if profiling reveals read-lock contention is measurable (unlikely given <1ms query times).

```rust
// Every tool handler follows this pattern
pub async fn get_symbol(index: Arc<RwLock<LiveIndex>>, name: &str) -> String {
    let guard = index.read();  // Multiple readers allowed concurrently
    let symbols = guard.symbols_by_name.get(name);
    format_symbols(symbols)    // guard dropped at end of scope
}
```

### Pattern 2: Parse → Populate, Never Mutate In-Place

**What:** The indexer pipeline treats parsing as pure: `parse_file(bytes) -> (Vec<SymbolEntry>, Vec<ReferenceRecord>)`. Populating LiveIndex is a separate step that acquires a write lock, removes stale entries, inserts new ones, and releases. The parse step (which can take milliseconds) runs without holding any lock.

**When to use:** Always for incremental updates. Avoids holding the write lock during tree-sitter parsing, keeping read availability high.

**Trade-offs:** Slightly more memory (parsed results held briefly as owned Vecs before lock acquisition). Correct tradeoff — tree-sitter parse is 10-50x slower than HashMap insertion.

```rust
// Parse without lock
let (symbols, refs) = parse_file(&content, &language)?;
// Only then acquire write lock — hold it for microseconds
{
    let mut guard = index.write();
    guard.remove_file(path);
    guard.insert_file(path, file_entry, symbols, refs);
} // lock dropped
```

### Pattern 3: HTTP Sidecar on Ephemeral Port

**What:** On startup, `axum::Server::bind("127.0.0.1:0")` assigns an OS-assigned port. The bound port is written to `.tokenizor/sidecar.port`. Hook scripts read this file to know where to connect. The sidecar shares the same `Arc<LiveIndex>` as the MCP tools — no data duplication.

**When to use:** This pattern is required because Claude Code hooks are external processes (Python scripts) that cannot share in-process memory with the Rust MCP server.

**Trade-offs:** Adds ~10ms HTTP round-trip on top of ~40ms Python startup = ~50ms hook latency. Acceptable given Claude Code hook timeout is 600s. The alternative (CLI subprocess) would be slower due to process startup cost per invocation.

**Port file location:** `.tokenizor/sidecar.port` — created on startup, deleted on shutdown. Hook scripts check for existence before attempting connection; if missing, skip context injection gracefully.

```rust
// In main.rs startup sequence
let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
let port = listener.local_addr()?.port();
std::fs::write(".tokenizor/sidecar.port", port.to_string())?;
tokio::spawn(axum_server(listener, Arc::clone(&index)));
```

### Pattern 4: Debounced Watcher → Incremental Update Channel

**What:** `notify-debouncer-full` watches the repo root with a 200ms debounce window. Events are sent to a `tokio::sync::mpsc` channel. A background task consumes the channel and calls the incremental indexer for each changed file. This decouples event receipt from index update work.

**When to use:** Always for file watching. The channel provides backpressure if many files change simultaneously (e.g., `git checkout` switching branches).

**Trade-offs:** 200ms debounce means LiveIndex is momentarily stale after a file save, but reflects reality within the debounce window. The Edit hook triggers immediate re-index (<50ms) in parallel, so the hook path is faster than the watcher path for model-initiated edits.

```rust
// Watcher sends paths; consumer does incremental work
let (tx, mut rx) = tokio::sync::mpsc::channel(100);
// ... watcher sends to tx ...
tokio::spawn(async move {
    while let Some(path) = rx.recv().await {
        incremental_reindex(Arc::clone(&index), path).await;
    }
});
```

---

## Data Flow

### Flow 1: Startup (Cold Index)

```
main() starts
    │
    ├─ load_repository(repo_root)
    │      │
    │      ├─ discovery::discover_files() [ignore crate, respects .gitignore]
    │      │
    │      ├─ tokio::spawn per file (bounded semaphore, N = num_cpus)
    │      │      └─ parsing::process_file(path, bytes, lang)
    │      │              ├─ tree-sitter parse → symbols
    │      │              └─ xref::extract_refs() → references
    │      │
    │      └─ LiveIndex::batch_insert(all results)
    │             (single write lock acquisition for entire batch)
    │
    ├─ write .tokenizor/sidecar.port
    ├─ tokio::spawn axum_sidecar(Arc::clone(&index))
    ├─ tokio::spawn file_watcher(Arc::clone(&index))
    └─ rmcp::serve_stdio(mcp_handler(Arc::clone(&index)))
```

### Flow 2: MCP Tool Query (Read Path)

```
Claude Code → MCP call "get_symbol" {name: "parse_file"}
    │
    └─ rmcp dispatch → mcp::get_symbol(Arc<LiveIndex>, input)
           │
           ├─ index.read()  [acquire read lock — non-blocking if no writer]
           ├─ symbols_by_name.get("parse_file")  [O(1) HashMap]
           ├─ fetch SymbolEntry → get byte range → serve from FileEntry.content
           └─ format::compact_symbol_response(symbol, callers, callees)
                  └─ "[indexed 0.3s ago | 1,847 tokens saved]"
```

### Flow 3: File Change (Watcher Path)

```
Editor saves src/parsing/mod.rs
    │
    └─ notify::RecommendedWatcher detects Modify(Data)
           │
           └─ notify-debouncer-full collects events (200ms window)
                  │
                  └─ channel.send("src/parsing/mod.rs")
                         │
                         └─ background task receives
                                │
                                ├─ read bytes from disk
                                ├─ hash check: same as FileEntry.content_hash? → skip
                                ├─ parsing::process_file() [WITHOUT lock]
                                │
                                └─ index.write()  [hold for ~1ms]
                                       ├─ remove old symbols for this file
                                       ├─ remove old refs for this file
                                       ├─ insert new FileEntry + symbols + refs
                                       └─ update trigram index
```

### Flow 4: PostToolUse Hook (Edit Path)

```
Claude Code executes Edit tool on src/parsing/mod.rs
    │
    └─ PostToolUse hook fires: post_edit.py
           │
           ├─ reads .tokenizor/sidecar.port
           ├─ GET http://localhost:{port}/impact?file=src/parsing/mod.rs
           │      │
           │      └─ sidecar handler:
           │             ├─ index.read()
           │             ├─ find symbols in this file
           │             ├─ for each symbol: refs_to[name] → callers
           │             └─ return compact impact text
           │
           ├─ also: GET http://localhost:{port}/reindex?file=...
           │        (triggers immediate incremental re-index, guaranteed <50ms)
           │
           └─ print JSON: {"hookSpecificOutput": {"additionalContext": "..."}}
                  └─ Claude Code appends to tool result
```

### Flow 5: Incremental Re-index (Edit-Triggered)

```
POST http://localhost:{port}/reindex?file=src/parsing/mod.rs
    │
    └─ sidecar /reindex handler
           │
           ├─ tokio::spawn incremental_reindex(Arc::clone(&index), path)
           │      │
           │      ├─ read file from disk (O(file size))
           │      ├─ parsing::process_file() [WITHOUT lock — pure computation]
           │      └─ index.write()  [hold for <1ms — just HashMap ops]
           │
           └─ return 200 immediately (non-blocking for hook response)
```

---

## Component Boundaries (What Talks to What)

| Boundary | Communication | Direction | Notes |
|----------|---------------|-----------|-------|
| `mcp.rs` ↔ `LiveIndex` | `Arc<RwLock<LiveIndex>>` direct | Tools read | Read lock per call, no persistent reference |
| `sidecar.rs` ↔ `LiveIndex` | `Arc<RwLock<LiveIndex>>` direct | HTTP handlers read | Same Arc as MCP tools — zero duplication |
| `watcher.rs` ↔ `incremental.rs` | `tokio::mpsc::channel` | Watcher sends paths | Decouples event receipt from indexing work |
| `incremental.rs` ↔ `LiveIndex` | `Arc<RwLock<LiveIndex>>` direct | Writes | Write lock held <1ms |
| `parsing/` ↔ `index/` | Return values (`Vec<SymbolEntry>`, `Vec<ReferenceRecord>`) | Parsing produces, index consumes | No shared state — parsing is stateless |
| Hook scripts ↔ `sidecar.rs` | HTTP on localhost | Scripts call sidecar | Port from `.tokenizor/sidecar.port` file |
| Hook scripts ↔ Claude Code | stdout JSON | Scripts emit `additionalContext` | Claude Code appends to tool result |
| `main.rs` ↔ all components | `Arc` clones passed at startup | One-time wiring | No global state, no dependency injection framework |

---

## Build Order (Phase Dependencies)

The component dependencies mandate this build order:

```
1. src/index/types.rs        ← No dependencies. All other modules depend on these types.
       │
2. src/index/live_index.rs   ← Depends on types. Core store. Nothing works without it.
       │
3. src/parsing/ (extended)   ← Depends on types. Parse → (symbols, refs). Stateless.
       │
4. src/indexing/loader.rs    ← Depends on parsing + LiveIndex. Startup load.
       │
5. src/indexing/incremental  ← Depends on parsing + LiveIndex. Single-file update.
       │
6. src/protocol/mcp.rs       ← Depends on LiveIndex. Tools can be wired + tested.
       │
       ├─ 7a. src/watcher.rs ← Depends on incremental indexer. Independent of MCP.
       │
       ├─ 7b. src/sidecar.rs ← Depends on LiveIndex. Independent of MCP and watcher.
       │
       └─ 7c. hooks/         ← Depends on sidecar being deployed. Integration layer.
```

**Critical path:** types → LiveIndex → parser → loader → MCP tools. Everything else (watcher, sidecar, hooks) branches off after the critical path is functional.

**Milestone 1 builds:** 1, 2, 3, 4, 5, 6 (first four MCP tool phases)
**Milestone 2 builds:** Extends 3 (xref extraction), extends 6 (new tools), new formatter
**Milestone 3 builds:** 7a, 7b, 7c (hook infrastructure and integration)
**Milestone 4 builds:** Extensions to 2 (trigram), 3 (more languages), 6 (scored search, file tree), new persistence

---

## Anti-Patterns

### Anti-Pattern 1: Parsing Inside a Lock

**What people do:** Acquire a write lock, then call `process_file()` while holding it.

**Why it's wrong:** Tree-sitter parsing takes 10-100ms per file. Holding a write lock for that duration blocks all MCP queries and sidecar requests. For a 70-file initial load, this serializes to 700ms-7s of total lock hold time.

**Do this instead:** Parse outside the lock. Collect results as owned Vecs. Acquire the write lock only for the HashMap insert operations (microseconds each).

### Anti-Pattern 2: Per-Tool Arc Clones in Hot Path

**What people do:** Clone `Arc<LiveIndex>` inside every tool handler invocation.

**Why it's wrong:** Not a correctness issue, but unnecessary atomic reference count churn. At <1ms query latency, it's measurable noise.

**Do this instead:** Hold the `Arc` at the server struct level. Tool handlers receive `&Self` which contains the `Arc`. Clone only when spawning a new async task that needs ownership.

### Anti-Pattern 3: Blocking the MCP Event Loop with Watcher Work

**What people do:** Process file watcher events synchronously in the `tokio::main` task.

**Why it's wrong:** The rmcp stdio handler and the file watcher both need to be responsive. If the watcher's re-index blocks the event loop, MCP tool calls queue up during indexing.

**Do this instead:** `tokio::spawn` both the watcher consumer and the MCP server as independent tasks. The write lock (<1ms) is the only synchronization point between them.

### Anti-Pattern 4: Rebuilding the Full Index on Any File Change

**What people do:** On any file change, re-discover all files and re-parse everything.

**Why it's wrong:** This is jCodeMunch's failure mode. A 70-file repo takes 300-500ms to fully re-index. Any edit makes the index stale for 300-500ms, defeating the purpose.

**Do this instead:** Track which symbols belong to which file (`symbols_by_file`). On a file change: remove all symbols for that file, re-parse only that file, insert new symbols. This is a <50ms operation regardless of repo size.

### Anti-Pattern 5: Verbose JSON Responses to Model

**What people do:** Wrap every response in `{"outcome": "success", "trust": "verified", "data": {...}}` envelopes.

**Why it's wrong:** The model receives this and must parse JSON before extracting useful information. Every key, brace, and quote is tokens consumed for zero value. The model's Read tool returns plain text — MCP tools should match.

**Do this instead:** Return formatted text matching the model's Read/Grep expectations. One metadata line at the bottom (`[indexed 0.3s ago | 1,847 tokens saved]`). Let the model absorb the content naturally.

---

## Scaling Considerations

This system is scoped to **single-repo, in-process** use. Scaling is about repo size, not user count.

| Repo Size | Architecture Adjustments |
|-----------|--------------------------|
| 0–1,000 files (~5MB) | No adjustments. In-memory comfortable. Cold start <1s. |
| 1,000–10,000 files (~50MB) | Target design point. All structures fit in RAM. Parallel initial load with bounded semaphore. Cold start 2-5s. |
| 10,000–100,000 files (~500MB) | Content eviction: keep symbols + refs in RAM, evict `FileEntry.content` bytes for files not accessed in N minutes. Lazy reload on access. |
| 100,000+ files | Out of scope for v2. Requires SpacetimeDB or similar persistent index. Deferred to v3. |

### Scaling Priorities

1. **First bottleneck: Initial load time.** At 10,000 files, serial parsing takes ~10s. Fix: bounded parallel `tokio::spawn` with semaphore (N = 2× num_cpus). Already planned in Phase 1.2.

2. **Second bottleneck: Memory for file content.** At 10,000 files × 5KB avg = 50MB — fine. At 50,000 files × 5KB = 250MB — needs content eviction. Content is only needed for `get_file_content`; symbols and refs are much smaller.

3. **Third bottleneck: Trigram index size.** 3-gram count grows linearly with total source bytes. At 50MB source, expect ~16M trigrams. At 4 bytes per entry and 5 entries per posting: ~320MB. Use `Vec<u32>` file+line encoding (4 bytes per hit) rather than structs.

---

## Integration Points

### External Services

| Service | Integration Pattern | Notes |
|---------|---------------------|-------|
| Claude Code | stdio JSON-RPC (rmcp crate) | MCP protocol. Tool calls in, results out. No state between calls. |
| Claude Code hooks | stdin/stdout JSON per hook invocation | `additionalContext` field appended to tool result. 600s timeout. |
| Filesystem | `notify` crate for events; `std::fs` for reads | All reads are synchronous blocking I/O — acceptable since parsing runs in `tokio::spawn` |
| npm registry | npm distribution (kept from v1) | No changes needed |

### Internal Boundaries

| Boundary | Communication | Notes |
|----------|---------------|-------|
| `parsing/` ↔ `index/` | Owned Vec return values | Parsing is stateless — no shared state, no Arc needed |
| MCP tools ↔ sidecar | Both read the same Arc — no IPC | They run in the same process; the Arc IS the boundary |
| Watcher ↔ incremental indexer | `tokio::mpsc` channel | Backpressure prevents runaway re-index queue on bulk changes |
| Sidecar ↔ hooks | HTTP on localhost | Port discovery via `.tokenizor/sidecar.port` file |

---

## Sources

- ROADMAP-v2.md — finalized architecture decisions and phase breakdown (HIGH confidence, primary source)
- `docs/summaries/research-xref-extraction-and-file-watching.md` — tree-sitter node types, notify crate API (HIGH confidence, research-backed)
- rust-analyzer architecture docs: https://rust-analyzer.github.io/book/contributing/architecture.html (MEDIUM confidence — Salsa is more complex than needed here, but parse-outside-lock pattern applies)
- `arc-swap` crate docs: https://docs.rs/arc-swap/latest/arc_swap/docs/patterns/index.html (MEDIUM confidence — relevant alternative to RwLock if contention proves measurable)
- notify-rs GitHub: https://github.com/notify-rs/notify (HIGH confidence — current implementation reference)
- MCP stdio server pattern: https://www.shuttle.dev/blog/2025/07/18/how-to-build-a-stdio-mcp-server-in-rust (MEDIUM confidence — confirms rmcp pattern)
- Existing v1 source: `src/parsing/`, `src/protocol/mcp.rs` — confirms what's kept and what's replaced (HIGH confidence)

---
*Architecture research for: Tokenizor v2 — in-memory code intelligence MCP server*
*Researched: 2026-03-10*
