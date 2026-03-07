# Story 2.1: Start an Indexed Run with Durable Run Identity

Status: ready-for-dev

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a power user,
I want to start an indexing run for a repository or workspace and receive a durable run ID,
so that I can track and manage indexing work over time.

## Acceptance Criteria

1. Given a registered repository or workspace is ready for indexing
   When I start an indexing operation
   Then Tokenizor creates a durable run record, assigns an explicit initial lifecycle state, and returns its run ID
   And the run is associated with the correct project and workspace context plus durable ownership or lease semantics where applicable

2. Given the indexing request is retried with the same idempotent inputs
   When Tokenizor processes the request
   Then it returns the stored result for the same effective request
   And it does not create a duplicate run

3. Given an indexing run is already active for the same repository
   When a new indexing request arrives for that repository
   Then Tokenizor rejects the request with an explicit conflict error
   And it does not silently queue or replace the active run

4. Given the process starts and a previous run was left in `Running` status without an active owner
   When the startup sweep executes
   Then Tokenizor transitions the stale run to `Interrupted` status
   And it does not leave orphaned `Running` records that block new work

5. Given a run is successfully created
   When the run record is persisted
   Then it survives process exit (durable via the local bootstrap registry)
   And it can be read back with all fields intact after restart

## Tasks / Subtasks

- [ ] Task 1: Add `Interrupted` variant to `IndexRunStatus` and add `is_systemic()` to `TokenizorError` (AC: 4, 5)
  - [ ] Add `Interrupted` to `IndexRunStatus` enum in `src/domain/index.rs`
  - [ ] Add `is_systemic() -> bool` method to `TokenizorError` in `src/error.rs`
  - [ ] Verify backward-compatible deserialization of existing `IndexRunStatus` values

- [ ] Task 2: Implement `RegistryPersistence` for durable run and idempotency state (AC: 1, 2, 5)
  - [ ] Create `src/storage/registry_persistence.rs` with `RegistryPersistence` struct
  - [ ] Constructor takes `PathBuf` for registry file location
  - [ ] Implement read-modify-write cycle with advisory file locking (`fs2` crate)
  - [ ] Implement atomic writes via write-to-temp-then-rename (reuse Epic 1 pattern)
  - [ ] Add read-before-write integrity check (verify project/workspace identity before writing)
  - [ ] Support persisting `IndexRun` records alongside existing project/workspace data
  - [ ] Support persisting `IdempotencyRecord` entries alongside run data
  - [ ] Ensure new fields are backward-compatible with Epic 1 registry format (`Option<T>` with `#[serde(default)]`)
  - [ ] Create an Epic 1 registry JSON fixture file (projects + workspaces, no runs/checkpoints/idempotency) for backward-compatibility deserialization tests
  - [ ] Add round-trip serialization tests and backward-compatibility deserialization tests against the Epic 1 fixture

- [ ] Task 3: Implement `RunManager` for run lifecycle orchestration (AC: 1, 3, 4)
  - [ ] Create `src/application/run_manager.rs` with `RunManager` struct
  - [ ] Add `tokio-util` (v0.7.x, `rt` feature) to `Cargo.toml` for `CancellationToken` (new dependency, not currently in Cargo.toml)
  - [ ] Add `fs2` (v0.4.x) to `Cargo.toml` for advisory file locking (new dependency, not currently in Cargo.toml)
  - [ ] `RunManager` stores `HashMap<String, ActiveRun>` where `ActiveRun` = `JoinHandle` + `CancellationToken` + progress arc
  - [ ] Wrap as `Arc<RunManager>` since `JoinHandle` is not `Clone`
  - [ ] Implement `start_run()` that creates an `IndexRun` with `Queued` status, persists via `RegistryPersistence`, and returns `run_id`
  - [ ] Enforce one-active-run-per-repository (reject new run if existing active run for same repo)
  - [ ] Implement startup sweep: scan persisted runs, transition any `Running` to `Interrupted`
  - [ ] Generate `run_id` deterministically using `digest_hex` over `(repo_id, mode, requested_at_unix_ms)` or equivalent unique composite

- [ ] Task 4: Implement idempotency checking for run creation (AC: 2)
  - [ ] Idempotency key = `"index::{repo_id}::{workspace_id}"` (operation + target identity)
  - [ ] Request hash covers all effective inputs (mode, repo_id, workspace context)
  - [ ] Same key + same hash = return stored run result (no duplicate)
  - [ ] Same key + different hash = reject as conflicting replay with explicit error
  - [ ] Persist idempotency records via `RegistryPersistence`

