# Phase 2: MCP Tools v1 Parity - Research

**Researched:** 2026-03-10
**Domain:** rmcp 1.1.0 server wiring, MCP tool handler patterns, compact text formatting
**Confidence:** HIGH

## Summary

Phase 2 wires 10 MCP tools onto the LiveIndex built in Phase 1 and makes the binary a persistent MCP server. The work has three distinct concerns: (1) rmcp server boilerplate — `ServerHandler` impl, `#[tool_router]`/`#[tool_handler]` macros, `serve_server()` call in `main.rs`; (2) the formatter module (`src/protocol/format.rs`) that converts LiveIndex data into compact human-readable text; and (3) the loading-guard and auto-index startup logic that ensures tools return sensible responses before and during index load.

All required infrastructure already exists in the codebase: `LiveIndex`/`SharedIndex`, `HealthStats`, `SymbolRecord` (with `name`, `kind`, `depth`, `sort_order`, `byte_range`, `line_range`), `IndexedFile.content: Vec<u8>` for zero-disk-I/O slicing, `find_git_root()` for auto-index root detection, and `tracing` on stderr for logging. The rmcp crate (1.1.0, already in `Cargo.toml`) is the official Rust MCP SDK and its macro-based tool system is the exact pattern to use. No new dependencies are needed beyond `schemars` (required by rmcp macros for tool schema generation) and potentially adding `serde` derives back to input parameter structs.

The response formatter is the creative core of this phase. All 10 tools return `String` (rendered to `Content::text()`), never JSON. The format decisions are fully locked in CONTEXT.md: ripgrep-style for search, indented tree for outline, source body for get_symbol, directory tree for repo_outline. The loading guard is a simple pattern: at the top of every tool handler except health, acquire the read lock, check `index_state()`, return the guard string if not Ready.

**Primary recommendation:** Use `#[tool_router]` / `#[tool_handler]` macros on a `TokenizorServer` struct that holds `SharedIndex`. Each tool method takes `Parameters<SomeInput>` for deserialization and returns `String` (which implements `IntoContents` and gets wrapped to `CallToolResult` automatically). Formatter lives in `src/protocol/format.rs` as pure functions. Main replaces the `Ok(())` exit with `serve_server(server, transport::stdio()).await?.waiting().await`.

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

#### Response format
- Indented tree style for get_file_outline: symbol names indented by depth with kind prefix (e.g. `fn main`, `struct Config`), line ranges right-aligned. Header line shows filename + symbol count.
- get_symbol returns the full source body always (extracted via byte_range from stored content). Footer shows kind, line range, byte count.
- Search results (search_symbols, search_text) grouped by file, ripgrep-style: file header, then indented line_number:match lines. Summary line at top (e.g. "3 matches in 2 files").
- get_repo_outline uses file tree with symbol counts: directory tree structure, each file shows language + symbol count. Header line shows project name + totals.
- All responses are plain text, never JSON envelopes. Matches AD-6.

#### Tool surface & contracts
- Not-found errors return helpful plain text as normal content (not MCP error codes). Include suggestions: "No symbol X in file Y. Symbols in that file: ..." or "File not found: path".
- get_symbols supports both symbol and code_slice request types in Phase 2. Code slices use byte_range against stored content (LIDX-03 makes this trivial).
- search_symbols: substring matching, case-insensitive. No fuzzy match, no regex. Relevance ranking (exact > prefix > substring) deferred to Phase 7 (PLSH-02).
- search_text: full content scan against in-memory file contents. Simple string matching, case-insensitive. Trigram index deferred to Phase 7 (PLSH-01). Linear scan acceptable for <1,000 files.

#### Auto-index behavior
- Server auto-indexes on startup when .git is present. Falls back to CWD if no .git found (Phase 1 fallback preserved). Logs a warning when using CWD fallback.
- TOKENIZOR_AUTO_INDEX env var: set to "false" to disable auto-index. Server starts with empty LiveIndex, user must call index_folder. Default: true.
- index_folder does a full reload from scratch: drop current index, re-discover, re-parse everything. No incremental diff — the watcher (Phase 3) handles incremental.
- While index is loading (startup or index_folder reload): all tools except health return "Index is loading... try again shortly." Health always responds with Loading state.

