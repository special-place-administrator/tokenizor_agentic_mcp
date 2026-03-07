# Story 2.3: Persist File-Level Indexing Outputs and Symbol/File Metadata for the Initial Quality-Focus Language Set

Status: review

## Story

As a power user,
I want Tokenizor to persist file-level indexing outputs and symbol/file metadata for the initial quality-focus language set,
so that the first bounded indexing slice produces durable, inspectable indexing state instead of only transient run activity.

## Acceptance Criteria

1. **Given** an indexing run successfully processes eligible Rust, Python, JavaScript/TypeScript, or Go files
   **When** file-level indexing results are committed
   **Then** Tokenizor persists durable file records plus symbol and file metadata for those processed files
   **And** the persisted outputs are linked to the correct repository, workspace, and run context

2. **Given** a processed file has no extractable symbols or produces suspect metadata during persistence
   **When** commit-time validation runs
   **Then** Tokenizor records an explicit file-level outcome such as empty-symbol, failed, or quarantined
   **And** it does not silently claim trusted symbol coverage for that file

3. **Given** the repository contains files outside the initial quality-focus language set
   **When** persistence for this story completes
   **Then** Tokenizor persists usable indexing outputs only for the in-scope initial quality-focus slice
   **And** it does not represent out-of-scope languages as supported persisted outputs for this story

## Tasks / Subtasks