- [ ] Task 5: Wire `RunManager` into `ApplicationContext` and expose via MCP/CLI (AC: 1, 3)
  - [ ] Add `run_manager: Arc<RunManager>` to `ApplicationContext`
  - [ ] Add `start_indexing` method to `ApplicationContext` that delegates to `RunManager`
  - [ ] Add `index_folder` MCP tool as a non-blocking launcher (spawns background task, returns `run_id` immediately)
  - [ ] Run startup sweep during `ApplicationContext` initialization (before accepting new work)

- [ ] Task 6: Add comprehensive tests (AC: 1, 2, 3, 4, 5)
  - [ ] Test run creation returns valid `IndexRun` with `Queued` status and all required fields
  - [ ] Test one-active-run enforcement rejects concurrent run for same repo
  - [ ] Test idempotent replay with same inputs returns stored result
  - [ ] Test conflicting replay with different inputs is rejected
  - [ ] Test startup sweep transitions `Running` to `Interrupted`
  - [ ] Test `RegistryPersistence` round-trip: write run, restart, read back intact
  - [ ] Test backward-compatible deserialization of Epic 1 registry (no runs) with Epic 2 registry (with runs)
  - [ ] Test advisory file locking prevents concurrent registry corruption

## Dev Notes

> **CRITICAL: Before implementing, load `_bmad-output/project-context.md` in full.** It contains 87 agent rules scoped to Epic 2 covering persistence architecture, type design, concurrency, error handling, testing, and anti-patterns. Every rule applies to this story. Do not start coding without reading it.

### Story Requirements

- This is the foundation story for Epic 2. Every subsequent indexing story depends on durable run identity and the `RegistryPersistence` bridge established here.
- The `RunManager` created here will be extended in later stories (2.5 for status, 2.6 for progress, 2.7 for cancellation, 2.8 for checkpointing).
- Story 2.1 scope is run creation and identity only. Actual file indexing execution is Story 2.2. Do not implement the indexing pipeline here.
- The startup sweep for stale runs is explicitly part of this story (per project-context.md), not deferred.

### Current Implementation Baseline

- `src/domain/index.rs` already has `IndexRun`, `IndexRunStatus`, `IndexRunMode`, and `Checkpoint` scaffolded with correct derive macros.
- `src/domain/idempotency.rs` already has `IdempotencyRecord` and `IdempotencyStatus` scaffolded.
- `src/indexing/mod.rs` has an empty `IndexingScaffold` placeholder.
- `src/storage/control_plane.rs` has `ControlPlane` trait with `create_index_run`, `write_checkpoint`, `put_idempotency_record` methods. `InMemoryControlPlane` implements these for tests. `SpacetimeControlPlane` returns `pending_write_error()` for all write methods.
- `src/application/mod.rs` has `ApplicationContext` with `config`, `blob_store`, `control_plane` fields.
- `src/main.rs` exposes: `run`, `doctor`, `init`, `attach`, `migrate`, `inspect`, `resolve`, `guard_and_serve`.
- Current test baseline: 59 tests (56 library + 3 binary). Do not drop below this count.
- `digest_hex` in `src/storage/sha256.rs` for deterministic ID generation.
- `unix_timestamp_ms()` in `src/domain/health.rs` for timestamps.

### Developer Context

- **Epic 2 does NOT use `ControlPlane` trait write methods for durable persistence.** Those remain stubs. Use the new `RegistryPersistence` struct for all durable state writes (runs, checkpoints, idempotency records).
- **`RegistryPersistence` is a struct, not a trait.** No trait abstraction for interim code. Constructor takes `PathBuf`. Tests use temp directories.
- **Registry writes use write-to-temp-then-rename.** Follow the existing Epic 1 pattern in `application/init.rs`. Never write directly to the registry file.
- **Advisory file locking with `fs2` crate.** Lock scope = the read-modify-write cycle only, NOT the entire run duration.
- **Read-before-write integrity check.** Before writes, verify the registry file still contains the expected project/workspace identity. Fail explicitly if missing or mismatched.
- **New fields on persisted types must be backward-compatible.** Use `Option<T>` with `#[serde(default)]`. Test deserialization against the existing Epic 1 registry format (which has no run/checkpoint data).
- **`InMemoryControlPlane` is for tests only.** "Durable" means survives process exit -- verify via the registry file, not in-memory state.

### Project Structure Notes

- `RegistryPersistence` belongs in `src/storage/registry_persistence.rs` (storage layer, persistence concern).
- `RunManager` belongs in `src/application/run_manager.rs` (application layer, orchestration concern).
- Do not create `src/domain/persistence.rs` -- persistence logic is a storage concern, not domain.
- This codebase uses `mod.rs` style exclusively. Add new modules via `pub mod` in the parent `mod.rs`.
- Do not introduce `module_name.rs` + `module_name/` directory style.

### Technical Requirements