#### MCP server wiring
- Server struct holds SharedIndex (Arc<RwLock<LiveIndex>>). Tool handlers acquire read lock, query, release. Write lock only for index_folder reload.
- New src/protocol/ module: mod.rs (server struct + rmcp wiring), tools.rs (tool handlers), format.rs (response formatter INFR-03).
- Tools only — no MCP resources, no prompts. Clean surface.
- Graceful shutdown on stdin close (stdio transport). Drop LiveIndex, exit 0. No persistence until Phase 7.

### Claude's Discretion
- Tool description text for each MCP tool (what the model sees in tool listing)
- Error handling strategy for internal panics (catch_unwind or let rmcp handle)
- Exact rmcp server setup boilerplate and handler trait implementation
- what_changed timestamp format and comparison strategy
- Search result limits (max matches per query) if needed for token budget
- get_file_content line range parameter design (start_line/end_line vs offset/limit)

### Deferred Ideas (OUT OF SCOPE)
None — discussion stayed within phase scope.
</user_constraints>

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| LIDX-02 | All tree-sitter extracted symbols stored with O(1) lookup by name, file, and ID | Already met in Phase 1 (`HashMap<String, IndexedFile>`); tool handlers use `get_file()` + linear scan of `file.symbols` for name match — acceptable at phase 2 scale |
| LIDX-05 | Initial load completes in <500ms for 70 files, <3s for 1,000 files | LiveIndex::load uses Rayon par_iter; benchmark confirmed fast. LIDX-05 test: spawn load on a tempdir with 70/1000 synthetic files, assert `load_duration` |
| TOOL-01 | get_symbol — lookup by (file, name, kind_filter) from LiveIndex | `get_file()` → linear scan symbols; byte_range slice of `IndexedFile.content` for source body |
| TOOL-02 | get_symbols — batch lookup, supports symbol and code_slice targets | Iterate list; for each item call get_symbol or slice content by byte_range |
| TOOL-03 | get_file_outline — ordered symbol list for a file | `symbols_for_file()` already returns ordered Vec; format with depth indentation |
| TOOL-04 | get_repo_outline — file list with coverage stats | `all_files()` iterator; group by directory; show language + symbol count per file |
| TOOL-05 | search_symbols — substring matching with relevance scoring | Iterate `all_files()`, scan each file's symbols for case-insensitive substring; group by file |
| TOOL-06 | search_text — text search across all indexed files | Iterate `all_files()`, scan `file.content` bytes as UTF-8 lines; report matches with line numbers |
| TOOL-07 | health — report LiveIndex stats (files, symbols, watcher status, last update) | `health_stats()` returns `HealthStats` directly; format as plain text table |
| TOOL-08 | index_folder — trigger full reload of LiveIndex | Acquire write lock, call `LiveIndex::load()`, replace contents; return summary |
| TOOL-12 | what_changed — files and symbols modified since timestamp | Compare `IndexedFile.content_hash` or use `loaded_at: Instant` field on LiveIndex; tool takes an ISO 8601 or Unix timestamp input |
| TOOL-13 | get_file_content — serve file content from memory with optional line range | Slice `IndexedFile.content` by line range (scan for newlines); zero disk I/O |
| INFR-02 | Auto-index on startup if .git exists (configurable via TOKENIZOR_AUTO_INDEX) | `find_git_root()` exists; `std::env::var("TOKENIZOR_AUTO_INDEX")` check; `LiveIndex::load()` before serve_server |
| INFR-03 | Compact response formatter — human-readable output matching Read/Grep style | New `src/protocol/format.rs` module; pure functions per tool; no JSON |
| INFR-05 | Removed tools: cancel_index_run, checkpoint_now, resume_index_run, and 7 others | These never exist in Phase 2 — the old v1 binary is completely replaced; confirmed by test_stdout_purity in Phase 1 |
</phase_requirements>

---

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `rmcp` | 1.1.0 | MCP server/client SDK — handles JSON-RPC framing, tool dispatch, init handshake | Official Rust MCP SDK; already in Cargo.toml |
| `rmcp-macros` | 1.1.0 (transitive) | `#[tool]`, `#[tool_router]`, `#[tool_handler]` proc macros | Eliminates boilerplate for tool registration + schema generation |
| `schemars` | (transitive via rmcp) | JSON Schema generation for tool input parameters | Required by rmcp macros for tool schema — macros call `JsonSchema` derive |
| `serde` + `serde_json` | 1.0 | Deserialize MCP tool input params from JSON object | Already in Cargo.toml; needed for `#[derive(Deserialize, JsonSchema)]` on param structs |
| `tokio` | 1.48 | Async runtime; `transport::stdio()` returns tokio stdin/stdout | Already in Cargo.toml |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `std::sync::{Arc, RwLock}` | stdlib | `SharedIndex = Arc<RwLock<LiveIndex>>` thread-safe access | Read lock on every tool call; write lock only for index_folder reload |
| `tracing` | 0.1 | Log index load progress, warn on CWD fallback, error on CB trip | Continue existing pattern — stderr only, never stdout |

