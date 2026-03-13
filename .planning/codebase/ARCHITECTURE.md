# Architecture

**Analysis Date:** 2026-03-14

## Pattern Overview

**Overall:** Layered event-driven architecture with symbol-aware code indexing, real-time file watching, and MCP protocol bridging.

**Key Characteristics:**
- **Multi-mode execution:** Standalone MCP server, daemon-backed persistent indexing, or sidecar HTTP proxy for token hook integration
- **Dual persistence layers:** In-memory live index with serialized snapshots + git temporal analysis for historical context
- **Symbol-centric query model:** All code navigation is symbol-aware (not string-based) with cross-references, enclosing contexts, and dependent tracking
- **Zero-copy on read path:** Raw file bytes stored in-memory; only slicing performed, never copying
- **Async task isolation:** File watching, git analysis, and sidecar HTTP server run in tokio background tasks without blocking MCP queries

## Layers

**Discovery & Loading:**
- Purpose: Recursively find source files, infer languages, and load initial index
- Location: `src/discovery/mod.rs`, `src/live_index/store.rs`
- Contains: `.gitignore`-respecting file walker, language detection, parallel parsing
- Depends on: File system, tree-sitter language grammars, parsing layer
- Used by: Startup initialization (main.rs) and `index_folder` tool

**Parsing & Xref Extraction:**
- Purpose: Parse source files with tree-sitter, extract symbols, and cross-reference tracking
- Location: `src/parsing/mod.rs`, `src/parsing/languages/`, `src/parsing/xref.rs`
- Contains: Per-language symbol extraction (14 languages), reference detection, alias maps for imports
- Depends on: Tree-sitter library, domain types
- Used by: LiveIndex initialization and watcher event handlers

**Live Index (In-Memory Store):**
- Purpose: Thread-safe queryable store of all indexed files, symbols, and cross-references with circuit breaker resilience
- Location: `src/live_index/store.rs`, `src/live_index/query.rs`, `src/live_index/search.rs`, `src/live_index/trigram.rs`
- Contains: `LiveIndex` struct (RwLock-protected), `IndexedFile`, symbol stores, reverse reference index, trigram search accelerators
- Depends on: Parsing layer, domain models
- Used by: Protocol layer, watcher, persistence layer, git temporal computation

**Persistence & Snapshots:**
- Purpose: Serialize/deserialize live index to `.tokenizor/index.bin` and verify disk state against snapshot at startup
- Location: `src/live_index/persist.rs`
- Contains: Postcard binary encoding, background verification with mtime tracking, fast-path snapshot loading
- Depends on: Live index, postcard crate
- Used by: Main.rs startup flow and graceful shutdown

**Git Temporal Intelligence:**
- Purpose: Asynchronously compute commit history, blame, co-change patterns for deeper context
- Location: `src/live_index/git_temporal.rs`
- Contains: Background git repository analysis, blame computation, co-change correlation
- Depends on: git2 crate, live index
- Used by: Protocol tools (`get_cochanges`, `trace_symbol`)

**MCP Protocol Bridge:**
- Purpose: Map MCP tool calls to query functions and format responses as plain text
- Location: `src/protocol/mod.rs`, `src/protocol/tools.rs`, `src/protocol/format.rs`, `src/protocol/resources.rs`
- Contains: `TokenizorServer` (rmcp handler), 24 tool handlers, output formatters, resource templates, prompt definitions
- Depends on: Live index, rmcp library, format layer
- Used by: Main stdio transport server

**Edit & Code Mutation:**
- Purpose: Support in-place file editing with symbol-aware mutations (batch operations, refactoring)
- Location: `src/protocol/edit.rs`, `src/protocol/edit_format.rs`
- Contains: Symbol replacement, batch edits, range-scoped insertions, diff formatting
- Depends on: Live index, protocol layer
- Used by: Edit/write tool handlers

**Daemon (Persistent Project State):**
- Purpose: Share indexed projects across multiple MCP sessions, managing project instances and session lifecycle
- Location: `src/daemon.rs`
- Contains: `DaemonState` (projects, sessions), `DaemonSessionClient` (HTTP proxy), `ProjectInstance` (index + watchers), token stats tracking
- Depends on: Live index, watcher, token stats, axum HTTP server
- Used by: Main.rs when `TOKENIZOR_AUTO_INDEX=true` (daemon mode) or daemon CLI subcommand