- **Rust Edition 2024.** `gen` is reserved. `unsafe_op_in_unsafe_fn` enforced. Do not assume 2021-era closure capture behavior.
- **`RunManager` must be `Arc`-wrapped.** It holds `JoinHandle` (not `Clone`) and must be shared. Adding it directly to `ApplicationContext` without `Arc` breaks `#[derive(Clone)]` if applicable.
- **`RunManager` is the deliberate exception to the service pattern.** It is long-lived and stateful. Do not model as a short-lived service.
- **MCP tools are non-blocking launchers.** `index_folder` spawns a background task, returns `run_id` immediately. Tool handlers never `.await` the full pipeline.
- **`ControlPlane` trait methods are synchronous.** Async boundary lives in the application layer.
- **All timestamps use `u64` millis via `unix_timestamp_ms()`.** No chrono, no f64, no direct SystemTime.
- **All lifecycle states are exhaustive enums.** Never use raw strings for state.
- **Error boundary:** `TokenizorError` (thiserror 2.0) inside all library code. `anyhow::Result` only in `main.rs`.
- **Two separate MCP macro blocks -- don't confuse them:**
  - `#[tool_router] impl TokenizorServer` -- where tools are defined. Add `index_folder` here.
  - `#[tool_handler] impl ServerHandler for TokenizorServer` -- connects to rmcp runtime. Do NOT add tools here.
- **Expand `to_mcp_error()` for each new `TokenizorError` variant.** Every variant gets an explicit mapping decision.

### Architecture Compliance

- Preserve the layered flow: `main.rs` -> `application` -> `domain/storage`.
- `domain` defines core entities, value types, invariants, state-machine semantics.
- `application` orchestrates use cases, policies, workflow coordination.
- `storage` implements persistence boundaries.
- `protocol` adapts external surfaces (MCP/CLI).
- `indexing` owns discovery, hashing, pipeline coordination, commit preparation (not yet needed for 2.1).
- No catch-all `utils` module. Shared helpers only when ownership is explicit and bounded.

### Library / Framework Requirements

- **Add `fs2` crate** to `Cargo.toml` for advisory file locking on the registry file. This is the recommended approach from the project-context.md.
- **Add `tokio-util` crate** with `rt` feature for `CancellationToken` (used by `RunManager` for background run ownership).
- Stay on the current Rust dependency set otherwise. Do not add `uuid` -- use `digest_hex` for deterministic ID generation.
- `serde` / `serde_json` for registry persistence serialization.
- `tracing` for structured logging. `info!` for run lifecycle events. Never `info!`-per-file.

### File Structure Requirements

- **Files to create:**
  - `src/storage/registry_persistence.rs` -- `RegistryPersistence` struct for durable run/idempotency state
  - `src/application/run_manager.rs` -- `RunManager` struct for run lifecycle orchestration

- **Files to modify:**
  - `src/domain/index.rs` -- add `Interrupted` to `IndexRunStatus`
  - `src/error.rs` -- add `is_systemic()` method to `TokenizorError`
  - `src/storage/mod.rs` -- add `pub mod registry_persistence`
  - `src/application/mod.rs` -- add `pub mod run_manager`, add `run_manager: Arc<RunManager>` to `ApplicationContext`
  - `src/protocol/mcp.rs` -- add `index_folder` tool
  - `src/main.rs` -- wire startup sweep, possibly add indexing CLI subcommand
  - `Cargo.toml` -- add `fs2` and `tokio-util` dependencies
  - `src/lib.rs` -- ensure new modules are re-exported as needed

- **Files NOT to modify:**
  - `src/storage/control_plane.rs` -- do not wire SpacetimeDB write methods
  - `src/parsing/mod.rs` -- not in scope for this story
  - `src/indexing/mod.rs` -- minimal changes only (Story 2.2 builds the pipeline)

### Testing Requirements

- **Unit tests (co-located in modules):**
  - `RegistryPersistence`: round-trip, backward-compat, locking, integrity check
  - `RunManager`: create run, one-active-run enforcement, startup sweep
  - Idempotency: replay detection, conflict rejection

- **Integration tests (`tests/` at crate root):**
  - End-to-end: create run via `ApplicationContext`, verify persisted to registry file, simulate restart, read back

- **Test naming:** `test_verb_condition` (e.g., `test_start_run_creates_queued_record`)
- **Fakes:** Hand-written fakes implementing traits with `AtomicUsize` call counters. No mock crates.
- **Assertions:** Plain `assert!`, `assert_eq!`. No assertion crates.
- **`#[test]` by default.** `#[tokio::test]` only for `async fn` tests.
- **Tests use temp directories** for `RegistryPersistence`, not production paths.
- **Baseline: 59 tests.** Must not drop below. Target adding ~15-20 new tests.