- [x] Task 1: Define `FileRecord` and `PersistedFileOutcome` domain types (AC: #1, #2)
  - [x] 1.1: Define `FileRecord` struct — fields: `relative_path`, `language` (LanguageId), `blob_id` (SHA-256 hex), `byte_len`, `content_hash`, `outcome` (PersistedFileOutcome), `symbols` (Vec\<SymbolRecord\>), `run_id`, `repo_id`, `committed_at_unix_ms`
  - [x] 1.2: Define `PersistedFileOutcome` enum — variants: `Committed`, `EmptySymbols`, `Failed { error: String }`, `Quarantined { reason: String }`
  - [x] 1.3: Add standard derives: `Clone, Debug, Serialize, Deserialize, PartialEq, Eq`
  - [x] 1.4: New fields on `RegistryData` use `Option<T>` with `#[serde(default)]` for backward compatibility with existing registry files
  - [x] 1.5: Unit tests — construction, serde round-trip for all outcome variants, backward-compat deserialization

- [x] Task 2: Verify/extend CAS blob storage for file content (AC: #1)
  - [x] 2.1: Verify `LocalCasBlobStore` has a store method that accepts `&[u8]` and returns `blob_id` (SHA-256 hex) — extend if needed
  - [x] 2.2: Ensure atomic write pattern (temp file then rename) is used
  - [x] 2.3: Ensure deduplication — skip write if blob already exists (content-addressed means same bytes = same blob_id)
  - [x] 2.4: Unit tests — store and verify blob_id matches expected hash, deduplication skips write, empty content handling

- [x] Task 3: Implement commit-time validation as pure function (AC: #2)
  - [x] 3.1: Create `validate_for_commit(result: &FileProcessingResult, blob_id: &str) -> PersistedFileOutcome` in `src/indexing/commit.rs`
  - [x] 3.2: `FileOutcome::Processed` with non-empty symbols maps to `PersistedFileOutcome::Committed`
  - [x] 3.3: `FileOutcome::Processed` with empty symbols maps to `PersistedFileOutcome::EmptySymbols`
  - [x] 3.4: `FileOutcome::Failed { error }` maps to `PersistedFileOutcome::Failed { error }`
  - [x] 3.5: `FileOutcome::PartialParse { warning }` — if symbols present, `Committed`; if no symbols, `Quarantined { reason: warning }`
  - [x] 3.6: Verify `blob_id` matches `result.content_hash` — mismatch produces `Quarantined { reason: "blob_id/content_hash mismatch" }`
  - [x] 3.7: Unit tests — all mapping paths, hash mismatch detection, edge cases (empty symbols with PartialParse)

- [x] Task 4: Implement file record persistence in registry (AC: #1, #2)
  - [x] 4.1: Add `run_file_records: HashMap<String, Vec<FileRecord>>` field to `RegistryData` with `#[serde(default)]` — keyed by `run_id`
  - [x] 4.2: Add `save_file_records(&self, run_id: &str, records: &[FileRecord]) -> Result<()>` to `RegistryPersistence`
  - [x] 4.3: Use existing read-modify-write pattern with `fs2` advisory locking
  - [x] 4.4: Add `get_file_records(&self, run_id: &str) -> Result<Vec<FileRecord>>` for verification/testing
  - [x] 4.5: Unit tests — round-trip persistence, backward-compat deserialization of old registry files missing `run_file_records`, retrieval by run_id

- [x] Task 5: Create `src/indexing/commit.rs` orchestration module (AC: #1, #2, #3)
  - [x] 5.1: Create `src/indexing/commit.rs` with `commit_file_result(result: FileProcessingResult, bytes: &[u8], cas: &LocalCasBlobStore, run_id: &str, repo_id: &str) -> Result<FileRecord>`
  - [x] 5.2: Flow: store bytes in CAS -> get `blob_id` -> call `validate_for_commit` -> construct `FileRecord` with `committed_at_unix_ms` -> return
  - [x] 5.3: Verify language is in-scope (Rust, Python, JavaScript, TypeScript, Go) — return error for out-of-scope languages
  - [x] 5.4: CAS write failure for individual file: return `FileRecord` with `PersistedFileOutcome::Failed` — do NOT propagate as systemic error
  - [x] 5.5: CAS root inaccessible (e.g., permissions on `.tokenizor/blobs/`): propagate as systemic `TokenizorError::Storage` for pipeline abort
  - [x] 5.6: Register module in `src/indexing/mod.rs`
  - [x] 5.7: Unit tests with hand-written fake CAS (AtomicUsize call counter pattern)

- [x] Task 6: Wire persistence into pipeline background task (AC: #1, #3)
  - [x] 6.1: Modify `IndexingPipeline` to accept `Arc<LocalCasBlobStore>` reference
  - [x] 6.2: Modify per-file concurrent task: after `process_file`, call `commit_file_result` with the same bytes — persist each file within its bounded-concurrency slot
  - [x] 6.3: Collect `Vec<FileRecord>` from all committed files
  - [x] 6.4: After pipeline completes: batch-save file records to registry via `RegistryPersistence::save_file_records`
  - [x] 6.5: Modify `RunManager::launch_run` to pass CAS and RegistryPersistence to pipeline
  - [x] 6.6: Update run status finish summary with persisted file count and outcome breakdown
  - [x] 6.7: CAS root systemic failures abort pipeline (existing circuit breaker / systemic error path)

- [x] Task 7: Integration testing (AC: #1, #2, #3)
  - [x] 7.1: End-to-end test: create temp repo with multi-language files -> run pipeline -> verify FileRecords persisted in registry -> verify CAS blobs exist on disk
  - [x] 7.2: Test: file with no extractable symbols -> `EmptySymbols` outcome persisted, NOT silently claimed as trusted
  - [x] 7.3: Test: failed file -> `Failed` outcome persisted with error message
  - [x] 7.4: Test: out-of-scope language files (e.g., `.java`, `.rb`) discovered but NOT persisted as FileRecords
  - [x] 7.5: Test: FileRecords correctly linked to `run_id` and `repo_id`
  - [x] 7.6: Test: existing registry without `run_file_records` field deserializes successfully (backward compat)
  - [x] 7.7: Verify test count does not regress below 143 (Story 2.2 baseline)

## Dev Notes

### CRITICAL: Load project-context.md FIRST

MUST load `_bmad-output/project-context.md` BEFORE starting implementation. It contains 87 agent rules scoped to Epic 2 covering persistence architecture, type design, concurrency, error handling, testing, and anti-patterns. Failure to load this will cause architectural violations.

### Build Order (MANDATORY)

Follow the same build-then-test pattern established in Story 2.2:

1. **Domain types** (Task 1) — `FileRecord`, `PersistedFileOutcome` in `src/domain/index.rs`
2. **Storage verification** (Task 2) — Verify `LocalCasBlobStore` store method exists/works
3. **Pure validation function** (Task 3) — `validate_for_commit()` — test independently before wiring
4. **Registry persistence** (Task 4) — `save_file_records` / `get_file_records` on `RegistryPersistence`
5. **Commit orchestration** (Task 5) — `src/indexing/commit.rs` — combines CAS + validation + record creation
6. **Pipeline wiring** (Task 6) — Connect persistence into background task execution
7. **Integration tests** (Task 7) — End-to-end verification

### Consuming Story 2.2 Output

The pipeline (Story 2.2) produces `Vec<FileProcessingResult>` in memory. Each result contains:

- `relative_path: String` — normalized forward-slash path
- `language: LanguageId` — Rust, Python, JavaScript, TypeScript, or Go
- `outcome: FileOutcome` — `Processed`, `PartialParse { warning }`, or `Failed { error }`
- `symbols: Vec<SymbolRecord>` — flat with `depth` + `sort_order` for hierarchy
- `byte_len: u64` — raw file size
- `content_hash: String` — SHA-256 hex of raw bytes (computed via `digest_hex`)

The raw file bytes are read within each per-file concurrent task for parsing. Reuse those same bytes for CAS storage within the same task — do NOT re-read the file.

### Per-File Persistence During Pipeline (Not Batch-After)

Persist each file's CAS blob within the bounded-concurrency task that processes it. This is:
- **Memory efficient** — bytes released after each file commits
- **Architecturally aligned** — checkpoint cursor (Story 2.8) requires knowing which files were durably committed
- **Correctness invariant** — checkpoint writes (future) happen AFTER durable file commit, never before

Collect `FileRecord` results in memory, then batch-save to registry after pipeline completes.

### Key Architecture Constraints

**Interim Persistence (NOT SpacetimeDB):**
- All structured state persists via local bootstrap registry JSON file
- `RegistryPersistence` handles all durable read/write with `fs2` advisory locking
- Do NOT attempt `SpacetimeControlPlane` writes — all write methods return `pending_write_error()`
- `RegistryPersistence` is interim code that retires when SpacetimeDB writes are wired

**Registry Write Pattern:**
- Write-to-temp-then-rename — NEVER write directly to registry file
- Advisory file locking (`fs2`) — lock scope = read-modify-write cycle only, NOT run duration
- Read-before-write integrity check before writes
- New fields: `Option<T>` with `#[serde(default)]` for backward compatibility

**Dual-Storage Split:**
- Raw file bytes -> `LocalCasBlobStore` (`.tokenizor/blobs/sha256/`)
- Structured metadata (FileRecords, symbols) -> Registry JSON via `RegistryPersistence`
- Reference blobs by `blob_id` (SHA-256 hex), never embed raw bytes in registry
- `FileRecord` MUST include `blob_id` and `byte_len` — Epic 3 trusted retrieval verification anchors

**Error Classification for CAS Writes:**
- Individual file CAS write failure (e.g., single file too large, transient I/O): file-local error, record as `Failed`, continue pipeline — goes through consecutive-failure counter
- CAS root inaccessible (`.tokenizor/blobs/` directory missing, permissions): systemic error, abort pipeline immediately — do NOT wait for 4 more files to confirm
- Distinguish by testing CAS root accessibility at pipeline start (probe write) vs individual file write errors during execution

**Domain Type Rules:**
- All types: `#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]`
- Lifecycle states: exhaustive enums, NEVER raw strings
- Time: `*_unix_ms` fields via `unix_timestamp_ms()` utility — no `chrono`, no `f64`
- Hashes: lowercase hex strings
- Check `src/domain/` for existing enums before creating new ones

**Concurrency:**
- NEVER hold `std::sync::Mutex` across `.await`
- CAS writes within bounded-concurrency tasks are safe (content-addressed = no write conflicts)
- Acquire lock, extract, drop guard, then await

### What This Story Does NOT Implement

- Checkpointing / resume (Story 2.8)
- Cancellation response (Story 2.7)
- Live progress exposure (Story 2.6)
- Search or retrieval of persisted data (Epic 3)
- SpacetimeDB write integration (future epic)
- Repair/recovery of persisted state (Epic 4)

### Previous Story Review Patterns (Apply Defensively)

Story 2.2 code review found 8 issues (2 High, 4 Medium, 2 Low). Common patterns to prevent:

| Pattern | Risk | Prevention |
|---------|------|------------|
| **H1-equivalent** | Over-aggressive systemic classification — single file CAS write failure aborting pipeline | CAS write failures for individual files go through consecutive-failure counter. Reserve systemic abort for CAS root accessibility. |
| **H2-equivalent** | Test asserting wrong status — false confidence | Verify tests assert the CORRECT expected outcome for each scenario. Circuit breaker tests must assert `Aborted`. |
| **M1-equivalent** | Missing timestamp initialization | Ensure `committed_at_unix_ms` is set on every `FileRecord` via `unix_timestamp_ms()`. |
| **M3-equivalent** | Circuit breaker not breaking outer loop | Verify existing circuit breaker early-exit still works with persistence added to the task body. |
| **M4-equivalent** | Missing edge case tests | Test adversarial inputs: empty files, binary files, files with no symbols, files with suspect metadata. |
| **Backward compat** | New registry fields breaking old files | All new fields on `RegistryData` must be `Option<T>` with `#[serde(default)]`. Write a test deserializing a registry JSON without the new fields. |

### Testing Standards

- Naming: `test_verb_condition` (e.g., `test_commit_file_result_stores_blob_and_creates_record`)
- Assertions: plain `assert!`, `assert_eq!` — NO assertion crates
- `#[test]` by default; `#[tokio::test]` only for async
- Fakes: hand-written implementing trait with `AtomicUsize` call counters — NO mock crates
- Unreachable methods: `unreachable!("reason")`
- Temp directories for all file operations (CAS, registry)
- Current baseline: 143 tests — must not regress
- Logging: `debug!` for per-file outcomes, `info!` for run-level events — NEVER `info!` per-file

### Existing Code Locations

| Component | Path |
|-----------|------|
| Domain types (extend) | `src/domain/index.rs` |
| Domain re-exports | `src/domain/mod.rs` |
| Local CAS blob store | `src/storage/local_cas.rs` |
| Registry persistence (extend) | `src/storage/registry_persistence.rs` |
| Indexing pipeline (wire) | `src/indexing/pipeline.rs` |
| Indexing module (register) | `src/indexing/mod.rs` |
| Run manager (pass deps) | `src/application/run_manager.rs` |
| Application context | `src/application/mod.rs` |
| MCP tools | `src/protocol/mcp.rs` |
| Error types | `src/error.rs` |
| Integration tests (extend) | `tests/indexing_integration.rs` |
| Time utility | `src/domain/health.rs` — `unix_timestamp_ms()` |

### Tree-sitter / Parsing Notes (Inherited)

- `tree-sitter` 0.24 — `Node::is_null()` removed, use `node.kind().is_empty()`
- `tree-sitter-typescript` exposes `LANGUAGE_TYPESCRIPT` not `LANGUAGE`
- `SymbolKind` has `Copy` derive
- `ignore` crate requires `.git/` directory to respect `.gitignore` in tests

### Project Structure Notes

Files to create:
- `src/indexing/commit.rs` — commit orchestration module

Files to modify:
- `src/domain/index.rs` — add `FileRecord`, `PersistedFileOutcome`
- `src/domain/mod.rs` — re-export new types
- `src/indexing/mod.rs` — register `commit` module
- `src/indexing/pipeline.rs` — wire CAS persistence into per-file task
- `src/application/run_manager.rs` — pass CAS + RegistryPersistence to pipeline
- `src/application/mod.rs` — thread dependencies through
- `src/storage/registry_persistence.rs` — add `save_file_records`, `get_file_records`, update `RegistryData`
- `src/storage/local_cas.rs` — verify/extend store method
- `tests/indexing_integration.rs` — add persistence integration tests

No conflicts with unified project structure detected. All new code follows established `mod.rs` module style and existing directory layout.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Epic-2-Story-2.3]
- [Source: _bmad-output/planning-artifacts/architecture.md#Storage-Architecture]
- [Source: _bmad-output/planning-artifacts/architecture.md#Indexing-Pipeline-Architecture]
- [Source: _bmad-output/planning-artifacts/architecture.md#ADR-Interim-Control-Plane-Persistence]
- [Source: _bmad-output/planning-artifacts/architecture.md#Dual-Storage-Boundary]
- [Source: _bmad-output/planning-artifacts/architecture.md#Domain-Type-Requirements]
- [Source: _bmad-output/implementation-artifacts/2-2-execute-indexing-for-the-initial-quality-focus-language-set.md]
- [Source: _bmad-output/implementation-artifacts/2-1-start-an-indexed-run-with-durable-run-identity.md]
- [Source: _bmad-output/project-context.md]

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

No debug issues encountered. Clean implementation following build order.

### Completion Notes List

- Task 1: Added `PersistedFileOutcome` enum (Committed, EmptySymbols, Failed, Quarantined) and `FileRecord` struct to `src/domain/index.rs`. Re-exported from `src/domain/mod.rs`. 9 unit tests added.
- Task 2: Verified `LocalCasBlobStore::store_bytes` already implements atomic write + deduplication. Added 2 tests: SHA-256 blob_id verification and empty content handling.
- Task 3: Created `validate_for_commit` pure function in `src/indexing/commit.rs` mapping `FileOutcome` -> `PersistedFileOutcome` with hash mismatch detection. 7 unit tests.
- Task 4: Added `run_file_records: HashMap<String, Vec<FileRecord>>` to `RegistryData` with `#[serde(default)]`. Added `save_file_records` / `get_file_records` methods using existing `read_modify_write` pattern with `fs2` advisory locking. 5 unit tests including backward-compat.
- Task 5: Added `commit_file_result` orchestration function: stores bytes in CAS -> validates -> constructs FileRecord. Handles CAS root inaccessibility as systemic error vs file-local CAS failures as degraded FileRecord. Uses `&dyn BlobStore` trait for testability. 5 unit tests with hand-written fake CAS (AtomicUsize call counter).
- Task 6: Wired CAS persistence into pipeline. `IndexingPipeline` accepts optional `Arc<dyn BlobStore>` via `.with_cas()` builder. Per-file tasks commit to CAS within bounded-concurrency slot. FileRecords collected and batch-saved to registry by RunManager after pipeline completes. CAS systemic errors immediately trip circuit breaker. Pipeline finish summary includes persistence outcome breakdown. Updated `RunManager::launch_run` to accept and forward blob_store. 2 pipeline tests added.
- Task 7: 7 integration tests verifying end-to-end persistence, empty symbols outcome, out-of-scope language exclusion, run/repo linkage, CAS blob existence on disk, backward-compat registry deserialization, and test count regression guard.

### Change Log

- 2026-03-07: Implemented Story 2.3 — file-level indexing output persistence with CAS blob storage, commit-time validation, and registry persistence. Total test count: 180 (baseline was 143).

### File List

- `src/domain/index.rs` — Added `PersistedFileOutcome` enum, `FileRecord` struct, 9 unit tests
- `src/domain/mod.rs` — Added re-exports for `FileRecord`, `PersistedFileOutcome`
- `src/indexing/commit.rs` — NEW: `validate_for_commit` pure function, `commit_file_result` orchestration, 12 unit tests
- `src/indexing/mod.rs` — Registered `commit` module
- `src/indexing/pipeline.rs` — Added CAS wiring, `file_records` field on `PipelineResult`, per-file commit, persistence outcome breakdown in finish summary, 2 new tests
- `src/storage/registry_persistence.rs` — Added `run_file_records` field to `RegistryData`, `save_file_records` / `get_file_records` methods, 5 new tests
- `src/storage/local_cas.rs` — Added 2 tests (SHA-256 verification, empty content)
- `src/application/run_manager.rs` — Updated `launch_run` to accept `blob_store`, batch-save file records after pipeline
- `src/application/mod.rs` — Updated `launch_indexing` to pass `blob_store` from `ApplicationContext`
- `tests/indexing_integration.rs` — Added 7 Story 2.3 integration tests, refactored setup helper
