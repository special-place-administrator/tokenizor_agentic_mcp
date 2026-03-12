# Research: Phase 1 Query Layer Shape

Related plan:

- [03-P-architecture-and-guardrails.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/03-P-architecture-and-guardrails.md)
- [20-D-phase1-query-duplication-discovery.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/20-D-phase1-query-duplication-discovery.md)
- [21-T-phase1-query-layer-shape-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/21-T-phase1-query-layer-shape-research.md)

Goal:

- choose the smallest safe internal query-layer shape for the first Phase 1 substrate slice
- keep the first extraction incremental enough to preserve current behavior and benchmarkability

## Constraints From The Current Codebase

- `src/live_index/mod.rs` currently exposes only `persist`, `query`, `store`, and `trigram`
- `src/live_index/query.rs` already mixes several concerns:
- module and import heuristics for dependents
- builtin-name filtering
- health statistics
- low-level reference and callee lookup primitives
- `src/protocol/format.rs` currently owns too much semantic work:
- text-search normalization and candidate selection
- symbol ranking and result caps
- grouped reference selection and context windows
- symbol-resolution work for `get_context_bundle`
- `src/sidecar/handlers.rs` contains separate prompt-path, prompt-symbol, outline, and symbol-context selection logic
- the current `RwLock<LiveIndex>` read path is simple and fast; the first substrate slice should not force a large lock-model rewrite before benchmarks prove it is needed

## Candidate A: Expand `src/live_index/query.rs`

### Shape

- keep a single `query.rs`
- move extracted search, ranking, and grouped-reference helpers into that file beside the existing `LiveIndex` query primitives

### Advantages

- smallest file-count delta
- no new module naming decision
- easy for the first implementation patch to wire in

### Risks

- `query.rs` is already carrying health stats, module-resolution helpers, builtin filters, and low-level reference primitives
- adding higher-level request and result structs here would turn it into a catch-all rather than a clean substrate
- future Phase 2 through Phase 4 work would keep piling path, text-lane, and read-shaping logic into the same file
- the distinction between primitive index lookups and shared query contracts would still be blurry

### Migration Risk

- low short-term churn
- medium long-term cleanup cost because the first extraction would likely need another re-split later

### Benchmark Implications

- near-zero overhead compared with today if functions still take `&LiveIndex`
- but little structural help for the later snapshot-based read-path work because the file boundary remains muddled

### Verdict

- workable, but too likely to become a temporary holding pen that must be split again soon

## Candidate B: Add `src/live_index/search.rs` As A Sibling Module

### Shape

- keep `src/live_index/query.rs` as the home for low-level lookup primitives already attached to `LiveIndex`
- add `src/live_index/search.rs` as a thin orchestration layer over those primitives
- place shared request and result structs in `search.rs`
- let `src/protocol/format.rs` and `src/sidecar/handlers.rs` consume those structs for presentation

### Intended Responsibilities

- text-search request normalization
- symbol-match ranking and tie-breakers
- grouped reference selection and per-surface caps
- path and basename matching for prompt hints and future path tools
- shared symbol-resolution helpers for `get_context_bundle` and sidecar symbol context

### Explicit Non-Responsibilities

- no string rendering
- no token-budget footers or sidecar stats recording
- no index loading, mutation, or circuit-breaker behavior
- no large snapshot or ownership redesign in the first slice

### Advantages

- creates a clean seam between primitive index lookups and query semantics without disturbing `store.rs`
- keeps `query.rs` small enough to remain the primitive layer
- gives both MCP tools and sidecar handlers one shared semantic owner
- supports incremental migration:
- first move text and symbol selection
- then move grouped reference and symbol-resolution helpers
- then repoint sidecar prompt and context helpers
- aligns with the plan’s suggested end state of a shared search or query-engine module

### Risks

- `search.rs` is slightly narrower as a name than the eventual full query layer because it may also own reference grouping and path hint logic
- during migration, `format.rs` will temporarily call into `search.rs` while still formatting strings, so the split will be partial before it is complete

### Migration Risk

- lowest net risk
- requires only one new module export in `src/live_index/mod.rs`
- lets existing call sites keep their current lock and handler structure while semantics move underneath them
- minimizes test churn because existing formatter-output tests can stay in place while internal helpers move behind them

### Benchmark Implications

