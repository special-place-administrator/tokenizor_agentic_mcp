# Architecture

`tokenizor_agentic_mcp` is a Rust crate with a CLI/runtime entry in `src/main.rs` and a small public re-export surface in `src/lib.rs`.

The live runtime starts in `src/main.rs`:
- `run()` loads `ServerConfig` from `src/config.rs`.
- `ApplicationContext::from_config()` in `src/application/mod.rs` builds storage services and runs startup recovery before the server accepts work.
- `guard_and_serve()` blocks serving if the deployment report is not ready.
- `TokenizorServer` in `src/protocol/mcp.rs` serves over RMCP stdio transport.

The crate is layered in the same order that requests flow:
- `src/protocol/mcp.rs`: MCP tool/resource surface, parameter validation, JSON serialization, URI handling.
- `src/application/`: orchestration and use-case logic. This is the main coordination layer.
- `src/domain/`: transport-agnostic records and enums such as `IndexRunMode`, `IndexRunStatus`, `Checkpoint`, `DiscoveryManifest`, `FileRecord`, `RunStatusReport`, `ResultEnvelope`, and `VerifiedSourceResponse`.
- `src/indexing/`: file discovery, bounded-concurrency execution, and commit validation.
- `src/parsing/`: tree-sitter parse/extract logic plus per-language adapters in `src/parsing/languages/*.rs`.
- `src/storage/`: blob storage, control-plane abstraction, and the current registry-backed durability path.

`ApplicationContext` in `src/application/mod.rs` is the runtime facade. It owns:
- `config`
- `blob_store`
- `control_plane`
- `run_manager`
- `startup_recovery`

Control flow for indexing mutations is:
1. `src/protocol/mcp.rs` parses a tool request such as `index_folder`, `reindex_repository`, `resume_index_run`, `cancel_index_run`, or `checkpoint_now`.
2. `src/application/mod.rs` delegates to `RunManager` in `src/application/run_manager.rs`.
3. `RunManager` enforces one active run per repo, persists run state through the `ControlPlane` trait in `src/storage/control_plane.rs`, and spawns the async pipeline.
4. `spawn_pipeline_for_run_with_resume()` wires three durability callbacks into `IndexingPipeline` in `src/indexing/pipeline.rs`:
   - discovery manifest persistence
   - durable file record persistence
   - periodic checkpoint creation
5. `IndexingPipeline::execute()` discovers files or rematerializes a persisted manifest on resume, then `process_discovered()` runs bounded concurrent file work.
6. Each file is read from disk, parsed by `parsing::process_file()` in `src/parsing/mod.rs`, then committed through `commit_file_result()` in `src/indexing/commit.rs`.
7. `commit_file_result()` writes raw bytes to the CAS, validates the blob hash against the parse result, and emits a `FileRecord`.
8. When the pipeline completes, `RunManager` writes final run status and clears repository invalidation on successful completion.

The indexing/retrieval pipeline is intentionally split between exact bytes and metadata:
- `src/storage/local_cas.rs` is the byte-exact store. It hashes raw bytes, writes through a temp file, calls `sync_all`, and renames into place. Existing blobs are reused by hash.
- `src/indexing/commit.rs` converts parse results into persisted outcomes: `Committed`, `EmptySymbols`, `Failed`, or `Quarantined`.
- `src/storage/registry_persistence.rs` is the current durable metadata plane for repos, runs, checkpoints, manifests, idempotency records, and file records.
- `src/application/search.rs` reads metadata from the registry/control-plane side and raw content from the blob store side, then returns verified retrieval payloads from `src/domain/retrieval.rs`.

Retrieval control flow is stricter than the mutation path:
- `check_request_gate()` in `src/application/search.rs` blocks requests when the repo is unknown, invalidated, failed, degraded, quarantined, actively mutating, or never successfully indexed.
- `search_text`, `search_symbols`, `get_file_outline`, `get_repo_outline`, `get_symbol`, and `get_symbols` all pass through that gate before reading persisted state.
- Symbol and batch retrieval paths verify blob readability, byte-range validity, and source integrity before returning `VerifiedSourceResponse` or code-slice results.
- MCP resources in `src/protocol/mcp.rs` currently expose recent run-status documents, not general repository snapshots.

Recovery boundaries are explicit:
- `startup_sweep()` in `src/application/run_manager.rs` transitions stale persisted `Running` runs to `Interrupted` and stale `Queued` runs to either `Interrupted` or `Aborted`.
- The same startup sweep removes owned temp artifacts for the registry and CAS surfaces.
- `checkpoint_run()` only succeeds for a non-terminal run that already has an active pipeline and a non-empty committed cursor.
- `resume_run()` only resumes an `Interrupted` run when all of these are present and consistent:
  - a checkpoint
  - a non-empty checkpoint cursor
  - a persisted discovery manifest
  - the cursor inside that manifest
  - durable file records covering every file up to the cursor
- If resume is unsafe, `RunManager` persists a `RunRecoveryState` rejection with a concrete next action instead of guessing.

Idempotency boundaries are also explicit:
- `start_run_idempotent()` derives `index::{repo_id}::{workspace_id}` plus a SHA-256 request hash from normalized inputs.
- `reindex_repository()` does the same with a `reindex::...` key before checking for active runs, so true replays return the original active run.
- `invalidate_repository()` uses `invalidate::...` plus a dedicated invalidation hash and treats "already invalidated" as domain-level success.
- Same key plus same hash returns the stored result while work is active.
- Same key plus different hash yields a deterministic `ConflictingReplay` error when the referenced work is still active.
- Stale or orphaned idempotency records are allowed to fall through to new work or re-application.

Major abstractions to know first:
- `ApplicationContext` in `src/application/mod.rs`
- `RunManager` in `src/application/run_manager.rs`
- `ControlPlane` in `src/storage/control_plane.rs`
- `BlobStore` in `src/storage/blob.rs`
- `RegistryPersistence` in `src/storage/registry_persistence.rs`
- `IndexingPipeline`, `PipelineProgress`, and `CheckpointTracker` in `src/indexing/pipeline.rs`
- `FileProcessingResult` and parsing adapters in `src/parsing/`

Current backend reality matters:
- `build_control_plane()` in `src/storage/control_plane.rs` can select `InMemory`, `LocalRegistry`, or `SpacetimeDb`.
- `LocalRegistry` is the complete durable path today.
- `SpacetimeControlPlane` currently implements deployment and health probes, but most CRUD operations still return "not wired in this slice yet" errors.
- The `spacetime/` directory is therefore a future landing zone, not the current authoritative execution path.