**Sidecar HTTP Server:**
- Purpose: Standalone HTTP proxy for token hook integration (Read, Edit, Write, Grep hooks fire HTTP requests)
- Location: `src/sidecar/mod.rs`, `src/sidecar/server.rs`, `src/sidecar/handlers.rs`, `src/sidecar/router.rs`
- Contains: Axum routes for tool handlers, `TokenStats` atomic counters for hook metrics, port/PID file management
- Depends on: Live index, axum, protocol handlers, token stats
- Used by: Main.rs (spawned as background task alongside MCP server)

**File Watcher:**
- Purpose: Monitor filesystem changes and incrementally update live index without full re-parse
- Location: `src/watcher/mod.rs`
- Contains: notify-debouncer-full integration, event filtering, adaptive debounce windowing (200ms-500ms), parse + index update
- Depends on: notify crate, parsing layer, live index
- Used by: Main.rs (spawned as background task during local MCP server)

**CLI & Hook Handlers:**
- Purpose: Command-line interface for setup, daemon management, and hook script integration
- Location: `src/cli/mod.rs`, `src/cli/init.rs`, `src/cli/hook.rs`
- Contains: clap parser, init flow for Claude/Codex/Gemini, hook handlers for Read/Edit/Write/Grep/SessionStart/PromptSubmit
- Depends on: Live index, protocol formats, discovery
- Used by: Main entry point (src/main.rs)

## Data Flow

**Index Initialization (Startup):**

1. Discovery walks project root respecting `.gitignore`
2. Files grouped by language and classified (test, vendor, generated, etc.)
3. Parallel parsing via rayon extracts symbols and cross-references
4. Live index populates with `IndexedFile` records
5. Circuit breaker checks parse failure rates; if >20%, marks degraded
6. On graceful shutdown: serialize to `.tokenizor/index.bin`

**File Change Detection (Watcher):**

1. Filesystem event arrives (create, modify, delete, rename)
2. Debouncer accumulates events with adaptive 200ms-500ms window
3. Batch of events deduplicated and parsed
4. Live index atomically updated (new/modified files replaced, deleted files removed)
5. Watcher info updated for health reporting

**Query Path (MCP Tool Call):**

1. MCP stdin receives JSON `call_tool` request
2. Tool handler acquires RwLockReadGuard on live index (non-blocking; extractors hold lock briefly)
3. Query function (in `src/live_index/query.rs`) executes against snapshot data
4. Lock released; formatting happens outside lock
5. Response formatted as plain text (never JSON)
6. Response sent via stdout

**Git Temporal Analysis (Background):**

1. Spawned async task after index ready
2. Opens repo, walks commit history for each file/symbol
3. Computes blame, co-change patterns
4. Stores results in live index
5. Runs non-blocking; queries fall back gracefully if not ready

**Daemon Mode (Project Sharing):**

1. First MCP instance connects; spawns daemon process
2. Daemon loads index once, watches for file changes
3. Subsequent MCP instances connect to daemon via HTTP
4. All tool calls forwarded to daemon's shared index
5. Token stats accumulated at daemon level
6. Session records track client PIDs and activity timestamps

**State Management:**

- **Mutable state:** Live index (`Arc<RwLock<IndexState>>`) for file/symbol storage
- **Immutable derived data:** Query result views (no caching; computed on-demand)
- **Atomic counters:** TokenStats (read/edit/write/grep fires and token savings)
- **Background tasks:** Watcher (tokio), git temporal (tokio), sidecar HTTP (axum)
- **Atomic flags:** Daemon degradation flag, circuit breaker trip flag

## Key Abstractions

**IndexedFile:**
- Purpose: Represents all parsed/indexed data for a single source file
- Examples: `src/live_index/store.rs` (definition), `src/live_index/query.rs` (queries)
- Pattern: Immutable, cloned on read; content bytes, symbols, references all co-located

**ReferenceLocation:**
- Purpose: Points to a specific cross-reference within a file (file path + index)
- Examples: `src/live_index/store.rs` (definition), reverse index keys
- Pattern: Compact location tracking; enables efficient dependent finding

**SymbolRecord & ReferenceRecord:**
- Purpose: Domain types capturing symbol metadata (kind, byte range, line range) and reference metadata (kind, enclosing symbol)
- Examples: `src/domain/index.rs` (definitions), used throughout query layer
- Pattern: Immutable, owned by `IndexedFile`; enables query filters (e.g., "find all Function references")

**FileClassification:**
- Purpose: Semantic categorization (test, vendor, generated, config) computed at discovery time
- Examples: `src/domain/index.rs`, used in filter logic
- Pattern: Deterministic based on path; never changes for a file path

**LiveIndex / IndexState:**
- Purpose: Thread-safe queryable container for all indexed state
- Examples: `src/live_index/store.rs`, central to all queries
- Pattern: Arc<RwLock<IndexState>>; readers hold brief locks (extract then release); writers (watcher) batch multiple file updates

