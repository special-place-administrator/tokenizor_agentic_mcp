---
project_name: 'tokenizor_agentic_mcp'
user_name: 'Sir'
date: '2026-03-07'
sections_completed:
  - technology_stack
  - language_rules
  - framework_rules
  - testing_rules
  - code_quality
  - workflow_rules
  - critical_rules
  - epic_4_recovery_architecture
  - agent_selection
status: 'complete'
rule_count: 99
optimized_for_llm: true
---

# Project Context for AI Agents

_This file contains critical rules and patterns that AI agents must follow when implementing code in this project. Focus on unobvious details that agents might otherwise miss. Scoped to Epic 2: Durable Indexing and Run Control + Epic 3: Trusted Code Discovery and Verified Retrieval + Epic 4: Recovery, Repair, and Operational Confidence._

---

## Technology Stack & Versions

| Technology | Version | Critical Agent Notes |
|-----------|---------|---------------------|
| Rust | Edition 2024 | `gen` reserved, `unsafe_op_in_unsafe_fn` enforced, expanded `impl Trait` |
| Tokio | 1.48 | `rt-multi-thread`, `io-std`, `macros` -- async runtime for MCP server |
| rmcp | 1.1.0 | `transport-io` only; use `#[tool]`/`#[tool_router]`/`#[tool_handler]` macros |
| serde + serde_json | 1.0 | `derive` feature; all domain types derive Serialize/Deserialize |
| schemars | 1.1 | MCP tool parameters must derive `JsonSchema` |
| thiserror | 2.0 | Domain errors only -- never in main.rs |
| anyhow | 1.0 | CLI boundary only (main.rs) -- never in library code |
| tracing | 0.1 | Structured logging instrumentation; never println! in library code |
| tracing-subscriber | 0.3 | Log output formatting with `env-filter` feature |
| SpacetimeDB | Local @ 127.0.0.1:3007 | Authoritative control-plane state for mutable operational metadata |
| Local CAS | SHA-256 content-addressed | Raw bytes only -- no metadata |

---

## Architecture Decisions

**ADR-1: Rust Edition 2024** -- Most LLM training data covers edition 2021. Never use `gen` as an identifier. Always qualify unsafe operations inside unsafe functions. Do not assume 2021-era closure capture behavior.

**ADR-2: Dual-storage split (target state)** -- Control-plane metadata belongs in SpacetimeDB, raw bytes in local CAS. Never cross this boundary. Reference blobs from the control plane by `blob_id` (SHA-256 hex), never by embedding bytes.

**ADR-3: Error boundary** -- `TokenizorError` (thiserror 2.0) inside all library code. `anyhow::Result` only in `main.rs`. Add new variants to `TokenizorError` when needed.

**ADR-4: rmcp macro-driven MCP tools** -- All MCP tools are methods on `TokenizorServer` annotated with `#[tool(description = "...")]`. Parameter types must derive `schemars::JsonSchema`. Tool errors map through `to_mcp_error()`, not raw panics or anyhow.

**ADR-5: Trait-first storage abstraction** -- `BlobStore` and `ControlPlane` are traits held as `Arc<dyn T>`. When adding new storage capabilities: add to trait first, implement on all backends, test with fakes.

**ADR-6: No mock crates** -- Hand-written fakes implementing the relevant trait with `AtomicUsize` call counters. Unreachable methods use `unreachable!("reason")`.

**ADR-7: Bootstrap registry narrowed; mutable run state moves to SpacetimeDB** -- The local bootstrap registry remains an interim persistence path only for bootstrap/project/workspace compatibility concerns. Starting with the Epic 4 control-plane correction, mutable operational state must move to SpacetimeDB-backed control-plane writes. Do not add new run/checkpoint/file-record/idempotency durability features to `RegistryPersistence` except temporary migration support.

---

## Persistence Architecture Correction