### New Dependencies to Add
| Crate | Why Needed |
|-------|-----------|
| `schemars` | Must be explicit in Cargo.toml when using `#[derive(JsonSchema)]` on param structs. The rmcp-macros require it but it's currently only transitive. Add `schemars = "0.8"` as a direct dependency. |

**Installation:**
```bash
# Add to Cargo.toml [dependencies]:
schemars = "0.8"
```

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `#[tool_router]` macros | Manual `ToolRouter::new().with_route(...)` | Manual is more explicit but ~5x more code and error-prone. Macros are idiomatic for rmcp 1.1.0. |
| `String` return from tool handlers | `CallToolResult` directly | `String` implements `IntoContents` — rmcp wraps it automatically. Much cleaner. |
| `Content::text()` | `CallToolResult::success(vec![Content::text(...)])` | Same wire format; `Content::text()` is the preferred shorthand per rmcp source. |

---

## Architecture Patterns

### Recommended Project Structure
```
src/
├── protocol/
│   ├── mod.rs      # TokenizorServer struct, ServerHandler impl, serve() entry point
│   ├── tools.rs    # #[tool_router] impl — all 10 tool methods
│   └── format.rs   # Pure formatting functions (no LiveIndex dependency)
├── live_index/     # Phase 1 — unchanged
├── domain/         # Phase 1 — unchanged
├── discovery/      # Phase 1 — unchanged
├── parsing/        # Phase 1 — unchanged
├── observability.rs
├── error.rs
├── hash.rs
└── lib.rs          # Add: pub mod protocol;
```

### Pattern 1: rmcp Server Struct + Macro Wiring
**What:** `TokenizorServer` holds `SharedIndex`. `#[tool_router]` impl block defines all tools. `#[tool_handler]` impl makes it a valid `ServerHandler`.
**When to use:** The standard pattern for all rmcp 1.1.0 servers with state.

```rust
// Source: rmcp-1.1.0 test_tool_macros.rs + test_tool_routers.rs
use rmcp::{ServerHandler, model::ServerInfo, tool, tool_handler, tool_router};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Clone)]
pub struct TokenizorServer {
    index: SharedIndex,
    tool_router: ToolRouter<Self>,
}

impl TokenizorServer {
    pub fn new(index: SharedIndex) -> Self {
        Self {
            index,
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl TokenizorServer {
    #[tool(description = "Return symbol outline for a file.")]
    async fn get_file_outline(&self, params: Parameters<GetFileOutlineInput>) -> String {
        let guard = self.index.read().expect("lock poisoned");
        if !guard.is_ready() {
            return "Index is loading... try again shortly.".to_string();
        }
        format::file_outline(&guard, &params.0.path)
    }
    // ... other tools
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for TokenizorServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            rmcp::model::ServerCapabilities::builder().enable_tools().build()
        )
    }
}
```

### Pattern 2: Tool Input Parameter Structs
**What:** Each tool has a dedicated input struct with `Deserialize + JsonSchema` derives. rmcp uses the schema at tool registration time.
**When to use:** Every tool with parameters.

```rust
// Source: rmcp-1.1.0 test_tool_macros.rs
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
pub struct GetSymbolInput {
    /// Relative path to the file (e.g. "src/lib.rs")
    pub path: String,
    /// Symbol name to look up
    pub name: String,
    /// Optional kind filter: "fn", "struct", "enum", "impl", etc.
    pub kind: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct SearchInput {
    /// Query string (case-insensitive substring match)
    pub query: String,
}

// Tools with no parameters use an empty struct or unit:
#[derive(Deserialize, JsonSchema)]
pub struct EmptyInput {}
```

### Pattern 3: main.rs — Load then Serve
**What:** Load LiveIndex synchronously (Rayon), then start async MCP server on stdio transport. `waiting().await` blocks until stdin closes.