**DaemonSessionClient:**
- Purpose: HTTP client proxy representing a single session connection to the daemon
- Examples: `src/daemon.rs` (definition), `src/protocol/mod.rs` (wrapped in TokenizorServer)
- Pattern: Reconnection-aware; retries on failure; degrades gracefully

**TokenStats:**
- Purpose: Atomic counters for hook metrics (fires, estimated token savings)
- Examples: `src/sidecar/mod.rs` (definition), reported by health tool
- Pattern: Fire-and-forget updates with Ordering::Relaxed; no blocking reads

## Entry Points

**Binary: `tokenizor` (no args or `mcp`):**
- Location: `src/main.rs` → `run_mcp_server()` / `run_local_mcp_server_async()`
- Triggers: Direct invocation or by Claude Code MCP client registration
- Responsibilities: Project discovery, index loading (snapshot or fresh parse), watcher spawn, sidecar HTTP spawn, MCP server startup on stdio

**Binary: `tokenizor daemon`:**
- Location: `src/main.rs` → `run_daemon()`
- Triggers: Manual invocation or spawned by MCP server
- Responsibilities: Long-lived HTTP daemon accepting project open/close requests, managing persistent index, coordinating sessions

**Binary: `tokenizor init`:**
- Location: `src/cli/init.rs`
- Triggers: Manual invocation to configure Claude/Codex/Gemini
- Responsibilities: Write MCP server config to client metadata

**Binary: `tokenizor hook <subcommand>`:**
- Location: `src/cli/hook.rs`
- Triggers: Called by Claude Code PostToolUse and SessionStart hooks
- Responsibilities: Read stdin (file path or prompt), query live index, output formatted context to stdout

**HTTP Sidecar:**
- Location: `src/sidecar/server.rs`
- Triggers: Spawned by main MCP server async startup
- Responsibilities: Expose `/query`, `/stats`, `/health` endpoints for token hooks (Read, Edit, Grep); bridge HTTP to live index

**Watcher Task:**
- Location: `src/watcher/mod.rs`
- Triggers: Spawned by main MCP server after index ready
- Responsibilities: Monitor filesystem, debounce events, parse changes, update live index atomically

**Git Temporal Task:**
- Location: `src/live_index/git_temporal.rs`
- Triggers: Spawned by main MCP server after index ready
- Responsibilities: Compute commit history, blame, co-changes in background (non-blocking)

## Error Handling

**Strategy:** Fail gracefully with detailed tracing; circuit breaker for parse failures; daemon degradation flag for session faults.

**Patterns:**

1. **Parse errors:** Caught at `process_file()` (panic catch_unwind), recorded as `ParseStatus::Failed` in index, counted by circuit breaker. Query tools return explanatory text, not error codes.

2. **File I/O errors:** Watcher logs and continues on read/parse failures; live index remains usable for previously-parsed files.

3. **Daemon connection errors:** Client logs, sets degradation flag, falls back to local in-process execution on subsequent calls.

4. **Index mutation races:** Watcher batches updates and uses atomic compare-exchange; concurrent reads never see partial state.

5. **Tracing levels:** Uses `tracing` crate with `RUST_LOG` environment variable for filtering (info, debug, error, warn).

## Cross-Cutting Concerns

**Logging:**
- Framework: `tracing` crate with `tracing-subscriber` for filtering and formatting
- Startup logs include index stats (file count, symbol count, parse status, duration)
- Tool call logs at debug level; errors at warn/error
- Watcher logs filesystem events at debug, parse failures at warn
- Daemon logs session open/close and HTTP requests at info

**Validation:**
- Language detection: `LanguageId::from_extension()` validates file extensions against tree-sitter support
- Path normalization: All relative paths use forward slashes (even on Windows)
- Byte range validation: Cross-references validated against file content length
- Symbol filtering: Enclosing symbol computed deterministically via `find_enclosing_symbol()`

**Authentication:**
- None at protocol level (MCP operates over stdio, no auth)
- Daemon HTTP accepts requests from localhost only (127.0.0.1 bind)
- Session tracking via process ID and timestamp for lifecycle management

**Performance:**
- Zero-copy reads: Content bytes stored once; queries return slices
- Trigram search: 3-byte substring indexing for sub-linear full-text search
- Parallel parsing: rayon worksteal for multi-file parse phase
- Debounced watcher: Adaptive windowing prevents re-index storms
- Snapshot fast-path: Mtime verification reuses serialized index if disk state unchanged

---

*Architecture analysis: 2026-03-14*