- **Mutable operational durability must go through the control-plane boundary.** Runs, checkpoints, per-run durable file metadata, idempotency records, typed recovery metadata, and operational history should no longer be extended on the local registry JSON path.
- **`RunManager` should depend on a run-persistence abstraction, not directly on `RegistryPersistence`.** The concrete backend for the corrected path is SpacetimeDB via the control plane.
- **`RegistryPersistence` remains temporary bootstrap and compatibility code.** It may still read or bridge older local state during migration, but it is no longer the intended long-term write path for mutable run durability.
- **Discovery for resumable runs must be frozen per run.** Persist a discovery manifest and make checkpoint resume operate against that manifest rather than rediscovering the current filesystem and inferring the skip boundary from current paths.
- **Raw bytes remain in local CAS.** Do not move byte-exact source content or large byte-sensitive artifacts into SpacetimeDB.
- **Compatibility rules remain explicit.** Older registry-backed state must deserialize safely and any mixed-state migration behavior must fail clearly rather than mutate ambiguously.

---

## Epic 2 Type Design

- **`LanguageId` is an enum, not a config.** Each variant knows its extensions, tree-sitter language function, and support tier via methods (`from_extension`, `extensions`, `support_tier`). Adding a language = adding a variant. The compiler enforces exhaustive handling across discovery, parsing, and persistence. All components query this single source -- never hardcode extensions in multiple places.
- **`FileProcessingResult` carries an explicit `FileOutcome` enum.** Variants: `Processed`, `PartialParse { warning }`, `Failed { error }`. Symbols are empty for `Failed`, possibly incomplete for `PartialParse`. Every consumer must handle all three variants.
- **`FileProcessingResult` contains extracted symbols and file metadata, not blob storage outcomes.** The orchestrator stores bytes in CAS separately, then combines the `blob_id` with the processing result to produce the final `FileRecord`.
- **Every `FileRecord` must include `blob_id` and `byte_len`.** These are the verification anchors for Epic 3 trusted retrieval. Building file records without them creates a data migration obligation.
- **Symbol storage shape:** Flat `Vec<SymbolRecord>` with `depth: u32` + `sort_order: u32` fields. Enables both flat search and hierarchical outline reconstruction.
- **`TokenizorError::is_systemic() -> bool` classifies errors.** Default `true` (abort-safe). Known file-local errors return `false`. Unknown error = assume systemic = abort.
- **All lifecycle states are exhaustive enums.** Check `src/domain/` for existing enums before creating new ones. Never use raw strings for state.
- **All timestamps use `u64` millis via `unix_timestamp_ms()`.** No chrono, no f64, no direct SystemTime.

---

## Indexing Pipeline Architecture

### Build Order

- **Single-file processing is a pure function.** `fn process_file(path, bytes, language) -> Result<FileProcessingResult>`. No I/O, no state, no concurrency. Build and test this FIRST before any orchestration. If the agent builds the orchestrator first, they'll be debugging parsing logic inside concurrent async code.

### File Discovery

