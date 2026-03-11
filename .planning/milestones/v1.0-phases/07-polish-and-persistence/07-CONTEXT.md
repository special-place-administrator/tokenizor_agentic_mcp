# Phase 7: Polish and Persistence - Context

**Gathered:** 2026-03-10
**Status:** Ready for planning

<domain>
## Phase Boundary

The server restarts in under 100ms by loading a serialized index, search returns ranked results, and C/C++ languages are supported. Delivers LiveIndex persistence (PLSH-04, PLSH-05), trigram text search (PLSH-01), scored symbol search (PLSH-02), file tree navigation tool (PLSH-03), and tree-sitter parsing for C and C++ (LANG-01, LANG-02). Other languages (C#, Ruby, PHP, Swift, Dart) are deferred to post-v2 releases.

</domain>

<decisions>
## Implementation Decisions

### Index persistence (PLSH-04, PLSH-05)
- Serialization format: postcard 1.1.3 — replaces bincode 2.x per RUSTSEC-2025-0141 (unmaintained), near-identical serde API, community-endorsed replacement
- No schema migration — version tag at file header, mismatch triggers full re-index
- Serialize on shutdown only (stdin EOF / SIGTERM) — no periodic or per-change writes
- If process crashes, next start does full re-index (same as today, no regression)
- File location: `.tokenizor/index.bin` alongside existing `sidecar.port` and `sidecar.pid`
- Background verification after loading serialized index:
  1. Deserialize index.bin (~3ms)
  2. Stat-check all files (mtime + size) — changed files queued for re-parse, deleted files removed, new files queued
  3. Serve queries immediately (stale-ok for brief window)
  4. Background: re-parse queued files
  5. Background: spot-verify 10% of files by content hash — mismatches trigger re-parse
- Corrupted or unreadable index.bin: fall back to full re-index without crashing

### Trigram text search (PLSH-01)
- In-memory trigram index built alongside LiveIndex from content bytes already in RAM
- Data structure: HashMap<[u8;3], Vec<u32>> — trigram to list of file IDs (positions dropped; file-level search is the use case)
- Rebuilt on startup from content bytes (~50ms for 1000 files) — not persisted to disk
- Updated incrementally when watcher re-indexes a file (remove old trigrams, add new)
- Replaces current linear scan in search_text transparently — same tool contract, same response format, just faster
- Target: <10ms for any query on 10,000-file repo

### Scored symbol search (PLSH-02)
- 3-tier ranking: exact match (score 100) > prefix match (score 75) > substring match (score 50)
- Tiebreak within tiers: exact = alpha, prefix = shorter name wins, substring = earlier position wins
- Results grouped under tier headers in output: `── Exact matches ──`, `── Prefix matches ──`, `── Substring matches ──`
- Replaces current unranked search_symbols — same tool, upgraded ranking
- Case-insensitive matching continues (established in Phase 2)

### File tree tool (PLSH-03)
- New tool: get_file_tree — browsable subtree separate from existing get_repo_outline
- Optional `path` parameter: browse specific directory subtree (default: project root)
- Optional `depth` parameter: how many levels deep (default 2, max 5)
- Source files only — non-source files (README, Cargo.toml, etc.) omitted
- Each file shows: language + symbol count
- Each directory shows: file count + total symbol count
- Collapsed summary for directories beyond depth limit
- Footer: total dirs, files, symbols

### Language additions (LANG-01, LANG-02)
- C: tree-sitter-c grammar, extensions `.c`, `.h`
- C++: separate tree-sitter-cpp grammar (extends C), extensions `.cpp`, `.cxx`, `.cc`, `.hpp`, `.hxx`, `.hh`
- Two new LanguageId variants: `C` and `Cpp`
- Full parity with original 6 languages: symbol extraction + cross-reference extraction
- C xrefs: function calls, #include imports, type identifiers, struct field access
- C++ xrefs: method calls, namespace-qualified identifiers, template instantiation, using declarations (alias map)
- Deferred languages (post-v2): C# (LANG-03), Ruby (LANG-04), PHP (LANG-05), Swift (LANG-06), Dart (LANG-07)

### Claude's Discretion
- postcard 1.1.3 exact API usage and configuration
- Trigram index internal data structure optimizations (e.g., sorted posting lists for faster intersection)
- Exact tree-sitter query patterns for C and C++ symbol/xref extraction
- Shutdown hook implementation (signal handler vs stdin EOF detection)
- get_file_tree formatter details (exact alignment, indentation)
- How stat-check handles files with identical mtime but different content (edge case)

</decisions>

<specifics>
## Specific Ideas

- Search results with tier headers make ranking observable: `── Exact matches ──`, `── Prefix matches ──`, `── Substring matches ──` — directly satisfies success criteria #3
- File tree output format follows the established indented-tree pattern from get_file_outline and get_repo_outline
- Trigram index is a transparent upgrade — the model doesn't know or care that search_text got faster
- `.h` files map to C (pragmatic default) — C++ headers should use `.hpp`/`.hxx`

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `src/live_index/store.rs`: LiveIndex with all query methods, content bytes in memory — trigram index builds from these bytes, persistence serializes this store
- `src/protocol/format.rs`: Compact response formatters for outline, search, symbol detail — file tree and ranked search formatters follow the same pattern
- `src/protocol/tools.rs`: MCP tool handlers with `loading_guard!` macro — get_file_tree handler follows same pattern
- `src/parsing/languages/*.rs`: 6 language modules — C and C++ modules follow the same extraction pattern
- `src/live_index/xref.rs`: Cross-reference extraction with tree-sitter queries — C/C++ xref queries follow the same pattern
- `src/watcher/mod.rs`: maybe_reindex with content hash skip — trigram index update hooks into the same re-index path
- `IndexedFile.content_hash`: SHA-256 hash already computed — used for background spot-verification after loading serialized index

### Established Patterns
- Parse-before-lock: read → hash check → parse → write lock → update. Trigram update piggybacks on the same flow.
- `loading_guard!` macro for all tool handlers — get_file_tree uses it too
- Compact human-readable output (AD-6) — tier headers and file tree follow this style
- `LanguageId` enum with `from_extension()` and `extensions()` — extend with C and Cpp variants
- `src/parsing/languages/` module per language — add `c.rs` and `cpp.rs`

### Integration Points
- `src/domain/index.rs`: Add `LanguageId::C` and `LanguageId::Cpp` variants with extension mappings
- `src/parsing/mod.rs`: Wire C and C++ grammar dispatch in `process_file()`
- `src/live_index/store.rs`: Add serialization support (serde derives or manual bincode), add trigram index field
- `src/protocol/tools.rs`: Add get_file_tree handler, update search_symbols with ranking, update search_text with trigram backend
- `src/protocol/format.rs`: Add file_tree formatter, update search_symbols formatter for tier headers
- `src/main.rs`: Add shutdown hook for index serialization, add startup path for deserialization before auto-index
- `Cargo.toml`: Add `postcard` 1.1.3, `tree-sitter-c`, `tree-sitter-cpp` dependencies

</code_context>

<deferred>
## Deferred Ideas

- C# language support (LANG-03) — post-v2 release
- Ruby language support (LANG-04) — post-v2 release
- PHP language support (LANG-05) — post-v2 release
- Swift language support (LANG-06) — post-v2 release
- Dart language support (LANG-07) — post-v2 release

</deferred>

---

*Phase: 07-polish-and-persistence*
*Context gathered: 2026-03-10*
