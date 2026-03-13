# Symbol-Addressed Edit Operations — Handoff Document

## Status
- **Branch:** `feat/symbol-addressed-edits`
- **Phase:** Research complete, ready to write implementation plan
- **Plan target:** `docs/superpowers/plans/2026-03-13-symbol-addressed-edits.md`

## What We're Building

Add edit tools to Tokenizor that accept **symbol addresses** instead of requiring the LLM to read file contents. The index resolves positions server-side. The LLM names the target symbol and provides new content — never reads the raw file.

### Tools (Tier 1 — Single-file)
- **replace_symbol_body** — Replace a symbol's entire body by name + path
- **insert_before_symbol / insert_after_symbol** — Insert code adjacent to a named symbol
- **delete_symbol** — Remove a symbol cleanly
- **edit_within_symbol** — Scoped text replacement constrained to a symbol's byte range

### Tools (Tier 2 — Batch)
- **batch_edit** — Apply edits across multiple files/symbols in one call (atomic)
- **batch_rename** — Rename a symbol and update all references project-wide
- **batch_insert** — Add same code to multiple locations

## Research Findings

### Index Internals
- `SymbolRecord` in `src/domain/index.rs:229-236` has `byte_range: (u32, u32)` — absolute byte offsets, half-open `[start, end)`
- `IndexedFile` in `src/live_index/store.rs:32-46` has `content: Vec<u8>` — raw file bytes in memory
- Symbol body extraction: `content[sym.byte_range.0 as usize..sym.byte_range.1 as usize]`
- Parent-child via `depth: u32` field + `find_enclosing_symbol()` in `query.rs:324-342`
- 16 languages, all produce uniform `SymbolRecord` — editing is language-agnostic

### Reference Resolution (for batch_rename)
- `ReferenceRecord` in `src/domain/index.rs:279-293` has `byte_range: (u32, u32)` — byte-precise reference sites
- `reverse_index: HashMap<String, Vec<ReferenceLocation>>` in `store.rs:300` — O(1) name→locations lookup
- `find_exact_references_for_symbol()` in `query.rs:1315-1346` — finds all references to a specific symbol
- `find_references_for_name()` in `query.rs:1890-1948` — global name search with alias resolution
- References include `enclosing_symbol_index` for context

### File Write & Re-index
- **No file write path exists today** — tools are read-only
- `update_file()` at `store.rs:725` re-indexes a single file (re-parse + reverse index rebuild)
- `SharedIndexHandle` at `store.rs:313-453` uses `RwLock` with write guard + generation tracking
- Watcher uses `notify` crate with debounce (200-500ms) in `src/watcher/mod.rs`
- Watcher's `maybe_reindex` compares SHA hash — if we write+reindex before watcher fires, hash matches and watcher skips (natural dedup)
- No incremental tree-sitter — full re-parse per file (~1-5ms, acceptable)

### Tool Handler Pattern
- Tools defined in `src/protocol/tools.rs` with `#[tool(description = "...")]` attribute
- Input structs: `#[derive(Deserialize, Serialize, JsonSchema)]` with `pub struct FooInput { ... }`
- Handler signature: `pub(crate) async fn foo(&self, params: Parameters<FooInput>) -> String`
- Index access: `self.index.read()` for reads, `self.index.write()` for mutations (see `analyze_file_impact`)
- Proxy support: `self.proxy_tool_call("tool_name", &params.0).await`
- Sidecar state: `sidecar_state_for_server(self)` for handlers that need the sidecar
- Mutation tools exist: `analyze_file_impact` reads from disk + calls `update_file()`
- `loading_guard!` macro checks index is loaded before proceeding
- Daemon registration: tools auto-discovered via the `#[tool]` macro on `TokenizorServer` impl

### Symbol Disambiguation
- `resolve_symbol_selector()` in `query.rs:314-348` — resolves name+kind+line to exact symbol
- Returns `SymbolSelectorMatch` enum: `Exact(usize)`, `Ambiguous(Vec<u32>)`, `NotFound`
- Ambiguous case returns candidate line numbers for user to disambiguate

## Architecture Decision: Edit Flow

```
Agent calls replace_symbol_body(path, name, new_body)
  → Acquire read lock, resolve symbol via resolve_symbol_selector (get byte_range)
  → Read content from IndexedFile.content (in-memory, no disk I/O)
  → Drop read lock
  → Splice new_body into content at byte_range (in memory)
  → Write modified bytes to disk (atomic: write temp file, rename)
  → Re-parse file + update_file() (acquires write lock, rebuilds indices)
  → Check callers for signature changes → emit stale ref warnings
  → Return compact summary: "path:symbol — replaced (N bytes → M bytes)"
```

For batch operations: collect all edits, validate all symbol resolutions first, then apply all writes atomically (temp files → rename all on success, rollback on failure).

## Design Constraints
1. Every edit tool must re-index affected files after writing
2. Return compact summaries, not changed code
3. Stale reference warnings after signature-changing edits
4. Symbol resolution must be fuzzy-tolerant (same matching as search_symbols)
5. Indentation preservation on insert operations

## File Structure for Implementation

New files to create:
- `src/protocol/edit.rs` — Edit tool input structs, core edit logic (splice, indent, atomicity)
- `src/protocol/edit_format.rs` — Output formatting for edit results

Files to modify:
- `src/protocol/tools.rs` — Register new tool handlers, wire to edit.rs
- `src/protocol/mod.rs` — Add `pub mod edit; pub mod edit_format;`
- `src/live_index/store.rs` — May need a `write_and_reindex()` convenience method
- `src/daemon.rs` — Import new input types for daemon tool registration

## Token Savings Impact

| Operation | Current cost | With symbol-addressed edits | Savings |
|---|---|---|---|
| Add method to large file | ~3,400 tokens | ~600 tokens | 82% |
| Edit 2 lines within a method | ~3,300 tokens | ~300 tokens | 91% |
| Rename across 20 files | ~50,000 tokens | ~400 tokens | 99% |
| Pattern change in 20 files | ~44,000 tokens | ~700 tokens | 98% |

## Next Step
Write the implementation plan at `docs/superpowers/plans/2026-03-13-symbol-addressed-edits.md` using the writing-plans skill. Start with Tier 1 tools, then Tier 2.