- **Use the `ignore` crate** (ripgrep's walker), not raw `walkdir` or `git ls-files`. Handles nested `.gitignore` files (subdirectory-level ignores) automatically. Filter by language extension after discovery.
- **Deterministic file processing order.** After discovery, sort by normalized relative path (forward slashes, lowercase on case-insensitive FS like Windows). Same files = same order = reproducible resume.
- **Content-addressed storage is self-healing for mid-edit races.** No file-locking or snapshot-at-discovery needed. Do NOT add complexity here.

### File Hashing & Storage

- **Use existing `digest_hex` utility.** Read whole file, hash, store. Don't over-engineer streaming or mmap for source files.
- **CAS bytes are `Vec<u8>`, not `String`.** Encoding handling is the parsing layer's job.

### Concurrency

- **Bounded `tokio::spawn` with `tokio::sync::Semaphore`.** Default cap ~8 or `num_cpus`. No `rayon` (blocks async runtime).
- **Each task is self-contained for error handling.** A single file failing records a degraded file status, releases the permit, and continues. One file failure never poisons the entire run.
- **Never hold `std::sync::Mutex` across `.await`.** `InMemoryControlPlane` uses `std::sync::Mutex<InMemoryState>`. Acquire, extract, drop guard, then await.

### Error Handling

- **Two failure domains require opposite responses.** File-local errors (parse, encoding): isolate and continue. Systemic errors (disk, registry, CAS root): abort immediately. A single `TokenizorError::Io` on the CAS root should abort immediately -- don't wait for 4 more files to confirm.
- **Consecutive-failure circuit breaker (fallback).** If N consecutive file tasks fail (default N=5), abort with explicit `Aborted` status. Counter resets on any successful file. Don't checkpoint past files that failed CAS writes.

### Checkpointing & Resume

- **Checkpoint cursor = last successfully committed file path (sorted).** Resume skips files at or before cursor.
- **Correctness invariant:** checkpoint writes happen AFTER durable file commit, never before. If the checkpoint is written first and the commit fails, resume skips a file that was never processed.
- **Checkpoint frequency is proportional to accumulated work.** Default every ~100-500 files, configurable via run config. Checkpoint I/O must be <1% of the processing time it covers.

### Idempotency

- **Idempotency key = operation + target identity** (e.g., `"index::{repo_id}::{workspace_id}"`). Request hash covers all effective inputs. Same key + different hash = conflicting replay = reject.

### Startup Recovery

- **Startup sweep for stale runs.** On startup, transition any `Running` runs to `Interrupted` if no active process owns them. Implement as part of Story 2.1, not deferred.

---

## Tree-sitter Rules

- **Nodes are borrowed, not owned.** Extract into owned domain types (`SymbolRecord`, `FileRecord`) immediately during the parse walk. Never store `Node` or `Tree` beyond the parse function scope.
- **Pin core + grammar crate versions together.** Version matrix mismatches cause build errors or runtime ABI crashes.
- **Treat partial parses as valid** with degraded file status. Don't fail the entire run.
- **Parsing runs inside a panic isolation boundary.** Prefer `std::panic::catch_unwind` -- tree-sitter parsing is CPU-bound sync work and `catch_unwind` communicates isolation intent explicitly. Record panicked files as `Failed`, continue the run.
- **Native/C dependencies require Windows build AND runtime verification.** A successful compile does not mean the grammar .so/.dll loaded correctly. Include an integration test that parses a small sample file (e.g., 5-line Rust file) through tree-sitter to catch load failures.

---

## MCP Server & Run Management

- **`RunManager` owns background indexing lifecycle.** Held as `Arc<RunManager>` by `ApplicationContext`. Stores `HashMap<RepoId, ActiveRun>` where `ActiveRun` = join handle + cancellation token + progress arc. Enforces one-active-run-per-project.
- **`RunManager` must be `Arc`-wrapped** -- it holds `JoinHandle` (not `Clone`) and must be shared, not duplicated. Adding it directly to `ApplicationContext` without `Arc` breaks `#[derive(Clone)]`.
- **`RunManager` is the deliberate exception to the service pattern.** It is long-lived and stateful. Do not model it as a short-lived service created and dropped per request.
- **MCP indexing tools are non-blocking launchers.** `index_folder` spawns a background task, returns `run_id` immediately. Progress via `get_index_run`. Cancellation via `CancellationToken`. Tool handlers never `.await` the full pipeline.
- **Active run progress lives in-memory, durable checkpoints live on disk.** Progress queries read from a shared concurrent structure. Checkpoint writes flush to the registry file. Two separate read paths.
- **`ControlPlane` trait methods are synchronous.** Async boundary lives in the application layer.
- **Expand `to_mcp_error()` for each new `TokenizorError` variant.** `NotFound` -> invalid_params. `Integrity` -> explicit suspect-data error. Every variant gets an explicit mapping decision.
- **Two separate macro blocks -- don't confuse them:**
  - `#[tool_router] impl TokenizorServer` -- where tools are defined. Add new tools here.
  - `#[tool_handler] impl ServerHandler for TokenizorServer` -- connects to the rmcp runtime. Do NOT add tools here.

---

## Epic 3 Retrieval Architecture

_Epic 3 is read-heavy. The blind spot profile shifts from write-correctness (Epic 2) to trust-boundary enforcement and response disambiguation. These rules are mandatory for all retrieval and search code._

1. **Never return blob content without verifying blob_id matches.** Every retrieval path must re-verify that the blob_id stored in the `FileRecord` matches the content-addressed hash of the bytes returned from CAS. A mismatch means the data is suspect — return an integrity error, never stale bytes.
2. **Every retrieval function must check repository status before returning trusted content.** If `RepositoryStatus` is `Invalidated`, `Failed`, or `Degraded`, retrieval must refuse the request with an explicit status-based rejection. Never silently return data from an unhealthy repository.
3. **Search results must include provenance metadata.** Every result must carry `run_id` and `committed_at_unix_ms` from the originating `FileRecord` so consumers can observe staleness. Omitting provenance violates the trust contract.
4. **"No results" responses must disambiguate three states.** Empty (searched, nothing matched) vs missing (target not indexed) vs stale (repository invalidated or unhealthy). A generic empty response that collapses these three cases violates Epic 3 acceptance criteria.
5. **Retrieval paths must reject requests against invalidated or unhealthy repositories.** This is the read-side counterpart of Story 2.10's write-side invalidation. The gate check happens early — before any CAS reads or search queries execute.
6. **Quarantined file records must never appear in search results.** Files with `PersistedFileOutcome::Quarantined` are excluded from all retrieval and search paths. They exist in the registry for audit/repair purposes only.

---

## Epic 4 Recovery Architecture

_Epic 4 is stateful and recovery-heavy. The main failure modes shift from read-side trust enforcement to durable state mutation, observable repair effects, and action-required classification._

1. **Every repair path must be tested from both sides: the state mutation AND the retrieval behavior change.** A recovery test that only checks the write-side transition is incomplete.
2. **Repairs that report success must provably change the repository/run state observable by the next health check or retrieval request.** "Success" without a follow-on observable state change is a bug.
3. **Operational history writes must be durable before reporting the action as completed.** Do not acknowledge recovery, repair, checkpoint, or health transitions before the audit trail is safely written.
4. **Recovery paths must classify stale, interrupted, suspect, quarantined, degraded, and invalid states explicitly.** Do not collapse action-required states into generic failures or generic unhealthy status.
5. **Next-action guidance must stay consistent across recovery and retrieval surfaces.** Reuse the shared action vocabulary (`resume`, `repair`, `reindex`, `migrate`, `wait`, `resolve_context`) instead of inventing one-off strings.

**Recovery-specific blind spot analysis**

- **Agent integrity:** a recovery story is not complete because tasks are checked off. Code, tests, and cited evidence must all exist.
- **Architectural judgment:** trust and recovery decisions must trace back to the architecture docs or this file. If the trace is missing, the decision is not approved.
- **Spec completeness:** every acceptance criterion must be mapped before implementation starts. Recovery work fails badly when "obvious" edge states are left implicit.
- **Two-sided verification:** the mutation path and the user-visible read path must both change as intended. A fixed write path with unchanged health/retrieval behavior is still broken.

---

## Rust Language Rules

**Error Handling:**
- `crate::error::{Result, TokenizorError}` everywhere in library code. `anyhow` only in `main.rs`.
- New error scenarios = new `TokenizorError` variants. Follow existing `From<serde_json::Error>` pattern.

**Module Organization:**
- This codebase uses `mod.rs` style exclusively. Do not introduce `module_name.rs` + `module_name/` directory style.
- Each `mod.rs` re-exports public types. `lib.rs` re-exports the top-level public API.

**Import Style:**
- Group: `std` first, then external crates, then `crate::` locals.
- Inside `#[cfg(test)]` blocks: `super::` for the module under test, `crate::` for cross-module imports.
- `main.rs` imports via the crate name `tokenizor_agentic_mcp::`, not `crate::`.

**Ownership Patterns:**
- `Arc<dyn Trait>` for shared runtime-polymorphic services. `.clone()` on `Arc` is cheap.
- `&self` for reads, `&self` + interior mutability (`Mutex`) for writes. No `&mut self` on shared services.

**Struct & Enum Conventions:**
- All domain types: `#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]`
- Enums: `#[serde(rename_all = "snake_case")]`
- Constructors: associated functions (`::new()`, `::ok()`, `::error()`), not `Default` when parameters are required.

**Documentation:**
- No `///` doc comments is intentional. Descriptive naming and type signatures are the primary documentation strategy. Only add doc comments where the name genuinely cannot convey the semantics.

**Service Pattern:**
- Services are short-lived and stateless -- constructed, used, dropped. `RunManager` is the deliberate exception (long-lived, stateful, `Arc`-wrapped).

---

## Testing Rules

**Conventions:**
- Test naming: `test_verb_condition` (e.g., `test_process_file_handles_partial_parse`).
- Fakes live inside `#[cfg(test)] mod tests` of the module that owns the trait.
- Assertions: plain `assert!`, `assert_eq!`. No assertion crates.
- `#[test]` by default. `#[tokio::test]` only for `async fn` tests.
- Unit tests: `#[cfg(test)]` blocks. Integration tests: `tests/` at crate root (e.g., tree-sitter grammar verification).
- Verify interaction counts via `AtomicUsize` counters on fakes.

**Epic 2 Testing Priorities:**
- `process_file`: unit tests with sample source bytes for each `LanguageId`. Test all three `FileOutcome` variants.
- `RegistryPersistence`: round-trip and backward-compatibility tests.
- `RunManager`: lifecycle tests, one-active-run enforcement.
- Tree-sitter: integration tests parsing a sample file per language.
- Circuit breaker: consecutive failures abort, counter resets on success.
- Checkpoint resume: skips files at or before cursor in sorted order.

**Logging Levels:**
- `error!` systemic failures/aborts. `warn!` degraded files. `info!` run lifecycle events. `debug!` per-file outcomes. `trace!` parser internals. Never `info!`-per-file.

---

## Development Workflow

- `cargo test` -- baseline: 59 tests (56 library + 3 binary). Drops below = regression.
- `cargo run -- doctor` -- verify deployment readiness after control-plane changes.
- After modifying source files, re-index with jcodemunch: `index_folder` with `incremental: true`.
- `TOKENIZOR_CONTROL_PLANE_BACKEND=in_memory` for local dev without SpacetimeDB.
- Windows/MSYS2 development environment.

---

## Agent Selection

- **Claude Opus 4.6** is the default primary implementer for Epic 4 story execution.
- **GPT-5 Codex** is eligible for review, spikes, and narrowly bounded fixes. It is not eligible for unsupervised end-to-end ownership of trust-critical Epic 4 implementation until two consecutive clean stories land with zero integrity or process findings.
- If Epic 4 implementation uses a different primary model, record the exception and rationale in the story file before development starts.

---

## Critical Don't-Miss Rules

**Anti-Patterns (ranked by likelihood of agent violation):**
1. Using `anyhow` inside library code (muscle memory from other Rust projects)
2. Making `ControlPlane` trait methods async (indexing "feels" async)
3. Trying to wire SpacetimeDB writes during Epic 2 (the trait methods exist but are stubs)
4. Storing tree-sitter `Node`/`Tree` objects beyond the parse scope (fighting the borrow checker)
5. Making MCP tool handlers block on the full indexing pipeline (instinct vs architecture)
6. Adding `RunManager` directly to `ApplicationContext` without `Arc` wrapping (`JoinHandle` isn't `Clone`)
7. Logging per-file at `info!` level (floods on large repos)
8. Writing checkpoints before durable file commits (breaks resume correctness)
9. Adding language file extensions in multiple places instead of the central `LanguageId` enum (shotgun surgery)

**Edge Cases:**
- Windows case-insensitivity in file path sorting -- normalize to lowercase
- Non-UTF-8 source files in CAS -- bytes are `Vec<u8>`, not `String`
- Old registry files missing new fields -- backward-compatible with `#[serde(default)]`
- Tree-sitter grammar panics vs errors -- `catch_unwind` for isolation

---

## Usage Guidelines

**For AI Agents:**
- Read this file before implementing any Epic 2 code
- Follow ALL rules exactly as documented
- When in doubt, prefer the more restrictive option
- Build `process_file` first, then layer orchestration on top

**For Humans:**
- Keep this file lean and focused on agent needs
- Update when technology stack or architectural decisions change
- Remove rules that become obvious over time

Last Updated: 2026-03-08
