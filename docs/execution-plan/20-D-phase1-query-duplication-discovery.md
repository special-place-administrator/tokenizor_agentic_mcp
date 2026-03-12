# Discovery: Phase 1 Query Duplication

Related plan:

- [03-P-architecture-and-guardrails.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/03-P-architecture-and-guardrails.md)
- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [20-T-phase1-query-duplication-discovery.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/20-T-phase1-query-duplication-discovery.md)

Goal:

- identify where current query semantics live today instead of assuming a clean query layer already exists
- name the smallest real edit points that can support the Phase 1 shared query substrate without rewriting unrelated code

## Files Inspected

- `src/protocol/tools.rs`
- `src/protocol/format.rs`
- `src/live_index/query.rs`
- `src/live_index/store.rs`
- `src/sidecar/handlers.rs`

## Current Semantic Split

### 1. Protocol handlers are thin, but they route into inconsistent semantic owners

- `TokenizorServer::search_symbols`, `search_text`, `find_references`, `get_file_content`, and `get_context_bundle` in `src/protocol/tools.rs` lines 519-755 hand raw input directly into `src/protocol/format.rs`
- `TokenizorServer::get_file_context` and `get_symbol_context` in `src/protocol/tools.rs` lines 438-486 bypass `src/protocol/format.rs`, drop the read guard, and re-enter the same index through sidecar helpers
- result: the public tool layer looks thin, but there are already two downstream semantic owners for read and query behavior: `format.rs` and `sidecar/handlers.rs`

### 2. `format.rs` owns query planning and ranking, not just rendering

- `search_text_result_with_options` in `src/protocol/format.rs` lines 219-292 normalizes terms, validates empty input, compiles regexes, selects trigram candidates, and defines OR semantics before formatting anything
- `collect_text_matches` in `src/protocol/format.rs` lines 294-347 sorts candidate paths, scans lines, counts totals, and shapes grouped output in one function
- `search_symbols_result_with_kind` in `src/protocol/format.rs` lines 116-205 implements substring matching, exact/prefix/substring tiers, tier-specific tie-breakers, and the 50-result cap
- `find_references_result` in `src/protocol/format.rs` lines 803-866 parses the kind filter, calls the index query, groups by file, materializes context windows, and annotates enclosing symbols
- `context_bundle_result` in `src/protocol/format.rs` lines 929-985 resolves the symbol match, slices symbol body bytes, decides which follow-up queries count as callers/callees/type usages, and then formats all sections
- result: the formatter currently decides search semantics, ranking, filtering, caps, and context rules, so a future query layer cannot be added cleanly without extracting logic out of this file first

### 3. `live_index/query.rs` owns low-level lookup primitives, but not shared query contracts

- `find_references_for_name` in `src/live_index/query.rs` lines 508-567 decides builtin filtering, qualified-name fallback, reverse-index lookup, and alias expansion
- `collect_refs_for_key` in `src/live_index/query.rs` lines 573-597 is the actual reverse-index fetch primitive
- `find_dependents_for_file` in `src/live_index/query.rs` lines 610-686 owns import and module heuristics for dependent lookup
- `callees_for_symbol` in `src/live_index/query.rs` lines 693-722 owns intra-symbol call extraction
- `LiveIndex` in `src/live_index/store.rs` lines 228-243 exposes only raw storage and two accelerators today: `reverse_index` and `trigram_index`
- result: the index layer has usable primitives, but there is no reusable `QueryOptions`, `SearchRequest`, `ResultWindow`, or `PathMatch` type between those primitives and the formatters

### 4. Sidecar handlers duplicate both discovery and presentation logic

- `outline_text` in `src/sidecar/handlers.rs` lines 138-227 rebuilds a file-outline header and symbol rows independently instead of reusing `format::file_outline`, then adds its own "Key references" ranking based on `find_dependents_for_file`
- `symbol_context_text` in `src/sidecar/handlers.rs` lines 573-655 calls `find_references_for_name` directly, reapplies file filtering, caps output at 10 matches, groups by file, resolves enclosing symbols, and budgets the output
- `find_prompt_file_hint` in `src/sidecar/handlers.rs` lines 827-865 scans every indexed path for exact-path or basename matches, which is an early path-resolution implementation living outside the query layer
- `find_prompt_symbol_hint` in `src/sidecar/handlers.rs` lines 867-889 scans every file and symbol for an exact token hit, which is a second symbol-discovery path outside `search_symbols`
- result: prompt and sidecar flows already contain their own mini query engines, so adding a shared substrate only under MCP tools would leave semantic drift in place

