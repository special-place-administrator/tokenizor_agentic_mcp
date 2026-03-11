# Phase 4: Cross-Reference Extraction - Context

**Gathered:** 2026-03-10
**Status:** Ready for planning

<domain>
## Phase Boundary

The index tracks call sites, imports, and type usages across all 6 languages (Rust, Python, JS, TS, Go, Java) so find_references returns accurate results with low false-positive rates. Delivers three new MCP tools: find_references, find_dependents, get_context_bundle. References update incrementally when files are re-parsed by the watcher.

Requirements: XREF-01 through XREF-08, TOOL-09, TOOL-10, TOOL-11.

</domain>

<decisions>
## Implementation Decisions

### False Positive Filtering
- Index ALL references at extraction time; apply filters at query time only
- Hardcoded built-in type filter lists per language (string, number, bool, i32, u8, etc.) — no configuration env vars
- Hardcoded single-letter generic filter lists (T, K, V, E, etc.) — no configuration
- Common names like 'new', 'get', 'set' are indexed and queryable — not filtered by default
- Qualified match when available: find_references('process') matches all; find_references('Vec::new') uses qualified_name to narrow. Leverages tree-sitter's scoped_identifier/field_expression text

### Import Alias Resolution (XREF-05)
- Per-file alias map: build HashMap<String, String> (alias→original) from import statements during parse
- Stored persistently in the file entry alongside symbols and references
- find_references('HashMap') also matches references via alias 'Map' if 'use HashMap as Map' exists in that file

### get_context_bundle Scope (TOOL-11)
- Direct callers and callees only — depth 1, no transitive expansion
- Full symbol body included (not just signature) — this is "get_symbol + references in one call"
- Each caller/callee entry shows: symbol name, file path, line number, enclosing function name — one line per reference, no source snippets
- Cap at 20 per section (callers, callees, type usages). If more exist, show count ("...and 15 more callers")
- Must stay under 100ms per success criteria

### find_references Output (TOOL-09)
- Group results by file (file path headers, references listed with line numbers within each file)
- 3 lines of source context per reference (1 before, the reference line, 1 after)
- Enclosing symbol name shown inline with each result ("in fn handle_request")
- Optional kind filter parameter: kind=call|import|type_usage|all (default: all)

### find_dependents Output (TOOL-10)
- Returns files that import/use symbols from a given file
- Same compact format as find_references, grouped by file

### Reference Storage Model
- Per-file storage: each file entry gets Vec<ReferenceRecord> alongside Vec<SymbolRecord>
- Per-file alias map: HashMap<String, String> stored in file entry
- Incremental update: when a file is re-parsed, replace symbols, references, and alias map atomically (matches existing update_file pattern)
- Reverse index (name→locations) built lazily for fast find_references queries
- Reverse index rebuilt synchronously after each file update — always consistent, no stale results
- XREF-08 satisfied by the atomic per-file replacement + synchronous reverse index rebuild

### Claude's Discretion
- Tree-sitter query patterns per language (research doc has draft templates as starting point)
- ReferenceRecord struct details (research doc has suggested shape)
- Reverse index data structure choice (HashMap, BTreeMap, etc.)
- Exact built-in type and generic filter lists per language
- find_dependents implementation details (likely derived from import references)
- How to determine "callees" for get_context_bundle (references made BY the target symbol)

</decisions>

<specifics>
## Specific Ideas

- Research doc at `docs/summaries/research-xref-extraction-and-file-watching.md` has complete node type tables for all 6 languages and draft query templates — use as implementation starting point
- Extraction should use tree-sitter Query/QueryCursor API (not extending existing walk_node) per research recommendation
- The existing `src/parsing/` modules handle definitions; xref extraction is a parallel pipeline using the same parsed tree

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `src/parsing/mod.rs` + `src/parsing/languages/*.rs`: Tree-sitter parsing pipeline for all 6 languages — xref queries run on the same parsed trees
- `src/live_index/store.rs`: LiveIndex with update_file/add_file/remove_file — xref storage extends the per-file entry
- `src/protocol/format.rs`: Compact response formatters (health_report, file_outline, repo_outline, symbol_detail) — new tool formatters follow the same pattern
- `src/watcher/mod.rs`: maybe_reindex does parse-before-lock — xref extraction plugs into the same parse step
- `src/protocol/tools.rs`: MCP tool handlers with loading_guard! macro — new tools follow the same pattern

### Established Patterns
- Parse-before-lock: read lock for hash check → drop → parse → write lock for update. Xref extraction happens during the parse step (no extra lock needed)
- loading_guard! macro for tool handlers: eliminates IndexState boilerplate
- format.rs functions accept &LiveIndex directly — new formatters will too
- Compact human-readable output (AD-6): no JSON envelopes, match Read/Grep style

### Integration Points
- LiveIndex file entry needs new fields: references (Vec<ReferenceRecord>), alias_map (HashMap<String, String>), plus a repo-level reverse index
- maybe_reindex in watcher needs to extract references during the parse step and include them in the update_file call
- Three new tool handlers in tools.rs: find_references, find_dependents, get_context_bundle
- Three new formatters in format.rs for the tool responses

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 04-cross-reference-extraction*
*Context gathered: 2026-03-10*