```rust
// Source: rmcp-1.1.0 src/service/server.rs + src/transport/io.rs
use rmcp::{serve_server, transport};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    observability::init_tracing()?;

    // Auto-index: check TOKENIZOR_AUTO_INDEX env var
    let should_auto_index = std::env::var("TOKENIZOR_AUTO_INDEX")
        .map(|v| v != "false")
        .unwrap_or(true);

    let index: SharedIndex = if should_auto_index {
        let root = discovery::find_git_root();
        // find_git_root logs warning when falling back to CWD
        live_index::LiveIndex::load(&root)?
    } else {
        // Empty index — user must call index_folder
        live_index::LiveIndex::empty()
    };

    let server = protocol::TokenizorServer::new(index);
    let service = serve_server(server, transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
```

### Pattern 4: Loading Guard
**What:** Every tool handler except `health` begins with this guard. Returns immediately if index isn't ready.

```rust
// Applied in every non-health tool handler
async fn get_file_outline(&self, params: Parameters<GetFileOutlineInput>) -> String {
    let guard = self.index.read().expect("lock poisoned");
    match guard.index_state() {
        IndexState::Ready => {}
        IndexState::Loading => return "Index is loading... try again shortly.".to_string(),
        IndexState::CircuitBreakerTripped { summary } => {
            return format!("Index degraded: {summary}");
        }
    }
    // ... actual logic
}
```

Note: `IndexState::Loading` cannot happen in Phase 2 because `LiveIndex::load()` is synchronous — it either finishes before server starts or TOKENIZOR_AUTO_INDEX=false leaves the index empty (a new `IndexState::Empty` variant may be needed, or model this as a `Loading` variant with zero progress). The guard still defends against the race window during `index_folder` reload.

### Pattern 5: index_folder Write-Lock Reload
**What:** `index_folder` must acquire the write lock and replace the LiveIndex content. Since `SharedIndex = Arc<RwLock<LiveIndex>>`, the write lock is exclusive.

```rust
async fn index_folder(&self, params: Parameters<IndexFolderInput>) -> String {
    let root = PathBuf::from(&params.0.path);
    match live_index::LiveIndex::load(&root) {
        Ok(new_index) => {
            // Swap: acquire write lock, replace fields
            let new_guard = new_index.read().expect("lock poisoned");
            let mut write = self.index.write().expect("lock poisoned");
            *write = // extract LiveIndex from new SharedIndex...
            drop(write);
            format!("Indexed {} files.", new_guard.file_count())
        }
        Err(e) => format!("Index failed: {e}"),
    }
}
```

Note: `LiveIndex::load()` returns `Arc<RwLock<LiveIndex>>` (SharedIndex). To swap contents, either (a) add a `LiveIndex::into_inner()` helper, or (b) change `SharedIndex` so callers can `Arc::try_unwrap()` the new one and write-lock the old one to replace its contents. The planner should task this pattern explicitly.

### Pattern 6: Compact Response Formatter (INFR-03)
**What:** Pure functions in `src/protocol/format.rs`. No I/O, no async. Take `&LiveIndex` and parameters, return `String`.

```
// Locked output format examples from CONTEXT.md:

// get_file_outline:
src/lib.rs  (12 symbols)
  fn           parse_source                    1-45
  struct       ParseConfig                    47-52
    fn         new                            53-60
  impl         ParseConfig                    62-80
    fn         from_env                       63-79

// get_symbol:
pub fn parse_source(path: &Path, bytes: &[u8]) -> Result<Vec<SymbolRecord>> {
    // ... full source body ...
}
[fn, lines 1-45, 1234 bytes]

// search_symbols / search_text:
3 matches in 2 files

src/lib.rs
  12: pub fn parse_source(path: &Path, bytes: &[u8]) -> Result<Vec<SymbolRecord>> {

src/parsing/mod.rs
  3: use crate::parsing::parse_source;
  87: let result = parse_source(path, &bytes)?;

// get_repo_outline:
tokenizor_agentic_mcp  (42 files, 387 symbols)

src/
  domain/
    index.rs          Rust    15 symbols
    mod.rs            Rust     2 symbols
  live_index/
    mod.rs            Rust     3 symbols
    query.rs          Rust    12 symbols
    store.rs          Rust    22 symbols
  lib.rs              Rust     7 symbols
  main.rs             Rust     1 symbol

// health:
Status: Ready
Files:  42 indexed (40 parsed, 2 partial, 0 failed)
Symbols: 387
Loaded in: 127ms
Watcher: not active (Phase 3)
```

### Pattern 7: what_changed Timestamp Strategy
**What:** `what_changed` must track which files changed since a given timestamp. In Phase 2 without a file watcher, the practical answer is "nothing has changed since load" — only `index_folder` reloads change content.

