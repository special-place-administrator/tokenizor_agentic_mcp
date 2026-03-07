# Story 2.8: Checkpoint Long-Running Indexing Work

Status: review

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As an operator,
I want to checkpoint indexing progress during long-running work,
so that interrupted runs can later resume from durable progress.

## Acceptance Criteria

1. **Given** a long-running indexing run is in progress
   **When** a checkpoint is created
   **Then** Tokenizor persists checkpoint state durably before reporting success
   **And** the checkpoint is associated with the correct run identity

2. **Given** no valid active run exists
   **When** checkpoint creation is requested
   **Then** Tokenizor returns an explicit failure
   **And** it does not create orphan checkpoint state

## Tasks / Subtasks

- [x] Task 1: Export Checkpoint type and add to RegistryData (AC: #1)
  - [x] 1.1: Add `pub use index::Checkpoint;` to `src/domain/mod.rs` — the type exists in `src/domain/index.rs` (lines 192-198) but is not currently exported
  - [x] 1.2: Add `checkpoints: Vec<Checkpoint>` field to `RegistryData` in `src/storage/registry_persistence.rs` with `#[serde(default)]` for backward compatibility
  - [x] 1.3: Unit test: deserialize an existing registry JSON (without `checkpoints` field) — verify it loads successfully with empty checkpoints vec (backward compat)

- [x] Task 2: Add checkpoint persistence methods to RegistryPersistence (AC: #1, #2)
  - [x] 2.1: Add `save_checkpoint(&self, checkpoint: &Checkpoint) -> Result<()>` method using `read_modify_write` pattern
    - Inside the closure: verify a matching `IndexRun` exists with the checkpoint's `run_id` — if not found, return `TokenizorError::NotFound`
    - Verify the run is non-terminal (`!run.status.is_terminal()`) — if terminal, return `TokenizorError::InvalidOperation` (or a suitable variant) to prevent orphan checkpoint state (AC #2)
    - Push checkpoint to `data.checkpoints` vec
    - Update the matching `IndexRun.checkpoint_cursor` to `Some(checkpoint.cursor.clone())`
  - [x] 2.2: Add `get_latest_checkpoint(&self, run_id: &str) -> Result<Option<Checkpoint>>` method
    - Filter checkpoints by `run_id`, return the one with the highest `created_at_unix_ms`
  - [x] 2.3: Unit tests:
    - `test_save_checkpoint_persists_and_updates_run_cursor` — create run, save checkpoint, reload, verify checkpoint exists and `IndexRun.checkpoint_cursor` updated
    - `test_save_checkpoint_rejects_terminal_run` — create run, set status to `Succeeded`, attempt save checkpoint, verify error (AC #2)
    - `test_save_checkpoint_rejects_missing_run` — attempt save checkpoint with non-existent run_id, verify `NotFound` (AC #2)
    - `test_get_latest_checkpoint_returns_most_recent` — save 3 checkpoints with different timestamps, verify latest returned
    - `test_get_latest_checkpoint_returns_none_for_no_checkpoints` — verify `None` for run with no checkpoints

- [x] Task 3: Add sorted completion tracking to IndexingPipeline (AC: #1)
  - [x] 3.1: Add `symbols_extracted: AtomicU64` field to `PipelineProgress` in `src/indexing/pipeline.rs` — tracks total symbols extracted across all files (needed for checkpoint metadata). Update `PipelineProgress::new()` accordingly
  - [x] 3.2: Add a checkpoint cursor tracking mechanism to `IndexingPipeline`:
    - Add `completed_indices: Mutex<Vec<bool>>` field (sized to total discovered files)
    - Add `sorted_file_paths: Vec<PathBuf>` field (the discovered files in deterministic sorted order)
    - When a file at sorted index `i` completes successfully (committed to CAS + recorded), set `completed_indices[i] = true`
  - [x] 3.3: Add `pub fn checkpoint_cursor(&self) -> Option<String>` method on `IndexingPipeline`
    - Finds the highest index where ALL files at indices `0..=index` have `completed_indices[i] == true` (contiguous completion high-water mark)
    - Returns `Some(sorted_file_paths[index].to_string_lossy().to_string())` for the high-water mark, or `None` if no contiguous completion yet
  - [x] 3.4: Increment `symbols_extracted` in `PipelineProgress` as files are processed (add the count from each `FileProcessingResult`)
  - [x] 3.5: Unit tests:
    - `test_checkpoint_cursor_returns_none_when_no_files_complete` — fresh pipeline, verify `None`
    - `test_checkpoint_cursor_tracks_contiguous_completion` — complete files 0,1,2 → cursor = path[2]; then complete file 4 (gap at 3) → cursor still = path[2]
    - `test_checkpoint_cursor_advances_when_gap_fills` — complete 0,1,2,4 → cursor=path[2]; complete 3 → cursor=path[4]

- [x] Task 4: Add RunManager::checkpoint_run() orchestration (AC: #1, #2)
  - [x] 4.1: Add `pub fn checkpoint_run(&self, run_id: &str) -> Result<Checkpoint>` on `RunManager` in `src/application/run_manager.rs`
  - [x] 4.2: Load the run from persistence to get `repo_id` and validate existence — `NotFound` if run doesn't exist
  - [x] 4.3: If `run.status.is_terminal()` → return error (AC #2: no checkpoint for completed/cancelled/failed runs)
  - [x] 4.4: Lock `active_runs`, look up entry by `repo_id` — if not found (Queued run not yet started, or race), return error (no active pipeline to checkpoint)
  - [x] 4.5: Read progress from `ActiveRun.progress` — if `None`, return error (pipeline not yet initialized)
  - [x] 4.6: Read cursor from pipeline's checkpoint_cursor tracking — requires storing a reference to the pipeline's cursor method. Add `checkpoint_cursor_fn: Option<Arc<dyn Fn() -> Option<String> + Send + Sync>>` to `ActiveRun`, set when pipeline is created
  - [x] 4.7: Build `Checkpoint` struct: `run_id`, `cursor` (from pipeline), `files_processed` (from progress), `symbols_written` (from progress `symbols_extracted`), `created_at_unix_ms` (from `unix_timestamp_ms()`)
  - [x] 4.8: If cursor is `None` (no files completed yet), return error — cannot create a meaningful checkpoint with no committed work
  - [x] 4.9: Persist via `self.persistence.save_checkpoint(&checkpoint)`
  - [x] 4.10: Log `info!("checkpoint created for run {run_id}, cursor={cursor}, files_processed={n}")`
  - [x] 4.11: Return the checkpoint
  - [x] 4.12: Unit tests:
    - `test_checkpoint_active_run_creates_and_persists`
    - `test_checkpoint_terminal_run_returns_error`
    - `test_checkpoint_nonexistent_run_returns_not_found`
    - `test_checkpoint_run_without_progress_returns_error`
    - `test_checkpoint_run_without_cursor_returns_error`

- [x] Task 5: Add automatic periodic checkpointing in pipeline execution (AC: #1)
  - [x] 5.1: Add a checkpoint callback to `IndexingPipeline`: `checkpoint_callback: Option<Box<dyn Fn() + Send + Sync>>`
  - [x] 5.2: Add `checkpoint_interval: u64` field to `IndexingPipeline` (default: 100 files)
  - [x] 5.3: In the file processing completion path (where `files_processed` is incremented), check if `files_processed % checkpoint_interval == 0` — if so, invoke the callback
  - [x] 5.4: The callback is wired in the spawned task (in `run_manager.rs` or `mcp.rs`) to call `RunManager::checkpoint_run()` — handle errors by logging `warn!` (checkpoint failure should not abort the pipeline)
  - [x] 5.5: Update `IndexingPipeline::new()` to accept the checkpoint callback and interval as parameters
  - [x] 5.6: Update all callers of `IndexingPipeline::new()` to pass `None`/default callback for existing paths, and the real callback for the `index_folder` spawned task
  - [x] 5.7: Unit tests:
    - `test_pipeline_invokes_checkpoint_callback_at_interval` — set interval=2, process 5 files, verify callback invoked at files 2 and 4
    - `test_pipeline_skips_checkpoint_when_no_callback` — process files with `None` callback, verify no panic

- [x] Task 6: Add `checkpoint_now` MCP tool (AC: #1, #2)
  - [x] 6.1: Add `#[tool(description = "Create a checkpoint for an active indexing run. Persists current progress so interrupted work can later resume. Returns the checkpoint details. Fails if the run is not active or has no committed work yet.")]` method in the `#[tool_router] impl TokenizorServer` block in `src/protocol/mcp.rs`
  - [x] 6.2: Method signature: `fn checkpoint_now(&self, params: rmcp::model::JsonObject) -> Result<CallToolResult, McpError>`
  - [x] 6.3: Extract `run_id` required string parameter from `params` — missing → `McpError::invalid_params("missing required parameter: run_id")`
  - [x] 6.4: Call `self.application.run_manager().checkpoint_run(&run_id).map_err(to_mcp_error)?`
  - [x] 6.5: Serialize `Checkpoint` to JSON via `serde_json::to_string`, return as `CallToolResult::success(vec![Content::text(json)])`

- [x] Task 7: Integration testing (AC: #1, #2)
  - [x] 7.1: Test: create run → start pipeline with real files → checkpoint_run() mid-processing → verify checkpoint persisted with correct run_id, non-empty cursor, files_processed > 0, symbols_written >= 0
  - [x] 7.2: Test: create run → succeed → checkpoint_run() → verify error (AC #2: terminal run)
  - [x] 7.3: Test: checkpoint_run() with non-existent run_id → verify `TokenizorError::NotFound` (AC #2)
  - [x] 7.4: Test: create run → cancel → checkpoint_run() → verify error (AC #2: cancelled run)
  - [x] 7.5: Test: create run → automatic checkpoint fires during processing (set interval=1 or 2) → verify at least one checkpoint persisted after run completes
  - [x] 7.6: Test: verify checkpoint_cursor on IndexRun is updated after checkpoint → inspect run and confirm `checkpoint_cursor` matches checkpoint's cursor value
  - [x] 7.7: Verify test count does not regress below 275 (Story 2.7 baseline) — **299 tests total, all passing**

## Dev Notes

### CRITICAL: Load project-context.md FIRST

MUST load `_bmad-output/project-context.md` BEFORE starting implementation. It contains 87 agent rules scoped to Epic 2 covering persistence architecture, type design, concurrency, error handling, testing, and anti-patterns. Failure to load this will cause architectural violations.

### Build Order (MANDATORY)

Follow the build-then-test pattern established in Stories 2.2-2.7:

1. **Domain type export** (Task 1) — export `Checkpoint`, add to `RegistryData`
2. **Persistence methods** (Task 2) — `save_checkpoint` and `get_latest_checkpoint` with validation
3. **Pipeline tracking** (Task 3) — sorted completion tracking and cursor computation
4. **RunManager orchestration** (Task 4) — `checkpoint_run()` coordinates progress reading, cursor, and persistence
5. **Automatic checkpointing** (Task 5) — periodic callback during pipeline processing
6. **MCP tool** (Task 6) — `checkpoint_now` tool in `#[tool_router]` block
7. **Integration tests** (Task 7) — end-to-end verification of both acceptance criteria

### This Story Extends Existing Infrastructure — Not New Infrastructure

Most checkpoint infrastructure already exists but is **stubbed or unwired**:

| Component | Status | What exists |
|-----------|--------|-------------|
| `Checkpoint` struct | EXISTS | `run_id`, `cursor`, `files_processed`, `symbols_written`, `created_at_unix_ms` in `src/domain/index.rs` (lines 192-198) |
| `IndexRun.checkpoint_cursor` | EXISTS but ALWAYS `None` | `Option<String>` field, never set (5 locations initialize as `None`) |
| `write_checkpoint()` on ControlPlane | EXISTS but STUBBED | Trait method exists, InMemory impl works, deployment stub returns `Ok(())` |
| `RegistryData.checkpoints` | MISSING | No checkpoints field on the persisted registry structure |
| `Checkpoint` export | MISSING | Type defined but not re-exported from `src/domain/mod.rs` |
| `RegistryPersistence.save_checkpoint()` | MISSING | No checkpoint persistence method |
| `RegistryPersistence.get_latest_checkpoint()` | MISSING | No checkpoint retrieval method |
| Pipeline cursor tracking | MISSING | Pipeline has `files_processed` counter but no sorted completion tracking |
| `PipelineProgress.symbols_extracted` | MISSING | No symbol count tracking in progress |
| `RunManager::checkpoint_run()` | MISSING | No orchestration method |
| `checkpoint_now` MCP tool | MISSING | No user-facing checkpoint endpoint |
| Automatic periodic checkpointing | MISSING | No callback mechanism in pipeline |
| `ActiveRun.checkpoint_cursor_fn` | MISSING | No way to read cursor from active pipeline |

Do NOT redesign `Checkpoint`, `IndexRun`, or `ActiveRun` types. Extend and wire the existing pieces.

### Key Design Decisions

**Checkpoint persistence goes through RegistryPersistence, NOT ControlPlane trait writes.** Per ADR-7, all Epic 2 durable state uses the local bootstrap registry JSON. `ControlPlane.write_checkpoint()` remains stubbed. `RegistryPersistence.save_checkpoint()` uses the established `read_modify_write` pattern with advisory file locking, same as all other Epic 2 persistence.

**Checkpoint cursor = contiguous completion high-water mark in sorted file order.** Files are processed concurrently (bounded semaphore). File 5 might complete before file 3. The cursor only advances to file N when ALL files 0..=N have completed. This ensures resume (Story 4.2) can safely skip files at or before the cursor without missing any. Implementation uses a `Mutex<Vec<bool>>` indexed by sorted discovery order.

**Correctness invariant: checkpoint writes happen AFTER durable file commit.** The project-context is explicit: "If the checkpoint is written first and the commit fails, resume skips a file that was never processed." Mark a file as completed in the tracking structure only AFTER its CAS write and `FileRecord` persistence have both succeeded.

**`save_checkpoint` validates run state atomically.** Inside `read_modify_write`: verify run exists and is non-terminal before persisting. This prevents orphan checkpoints (AC #2) via a single atomic operation. If the run finishes between the caller's check and the persistence write, the atomic check catches it.

**Automatic checkpointing is callback-based, not persistence-coupled.** The pipeline receives an `Option<Box<dyn Fn() + Send + Sync>>` callback. The spawned task wires this to `RunManager::checkpoint_run()`. This keeps the pipeline decoupled from persistence concerns. Callback failures log `warn!` but never abort the pipeline — checkpoint loss is recoverable, pipeline abort is not.

**Checkpoint interval is file-count-based (default 100).** Per project-context: "Checkpoint frequency is proportional to accumulated work. Default every ~100-500 files." The interval is configurable on `IndexingPipeline`. Checking `files_processed % interval == 0` is O(1) and satisfies the constraint that "checkpoint I/O must be <1% of processing time."

**On-demand checkpoint via MCP tool complements automatic.** `checkpoint_now` lets operators explicitly save progress at any time. It reads the same pipeline progress and cursor state as automatic checkpointing. If no files have completed (cursor is `None`), it returns an error rather than creating a meaningless checkpoint.

**Terminal run checkpointing is an error, not a no-op.** AC #2 says "returns an explicit failure" and "does not create orphan checkpoint state." Unlike cancellation (Story 2.7 AC #2 where terminal cancellation is deterministic/idempotent), checkpoint creation on a terminal run is explicitly rejected. A terminal run has complete, deterministic state — checkpointing it adds no value and would create misleading artifacts.

### ActiveRun Extension for Cursor Access

The `checkpoint_run()` method needs to read the pipeline's checkpoint cursor. The pipeline runs in a spawned task and `ActiveRun` holds the `JoinHandle` + `CancellationToken` + `progress`. Add a cursor accessor:

```
ActiveRun {
    handle: JoinHandle<()>,
    cancellation_token: CancellationToken,
    progress: Option<Arc<PipelineProgress>>,
    checkpoint_cursor_fn: Option<Arc<dyn Fn() -> Option<String> + Send + Sync>>,  // NEW
}
```

The spawned task creates the pipeline, then registers `ActiveRun` with a closure that calls `pipeline.checkpoint_cursor()`. This requires the pipeline (or its cursor state) to be `Arc`-shared so the closure can capture it.

### Contiguous Completion Tracking Detail

```
Discovery: [file_a, file_b, file_c, file_d, file_e] (sorted)
Tracking:  [false,   false,   false,   false,   false]

file_c completes → [false, false, true, false, false] → cursor = None (gap at 0,1)
file_a completes → [true,  false, true, false, false] → cursor = None (gap at 1)
file_b completes → [true,  true,  true, false, false] → cursor = "file_c" (contiguous 0-2)
file_e completes → [true,  true,  true, false, true]  → cursor = "file_c" (gap at 3)
file_d completes → [true,  true,  true, true,  true]  → cursor = "file_e" (all done)
```

### Previous Story Review Patterns (Apply Defensively)

From Stories 2.6 and 2.7 code review learnings:

| Pattern | Risk | Prevention |
|---------|------|------------|
| **Wrong `#[tool_handler]` vs `#[tool_router]`** | Adding `checkpoint_now` in the handler block | `checkpoint_now` goes in `#[tool_router] impl TokenizorServer`, NOT `#[tool_handler]` |
| **Holding Mutex across .await** | `checkpoint_run()` holds `active_runs` lock while calling persistence | Drop the Mutex guard before calling persistence methods. Extract needed data, drop guard, then persist |
| **Missing parameter validation** | `checkpoint_now` tool doesn't validate `run_id` | Follow exact pattern from `get_index_run` / `cancel_index_run`: `.ok_or_else(\|\| McpError::invalid_params(...))` |
| **Unconditional state update** | `save_checkpoint` updates run cursor even if run is terminal | Validate run state inside `read_modify_write` before mutation |
| **Pipeline callback panics** | Checkpoint callback error crashes the pipeline | Wrap callback invocation in error handling; log `warn!`, never propagate |
| **Non-atomic cursor read** | Reading cursor while files complete concurrently | `Mutex<Vec<bool>>` ensures consistent snapshot of completion state |

### What This Story Does NOT Implement

- **Resume from checkpoint** — Story 4.2 implements reading checkpoints and skipping processed files
- **Checkpoint cleanup/purging** — Stale checkpoints are harmless; cleanup deferred to Story 4.3 or maintenance
- **Checkpoint on cancellation** — Cancellation (Story 2.7) transitions to terminal state; no automatic checkpoint-before-cancel
- **Checkpoint visibility via MCP resource** — The `run_status` resource from Story 2.6 does not include checkpoint details; could be added later
- **Checkpoint frequency auto-tuning** — Fixed interval for now; adaptive tuning is over-engineering at this stage
- **`ControlPlane.write_checkpoint()` wiring** — Trait method remains stubbed per ADR-7; `RegistryPersistence` handles durability

### Error Variant Consideration

`save_checkpoint` needs to reject checkpointing terminal runs. Options:
1. **New variant `TokenizorError::InvalidOperation(String)`** — clear semantics, reusable for future state-transition violations
2. **Reuse `TokenizorError::Integrity(String)`** — could work but conflates integrity with operational precondition failure

Prefer option 1 if no existing variant fits. Check existing `TokenizorError` variants before deciding. If an `InvalidOperation` or `Conflict` variant already exists, reuse it.

### Testing Standards

- Naming: `test_verb_condition` (e.g., `test_save_checkpoint_persists_and_updates_run_cursor`)
- Assertions: plain `assert!`, `assert_eq!` — NO assertion crates
- `#[test]` by default; `#[tokio::test]` only for async
- Fakes: hand-written with `AtomicUsize` call counters — NO mock crates
- Temp directories for all file operations
- Current baseline: 275 tests — must not regress
- Logging: `info!` for `checkpoint_run` completion, `debug!` for cursor tracking, `warn!` for checkpoint callback failures — NEVER `info!` per-file

### Existing Code Locations

| Component | Path | What to do |
|-----------|------|------------|
| `Checkpoint` struct (export) | `src/domain/index.rs` (lines 192-198) | Already defined — just export from `mod.rs` |
| `IndexRun.checkpoint_cursor` (wire) | `src/domain/index.rs` (line 151) | Already exists as `Option<String>` — set it in `save_checkpoint` |
| `RegistryData` (extend) | `src/storage/registry_persistence.rs` (lines 20-36) | Add `checkpoints: Vec<Checkpoint>` with `#[serde(default)]` |
| `RegistryPersistence` (extend) | `src/storage/registry_persistence.rs` | Add `save_checkpoint()` and `get_latest_checkpoint()` |
| `PipelineProgress` (extend) | `src/indexing/pipeline.rs` (lines 15-39) | Add `symbols_extracted: AtomicU64` |
| `IndexingPipeline` (extend) | `src/indexing/pipeline.rs` | Add completion tracking, cursor computation, checkpoint callback |
| `ActiveRun` (extend) | `src/application/run_manager.rs` (lines 21-25) | Add `checkpoint_cursor_fn` field |
| `RunManager` (extend) | `src/application/run_manager.rs` | Add `checkpoint_run()` orchestration method |
| `TokenizorServer` tools (extend) | `src/protocol/mcp.rs` | Add `checkpoint_now` tool in `#[tool_router]` block |
| Spawned task (modify) | `src/protocol/mcp.rs` or `run_manager.rs` | Wire checkpoint callback and cursor_fn into ActiveRun |
| Domain re-exports (extend) | `src/domain/mod.rs` | Add `pub use index::Checkpoint;` |
| Integration tests (extend) | `tests/indexing_integration.rs` | Add checkpoint integration tests |

### Project Structure Notes

Files to create: None

Files to modify:
- `src/domain/mod.rs` — export `Checkpoint`
- `src/domain/index.rs` — no structural changes needed (type already exists)
- `src/storage/registry_persistence.rs` — add `checkpoints` to `RegistryData`, add `save_checkpoint()` and `get_latest_checkpoint()` persistence methods
- `src/indexing/pipeline.rs` — add `symbols_extracted` to progress, add completion tracking, cursor computation, checkpoint callback
- `src/application/run_manager.rs` — add `checkpoint_cursor_fn` to `ActiveRun`, add `checkpoint_run()` orchestration method
- `src/protocol/mcp.rs` — add `checkpoint_now` MCP tool, wire checkpoint callback and cursor_fn in spawned task
- `tests/indexing_integration.rs` — add checkpoint integration tests

No conflicts with unified project structure detected. All changes follow existing module patterns.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Epic-2-Story-2.8]
- [Source: _bmad-output/planning-artifacts/prd.md#FR15]
- [Source: _bmad-output/planning-artifacts/architecture.md#Progress-Cancellation-Model — "checkpoint visibility"]
- [Source: _bmad-output/planning-artifacts/architecture.md#Mutation-and-Recovery-Rules — "validate before mutating"]
- [Source: _bmad-output/planning-artifacts/architecture.md#Source-Tree — checkpoint_now.rs target location]
- [Source: _bmad-output/planning-artifacts/architecture.md#Interim-Persistence-Decision — RegistryPersistence for all Epic 2 state]
- [Source: _bmad-output/project-context.md#Indexing-Pipeline-Architecture — checkpoint cursor, correctness invariant, frequency]
- [Source: _bmad-output/project-context.md#Epic-2-Persistence-Architecture — read_modify_write, advisory locking]
- [Source: _bmad-output/project-context.md#MCP-Server-Run-Management — tool_router vs tool_handler]
- [Source: _bmad-output/project-context.md#Testing-Rules]
- [Source: _bmad-output/implementation-artifacts/2-7-cancel-an-active-indexing-run-safely.md — previous story patterns, 275 test baseline]

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (claude-opus-4-6)

### Debug Log References

None — all tests passed on first run.

### Completion Notes List

- Added `InvalidOperation(String)` variant to `TokenizorError` for state-transition violations
- `CheckpointTracker` uses `Arc` sharing pattern (same as `PipelineProgress`) for cross-task cursor access
- `checkpoint_run()` drops Mutex guard before calling persistence — follows project pattern from `cancel_run`
- Automatic checkpoint callback uses `catch_unwind` for panic isolation; errors logged as `warn!`
- Files sorted by lowercase relative_path for deterministic cursor positions
- 299 total tests (249 unit + 3 main + 41 integration + 6 grammar), up from 275 baseline

### File List

| File | Action | Description |
|------|--------|-------------|
| `src/error.rs` | Modified | Added `InvalidOperation(String)` variant to `TokenizorError`, `is_systemic()` returns false |
| `src/storage/registry_persistence.rs` | Modified | Added `checkpoints: Vec<Checkpoint>` to `RegistryData`, `save_checkpoint()`, `get_latest_checkpoint()`, 7 unit tests |
| `src/indexing/pipeline.rs` | Modified | Added `symbols_extracted` to `PipelineProgress`, `CheckpointTracker` with contiguous cursor, checkpoint callback/interval, 6 unit tests |
| `src/application/run_manager.rs` | Modified | Added `checkpoint_cursor_fn` to `ActiveRun`, `checkpoint_run()` orchestration, wired callback in `launch_run()`, 5 unit tests |
| `src/protocol/mcp.rs` | Modified | Added `checkpoint_now` tool, `InvalidOperation` arm in `to_mcp_error()` |
| `tests/indexing_integration.rs` | Modified | Added 6 integration tests for checkpoint lifecycle |