### Previous Story Intelligence

- **Story 1.7** (last completed) implemented explicit registry migration/update flows with deterministic legacy identity upgrades. Key learnings:
  - Agent model: GPT-5 Codex. Code review by Claude Opus 4.6 fixed 3 MEDIUM + 3 LOW issues.
  - Production `expect()` calls were flagged -- use proper error propagation instead.
  - Large functions (400+ lines) were decomposed into focused functions during review.
  - `unsafe` Windows FFI blocks need `// SAFETY:` documentation.
  - All 59 tests pass after review fixes.
- **Story 1.3** hardened registry durability with atomic writes, locking, and explicit provenance fields. Reuse these patterns for `RegistryPersistence`.
- **Story 1.5** required context resolution to fail explicitly rather than relying on heuristic fallback. Apply the same principle to run creation.
- **Story 1.6** introduced canonical Git common-directory identity for new registrations. `RegistryPersistence` must preserve this identity when extending the registry format.
- **Epic 1 retrospective** unanimously recommended the interim registry persistence approach over coupling SpacetimeDB writes with Epic 2 business logic.

### Git Intelligence Summary

Recent commits:
- `ff7d8f6` chore: add missing validate-workflow.xml BMAD task
- `b47ba86` chore: regenerate sprint-status.yaml with all 5 epics and 39 stories
- `9c70010` docs: add project-context.md and interim persistence ADR
- `b539b76` feat: complete Epic 1 -- Reliable Local Setup and Workspace Identity
- `477399c` Initial commit

Key insights:
- Epic 1 was completed as a single large commit (`b539b76`). The full source tree shipped in that commit.
- Project-context.md was added post-Epic 1 retrospective to capture agent rules.
- Registry format and persistence patterns are established in the `b539b76` commit.

### Latest Technical Information

- **`fs2` crate (v0.4.x):** Provides cross-platform advisory file locking (`lock_exclusive`, `lock_shared`, `unlock`). Works on Windows (LockFileEx/UnlockFileEx) and Unix (flock). No breaking changes in recent versions.
- **`tokio-util` (v0.7.x):** Provides `CancellationToken` in `tokio_util::sync`. Use `CancellationToken::new()`, `.clone()` for sharing, `.cancel()` to signal, `.cancelled()` future to await. The `rt` feature is needed.
- **Rust Edition 2024:** `gen` keyword is reserved. Ensure no variables or functions named `gen`. `unsafe_op_in_unsafe_fn` lint is enforced by default.
- **rmcp 1.1.0:** MCP tool parameters must derive `schemars::JsonSchema`. New tools are methods on the `#[tool_router]` impl block with `#[tool(description = "...")]` attribute.

### Project Context Reference

- Full project context for AI agents at `_bmad-output/project-context.md` (87 rules, Epic 2 scoped).
- Key architectural decisions: ADR-1 through ADR-7 documented in project-context.md.
- Build order guidance: Build `process_file` first (Story 2.2), then layer orchestration. For Story 2.1, build `RegistryPersistence` first, then `RunManager`, then wire into `ApplicationContext`.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 2.1: Start an Indexed Run with Durable Run Identity]
- [Source: _bmad-output/planning-artifacts/epics.md#Epic 2: Durable Indexing and Run Control]
- [Source: _bmad-output/planning-artifacts/prd.md#FR8, FR16]
- [Source: _bmad-output/planning-artifacts/architecture.md#Interim Persistence Decision]
- [Source: _bmad-output/planning-artifacts/architecture.md#Implementation Patterns & Consistency Rules]
- [Source: _bmad-output/planning-artifacts/architecture.md#Project Structure & Boundaries]
- [Source: _bmad-output/project-context.md#Epic 2 Persistence Architecture]
- [Source: _bmad-output/project-context.md#MCP Server & Run Management]
- [Source: _bmad-output/project-context.md#Critical Don't-Miss Rules]
- [Source: _bmad-output/implementation-artifacts/1-7-migrate-or-update-workspace-state-safely.md]
- [Source: src/domain/index.rs -- IndexRun, IndexRunStatus, IndexRunMode, Checkpoint]
- [Source: src/domain/idempotency.rs -- IdempotencyRecord, IdempotencyStatus]
- [Source: src/error.rs -- TokenizorError]
- [Source: src/storage/control_plane.rs -- ControlPlane trait, InMemoryControlPlane]
- [Source: src/application/mod.rs -- ApplicationContext]
- [Source: src/storage/sha256.rs -- digest_hex]
- [Source: src/domain/health.rs -- unix_timestamp_ms]
- [Source: Cargo.toml -- current dependency set]

## Dev Agent Record

### Agent Model Used

{{agent_model_name_version}}

### Debug Log References

### Completion Notes List

### File List