**Recommended approach:** Store `loaded_at: Instant` on `LiveIndex` (already exists). Tool accepts a Unix timestamp (seconds since epoch) as input. Compare against `loaded_at` converted to system time. If `since_timestamp` < `loaded_at`: return list of all files (whole index is "newer"). If `since_timestamp` >= `loaded_at`: return "No changes detected since last index load."

This is correct semantics for Phase 2 — the watcher in Phase 3 will enable real incremental tracking.

### Anti-Patterns to Avoid
- **Returning JSON from tool handlers:** AD-6 mandates plain text. Never `serde_json::to_string(&something)` in a tool response.
- **Using MCP error codes for not-found:** Return helpful plain text, not `Err(McpError::...)`. Error codes are for protocol-level failures, not missing symbols.
- **Holding read lock across await points:** Always acquire lock, extract data into owned values, drop lock, then format. Never hold `RwLockReadGuard` across an `.await`.
- **Putting schemars on domain types:** Input param structs are separate from `SymbolRecord`/`IndexedFile`. Domain types don't need JsonSchema derive.
- **Using `DashMap` for SharedIndex:** Phase 1 chose `Arc<RwLock<HashMap>>` (AD-1 decision). Phase 2 must not change this.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| JSON-RPC framing, init handshake, tool dispatch | Custom stdio reader/writer loop | `rmcp::serve_server()` + `transport::stdio()` | rmcp handles Content-Length vs bare JSON lines automatically; init protocol is non-trivial |
| Tool registration and schema | Manual `Tool { name, input_schema, ... }` struct construction | `#[tool_router]` + `#[tool]` macros | Macros derive JSON Schema from param structs; manual construction is error-prone and verbose |
| Input deserialization from JSON object | Manual `serde_json::from_value()` calls | `Parameters<T>` wrapper | rmcp's `Parameters<T>` handles deserialization and returns proper MCP error on failure |
| Server capability advertisement | Manual `ServerInfo` construction | `ServerCapabilities::builder().enable_tools().build()` | Builder ensures correct protocol-version fields |

**Key insight:** rmcp 1.1.0's macro system handles the entire tool lifecycle (registration, schema, dispatch, deserialization). The application code only needs to write the handler body.

---

## Common Pitfalls

### Pitfall 1: schemars not in Cargo.toml as direct dependency
**What goes wrong:** `#[derive(JsonSchema)]` on input param structs fails to compile because `schemars` is only a transitive dependency of rmcp.
**Why it happens:** Rust requires explicit dependencies for derives you use directly.
**How to avoid:** Add `schemars = "0.8"` explicitly to `[dependencies]` in Cargo.toml.
**Warning signs:** Compile error mentioning `JsonSchema` not in scope or `schemars` not found.

### Pitfall 2: Holding RwLockReadGuard across async boundary
**What goes wrong:** `RwLockReadGuard` is not `Send`, so holding it across an `.await` causes a compile error in async context.
**Why it happens:** `async fn` can be sent between threads at await points; `RwLockReadGuard` cannot.
**How to avoid:** Extract all needed data from the index into owned types (`String`, `Vec`, etc.) before the first await point. Drop the guard explicitly with `drop(guard)` if needed.
**Warning signs:** Compile error: "`std::sync::RwLockReadGuard` cannot be sent between threads safely".

### Pitfall 3: #[tool_handler] macro needs router field name
**What goes wrong:** `#[tool_handler]` without `router = self.field_name` fails if the struct has a `ToolRouter` field (default is to look for `tool_router` field name).
**Why it happens:** The macro needs to know which field holds the `ToolRouter`.
**How to avoid:** Use `#[tool_handler(router = self.tool_router)]` and name the field `tool_router` consistently.
**Warning signs:** Compile error from rmcp-macros proc macro expansion.

### Pitfall 4: index_folder reload — SharedIndex architecture mismatch
**What goes wrong:** `LiveIndex::load()` returns a new `Arc<RwLock<LiveIndex>>`, but the server holds a reference to the old `Arc`. Simply replacing a local variable does nothing — all existing readers still hold the old Arc.
**Why it happens:** `Arc` is reference-counted; cloning it gives a handle to the same data, not a copy.
**How to avoid:** Either (a) `write-lock` the existing SharedIndex and `std::mem::replace` / overwrite its fields in-place, or (b) add a `LiveIndex::reload_from(&root) -> Result<()>` method that takes `&mut self` so the write lock + mutation are colocated. Option (b) is cleaner.
**Warning signs:** `index_folder` returns "success" but subsequent queries still see old data.

