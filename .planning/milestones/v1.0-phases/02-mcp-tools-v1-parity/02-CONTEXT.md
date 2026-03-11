# Phase 2: MCP Tools v1 Parity - Context

**Gathered:** 2026-03-10
**Status:** Ready for planning

<domain>
## Phase Boundary

A shippable MCP server where all core retrieval tools query the LiveIndex and return compact, human-readable responses. This phase wires 10 MCP tools (get_symbol, get_symbols, get_file_outline, get_repo_outline, search_symbols, search_text, health, index_folder, what_changed, get_file_content), adds auto-index on startup, builds the compact response formatter, and ensures all v1 over-infrastructure tools are absent. No file watcher, no cross-references, no hooks — those are later phases.

</domain>

<decisions>
## Implementation Decisions

### Response format
- Indented tree style for get_file_outline: symbol names indented by depth with kind prefix (e.g. `fn main`, `struct Config`), line ranges right-aligned. Header line shows filename + symbol count.
- get_symbol returns the full source body always (extracted via byte_range from stored content). Footer shows kind, line range, byte count.
- Search results (search_symbols, search_text) grouped by file, ripgrep-style: file header, then indented line_number:match lines. Summary line at top (e.g. "3 matches in 2 files").
- get_repo_outline uses file tree with symbol counts: directory tree structure, each file shows language + symbol count. Header line shows project name + totals.
- All responses are plain text, never JSON envelopes. Matches AD-6.

### Tool surface & contracts
- Not-found errors return helpful plain text as normal content (not MCP error codes). Include suggestions: "No symbol X in file Y. Symbols in that file: ..." or "File not found: path".
- get_symbols supports both symbol and code_slice request types in Phase 2. Code slices use byte_range against stored content (LIDX-03 makes this trivial).
- search_symbols: substring matching, case-insensitive. No fuzzy match, no regex. Relevance ranking (exact > prefix > substring) deferred to Phase 7 (PLSH-02).
- search_text: full content scan against in-memory file contents. Simple string matching, case-insensitive. Trigram index deferred to Phase 7 (PLSH-01). Linear scan acceptable for <1,000 files.

### Auto-index behavior
- Server auto-indexes on startup when .git is present. Falls back to CWD if no .git found (Phase 1 fallback preserved). Logs a warning when using CWD fallback.
- TOKENIZOR_AUTO_INDEX env var: set to "false" to disable auto-index. Server starts with empty LiveIndex, user must call index_folder. Default: true.
- index_folder does a full reload from scratch: drop current index, re-discover, re-parse everything. No incremental diff — the watcher (Phase 3) handles incremental.
- While index is loading (startup or index_folder reload): all tools except health return "Index is loading... try again shortly." Health always responds with Loading state.

### MCP server wiring
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

</decisions>

<specifics>
## Specific Ideas

- Response format should feel like ripgrep/Grep output — familiar to models that already use those tools
- Indented tree preview for outline: `fn main  1-15` with consistent column alignment
- get_symbol footer format: `[struct, lines 17-42, 312 bytes]`
- Search summary header: `3 matches in 2 files`
- The guard pattern (block tools while loading) was established in Phase 1 context — health is always the sole exception

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `LiveIndex` + `SharedIndex` (Arc<RwLock<LiveIndex>>): ready to use, all query methods exist (get_file, symbols_for_file, all_files, health_stats)
- `IndexedFile.content: Vec<u8>`: file bytes already in memory (LIDX-03), enables code_slice and get_file_content without disk I/O
- `SymbolRecord`: has name, kind, depth, sort_order, byte_range, line_range — all fields needed for formatting
- `HealthStats`: file_count, symbol_count, parsed/partial/failed counts, load_duration — ready for health tool
- `ParseStatus` enum: Parsed/PartialParse/Failed — queryable per file
- `CircuitBreakerState`: already tracks and reports failure details
- `discovery::find_git_root()`: returns root path, falls back to CWD

### Established Patterns
- `tracing` on stderr, ANSI disabled — continue for server logging
- `thiserror` for domain errors, `anyhow` at CLI/main boundary
- Rayon for parallel operations (LiveIndex::load uses par_iter)
- `rmcp` 1.1.0 with transport-io feature already in Cargo.toml
- `serde` + `serde_json` already dependencies

### Integration Points
- `src/main.rs`: currently loads index and exits — Phase 2 replaces the exit with MCP server startup
- `src/lib.rs`: needs to export new protocol module
- No src/protocol/ exists yet — create from scratch
- `Cargo.toml`: rmcp, serde, serde_json already present. May need serde derives added back to domain types for MCP input deserialization.

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 02-mcp-tools-v1-parity*
*Context gathered: 2026-03-10*
