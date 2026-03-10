# Phase 1: LiveIndex Foundation - Context

**Gathered:** 2026-03-10
**Status:** Ready for planning

<domain>
## Phase Boundary

A functional in-memory index that loads all project source files on startup, stores symbols with O(1) lookup, and never panics on bad input. This phase delivers the LiveIndex data structure, file discovery, symbol extraction integration, circuit breaker, and a clean-slate codebase. No MCP tools, no file watcher, no cross-references -- those are later phases.

</domain>

<decisions>
## Implementation Decisions

### Rewrite approach
- Clean-slate rewrite: delete all v1 modules except `src/parsing/` (978 lines, kept as-is), partial `src/domain/index.rs` (SymbolRecord, SymbolKind, LanguageId), `npm/`, and `tests/tree_sitter_grammars.rs`
- Deletion happens as the first task of Phase 1 (not a separate prep step)
- New module structure: `src/live_index/` (store, query), `src/discovery/` (file walking), plus rewritten `main.rs`, `lib.rs`, `error.rs`
- Kept domain types stripped to minimal derives -- remove JsonSchema and serde annotations that only served registry/CAS. Add back what's needed when MCP tools land in Phase 2.
- Fresh test suite: delete all `tests/` except `tree_sitter_grammars.rs` and `retrieval_conformance.rs`. Write new tests alongside new code.

### Discovery scope
- Only index files with known language extensions (6 languages: Rust, Python, JS, TS, Go, Java)
- Non-code files (.json, .toml, .yaml, .md) are NOT stored in LiveIndex -- they exist on disk only
- Discovery walks from .git root (auto-detected). Fallback: CWD if .git not found.
- .gitignore-respected filtering only (via `ignore::WalkBuilder`). No custom .tokenizorignore in v2.

### Index readiness model
- Block all MCP tool responses until full index load completes (guard pattern)
- `health` tool is the sole exception -- always responds regardless of state (reports: loading, ready, degraded, circuit_breaker_tripped)
- All other tools return an error ("Index loading") until ready
- Circuit breaker trips (>20% parse failures): server marks degraded, stops indexing, refuses all non-health queries
- Phase 1 validates readiness via integration tests (Rust API: `is_ready()` method). MCP wiring comes in Phase 2.

### Degradation messaging
- Partial parse (syntax errors, some symbols extracted): silent -- file stored with extracted symbols, warning logged to stderr via tracing. No user-facing indication.
- Total parse failure: file stored with content bytes but empty symbol list (Claude's discretion). Counts toward circuit breaker threshold.
- Circuit breaker error message: one-line summary + first 3-5 failed file paths with reasons, plus suggested action. Not a full file list.
- Circuit breaker threshold configurable via `TOKENIZOR_CB_THRESHOLD` env var (defaults to 20%)
- Failed files auto-retry on next watcher event (Phase 3 implements retry; Phase 1 stores failure state)
- Per-file parse status (Parsed, PartialParse, Failed) stored as queryable field in LiveIndex
- Health stats include: file counts, symbol counts, parse status breakdown, total index load duration
- Logging: tracing + tracing-subscriber on stderr, ANSI disabled, env-filter defaulting to `info`. Simplified from v1 (no deployment report, no readiness gate complexity).

### Claude's Discretion
- Exact DashMap vs RwLock<HashMap> choice for concurrent map
- Internal data layout of LiveIndex entries (field ordering, auxiliary indexes)
- Whether total-parse-failure files store content or are excluded (leaning toward store content)
- Exact error types and error taxonomy in rewritten `error.rs`

</decisions>

<specifics>
## Specific Ideas

- Module structure preview from discussion: `src/live_index/` (mod.rs, store.rs, query.rs), `src/discovery/` (mod.rs), `src/parsing/` (kept), `src/domain/` (partial keep)
- Circuit breaker error format: "Circuit breaker tripped: 15/50 files failed parsing (30%, threshold 20%)." + top 3-5 failures with reasons + "Action: fix failing files or raise threshold via TOKENIZOR_CB_THRESHOLD"
- v1 tagged as `v1-final` on main (already exists per PROJECT.md). v2 development on `v2-rewrite` branch.

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `src/parsing/mod.rs` + `src/parsing/languages/`: Full tree-sitter symbol extraction for 6 languages (978 lines). `process_file()` returns `FileProcessingResult` with panic catching. Battle-tested with adversarial input tests.
- `src/domain/index.rs`: `LanguageId` enum with `from_extension()`, `extensions()`, `support_tier()`. `SymbolRecord` and `SymbolKind` types.
- `ignore::WalkBuilder`: Already a dependency. Used in v1's `src/indexing/discovery.rs` for .gitignore-aware file walking.
- `tests/tree_sitter_grammars.rs`: Grammar sanity tests for all 6 languages.

### Established Patterns
- `tracing` + `tracing-subscriber` on stderr with ANSI disabled (from `src/observability.rs`)
- `thiserror` for domain errors, `anyhow` at CLI boundary
- Rust 2024 edition, async via tokio
- `snake_case` functions, `PascalCase` types, `crate::error::Result<T>` for error propagation

### Integration Points
- `src/parsing/mod.rs` currently imports `crate::domain::{FileOutcome, FileProcessingResult, LanguageId, SymbolRecord}` and `crate::storage::digest_hex` -- the storage import will need updating (digest_hex moves or is rewritten)
- `Cargo.toml` has all tree-sitter grammar crates and the `ignore` crate already listed
- npm packaging in `npm/` is untouched

</code_context>

<deferred>
## Deferred Ideas

None -- discussion stayed within phase scope.

</deferred>

---

*Phase: 01-liveindex-foundation*
*Context gathered: 2026-03-10*