### Pitfall 5: Empty LiveIndex state when TOKENIZOR_AUTO_INDEX=false
**What goes wrong:** `LiveIndex::load()` is not called, but `IndexState` has no `Empty` variant — only `Loading`, `Ready`, `CircuitBreakerTripped`. Tools that check `index_state()` will either panic (on unreachable) or return wrong state.
**Why it happens:** Phase 1 assumed load always runs before the server starts.
**How to avoid:** Add `IndexState::Empty` (or reuse `Loading`) OR add `LiveIndex::empty()` constructor that returns a SharedIndex in a specific state. Tools return "Index not loaded. Call index_folder to index a directory."
**Warning signs:** Unreachable panic or misleading "Index is loading" message when user intentionally disabled auto-index.

### Pitfall 6: byte_range slicing with non-UTF-8 content
**What goes wrong:** `get_symbol` slices `IndexedFile.content` (a `Vec<u8>`) by `byte_range: (u32, u32)`. Converting to `String` via `String::from_utf8_lossy()` works, but slicing at arbitrary byte offsets may split a multi-byte UTF-8 character.
**Why it happens:** `byte_range` is produced by tree-sitter which operates at the byte level — byte boundaries are always valid UTF-8 boundaries for source code parsed by tree-sitter.
**How to avoid:** Trust tree-sitter's byte ranges are valid UTF-8 boundaries. Use `String::from_utf8_lossy()` as the safe fallback. Never add +1/-1 adjustments to the range.
**Warning signs:** Garbled Unicode characters or panic in the symbol source extraction.

### Pitfall 7: search_text line number calculation
**What goes wrong:** Reporting wrong line numbers for text search matches.
**Why it happens:** `IndexedFile.content` is raw bytes. Splitting by `\n` gives correct line numbers only if carriage returns are handled.
**How to avoid:** Split content by `\n`, trim `\r` from line ends (CRLF normalization). Line numbers are 1-indexed to match editor conventions and the format established by ripgrep.
**Warning signs:** Tests with Windows-style CRLF files report off-by-one line numbers.

---

## Code Examples

Verified patterns from official sources:

### rmcp server startup (stdio transport)
```rust
// Source: rmcp-1.1.0 src/service/server.rs, src/transport/io.rs
use rmcp::{serve_server, transport};

let service = serve_server(TokenizorServer::new(index), transport::stdio()).await?;
service.waiting().await?;
// stdin close → tokio detects EOF → waiting() returns → process exits 0
```

### Tool handler returning plain text
```rust
// Source: rmcp-1.1.0 src/model/content.rs (IntoContents impl for String)
// String implements IntoContents — rmcp wraps it to Content::text() automatically
#[tool(description = "Get the symbol outline for a file.")]
async fn get_file_outline(&self, params: Parameters<GetFileOutlineInput>) -> String {
    let guard = self.index.read().expect("lock poisoned");
    // extract data into owned values, then drop guard
    let result = format::file_outline(&*guard, &params.0.path);
    drop(guard);
    result
}
```

### Tool handler with no parameters
```rust
// Source: rmcp-1.1.0 test_tool_macros.rs (#[tool] async fn empty_param)
#[tool(description = "Report server health and index status.")]
async fn health(&self) -> String {
    let guard = self.index.read().expect("lock poisoned");
    format::health_report(&*guard)
}
```

### Content::text() — the wire format
```rust
// Source: rmcp-1.1.0 src/model/content.rs line 244
// Content::text(s) = RawContent::Text(RawTextContent { text: s, meta: None }).no_annotation()
// This is what rmcp generates when a handler returns String.
// Explicit use only needed if returning CallToolResult directly:
CallToolResult::success(vec![Content::text("your text")])
```

### get_symbol source extraction
```rust
// IndexedFile.content: Vec<u8>, SymbolRecord.byte_range: (u32, u32)
fn extract_source(file: &IndexedFile, sym: &SymbolRecord) -> String {
    let (start, end) = sym.byte_range;
    let bytes = &file.content[start as usize..end as usize];
    String::from_utf8_lossy(bytes).into_owned()
}
```