## Real Duplication Points

### Text search duplication

- candidate selection happens in `format::search_text_result_with_options`, not in `live_index/query.rs`
- line scanning and grouping happen in `format::collect_text_matches`
- prompt-driven file discovery in `find_prompt_file_hint` is a separate path lookup implementation with different ambiguity behavior

### Symbol search duplication

- symbol existence and ranking for public tools live in `format::search_symbols_result_with_kind`
- prompt symbol discovery lives in `find_prompt_symbol_hint` and does its own full scan with a different matching rule
- reference-oriented symbol lookup in `context_bundle_result` resolves symbols again from raw file symbols instead of using a shared symbol-match contract

### Reference and context duplication

- `format::find_references_result`, `format::context_bundle_result`, and `sidecar::symbol_context_text` all start from reference primitives but each owns its own grouping, caps, and context decisions
- `outline_text` computes caller-like ranking from `find_dependents_for_file` separately from `context_bundle_result` and `find_references_result`
- the same underlying reference data therefore produces different selection and truncation behavior depending on which surface the user hits

## Most Likely First Edit Points

- first extraction target: move search and ranking decisions out of `src/protocol/format.rs` into a new query-facing module under `src/live_index/`, likely `search.rs` or `query_engine.rs`
- first concrete functions to extract:
- `search_text_result_with_options` query normalization and candidate selection
- `search_symbols_result_with_kind` match-tier and ordering logic
- the non-format parts of `find_references_result` and `context_bundle_result`, especially kind parsing, grouping, caps, and context-window selection
- first sidecar consumers to repoint after that extraction:
- `symbol_context_text`
- `outline_text`
- `find_prompt_file_hint`
- `find_prompt_symbol_hint`
- first substrate types worth introducing:
- `SearchScope` or `PathScope`
- `ResultLimit`
- a shared `TextSearchRequest` and `TextSearchHit`
- a shared `SymbolSearchRequest` and `SymbolSearchHit`
- a shared grouped-reference result shape for both MCP and sidecar consumers

## Modules Likely Safe To Leave Unchanged In The First Substrate Slice

- `src/protocol/tools.rs` input structs and top-level handler registration can stay thin wrappers in the first slice; they mainly need to call a new shared query layer instead of `format.rs`
- `src/live_index/store.rs` load, reload, mutation, and circuit-breaker behavior can stay unchanged while the first query-layer extraction happens
- `src/live_index/query.rs` primitives such as `find_references_for_name`, `collect_refs_for_key`, `find_dependents_for_file`, and `callees_for_symbol` can remain as the initial low-level substrate beneath the new shared layer
- sidecar token-budget accounting and savings reporting can stay where they are; the drift problem is in query selection, not in budget formatting itself

## Smallest Recommended Next Research Slice

- task 21 should compare only a few file-shape options for the shared query layer, not redesign the whole index
- the decision should focus on one question: whether the first shared layer should be a thin orchestration module over existing `LiveIndex` primitives or a broader `QueryEngine` type that also owns grouping and ranking helpers
- the smallest safe target is a layer that centralizes:
- text-search request normalization
- symbol-match ranking
- grouped reference selection and caps
- path and basename matching for prompt and future path tools

## Carry-Forward

- real duplication is concentrated in `src/protocol/format.rs` and `src/sidecar/handlers.rs`, not in `src/protocol/tools.rs`
- `src/live_index/query.rs` already has enough primitive behavior to support a thin first query layer
- `src/live_index/store.rs` is mainly a future home for new indices, not the first place to untangle semantics
- OPEN: task 21 still needs to decide whether the first shared layer belongs in `src/live_index/query.rs`, a sibling `search.rs`, or a new `query_engine.rs`