- lowest-risk benchmark profile for the first implementation slice
- the new functions can initially accept `&LiveIndex` and return small owned result structs, avoiding a new locking or snapshot regime
- this preserves comparability against the existing Phase 0 and existing `get_context_bundle` under-100ms anchor
- if later measurements show lock-hold time is still a problem, `search.rs` can become the place where snapshot extraction is introduced deliberately instead of forcing that decision now

### Verdict

- preferred
- this is the smallest safe starting point that improves structure now without pre-committing to a heavier engine abstraction

## Candidate C: Add A `query_engine.rs` Module Or `QueryEngine` Type

### Shape

- introduce a dedicated engine abstraction such as `QueryEngine<'a>` or a snapshot-backed engine object
- route tool and sidecar surfaces through that engine instead of calling `LiveIndex` primitives directly

### Advantages

- strongest long-term story for snapshot-based reads
- makes ownership of query orchestration very explicit
- could eventually provide a stable internal API for tools, resources, prompts, and sidecar surfaces

### Risks

- forces several decisions too early:
- engine lifetime and ownership model
- whether it borrows `LiveIndex` or copies a snapshot
- where grouped results and content excerpts are materialized
- how much existing `LiveIndex` API becomes engine-only
- broader call-site churn in `protocol` and `sidecar`
- higher chance of mixing structural redesign with the first semantic extraction

### Migration Risk

- highest of the three options
- likely to touch more tests and more internal APIs before the semantic split has even been proven

### Benchmark Implications

- potentially best long-term path for reduced lock hold times
- but highest immediate benchmark uncertainty because the first version would likely introduce extra allocations or copying before there is measurement evidence that the added abstraction pays for itself
- this is the option most likely to threaten the existing `context_bundle_result` performance anchor if done too early

### Verdict

- good future option once the shared semantics have already been extracted and benchmarked
- too large for the first Phase 1 substrate slice

## Preferred Shape

- add `src/live_index/search.rs`
- keep it as a module of shared query semantics, not a new engine type
- keep `src/live_index/query.rs` as the primitive layer for:
- reference retrieval
- dependent lookup
- callee lookup
- module-path and builtin-name heuristics
- move these first into `search.rs` or wrappers over them:
- text-search request normalization and candidate selection
- symbol ranking and capped result selection
- grouped reference selection and context-window rules
- shared symbol-resolution helpers for bundle and sidecar flows
- prompt path and prompt symbol matching helpers

## Recommended First Implementation Boundary

- `src/protocol/tools.rs` remains thin and continues to acquire the index read lock
- `src/protocol/format.rs` becomes presentation-only over shared result structs from `search.rs`
- `src/sidecar/handlers.rs` keeps budget enforcement and token-savings accounting, but stops owning its own query semantics
- `src/live_index/store.rs` remains unchanged in the first slice
- `src/live_index/query.rs` remains unchanged except for any tiny visibility adjustments needed so `search.rs` can reuse existing primitives

## Migration Sequence

1. Add `src/live_index/search.rs` plus a minimal set of internal request and result structs.
2. Repoint `search_text_result_with_options` and `search_symbols_result_with_kind` to shared `search.rs` helpers while preserving current output text.
3. Move grouped-reference and symbol-resolution logic out of `find_references_result`, `context_bundle_result`, and `symbol_context_text` into shared helpers.
4. Repoint `find_prompt_file_hint` and `find_prompt_symbol_hint` to the same path and symbol matching helpers.
5. Benchmark before considering any snapshot-backed engine or broader ownership rewrite.

## Benchmark Notes

- preserve the current handler shape for the first slice so before and after timings remain comparable
- use the existing `tests/xref_integration.rs` `get_context_bundle` under-100ms anchor plus the Phase 0 compatibility envelope as the regression guard
- do not combine the first semantic extraction with secondary-index or snapshot-copy work; that would make any latency change harder to attribute

## Carry-Forward

- chosen shape: add `src/live_index/search.rs` as a sibling shared-semantic module
- keep formatting in `src/protocol/format.rs` and budget rendering in `src/sidecar/handlers.rs`
- defer any `QueryEngine` type or snapshot-backed abstraction until after the smaller extraction is benchmarked
- OPEN: exact internal type names can stay flexible during implementation as long as the ownership boundary remains the same