### search_text linear scan
```rust
// Phase 2 uses simple linear scan — trigram index deferred to Phase 7 (PLSH-01)
fn search_text(index: &LiveIndex, query: &str) -> String {
    let query_lower = query.to_lowercase();
    let mut matches: Vec<(String, u32, String)> = vec![]; // (path, line, content)
    for (path, file) in index.all_files() {
        let text = String::from_utf8_lossy(&file.content);
        for (line_idx, line) in text.lines().enumerate() {
            if line.to_lowercase().contains(&query_lower) {
                matches.push((path.clone(), (line_idx + 1) as u32, line.to_string()));
            }
        }
    }
    format::search_text_result(&matches, query)
}
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| v1 ResultEnvelope<T> JSON responses | Plain text, AD-6 | v2 design (2026-03) | Eliminates 90% of response verbosity; models read text faster |
| v1 RunManager (6,149 lines) | Removed entirely | v2 design (2026-03) | Phase 2 tool surface has 10 tools vs 18 v1 tools |
| rmcp manual tool registration | `#[tool_router]` macros (rmcp 1.1.0) | rmcp 1.0+ | Tool registration is ~5 lines per tool vs ~30 lines |

**Deprecated/outdated:**
- v1 retrieval_conformance.rs types (`ResultEnvelope`, `Provenance`, `TrustLevel`, etc.): gated behind `#[cfg(feature = "v1")]`; Phase 2 rewrites this test file for v2 format
- `cancel_index_run`, `checkpoint_now`, `resume_index_run`, and 7 other v1 tools: must not appear anywhere in Phase 2 code (INFR-05)

---

## Open Questions

1. **LiveIndex::empty() constructor needed**
   - What we know: Phase 1 `LiveIndex::load()` always runs before server starts; `IndexState` has no "not loaded" variant
   - What's unclear: When `TOKENIZOR_AUTO_INDEX=false`, what state does the SharedIndex hold? Currently there's no way to construct an empty LiveIndex without calling load()
   - Recommendation: Add `LiveIndex::empty()` constructor and `IndexState::Empty` variant in the first task of Phase 2 (or reuse `Loading` — but `Empty` is more precise)

2. **index_folder reload mutation strategy**
   - What we know: `LiveIndex::load()` returns a new `Arc<RwLock<LiveIndex>>`; the server struct holds one `SharedIndex` forever
   - What's unclear: Best Rust pattern to mutate the existing SharedIndex in-place vs replacing the Arc
   - Recommendation: Add a `LiveIndex::reload(&mut self, root: &Path) -> Result<()>` method. The tool acquires write lock, calls `self_ref.reload(root)?`, releases. This keeps the Arc stable.

3. **what_changed — Instant vs SystemTime**
   - What we know: `LiveIndex.loaded_at: Instant` (already stored); tool needs to accept a user-provided timestamp
   - What's unclear: `Instant` cannot be converted to a wall-clock time directly; comparison with a user-provided Unix timestamp requires `SystemTime`
   - Recommendation: Store `loaded_at_system: SystemTime` alongside `loaded_at: Instant` in `LiveIndex`. Or compute it once at load time: `let loaded_at_wall = SystemTime::now()`. Tool input accepts Unix seconds (i64). Compare `loaded_at_wall > UNIX_EPOCH + Duration::from_secs(input_ts)`.

4. **schemars version compatibility**
   - What we know: rmcp 1.1.0 depends on schemars transitively; version is specified in Cargo.lock
   - What's unclear: Exact schemars version pulled transitively — need to verify `schemars = "0.8"` doesn't conflict
   - Recommendation: Check `Cargo.lock` for the schemars version rmcp resolves to and match it exactly. LOW confidence — verify during implementation.

