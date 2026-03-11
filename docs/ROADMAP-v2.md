# Tokenizor v2 — Full Rewrite Roadmap

> **Vision**: A Rust-native MCP server that keeps the entire project live in memory,
> parasitically integrates with the coding CLI's native tools via hooks, and delivers
> cross-reference-powered retrieval that saves 80-95% of tokens on typical code exploration tasks.

---

## Table of Contents

1. [Problem Statement](#1-problem-statement)
2. [Architecture Decisions](#2-architecture-decisions)
3. [What We Keep / Remove / Add](#3-what-we-keep--remove--add)
4. [Milestone 1: Foundation](#milestone-1-foundation)
5. [Milestone 2: Intelligence](#milestone-2-intelligence)
6. [Milestone 3: Integration](#milestone-3-integration)
7. [Milestone 4: Polish](#milestone-4-polish)
8. [Success Criteria](#success-criteria)
9. [Risk Register](#risk-register)

---

## 1. Problem Statement

### What v1 got wrong

- **Over-engineered infrastructure** (run lifecycle, checkpoints, repair, 4-tier trust, operational
  history) that doesn't deliver user value. 20,000+ lines (~56% of codebase) dedicated to indexing
  bookkeeping instead of retrieval intelligence.
- **No incremental updates** — editing 1 file requires re-indexing all 70. Index is immediately stale
  after any edit, which is the exact problem that killed jCodeMunch for us.
- **No cross-references** — can't answer "who calls this function?" which is the single highest
  token-saving query. The model still reads 5+ files to find callers after every get_symbol call.
- **Verbose response format** — JSON envelopes with trust/provenance/outcome metadata that the model
  doesn't act on. Adds token overhead without value.
- **Model doesn't use it** — the model's native Read/Grep/Edit tools are preferred. MCP tools are
  second-class citizens. No integration with the model's actual workflow.
- **SpacetimeDB scaffolding** — exists as an unused alternative backend replicating the JSON registry
  model. Not designed as the in-memory project model it should be.

### What v2 must deliver

| Requirement | Metric |
|-------------|--------|
| **Never stale** | Index updated within 50ms of any file change |
| **Always in memory** | All file content, symbols, references in-process RAM |
| **Token savings** | 80%+ reduction on multi-file exploration tasks |
| **Model adoption** | Zero behavior change required — hooks enrich native tools |
| **Reliability** | Circuit breaker on parse failures, graceful degradation |
| **Speed** | All queries < 1ms from in-memory index |
| **Cross-references** | find_references resolves in < 1ms for any symbol |

---

## 2. Architecture Decisions

### AD-1: In-Process LiveIndex is the primary data store

All queries resolve from Rust HashMaps/BTreeMaps in the MCP server process. No disk I/O on the
read path. No external database for queries. SpacetimeDB is deferred to Phase 5+ (if cold start
on very large repos becomes a problem).

**Rationale**: For repos under 10,000 files (~50MB source), in-process data structures are faster
than any external database including SpacetimeDB. The MCP server is a long-running process — data
persists for the session. On restart, re-index from disk (takes 2-5 seconds in Rust with tree-sitter).

### AD-2: Parasitic hook integration, not tool replacement

Do NOT try to replace the model's Read/Grep/Edit tools. Instead, hook into PostToolUse to enrich
every native tool call with structural context from the LiveIndex.

**Rationale**: Models are trained on their native tools. CLAUDE.md instructions to "prefer MCP tools"
are fragile — the model drifts. Hooks are deterministic: every Read gets an outline injected, every
Edit triggers re-index + impact analysis. Zero behavior change required from the model.

### AD-3: Cross-references are syntactic, not semantic

Use tree-sitter query patterns to extract call sites, imports, and type usages. Match references
to definitions by name (case-sensitive, within-repo). Do NOT attempt type inference, trait dispatch
resolution, or cross-crate analysis.

**Rationale**: Syntactic cross-references cover ~85% of "who calls this?" queries. Full semantic
analysis requires a language server (rust-analyzer, pyright) which adds massive complexity. The 85%
case is buildable in weeks; the 100% case requires months and external processes.

### AD-4: File watcher for continuous freshness

Use the `notify` crate with `notify-debouncer-full` to watch the repo root. On file change,
re-parse only the changed file and update LiveIndex in-place. Use RwLock so queries continue
serving during single-file updates.

**Rationale**: This is the #1 missing feature. Staleness killed jCodeMunch. Staleness makes
v1 pointless. The file watcher eliminates staleness by construction.

### AD-5: Keep the reliability infrastructure that matters

Keep: circuit breaker, cancellation tokens, content hashing for incremental detection.
Remove: run lifecycle, checkpoints, resume, repair, operational history, multi-tier trust.

**Rationale**: jCodeMunch gets stuck because it has no circuit breaker or cancellation. That's a
real problem v1 solved. But the run lifecycle (6,149 lines) is overkill — indexing takes seconds,
not minutes. If it fails, restart from scratch.

### AD-6: Compact, human-readable response format

Responses should look like what the model gets from Read/Grep — line numbers, file paths, source
code. Metadata is one line at the bottom: `[indexed 0.3s ago | 1,847 tokens saved]`. No JSON
envelopes with trust/provenance/outcome.

**Rationale**: The model processes Read-like output naturally. JSON envelopes are tokens the model
parses and discards. Match the model's expectations.

---

## 3. What We Keep / Remove / Add

### Keep (~2,400 lines, port directly)

| Component | Location | Lines | Why |
|-----------|----------|-------|-----|
| Tree-sitter parsing | `src/parsing/` | 978 | Clean, well-tested, self-contained |
| Retrieval domain types | `src/domain/retrieval.rs` | 416 | API contract types |
| Core domain types | `src/domain/index.rs` (partial) | ~600 | SymbolRecord, SymbolKind, LanguageId, FileOutcome |
| MCP input structs | `src/protocol/mcp.rs:28-196` | ~170 | Tool parameter definitions |
| Validation helpers | `src/protocol/mcp.rs:200-258` | ~60 | require_non_empty, parse_kind_filter |
| npm packaging | `npm/` | 160 | Distribution, fully independent |
| Grammar tests | `tests/tree_sitter_grammars.rs` | 63 | Direct reuse |
| Retrieval conformance tests | `tests/retrieval_conformance.rs` | 850 | Data model tests |

### Remove (~20,000 lines)

| Component | Location | Lines | Replaced By |
|-----------|----------|-------|-------------|
| RunManager | `run_manager.rs` | 6,149 | Simple index() function |
| JSON registry | `registry_persistence.rs` | 1,686 | LiveIndex in-memory |
| ControlPlane trait + impl | `control_plane.rs` | 2,039 | LiveIndex |
| CAS blob store | `local_cas.rs` + `sha256.rs` + `blob.rs` | 644 | LiveIndex stores content directly |
| SpacetimeDB store | `spacetime_store.rs` | 766 | Deferred |
| SpacetimeDB module + generated | `spacetime/` | ~2,500 | Deferred |
| Init/deployment | `init.rs` + `deployment.rs` | 3,886 | Simple startup |
| Health system | `domain/health.rs` + `application/health.rs` | 1,204 | Simple health check |
| Pipeline (checkpoint/resume parts) | `pipeline.rs` (partial) | ~800 | Simple parallel parse |
| Search (current impl) | `search.rs` | 5,358 | LiveIndex queries |

### Add (new code)

| Component | Estimated Lines | Purpose |
|-----------|----------------|---------|
| LiveIndex data structures | ~500 | HashMap-based in-memory store |
| File watcher | ~200 | notify + debouncer integration |
| Incremental indexer | ~300 | Single-file reparse + LiveIndex update |
| Cross-reference extractor | ~600 | Tree-sitter query-based ref extraction (6 languages) |
| Reference index | ~200 | refs_to / refs_from HashMaps |
| Hook scripts | ~400 | PostToolUse for Read/Edit/Write/Grep + SessionStart |
| New MCP tools | ~800 | find_references, get_context_bundle, find_dependents, explore, what_changed |
| Trigram text index | ~300 | In-memory trigram index for fast text search |
| Scored symbol search | ~150 | Relevance scoring (exact > prefix > substring > word overlap) |
| Compact response formatter | ~200 | Human-readable output matching Read/Grep format |
| **Total new** | **~3,650** | |

**Net result**: ~27,000 lines → ~6,000 lines (keep 2,400 + new 3,650). Dramatic simplification.

### New Dependencies

| Crate | Purpose |
|-------|---------|
| `notify` 8.x | File system event watching |
| `notify-debouncer-full` 0.7.x | Event debouncing + rename tracking |
| `parking_lot` | Fast RwLock for LiveIndex concurrent access |

### Removed Dependencies

| Crate | Why |
|-------|-----|
| `spacetimedb-sdk` | Deferred — not needed for v2 core |
| `fs2` | No more file-level locking on JSON registry |

---

## Milestone 1: Foundation

> **Goal**: LiveIndex in memory, file watcher keeps it fresh, basic retrieval works.
> **Exit criteria**: Model can query symbols and text from always-fresh in-memory index.

### Phase 1.1 — LiveIndex Data Structures

**What**: Define the in-memory data model that replaces CAS + JSON registry.

**Tasks**:
1. Define `LiveIndex` struct with:
   - `files: HashMap<String, FileEntry>` (relative_path → content bytes + metadata)
   - `symbols: HashMap<SymbolId, SymbolEntry>` (symbol ID → full metadata + byte range)
   - `symbols_by_file: HashMap<String, Vec<SymbolId>>` (file → ordered symbol list)
   - `symbols_by_name: HashMap<String, Vec<SymbolId>>` (name → all matching symbols)
2. Define `FileEntry`: relative_path, language, content (Vec<u8>), content_hash, byte_len,
   modified_at, symbol_count
3. Define `SymbolEntry`: id, name, kind, signature, file_path, line_range, byte_range,
   parent_id, children, depth
4. Wrap LiveIndex in `Arc<RwLock<LiveIndex>>` for concurrent access
5. Write unit tests: insert file, insert symbols, lookup by name, lookup by file

**Acceptance**: LiveIndex can store and retrieve files/symbols with O(1) lookups.

### Phase 1.2 — Initial Load from Disk

**What**: On startup, discover files, parse with tree-sitter, populate LiveIndex.

**Tasks**:
1. Reuse `src/indexing/discovery.rs` (discover_files with ignore crate)
2. Reuse `src/parsing/` (process_file for tree-sitter extraction)
3. Build `load_repository(repo_root: &Path) -> LiveIndex` function:
   - Discover files → parallel parse (tokio, bounded semaphore) → populate LiveIndex
   - Circuit breaker: if >20% of files fail parsing, abort with error
   - Store file content bytes in FileEntry (the "in-memory" part)
4. Wire into MCP server startup: `LiveIndex` is populated before serving tools
5. Benchmark: measure load time for this repo (70 files, target <500ms)

**Acceptance**: Server starts, indexes repo into memory, all file content + symbols queryable.

### Phase 1.3 — File Watcher + Incremental Update

**What**: Watch repo root, re-parse changed files, update LiveIndex in-place.

**Tasks**:
1. Add `notify` + `notify-debouncer-full` dependencies
2. Build `FileWatcher` struct:
   - Watches repo root recursively
   - Filters to supported file extensions
   - Debounce window: 200ms (batch rapid saves)
3. On file change event:
   - Read file from disk
   - Compute content hash, compare to LiveIndex entry
   - If different: re-parse with tree-sitter, update FileEntry + symbols in LiveIndex
   - If file deleted: remove from LiveIndex
   - If file created: add to LiveIndex
4. Use write lock on LiveIndex only during update (microseconds per file)
5. Integration test: modify a file on disk, verify LiveIndex reflects change within 500ms

**Acceptance**: Edit a file externally → LiveIndex updates automatically. Queries never serve stale data.

### Phase 1.4 — Basic MCP Tools on LiveIndex

**What**: Wire existing tool surface to query LiveIndex instead of CAS + registry.

**Tasks**:
1. Rewrite `get_symbol` → lookup in `symbols` HashMap by (file, name, kind_filter)
2. Rewrite `get_symbols` → batch lookup, support both symbol and code_slice targets
3. Rewrite `get_file_outline` → lookup in `symbols_by_file`, return ordered symbol list
4. Rewrite `get_repo_outline` → iterate `files` HashMap, compute coverage stats
5. Rewrite `search_symbols` → scan `symbols_by_name` with substring matching + scoring
6. Rewrite `search_text` → linear scan of file contents (upgraded to trigram in Phase 4.1)
7. Rewrite `health` → report LiveIndex stats (files loaded, symbols indexed, watcher status, last update)
8. Rewrite `index_folder` → trigger full reload of LiveIndex (for manual re-index)
9. Remove tools: cancel_index_run, checkpoint_now, resume_index_run, get_index_run,
   list_index_runs, invalidate_indexed_state, repair_index, inspect_repository_health,
   get_operational_history, reindex_repository
10. Add `get_file_content(repo_id, relative_path, start_line?, end_line?)` — serve file
    content from memory with optional line range

**Acceptance**: All retrieval tools work against LiveIndex. 749 existing tests updated or replaced.
Query latency <1ms for all tools.

---

## Milestone 2: Intelligence

> **Goal**: Cross-references, context bundles, impact analysis.
> **Exit criteria**: find_references works, get_context_bundle returns full context in one call.

### Phase 2.1 — Cross-Reference Extraction

**What**: Extract call sites, imports, and type usages during tree-sitter parsing.

**Tasks**:
1. Define `ReferenceRecord`: from_file, from_symbol (Option), to_name, ref_kind (Call/Import/TypeUse),
   line, column, context_snippet (the source line)
2. Add reference extraction to each language parser using tree-sitter queries:
   - **Rust**: `call_expression` → function field, `use_declaration`, `macro_invocation`, `type_identifier`
   - **Python**: `call` → function field, `import_statement`, `import_from_statement`
   - **JavaScript**: `call_expression`, `import_statement`, `new_expression`
   - **TypeScript**: inherits JS + `type_annotation`, `type_identifier`
   - **Go**: `call_expression`, `import_declaration`, `selector_expression`
   - **Java**: `method_invocation`, `import_declaration`, `object_creation_expression`
3. Store `enclosing_symbol` for each reference (which function/method contains the call site)
4. Add to LiveIndex:
   - `refs_to: HashMap<String, Vec<ReferenceRecord>>` (symbol name → call sites)
   - `refs_from: HashMap<SymbolId, Vec<ReferenceRecord>>` (symbol → what it calls)
   - `imports: HashMap<String, Vec<ImportRecord>>` (file → its imports)
5. Update incremental indexer: when a file is re-parsed, rebuild its references
6. Unit tests per language: parse sample code, verify references extracted correctly

**Acceptance**: For each supported language, call sites and imports are extracted and indexed.

### Phase 2.2 — Reference Query Tools

**What**: MCP tools that leverage cross-references.

**Tasks**:
1. `find_references(repo_id, symbol_name, kind_filter?)` → returns all call sites:
   - Each result: file, line, enclosing_symbol, context_snippet, ref_kind
   - Sorted by file path, then line number
2. `find_dependents(repo_id, file_path)` → returns files that import this file:
   - Uses import index to traverse dependency graph
   - Returns: importing_file, import_line, what_is_imported
3. `get_context_bundle(repo_id, symbol_name, relative_path, depth?)` → one-call full context:
   - The symbol's source code
   - All callers (from refs_to) with enclosing function source
   - All callees (from refs_from) with their source
   - The types/structs referenced in the symbol's body
   - The file's import list
   - depth=0: just the symbol; depth=1: +callers/callees; depth=2: +callers of callers
4. `what_changed(repo_id, since_unix_ms)` → files and symbols modified since timestamp:
   - Uses FileEntry.modified_at to filter
   - Returns: changed files with their changed symbols

**Acceptance**: find_references returns correct call sites for test cases in all 6 languages.
get_context_bundle returns a complete, compact context package in one call.

### Phase 2.3 — Compact Response Formatter

**What**: Responses that look like Read/Grep output, not JSON API responses.

**Tasks**:
1. Define a compact text format for each tool:
   ```
   src/protocol/mcp.rs:264-267 (struct TokenizorServer)
   pub struct TokenizorServer {
       tool_router: ToolRouter<Self>,
       application: ApplicationContext,
   }

   Referenced by: (3 sites)
     src/protocol/mcp.rs:271    impl TokenizorServer { fn new(...)
     src/main.rs:45             let server = TokenizorServer::new(app);
     tests/integration.rs:12    let server = TokenizorServer::new(test_app);

   [indexed 0.3s ago | 1,847 tokens saved vs full file read]
   ```
2. Add token_saved calculation: (file_byte_len - response_byte_len) / 4
3. Track cumulative tokens_saved per session
4. Apply to: get_symbol, get_symbols, get_context_bundle, find_references, get_file_outline

**Acceptance**: Responses are human-readable, model processes them without parsing JSON envelopes.

---

## Milestone 3: Integration

> **Goal**: Hook layer makes every native tool call smarter.
> **Exit criteria**: Model's Read/Edit/Grep calls are enriched with structural context from LiveIndex.

### Phase 3.1 — Hook Infrastructure

**What**: Python hook scripts that call into the running MCP server for context.

**Tasks**:
1. Design hook ↔ MCP server communication:
   - Option A: hooks call MCP tools via a lightweight CLI (`tokenizor-query`)
   - Option B: hooks communicate via a Unix socket / named pipe to the MCP process
   - Option C: hooks read a memory-mapped file that LiveIndex maintains
   - **Decision needed**: evaluate latency requirements (hooks must respond in <100ms)
2. Build the communication channel (chosen option)
3. Create hook registration JSON for Claude Code's hooks.json format
4. Test hook invocation latency end-to-end

**Acceptance**: A hook script can query LiveIndex and return context in <100ms.

### Phase 3.2 — PostToolUse Hooks

**What**: Enrich every Read/Edit/Write/Grep with LiveIndex context.

**Tasks**:
1. **PostToolUse(Read)** — when model reads a source file:
   - Query LiveIndex for file outline
   - If file >50 lines: append symbol list + key references
   - Include get_symbol hint with repo_id and file path pre-filled
2. **PostToolUse(Edit)** — when model edits a source file:
   - Trigger incremental re-index of the edited file (file watcher handles this,
     but hook can trigger immediate re-index for <50ms guarantee)
   - Query LiveIndex for changed symbols + their callers
   - Append impact analysis: "Modified X. N callers may need review: [list]"
3. **PostToolUse(Write)** — when model creates a new file:
   - Trigger index of the new file
   - Append: "File indexed. N symbols extracted."
4. **PostToolUse(Grep)** — when model greps for a pattern:
   - Query LiveIndex for symbol context of matched lines
   - Append: which function contains each match, call chain if available
5. **SessionStart** — on first tool call of a session:
   - Generate compact repo map (~500 tokens): file count, top-level module structure,
     key entry points, total symbol count
   - List available tokenizor tools with brief descriptions

**Acceptance**: Every Read of an indexed source file includes an appended symbol outline.
Every Edit includes an impact analysis. Grep results include structural context.

### Phase 3.3 — Incremental Re-index on Edit

**What**: Guarantee <50ms re-index after every Edit/Write tool call.

**Tasks**:
1. PostToolUse(Edit) hook triggers re-parse of the specific file
2. Re-parse flow: read file from disk → tree-sitter parse → extract symbols + references →
   acquire write lock → update FileEntry + symbols + references → release lock
3. Measure and optimize: target <50ms for a 1,000-line file
4. Handle edge cases: file deleted during edit, syntax errors (partial parse → keep old symbols)

**Acceptance**: After any Edit, the LiveIndex reflects the new file state within 50ms.

---

## Milestone 4: Polish

> **Goal**: Performance optimizations, additional languages, production hardening.
> **Exit criteria**: Tool is reliable, fast, and covers the languages that matter.

### Phase 4.1 — Trigram Text Search

**What**: Replace linear text scan with trigram index for instant search.

**Tasks**:
1. Build trigram index: `HashMap<[u8; 3], Vec<(FileId, LineNumber)>>`
2. Populate during initial load and incremental updates
3. Query: split search term into trigrams → intersect posting lists → verify matches
4. Benchmark: compare to current linear scan and to ripgrep

**Acceptance**: search_text is <10ms for any query on a 10,000-file repo.

### Phase 4.2 — Additional Language Support

**What**: Add tree-sitter grammars for the 9 languages defined in LanguageId but not yet parsing.

**Tasks** (prioritized by usage):
1. C (`tree-sitter-c`) — high priority
2. C++ (`tree-sitter-cpp`) — high priority
3. C# (`tree-sitter-c-sharp`) — medium priority
4. Ruby (`tree-sitter-ruby`) — medium priority
5. PHP (`tree-sitter-php`) — medium priority
6. Swift (`tree-sitter-swift`) — lower priority
7. Dart (`tree-sitter-dart`) — lower priority
8. Perl, Elixir — lowest priority

Each language needs: symbol extraction + reference extraction + tests.

### Phase 4.3 — Scored Symbol Search

**What**: Relevance-ranked symbol search (matching jCodeMunch's scoring).

**Tasks**:
1. Scoring algorithm:
   - Exact name match: 20 points
   - Prefix match: 15 points
   - Substring match: 10 points
   - Word overlap (camelCase/snake_case split): 5 points per word
   - Signature match: 8 points
   - Kind match (when filter specified): 5 bonus points
2. Return top-N results sorted by score
3. Include: symbol name, kind, file, line, signature, score

### Phase 4.4 — File Tree Navigation

**What**: `get_file_tree(repo_id, path_filter?)` for directory browsing.

**Tasks**:
1. Build tree structure from LiveIndex file paths
2. Each node: name, type (file/dir), language (if file), symbol_count, byte_size
3. Optional path filter to scope to subdirectory
4. Return as indented text, not JSON tree

### Phase 4.5 — Persistence for Fast Restart

**What**: Serialize LiveIndex to disk for fast cold start.

**Tasks**:
1. On clean shutdown: serialize LiveIndex to binary file (bincode or messagepack)
2. On startup: if serialized file exists AND is newer than any source file,
   load from serialized file (target: <100ms for 10,000 files)
3. If serialized file is stale: full re-index from disk
4. Background verification: after loading from serialized file, hash-check
   a random sample of files to detect staleness

---

## Success Criteria

### Token Savings (measured, not estimated)

| Scenario | Without Tokenizor | With Tokenizor v2 | Target Savings |
|----------|-------------------|-------------------|----------------|
| Explore unfamiliar codebase (10 files) | ~30,000 tokens | ~2,000 (repo map + outlines) | >90% |
| Understand a function + its callers | ~25,000 tokens | ~2,500 (get_context_bundle) | >85% |
| Post-edit impact check | ~15,000 tokens | ~500 (proactive hook injection) | >95% |
| Find all usages of a symbol | ~10,000 tokens | ~800 (find_references) | >90% |

### Performance

| Operation | Target |
|-----------|--------|
| Initial load (70 files) | <500ms |
| Initial load (1,000 files) | <3s |
| Single-file re-index after edit | <50ms |
| Any query from LiveIndex | <1ms |
| Hook response time | <100ms |
| File watcher detection | <200ms (debounce) |

### Reliability

| Guarantee | How |
|-----------|-----|
| Never serves stale data | File watcher + re-index on Edit hook |
| Never gets stuck | Circuit breaker aborts if >20% files fail |
| Graceful degradation | Partial parse → serve what parsed, log warning |
| Handles syntax errors | Keep previous symbols on parse failure |
| Handles file deletion | Remove from LiveIndex, update references |
| Handles concurrent access | RwLock: many readers, exclusive writer (microsecond writes) |

---

## Risk Register

| Risk | Impact | Likelihood | Mitigation |
|------|--------|-----------|------------|
| Tree-sitter reference extraction misses edge cases (macros, generics, closures) | Some references missing from index | High | Accept 85% coverage. Document known gaps. Syntactic matching is a feature, not a bug — it's predictable. |
| File watcher misses events on certain platforms (Docker, network FS) | Stale index | Medium | Fallback: periodic hash-check sweep every 30s. Manual re-index tool as escape hatch. |
| Hook response time >100ms degrades model UX | Hooks feel sluggish | Low | All queries from in-process memory (<1ms). Hook overhead is Python startup (~50ms). Consider compiled hook binary if needed. |
| Model ignores hook-injected context | No behavior change, no token savings | Medium | Track adoption metrics. Tune injection format. The proactive impact analysis (Edit hook) provides value regardless of whether model reads it — it surfaces information the model wouldn't otherwise have. |
| PostToolUse hook output gets truncated by Claude Code | Partial context injection | Low | Keep injections compact (<500 tokens). Prioritize most valuable information first. |
| LiveIndex memory usage too high for very large repos | OOM for monorepos with 100K+ files | Low | This serves repos up to ~50MB source comfortably. For larger repos, add file-level eviction (keep symbols, evict content bytes for untouched files). |

---

## Dependency Graph

```
Phase 1.1 (LiveIndex structs)
  └─→ Phase 1.2 (initial load)
        └─→ Phase 1.3 (file watcher)
        └─→ Phase 1.4 (basic MCP tools)
              └─→ Phase 2.1 (cross-reference extraction)
                    └─→ Phase 2.2 (reference query tools)
                          └─→ Phase 2.3 (compact responses)
                                └─→ Phase 3.1 (hook infrastructure)
                                      └─→ Phase 3.2 (PostToolUse hooks)
                                      └─→ Phase 3.3 (incremental re-index on Edit)

Phase 4.x (all independent, can run after Milestone 1):
  Phase 4.1 (trigram search) — after Phase 1.4
  Phase 4.2 (more languages) — after Phase 1.2
  Phase 4.3 (scored search) — after Phase 1.4
  Phase 4.4 (file tree) — after Phase 1.1
  Phase 4.5 (persistence) — after Phase 1.1
```

---

## Resolved Architecture Questions

### Q1: Hook ↔ MCP Server Communication — DECIDED: HTTP Sidecar

The MCP server spawns a tiny HTTP server on `127.0.0.1:0` (random free port) alongside
the stdio MCP handler using `axum` + `tokio::spawn`. Port written to `.tokenizor/sidecar.port`.

Hook scripts: read port file → `urllib.request.urlopen("http://localhost:{port}/outline?file=...")`
→ format response as `additionalContext` JSON → print to stdout.

Sidecar endpoints share the same `Arc<LiveIndex>` as MCP tools. Zero data duplication.

**Latency budget**: Python spawn (~40ms) + HTTP localhost (~10ms) + LiveIndex query (<1ms) = **~50-80ms**.
Claude Code hook timeout is 600s. Hooks are synchronous (blocking) so Claude sees context immediately.

For static data (session-start repo map): write `repo-outline.json` to `.tokenizor/derived/`.
SessionStart hook reads file directly, no HTTP needed.

**New dependency**: `axum` (lightweight, already uses tokio).

### Q2: Repo Auto-Detection — DECIDED: Auto-Index on Startup

Auto-index the working directory if `.git` exists. Write port file + repo map on completion.
Config option `TOKENIZOR_AUTO_INDEX=false` to disable.

### Q3: Multi-Repo — DECIDED: Single Repo for v2

One LiveIndex per MCP server process. Multi-repo is a v3 concern.

### Q4: Hook Installation — DECIDED: `tokenizor init` Command

A `tokenizor init` subcommand writes PostToolUse hook entries into the project's
`.claude/hooks.json` (creating it if needed). Hooks point to Python scripts bundled
with the npm package or installed alongside the binary.

Manual override: user can edit hooks.json directly. Init is idempotent (won't duplicate entries).

### Hook Input/Output Format (from research)

**Input** (stdin to hook script):
```json
{
  "session_id": "abc123",
  "cwd": "/path/to/project",
  "hook_event_name": "PostToolUse",
  "tool_name": "Read",
  "tool_input": { "file_path": "/path/to/file.rs" },
  "tool_response": { ... }
}
```

**Output** (stdout from hook script):
```json
{
  "hookSpecificOutput": {
    "hookEventName": "PostToolUse",
    "additionalContext": "Text Claude will see after the tool result"
  }
}
```
