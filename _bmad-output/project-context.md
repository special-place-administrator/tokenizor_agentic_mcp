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
status: 'complete'
rule_count: 87
optimized_for_llm: true
---

# Project Context for AI Agents

_This file contains critical rules and patterns that AI agents must follow when implementing code in this project. Focus on unobvious details that agents might otherwise miss. Scoped to Epic 2: Durable Indexing and Run Control._

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
| SpacetimeDB | Local @ 127.0.0.1:3007 | Target-state structured operational state -- not wired for writes yet |
| Local CAS | SHA-256 content-addressed | Raw bytes only -- no metadata |

---

## Architecture Decisions

**ADR-1: Rust Edition 2024** -- Most LLM training data covers edition 2021. Never use `gen` as an identifier. Always qualify unsafe operations inside unsafe functions. Do not assume 2021-era closure capture behavior.

**ADR-2: Dual-storage split (target state)** -- Control-plane metadata belongs in SpacetimeDB, raw bytes in local CAS. Never cross this boundary. Reference blobs from the control plane by `blob_id` (SHA-256 hex), never by embedding bytes.

**ADR-3: Error boundary** -- `TokenizorError` (thiserror 2.0) inside all library code. `anyhow::Result` only in `main.rs`. Add new variants to `TokenizorError` when needed.

**ADR-4: rmcp macro-driven MCP tools** -- All MCP tools are methods on `TokenizorServer` annotated with `#[tool(description = "...")]`. Parameter types must derive `schemars::JsonSchema`. Tool errors map through `to_mcp_error()`, not raw panics or anyhow.

**ADR-5: Trait-first storage abstraction** -- `BlobStore` and `ControlPlane` are traits held as `Arc<dyn T>`. When adding new storage capabilities: add to trait first, implement on all backends, test with fakes.

**ADR-6: No mock crates** -- Hand-written fakes implementing the relevant trait with `AtomicUsize` call counters. Unreachable methods use `unreachable!("reason")`.

**ADR-7: Local bootstrap registry as interim control-plane persistence** -- Until SpacetimeDB write-path is wired, all structured state (projects, workspaces, runs, checkpoints) persists via the local bootstrap registry JSON with atomic file writes and advisory locking. Do not attempt SpacetimeDB writes. All `SpacetimeControlPlane` write methods return `pending_write_error()`. Use `InMemoryControlPlane` for tests and local bootstrap registry for durable state.

---

## Epic 2 Persistence Architecture

- **Epic 2 durable persistence does NOT go through `ControlPlane` trait write methods.** Those methods remain stubs (`pending_write_error()` on SpacetimeDB, in-memory for tests).
- **Epic 2 uses a dedicated `RegistryPersistence` service** that reads/writes the local bootstrap registry JSON file directly. This service handles runs, checkpoints, idempotency records, and file/symbol metadata alongside the existing project/workspace data.
- **`RegistryPersistence` is a struct, not a trait.** Constructor takes `PathBuf` for registry file location. Tests use a temp directory. No trait abstraction for interim code.
- **This is a temporary bridge.** When SpacetimeDB writes are wired (future epic), the persistence path migrates to `ControlPlane` trait methods and the registry persistence service is retired.
- **`InMemoryControlPlane` is for tests only.** "Durable" means survives process exit -- verify via the registry file, not in-memory state.
- **Registry writes use write-to-temp-then-rename.** Never write directly to the registry file. Follow Epic 1's existing pattern.
- **Registry file access uses advisory file locking.** `fs2` crate or platform flock. Lock scope = the read-modify-write cycle only, NOT the entire run duration.
- **Read-before-write integrity check.** Before checkpoint writes, verify the registry file still contains the expected project/workspace identity. Fail explicitly if missing or mismatched.
- **New fields on persisted types must be backward-compatible.** Use `Option<T>` with `#[serde(default)]`. Test deserialization against the existing registry format from Epic 1.
- **Run state persistence:** Single registry file with atomic rewrite, same as Epic 1. Revisit if profiling shows registry file >~1MB or atomic rewrites taking noticeable time.

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

Last Updated: 2026-03-07