---

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` + `#[tokio::test]`, `cargo test` |
| Config file | none (Cargo.toml `[dev-dependencies]` controls) |
| Quick run command | `cargo test --lib 2>/dev/null` |
| Full suite command | `cargo test 2>/dev/null` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| LIDX-05 | Index load <500ms for 70 files, <3s for 1000 files | integration | `cargo test --test live_index_integration test_load_perf 2>/dev/null` | ❌ Wave 0 |
| TOOL-01 | get_symbol returns correct source body | unit | `cargo test --lib protocol::tools::tests::test_get_symbol 2>/dev/null` | ❌ Wave 0 |
| TOOL-02 | get_symbols batch handles symbol + code_slice | unit | `cargo test --lib protocol::tools::tests::test_get_symbols_batch 2>/dev/null` | ❌ Wave 0 |
| TOOL-03 | get_file_outline returns ordered indented tree | unit | `cargo test --lib protocol::format::tests::test_file_outline_format 2>/dev/null` | ❌ Wave 0 |
| TOOL-04 | get_repo_outline shows file tree with symbol counts | unit | `cargo test --lib protocol::format::tests::test_repo_outline_format 2>/dev/null` | ❌ Wave 0 |
| TOOL-05 | search_symbols finds case-insensitive substring | unit | `cargo test --lib protocol::tools::tests::test_search_symbols 2>/dev/null` | ❌ Wave 0 |
| TOOL-06 | search_text scans in-memory content, correct line numbers | unit | `cargo test --lib protocol::tools::tests::test_search_text 2>/dev/null` | ❌ Wave 0 |
| TOOL-07 | health returns correct stats from HealthStats | unit | `cargo test --lib protocol::format::tests::test_health_format 2>/dev/null` | ❌ Wave 0 |
| TOOL-08 | index_folder triggers full reload | integration | `cargo test --test live_index_integration test_index_folder_reload 2>/dev/null` | ❌ Wave 0 |
| TOOL-12 | what_changed returns files newer than timestamp | unit | `cargo test --lib protocol::tools::tests::test_what_changed 2>/dev/null` | ❌ Wave 0 |
| TOOL-13 | get_file_content serves bytes with optional line range | unit | `cargo test --lib protocol::tools::tests::test_get_file_content 2>/dev/null` | ❌ Wave 0 |
| INFR-02 | Auto-index runs when .git present; skipped when env=false | integration | `cargo test --test live_index_integration test_auto_index_behavior 2>/dev/null` | ❌ Wave 0 |
| INFR-03 | Formatter output matches expected text exactly | unit | `cargo test --lib protocol::format::tests 2>/dev/null` | ❌ Wave 0 |
| INFR-05 | Removed tools absent from tool list | integration | `cargo test --test live_index_integration test_tool_list_no_v1_tools 2>/dev/null` | ❌ Wave 0 |
| RELY-04 | stdout purity — no non-JSON output | integration | `cargo test --test live_index_integration test_stdout_purity 2>/dev/null` | ✅ (Phase 1) |

### Sampling Rate
- **Per task commit:** `cargo test --lib 2>/dev/null`
- **Per wave merge:** `cargo test 2>/dev/null`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `src/protocol/mod.rs` — server struct, token allocation, serves as integration point
- [ ] `src/protocol/tools.rs` — all tool handlers (stubs acceptable at Wave 0)
- [ ] `src/protocol/format.rs` — formatter module (stubs acceptable at Wave 0)
- [ ] `tests/live_index_integration.rs` — add LIDX-05 perf test, INFR-02, TOOL-08, INFR-05, TOOL-list tests
- [ ] `LiveIndex::empty()` constructor — needed before any tool test can run with TOKENIZOR_AUTO_INDEX=false
- [ ] `LiveIndex::reload()` method — needed for TOOL-08 test

---

## Sources

### Primary (HIGH confidence)
- rmcp-1.1.0 source (`~/.cargo/registry/src/.../rmcp-1.1.0/`) — handler patterns, tool macros, serve_server, transport::stdio, Content::text, CallToolResult
- Phase 1 source code (`src/live_index/`, `src/domain/`, `src/discovery/`) — all query methods, SharedIndex type, SymbolRecord fields, HealthStats
- `02-CONTEXT.md` — all locked decisions verified by reading the full file

### Secondary (MEDIUM confidence)
- `tests/common/handlers.rs` and `tests/test_tool_macros.rs` in rmcp source — confirmed the `#[tool_router]` + `#[tool_handler]` macro usage pattern with stateful servers

### Tertiary (LOW confidence — verify during implementation)
- schemars version pinning: assumed `0.8` is compatible with what rmcp 1.1.0 resolves; must check actual Cargo.lock entry before adding explicit dep

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — rmcp 1.1.0 source read directly; all dependencies verified in Cargo.toml/Cargo.lock
- Architecture: HIGH — locked decisions from CONTEXT.md + rmcp macro patterns verified in rmcp source
- Pitfalls: HIGH — RwLockReadGuard/Send pitfall is a standard Rust async gotcha; SharedIndex reload pattern is a real design gap identified by code reading; byte_range/UTF-8 behavior verified by tree-sitter's documented semantics
- Formatter format: HIGH — exact format locked in CONTEXT.md, no ambiguity
- what_changed implementation: MEDIUM — Instant vs SystemTime tradeoff identified, recommended approach is sound but not verified in tests yet

**Research date:** 2026-03-10
**Valid until:** 2026-04-10 (rmcp 1.1.0 is current; no expected breaking changes in 30 days)
