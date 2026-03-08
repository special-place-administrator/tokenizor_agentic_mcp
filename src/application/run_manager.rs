use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::domain::{
    Checkpoint, FileOutcomeSummary, FileRecord, IdempotencyRecord, IdempotencyStatus, IndexRun,
    IndexRunMode, IndexRunStatus, InvalidationResult, PersistedFileOutcome, RepositoryStatus,
    RunHealth, RunPhase, RunProgressSnapshot, RunStatusReport, unix_timestamp_ms,
};
use crate::error::{Result, TokenizorError};
use crate::indexing::pipeline::{IndexingPipeline, PipelineProgress};
use crate::storage::BlobStore;
use crate::storage::digest_hex;
use crate::storage::{RegistryPersistence, RegistryQuery};

pub struct ActiveRun {
    pub run_id: String,
    pub handle: JoinHandle<()>,
    pub cancellation_token: CancellationToken,
    pub progress: Option<Arc<PipelineProgress>>,
    pub checkpoint_cursor_fn: Option<Arc<dyn Fn() -> Option<String> + Send + Sync>>,
}

pub struct RunManager {
    persistence: RegistryPersistence,
    active_runs: Mutex<HashMap<String, ActiveRun>>,
}

impl RunManager {
    pub fn new(persistence: RegistryPersistence) -> Self {
        Self {
            persistence,
            active_runs: Mutex::new(HashMap::new()),
        }
    }

    pub fn startup_sweep(&self) -> Result<Vec<String>> {
        let running_runs = self
            .persistence
            .find_runs_by_status(&IndexRunStatus::Running)?;

        let mut transitioned = Vec::new();
        for run in &running_runs {
            self.persistence.update_run_status(
                &run.run_id,
                IndexRunStatus::Interrupted,
                Some("stale run detected during startup sweep".to_string()),
            )?;
            info!(
                run_id = %run.run_id,
                repo_id = %run.repo_id,
                "startup sweep: transitioned stale run from Running to Interrupted"
            );
            transitioned.push(run.run_id.clone());
        }

        Ok(transitioned)
    }

    pub fn start_run(&self, repo_id: &str, mode: IndexRunMode) -> Result<IndexRun> {
        let active_runs = self.active_runs.lock().unwrap_or_else(|e| e.into_inner());
        if active_runs.contains_key(repo_id) {
            return Err(TokenizorError::InvalidArgument(format!(
                "an active indexing run already exists for repository `{repo_id}`"
            )));
        }
        drop(active_runs);

        let persisted_active = self.persistence.list_runs()?;
        let has_active_persisted = persisted_active.iter().any(|r| {
            r.repo_id == repo_id
                && matches!(r.status, IndexRunStatus::Queued | IndexRunStatus::Running)
        });
        if has_active_persisted {
            return Err(TokenizorError::InvalidArgument(format!(
                "an active indexing run already exists for repository `{repo_id}`"
            )));
        }

        let requested_at = unix_timestamp_ms();
        let run_id = generate_run_id(repo_id, &mode, requested_at);

        let run = IndexRun {
            run_id,
            repo_id: repo_id.to_string(),
            mode,
            status: IndexRunStatus::Queued,
            requested_at_unix_ms: requested_at,
            started_at_unix_ms: None,
            finished_at_unix_ms: None,
            idempotency_key: None,
            request_hash: None,
            checkpoint_cursor: None,
            error_summary: None,
            not_yet_supported: None,
            prior_run_id: None,
            description: None,
        };

        self.persistence.save_run(&run)?;

        info!(
            run_id = %run.run_id,
            repo_id = %run.repo_id,
            mode = ?run.mode,
            "created new indexing run with Queued status"
        );

        Ok(run)
    }

    pub fn register_active_run(&self, repo_id: &str, active_run: ActiveRun) {
        let mut active_runs = self.active_runs.lock().unwrap_or_else(|e| e.into_inner());
        active_runs.insert(repo_id.to_string(), active_run);
    }

    pub fn has_active_run(&self, repo_id: &str) -> bool {
        let active_runs = self.active_runs.lock().unwrap_or_else(|e| e.into_inner());
        active_runs.contains_key(repo_id)
    }

    pub fn get_active_run_id(&self, repo_id: &str) -> Option<String> {
        let active_runs = self.active_runs.lock().unwrap_or_else(|e| e.into_inner());
        active_runs.get(repo_id).map(|r| r.run_id.clone())
    }

    pub fn get_active_progress(&self, repo_id: &str) -> Option<RunProgressSnapshot> {
        let active_runs = self.active_runs.lock().unwrap_or_else(|e| e.into_inner());
        active_runs.get(repo_id).and_then(|active| {
            active.progress.as_ref().map(|p| RunProgressSnapshot {
                phase: p.phase(),
                total_files: p.total_files.load(std::sync::atomic::Ordering::Relaxed),
                files_processed: p.files_processed.load(std::sync::atomic::Ordering::Relaxed),
                files_failed: p.files_failed.load(std::sync::atomic::Ordering::Relaxed),
            })
        })
    }

    pub fn start_run_idempotent(
        &self,
        repo_id: &str,
        workspace_id: &str,
        mode: IndexRunMode,
    ) -> Result<IdempotentRunResult> {
        let idempotency_key = format!("index::{repo_id}::{workspace_id}");
        let request_hash = compute_request_hash(repo_id, workspace_id, &mode);

        if let Some(existing) = self.persistence.find_idempotency_record(&idempotency_key)? {
            let run_id = existing.result_ref.as_deref().unwrap_or("");
            let referenced_run = self.persistence.find_run(run_id)?;
            let is_stale = match &referenced_run {
                Some(run) => run.status.is_terminal(),
                None => true, // orphaned record
            };

            if existing.request_hash == request_hash {
                if is_stale {
                    info!(
                        idempotency_key = %idempotency_key,
                        "stale idempotent record — referenced run is terminal, proceeding with new run"
                    );
                    // Fall through to new run creation
                } else {
                    // Same hash + active run → idempotent replay
                    info!(
                        idempotency_key = %idempotency_key,
                        "idempotent replay detected, returning stored result"
                    );
                    return Ok(IdempotentRunResult::ExistingRun {
                        run_id: run_id.to_string(),
                    });
                }
            } else if is_stale {
                info!(
                    idempotency_key = %idempotency_key,
                    "stale conflicting record — referenced run is terminal, allowing new run"
                );
                // Fall through to new run creation
            } else {
                // Different hash + active run → conflicting replay
                return Err(TokenizorError::ConflictingReplay(format!(
                    "idempotency key `{idempotency_key}`: \
                     request hash differs from stored record"
                )));
            }
        }

        let run = self.start_run(repo_id, mode)?;

        let record = IdempotencyRecord {
            operation: "index".to_string(),
            idempotency_key,
            request_hash,
            status: IdempotencyStatus::Pending,
            result_ref: Some(run.run_id.clone()),
            created_at_unix_ms: unix_timestamp_ms(),
            expires_at_unix_ms: None,
        };
        self.persistence.save_idempotency_record(&record)?;

        Ok(IdempotentRunResult::NewRun { run })
    }

    pub fn reindex_repository(
        self: &Arc<Self>,
        repo_id: &str,
        workspace_id: Option<&str>,
        reason: Option<&str>,
        repo_root: PathBuf,
        blob_store: Arc<dyn BlobStore>,
    ) -> Result<IndexRun> {
        // H1 fix: Idempotency check FIRST (before active-run check).
        // Same pattern as start_run_idempotent — if same request replays,
        // return stored result even if that run is still active.
        let ws_id = workspace_id.unwrap_or("");
        let idempotency_key = format!("reindex::{repo_id}::{ws_id}");
        let mode = IndexRunMode::Reindex;
        let request_hash = compute_request_hash(repo_id, ws_id, &mode);

        if let Some(existing) = self.persistence.find_idempotency_record(&idempotency_key)? {
            let run_id = existing.result_ref.as_deref().unwrap_or("");
            let referenced_run = self.persistence.find_run(run_id)?;
            let is_stale = match &referenced_run {
                Some(run) => run.status.is_terminal(),
                None => true, // orphaned record
            };

            if existing.request_hash == request_hash {
                if is_stale {
                    info!(
                        idempotency_key = %idempotency_key,
                        "stale idempotent reindex record — referenced run is terminal, proceeding with new run"
                    );
                    // Fall through to new run creation
                } else {
                    // Same hash + active run → idempotent replay
                    info!(
                        idempotency_key = %idempotency_key,
                        "idempotent reindex replay detected, returning stored result"
                    );
                    return Ok(referenced_run.unwrap());
                }
            } else if is_stale {
                info!(
                    idempotency_key = %idempotency_key,
                    "stale conflicting reindex record — referenced run is terminal, allowing new run"
                );
                // Fall through to new run creation
            } else {
                // Different hash + active run → conflicting replay
                return Err(TokenizorError::ConflictingReplay(format!(
                    "idempotency key `{idempotency_key}`: \
                     request hash differs from stored record"
                )));
            }
        }

        // Check for active runs — one active run per repo at a time
        let active_runs = self.active_runs.lock().unwrap_or_else(|e| e.into_inner());
        if active_runs.contains_key(repo_id) {
            return Err(TokenizorError::InvalidOperation(format!(
                "cannot re-index repository `{repo_id}`: an active indexing run exists"
            )));
        }
        drop(active_runs);

        let persisted_active = self.persistence.list_runs()?;
        let has_active_persisted = persisted_active.iter().any(|r| {
            r.repo_id == repo_id
                && matches!(r.status, IndexRunStatus::Queued | IndexRunStatus::Running)
        });
        if has_active_persisted {
            return Err(TokenizorError::InvalidOperation(format!(
                "cannot re-index repository `{repo_id}`: an active indexing run exists"
            )));
        }

        // Auto-discover prior_run_id
        let prior_run_id = self
            .persistence
            .get_latest_completed_run(repo_id)?
            .map(|r| r.run_id);

        // Create new reindex run
        let requested_at = unix_timestamp_ms();
        let run_id = generate_run_id(repo_id, &mode, requested_at);

        let run = IndexRun {
            run_id,
            repo_id: repo_id.to_string(),
            mode,
            status: IndexRunStatus::Queued,
            requested_at_unix_ms: requested_at,
            started_at_unix_ms: None,
            finished_at_unix_ms: None,
            idempotency_key: Some(idempotency_key.clone()),
            request_hash: Some(request_hash.clone()),
            checkpoint_cursor: None,
            error_summary: None,
            not_yet_supported: None,
            prior_run_id,
            description: reason.map(|r| r.to_string()),
        };

        self.persistence.save_run(&run)?;

        // Save idempotency record
        let record = IdempotencyRecord {
            operation: "reindex".to_string(),
            idempotency_key,
            request_hash,
            status: IdempotencyStatus::Pending,
            result_ref: Some(run.run_id.clone()),
            created_at_unix_ms: requested_at,
            expires_at_unix_ms: None,
        };
        self.persistence.save_idempotency_record(&record)?;

        info!(
            run_id = %run.run_id,
            repo_id = %run.repo_id,
            prior_run_id = ?run.prior_run_id,
            "created new reindex run, launching pipeline"
        );

        // H2 fix: Actually launch the pipeline (same pattern as launch_run)
        self.spawn_pipeline_for_run(&run, repo_root, blob_store);

        Ok(run)
    }

    pub fn invalidate_repository(
        &self,
        repo_id: &str,
        workspace_id: Option<&str>,
        reason: Option<&str>,
    ) -> Result<InvalidationResult> {
        // Validate repo exists first
        let repo = self
            .persistence
            .get_repository(repo_id)?
            .ok_or_else(|| TokenizorError::NotFound(format!("repository not found: {repo_id}")))?;

        // Domain-level idempotency: already invalidated → return success
        // regardless of idempotency key state or reason
        if repo.status == RepositoryStatus::Invalidated {
            info!(repo_id = %repo_id, "repository already invalidated, returning success");
            return Ok(InvalidationResult {
                repo_id: repo_id.to_string(),
                previous_status: RepositoryStatus::Invalidated,
                invalidated_at_unix_ms: repo.invalidated_at_unix_ms.unwrap_or(0),
                reason: repo.invalidation_reason.clone(),
                action_required: "re-index or repair required".to_string(),
            });
        }

        let ws_id = workspace_id.unwrap_or("");
        let idempotency_key = format!("invalidate::{repo_id}::{ws_id}");
        let request_hash = compute_invalidation_request_hash(repo_id, ws_id, reason);

        // Key-based idempotency check.
        // We already know repo is NOT Invalidated (domain-level check above returned early).
        // If an old idempotency record exists, the repo was previously invalidated but the
        // effect was later reversed (e.g., by re-indexing). The record is stale — fall
        // through to re-apply the invalidation and overwrite the idempotency record.
        if let Some(_stale) = self.persistence.find_idempotency_record(&idempotency_key)? {
            debug!(
                idempotency_key = %idempotency_key,
                "stale idempotency record found — repo no longer invalidated, re-applying"
            );
        }

        // Check for active runs — cannot invalidate while indexing is active
        let active_runs = self.active_runs.lock().unwrap_or_else(|e| e.into_inner());
        if active_runs.contains_key(repo_id) {
            return Err(TokenizorError::InvalidOperation(format!(
                "cannot invalidate repository `{repo_id}`: an active indexing run exists \
                 — cancel or wait for completion first"
            )));
        }
        drop(active_runs);

        let persisted_active = self.persistence.list_runs()?;
        let has_active_persisted = persisted_active.iter().any(|r| {
            r.repo_id == repo_id
                && matches!(r.status, IndexRunStatus::Queued | IndexRunStatus::Running)
        });
        if has_active_persisted {
            return Err(TokenizorError::InvalidOperation(format!(
                "cannot invalidate repository `{repo_id}`: an active indexing run exists \
                 — cancel or wait for completion first"
            )));
        }

        // Transition repo status to Invalidated
        let now = unix_timestamp_ms();
        let previous_status = repo.status.clone();

        self.persistence.update_repository_status(
            repo_id,
            RepositoryStatus::Invalidated,
            Some(now),
            reason.map(|r| r.to_string()),
            None,
            None,
        )?;

        // Save idempotency record
        let record = IdempotencyRecord {
            operation: "invalidate".to_string(),
            idempotency_key,
            request_hash,
            status: IdempotencyStatus::Succeeded,
            result_ref: Some(repo_id.to_string()),
            created_at_unix_ms: now,
            expires_at_unix_ms: None,
        };
        self.persistence.save_idempotency_record(&record)?;

        info!(
            repo_id = %repo_id,
            previous_status = ?previous_status,
            "repository indexed state invalidated"
        );

        Ok(InvalidationResult {
            repo_id: repo_id.to_string(),
            previous_status,
            invalidated_at_unix_ms: now,
            reason: reason.map(|r| r.to_string()),
            action_required: "re-index or repair required".to_string(),
        })
    }

    pub fn launch_run(
        self: &Arc<Self>,
        repo_id: &str,
        mode: IndexRunMode,
        repo_root: PathBuf,
        blob_store: Arc<dyn BlobStore>,
    ) -> Result<(IndexRun, Arc<PipelineProgress>)> {
        let run = self.start_run(repo_id, mode)?;
        let progress = self.spawn_pipeline_for_run(&run, repo_root, blob_store);
        Ok((run, progress))
    }

    fn spawn_pipeline_for_run(
        self: &Arc<Self>,
        run: &IndexRun,
        repo_root: PathBuf,
        blob_store: Arc<dyn BlobStore>,
    ) -> Arc<PipelineProgress> {
        let run_id = run.run_id.clone();
        let repo_id_owned = run.repo_id.clone();

        let token = CancellationToken::new();

        // Set up checkpoint callback: calls RunManager::checkpoint_run() periodically
        let manager_for_cb = Arc::clone(self);
        let run_id_for_cb = run_id.clone();
        let checkpoint_callback = Box::new(move || {
            if let Err(e) = manager_for_cb.checkpoint_run(&run_id_for_cb) {
                warn!(run_id = %run_id_for_cb, error = %e, "periodic checkpoint failed");
            }
        });

        let pipeline = IndexingPipeline::new(run_id.clone(), repo_root, token.clone())
            .with_cas(blob_store, repo_id_owned.clone())
            .with_checkpoint_callback(checkpoint_callback, 100);
        let progress = pipeline.progress();
        let tracker = pipeline.checkpoint_tracker();

        let manager = Arc::clone(self);

        let handle = tokio::spawn(async move {
            // Transition to Running with start timestamp (skips if already terminal)
            if let Err(e) = manager
                .persistence
                .transition_to_running(&run_id, unix_timestamp_ms())
            {
                error!(run_id = %run_id, error = %e, "failed to transition to Running");
                manager.deregister_active_run(&repo_id_owned);
                return;
            }

            // If already cancelled before pipeline starts, skip execution
            let already_terminal = match manager.persistence.find_run(&run_id) {
                Ok(Some(r)) => r.status.is_terminal(),
                Ok(None) => false,
                Err(e) => {
                    warn!(run_id = %run_id, error = %e, "failed to read run before pipeline start");
                    false
                }
            };
            if already_terminal {
                debug!(run_id = %run_id, "run already terminal before pipeline start — skipping");
                manager.deregister_active_run(&repo_id_owned);
                return;
            }

            let result = pipeline.execute().await;

            // Batch-save file records to registry
            let mut file_record_error: Option<String> = None;
            if !result.file_records.is_empty() {
                let record_count = result.file_records.len();
                if let Err(e) = manager
                    .persistence
                    .save_file_records(&run_id, &result.file_records)
                {
                    error!(
                        run_id = %run_id,
                        records = record_count,
                        error = %e,
                        "failed to save file records to registry"
                    );
                    file_record_error = Some(format!(
                        "failed to persist {record_count} file records: {e}"
                    ));
                } else {
                    info!(
                        run_id = %run_id,
                        records = record_count,
                        "file records saved to registry"
                    );
                }
            }

            // Merge file record save error into error_summary so it's visible on the run
            let final_error_summary = match (result.error_summary, file_record_error) {
                (Some(pipeline_err), Some(record_err)) => {
                    Some(format!("{pipeline_err}; {record_err}"))
                }
                (Some(err), None) | (None, Some(err)) => Some(err),
                (None, None) => None,
            };

            // Check if the run was already cancelled (or otherwise made terminal)
            // by cancel_run() before we update status — prevents overwriting Cancelled
            let already_terminal = match manager.persistence.find_run(&run_id) {
                Ok(Some(r)) => r.status.is_terminal(),
                Ok(None) => false,
                Err(e) => {
                    warn!(run_id = %run_id, error = %e, "failed to read run before status update");
                    false
                }
            };

            if !already_terminal {
                let finished_at = unix_timestamp_ms();
                let not_yet_supported = if result.not_yet_supported.is_empty() {
                    None
                } else {
                    Some(result.not_yet_supported)
                };
                if let Err(e) = manager.persistence.update_run_status_with_finish(
                    &run_id,
                    result.status.clone(),
                    final_error_summary,
                    finished_at,
                    not_yet_supported,
                ) {
                    error!(run_id = %run_id, error = %e, "failed to update final run status");
                }

                // Clear invalidation on successful run completion
                if result.status == IndexRunStatus::Succeeded {
                    if let Ok(Some(repo)) = manager.persistence.get_repository(&repo_id_owned) {
                        if repo.status == RepositoryStatus::Invalidated {
                            if let Err(e) = manager.persistence.update_repository_status(
                                &repo_id_owned,
                                RepositoryStatus::Ready,
                                None,
                                None,
                                None,
                                None,
                            ) {
                                warn!(repo_id = %repo_id_owned, error = %e, "failed to clear invalidation after successful run");
                            } else {
                                info!(repo_id = %repo_id_owned, "cleared invalidation after successful run");
                            }
                        }
                    }
                }
            } else {
                debug!(run_id = %run_id, "run already terminal — skipping status update");
            }

            // Deregister active run (idempotent — no-op if already removed by cancel_run)
            manager.deregister_active_run(&repo_id_owned);
        });

        let cursor_fn: Arc<dyn Fn() -> Option<String> + Send + Sync> =
            Arc::new(move || tracker.checkpoint_cursor());

        self.register_active_run(
            &run.repo_id,
            ActiveRun {
                run_id: run.run_id.clone(),
                handle,
                cancellation_token: token,
                progress: Some(Arc::clone(&progress)),
                checkpoint_cursor_fn: Some(cursor_fn),
            },
        );

        progress
    }

    pub fn deregister_active_run(&self, repo_id: &str) {
        let mut active_runs = self.active_runs.lock().unwrap_or_else(|e| e.into_inner());
        active_runs.remove(repo_id);
    }

    pub fn persistence(&self) -> &RegistryPersistence {
        &self.persistence
    }

    pub fn registry_query(&self) -> &dyn RegistryQuery {
        &self.persistence
    }

    pub fn inspect_run(&self, run_id: &str) -> Result<RunStatusReport> {
        let run = self
            .persistence
            .find_run(run_id)?
            .ok_or_else(|| TokenizorError::NotFound(format!("run '{run_id}' not found")))?;

        self.build_run_report(run)
    }

    pub fn cancel_run(&self, run_id: &str) -> Result<RunStatusReport> {
        let run = self
            .persistence
            .find_run(run_id)?
            .ok_or_else(|| TokenizorError::NotFound(format!("run '{run_id}' not found")))?;

        // AC #2: terminal runs return current report without mutation
        if run.status.is_terminal() {
            return self.inspect_run(run_id);
        }

        // Signal cancellation token and remove from active_runs
        // Drop Mutex guard before calling persistence methods
        {
            let mut active_runs = self.active_runs.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(active_run) = active_runs.remove(&run.repo_id) {
                active_run.cancellation_token.cancel();
                debug!(run_id = %run_id, repo_id = %run.repo_id, "cancellation token signaled");
            }
        }

        // Atomic, race-safe persistence update
        let changed = self
            .persistence
            .cancel_run_if_active(run_id, unix_timestamp_ms())?;

        if changed {
            info!(run_id = %run_id, "run cancelled");
        } else {
            debug!(run_id = %run_id, "cancel_run: run became terminal before persistence update");
        }
        self.inspect_run(run_id)
    }

    pub fn checkpoint_run(&self, run_id: &str) -> Result<Checkpoint> {
        let run = self
            .persistence
            .find_run(run_id)?
            .ok_or_else(|| TokenizorError::NotFound(format!("run '{run_id}' not found")))?;

        if run.status.is_terminal() {
            return Err(TokenizorError::InvalidOperation(format!(
                "cannot checkpoint run '{run_id}' with terminal status '{:?}'",
                run.status
            )));
        }

        // Extract needed data from active_runs, then drop the Mutex guard
        let (progress, cursor) = {
            let active_runs = self.active_runs.lock().unwrap_or_else(|e| e.into_inner());
            let active = active_runs.get(&run.repo_id).ok_or_else(|| {
                TokenizorError::InvalidOperation(format!(
                    "run '{run_id}' has no active pipeline (may be Queued or race condition)"
                ))
            })?;

            let progress = active.progress.as_ref().ok_or_else(|| {
                TokenizorError::InvalidOperation(format!(
                    "run '{run_id}' pipeline not yet initialized (no progress available)"
                ))
            })?;

            let files_processed = progress
                .files_processed
                .load(std::sync::atomic::Ordering::Relaxed);
            let symbols_extracted = progress
                .symbols_extracted
                .load(std::sync::atomic::Ordering::Relaxed);

            let cursor = active.checkpoint_cursor_fn.as_ref().and_then(|f| f());

            ((files_processed, symbols_extracted), cursor)
        };
        // Mutex guard dropped here

        let cursor = cursor.ok_or_else(|| {
            TokenizorError::InvalidOperation(format!(
                "run '{run_id}' has no committed work yet (cursor is empty)"
            ))
        })?;

        let checkpoint = Checkpoint {
            run_id: run_id.to_string(),
            cursor,
            files_processed: progress.0,
            symbols_written: progress.1,
            created_at_unix_ms: unix_timestamp_ms(),
        };

        self.persistence.save_checkpoint(&checkpoint)?;

        info!(
            run_id = %run_id,
            cursor = %checkpoint.cursor,
            files_processed = checkpoint.files_processed,
            "checkpoint created for run"
        );

        Ok(checkpoint)
    }

    pub fn list_runs_with_health(
        &self,
        repo_id: Option<&str>,
        status: Option<&IndexRunStatus>,
    ) -> Result<Vec<RunStatusReport>> {
        let runs = match status {
            Some(s) => self.persistence.find_runs_by_status(s)?,
            None => self.persistence.list_runs()?,
        };

        let filtered = match repo_id {
            Some(rid) => runs
                .into_iter()
                .filter(|r| r.repo_id == rid)
                .collect::<Vec<_>>(),
            None => runs,
        };

        let mut reports = Vec::with_capacity(filtered.len());
        for run in filtered {
            reports.push(self.build_run_report(run)?);
        }

        debug!(count = reports.len(), "listed runs with health");
        Ok(reports)
    }

    pub fn list_recent_run_ids(&self, limit: usize) -> Vec<String> {
        let all_runs = self.persistence.list_runs().unwrap_or_default();
        let mut sorted = all_runs;
        // Sort by requested_at (not started_at) because started_at is Option<u64>
        // and may be None for Queued runs. requested_at is always set.
        sorted.sort_by(|a, b| b.requested_at_unix_ms.cmp(&a.requested_at_unix_ms));
        sorted.into_iter().take(limit).map(|r| r.run_id).collect()
    }

    fn build_run_report(&self, run: IndexRun) -> Result<RunStatusReport> {
        let is_active = run.status == IndexRunStatus::Running && self.has_active_run(&run.repo_id);

        let progress = if is_active {
            self.get_active_progress(&run.repo_id)
        } else {
            None
        };

        let file_outcome_summary = if run.status.is_terminal() {
            let records = self.persistence.get_file_records(&run.run_id)?;
            if records.is_empty() {
                None
            } else {
                Some(build_file_outcome_summary(&records))
            }
        } else {
            None
        };

        let progress = match progress {
            Some(p) => Some(p),
            None if run.status.is_terminal() => {
                file_outcome_summary
                    .as_ref()
                    .map(|fos| RunProgressSnapshot {
                        phase: RunPhase::Complete,
                        total_files: fos.total_committed,
                        files_processed: fos.processed_ok + fos.partial_parse,
                        files_failed: fos.failed,
                    })
            }
            None => None,
        };

        let health = classify_run_health(&run, file_outcome_summary.as_ref());
        let mut action_required = action_required_message(&run, &health);

        // Surface repo-level invalidation in action_required
        if let Ok(Some(repo)) = self.persistence.get_repository(&run.repo_id) {
            if repo.status == RepositoryStatus::Invalidated {
                let invalidation_note =
                    "repository indexed state has been invalidated — re-index or repair required";
                action_required = Some(match action_required {
                    Some(existing) => format!("{existing}. {invalidation_note}"),
                    None => invalidation_note.to_string(),
                });
            }
        }

        Ok(RunStatusReport {
            run,
            health,
            is_active,
            progress,
            file_outcome_summary,
            action_required,
        })
    }
}

#[derive(Debug)]
pub enum IdempotentRunResult {
    NewRun { run: IndexRun },
    ExistingRun { run_id: String },
}

fn compute_invalidation_request_hash(
    repo_id: &str,
    workspace_id: &str,
    reason: Option<&str>,
) -> String {
    let reason_str = reason.unwrap_or("");
    let input = format!("invalidate:{repo_id}:{workspace_id}:{reason_str}");
    digest_hex(input.as_bytes())
}

fn compute_request_hash(repo_id: &str, workspace_id: &str, mode: &IndexRunMode) -> String {
    let mode_str = match mode {
        IndexRunMode::Full => "full",
        IndexRunMode::Incremental => "incremental",
        IndexRunMode::Repair => "repair",
        IndexRunMode::Verify => "verify",
        IndexRunMode::Reindex => "reindex",
    };
    let input = format!("index:{repo_id}:{workspace_id}:{mode_str}");
    digest_hex(input.as_bytes())
}

fn generate_run_id(repo_id: &str, mode: &IndexRunMode, requested_at_unix_ms: u64) -> String {
    let mode_str = match mode {
        IndexRunMode::Full => "full",
        IndexRunMode::Incremental => "incremental",
        IndexRunMode::Repair => "repair",
        IndexRunMode::Verify => "verify",
        IndexRunMode::Reindex => "reindex",
    };
    let input = format!("{repo_id}:{mode_str}:{requested_at_unix_ms}");
    digest_hex(input.as_bytes())
}

fn classify_run_health(run: &IndexRun, file_summary: Option<&FileOutcomeSummary>) -> RunHealth {
    match &run.status {
        IndexRunStatus::Queued | IndexRunStatus::Running | IndexRunStatus::Cancelled => {
            RunHealth::Healthy
        }
        IndexRunStatus::Failed | IndexRunStatus::Interrupted | IndexRunStatus::Aborted => {
            RunHealth::Unhealthy
        }
        IndexRunStatus::Succeeded => match file_summary {
            Some(summary) if summary.failed > 0 || summary.partial_parse > 0 => RunHealth::Degraded,
            _ => RunHealth::Healthy,
        },
    }
}

fn action_required_message(run: &IndexRun, health: &RunHealth) -> Option<String> {
    match &run.status {
        IndexRunStatus::Interrupted => {
            Some("Run was interrupted. Resume with re-index or repair.".into())
        }
        IndexRunStatus::Failed => {
            let detail = run.error_summary.as_deref().unwrap_or("unknown error");
            Some(format!("Run failed: {detail}. Investigate and re-run."))
        }
        IndexRunStatus::Aborted => Some(
            "Run aborted (circuit breaker). Check file-level errors, consider repair mode.".into(),
        ),
        IndexRunStatus::Succeeded if *health == RunHealth::Degraded => {
            Some("Run completed with degraded files. Review partial/failed outcomes.".into())
        }
        _ => None,
    }
}

fn build_file_outcome_summary(records: &[FileRecord]) -> FileOutcomeSummary {
    let mut summary = FileOutcomeSummary {
        total_committed: 0,
        processed_ok: 0,
        partial_parse: 0,
        failed: 0,
    };
    for record in records {
        summary.total_committed += 1;
        match &record.outcome {
            PersistedFileOutcome::Committed => summary.processed_ok += 1,
            PersistedFileOutcome::EmptySymbols => summary.processed_ok += 1,
            PersistedFileOutcome::Failed { .. } => summary.failed += 1,
            PersistedFileOutcome::Quarantined { .. } => summary.partial_parse += 1,
        }
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_run_manager() -> (tempfile::TempDir, RunManager) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("registry.json");
        let persistence = RegistryPersistence::new(path);
        let manager = RunManager::new(persistence);
        (dir, manager)
    }

    #[test]
    fn test_start_run_creates_queued_record() {
        let (_dir, manager) = temp_run_manager();
        let run = manager.start_run("repo-1", IndexRunMode::Full).unwrap();
        assert_eq!(run.repo_id, "repo-1");
        assert_eq!(run.status, IndexRunStatus::Queued);
        assert!(!run.run_id.is_empty());
        assert!(run.started_at_unix_ms.is_none());
        assert!(run.finished_at_unix_ms.is_none());

        let persisted = manager.persistence.find_run(&run.run_id).unwrap().unwrap();
        assert_eq!(persisted.run_id, run.run_id);
        assert_eq!(persisted.status, IndexRunStatus::Queued);
    }

    #[test]
    fn test_start_run_rejects_concurrent_run_for_same_repo() {
        let (_dir, manager) = temp_run_manager();
        manager.start_run("repo-1", IndexRunMode::Full).unwrap();

        let result = manager.start_run("repo-1", IndexRunMode::Full);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("active indexing run already exists"));
        assert!(err.contains("repo-1"));
    }

    #[test]
    fn test_start_run_allows_different_repos() {
        let (_dir, manager) = temp_run_manager();
        let run1 = manager.start_run("repo-1", IndexRunMode::Full).unwrap();
        let run2 = manager.start_run("repo-2", IndexRunMode::Full).unwrap();

        assert_ne!(run1.run_id, run2.run_id);
        assert_eq!(run1.repo_id, "repo-1");
        assert_eq!(run2.repo_id, "repo-2");
    }

    #[test]
    fn test_startup_sweep_transitions_running_to_interrupted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("registry.json");

        {
            let persistence = RegistryPersistence::new(path.clone());
            let run = IndexRun {
                run_id: "stale-run".to_string(),
                repo_id: "repo-1".to_string(),
                mode: IndexRunMode::Full,
                status: IndexRunStatus::Running,
                requested_at_unix_ms: 1000,
                started_at_unix_ms: Some(1001),
                finished_at_unix_ms: None,
                idempotency_key: None,
                request_hash: None,
                checkpoint_cursor: None,
                error_summary: None,
                not_yet_supported: None,
                prior_run_id: None,
                description: None,
            };
            persistence.save_run(&run).unwrap();
        }

        let persistence = RegistryPersistence::new(path);
        let manager = RunManager::new(persistence);
        let transitioned = manager.startup_sweep().unwrap();

        assert_eq!(transitioned, vec!["stale-run".to_string()]);
        let run = manager.persistence.find_run("stale-run").unwrap().unwrap();
        assert_eq!(run.status, IndexRunStatus::Interrupted);
        assert!(run.error_summary.is_some());
    }

    #[test]
    fn test_startup_sweep_ignores_non_running_statuses() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("registry.json");

        let persistence = RegistryPersistence::new(path.clone());
        persistence
            .save_run(&IndexRun {
                run_id: "queued-run".to_string(),
                repo_id: "repo-1".to_string(),
                mode: IndexRunMode::Full,
                status: IndexRunStatus::Queued,
                requested_at_unix_ms: 1000,
                started_at_unix_ms: None,
                finished_at_unix_ms: None,
                idempotency_key: None,
                request_hash: None,
                checkpoint_cursor: None,
                error_summary: None,
                not_yet_supported: None,
                prior_run_id: None,
                description: None,
            })
            .unwrap();
        persistence
            .save_run(&IndexRun {
                run_id: "succeeded-run".to_string(),
                repo_id: "repo-2".to_string(),
                mode: IndexRunMode::Full,
                status: IndexRunStatus::Succeeded,
                requested_at_unix_ms: 1000,
                started_at_unix_ms: Some(1001),
                finished_at_unix_ms: Some(2000),
                idempotency_key: None,
                request_hash: None,
                checkpoint_cursor: None,
                error_summary: None,
                not_yet_supported: None,
                prior_run_id: None,
                description: None,
            })
            .unwrap();

        let manager = RunManager::new(RegistryPersistence::new(path));
        let transitioned = manager.startup_sweep().unwrap();
        assert!(transitioned.is_empty());
    }

    #[test]
    fn test_generate_run_id_is_deterministic() {
        let id1 = generate_run_id("repo-1", &IndexRunMode::Full, 1000);
        let id2 = generate_run_id("repo-1", &IndexRunMode::Full, 1000);
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_generate_run_id_differs_for_different_inputs() {
        let id1 = generate_run_id("repo-1", &IndexRunMode::Full, 1000);
        let id2 = generate_run_id("repo-1", &IndexRunMode::Incremental, 1000);
        let id3 = generate_run_id("repo-2", &IndexRunMode::Full, 1000);
        let id4 = generate_run_id("repo-1", &IndexRunMode::Full, 2000);
        assert_ne!(id1, id2);
        assert_ne!(id1, id3);
        assert_ne!(id1, id4);
    }

    #[test]
    fn test_has_active_run_tracks_registration() {
        let (_dir, manager) = temp_run_manager();
        assert!(!manager.has_active_run("repo-1"));

        let token = CancellationToken::new();
        let handle = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .spawn(async {});
        manager.register_active_run(
            "repo-1",
            ActiveRun {
                run_id: "run-1".to_string(),
                handle,
                cancellation_token: token,
                progress: None,
                checkpoint_cursor_fn: None,
            },
        );

        assert!(manager.has_active_run("repo-1"));
        assert!(!manager.has_active_run("repo-2"));
    }

    #[test]
    fn test_idempotent_replay_returns_stored_result() {
        let (_dir, manager) = temp_run_manager();
        let result = manager
            .start_run_idempotent("repo-1", "ws-1", IndexRunMode::Full)
            .unwrap();
        let run_id = match &result {
            IdempotentRunResult::NewRun { run } => run.run_id.clone(),
            IdempotentRunResult::ExistingRun { .. } => panic!("expected NewRun"),
        };

        let replay = manager
            .start_run_idempotent("repo-1", "ws-1", IndexRunMode::Full)
            .unwrap();
        match replay {
            IdempotentRunResult::ExistingRun { run_id: stored_id } => {
                assert_eq!(stored_id, run_id);
            }
            IdempotentRunResult::NewRun { .. } => panic!("expected ExistingRun"),
        }
    }

    #[test]
    fn test_conflicting_replay_is_rejected() {
        let (_dir, manager) = temp_run_manager();
        manager
            .start_run_idempotent("repo-1", "ws-1", IndexRunMode::Full)
            .unwrap();

        let result = manager.start_run_idempotent("repo-1", "ws-1", IndexRunMode::Incremental);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, TokenizorError::ConflictingReplay(_)),
            "expected ConflictingReplay, got: {err:?}"
        );
        assert!(err.to_string().contains("conflicting replay"));
    }

    #[test]
    fn test_idempotent_run_persists_record() {
        let (_dir, manager) = temp_run_manager();
        manager
            .start_run_idempotent("repo-1", "ws-1", IndexRunMode::Full)
            .unwrap();

        let record = manager
            .persistence
            .find_idempotency_record("index::repo-1::ws-1")
            .unwrap();
        assert!(record.is_some());
        let record = record.unwrap();
        assert_eq!(record.operation, "index");
        assert!(record.result_ref.is_some());
    }

    #[test]
    fn test_idempotent_same_hash_terminal_run_creates_new_run() {
        let (_dir, manager) = temp_run_manager();
        let result = manager
            .start_run_idempotent("repo-1", "ws-1", IndexRunMode::Full)
            .unwrap();
        let first_run_id = match &result {
            IdempotentRunResult::NewRun { run } => run.run_id.clone(),
            _ => panic!("expected NewRun"),
        };

        // Terminate the run
        manager
            .persistence
            .update_run_status(&first_run_id, IndexRunStatus::Succeeded, None)
            .unwrap();

        // Same params replay after terminal → should create new run
        let replay = manager
            .start_run_idempotent("repo-1", "ws-1", IndexRunMode::Full)
            .unwrap();
        match replay {
            IdempotentRunResult::NewRun { run } => {
                assert_ne!(
                    run.run_id, first_run_id,
                    "should be a new run, not the old one"
                );
            }
            IdempotentRunResult::ExistingRun { .. } => panic!("expected NewRun for stale record"),
        }
    }

    #[test]
    fn test_idempotent_different_hash_active_run_returns_conflicting_replay() {
        let (_dir, manager) = temp_run_manager();
        manager
            .start_run_idempotent("repo-1", "ws-1", IndexRunMode::Full)
            .unwrap();

        // Different params while run is active → ConflictingReplay
        let result = manager.start_run_idempotent("repo-1", "ws-1", IndexRunMode::Incremental);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TokenizorError::ConflictingReplay(_)
        ));
    }

    #[test]
    fn test_idempotent_different_hash_terminal_run_creates_new_run() {
        let (_dir, manager) = temp_run_manager();
        let result = manager
            .start_run_idempotent("repo-1", "ws-1", IndexRunMode::Full)
            .unwrap();
        let first_run_id = match &result {
            IdempotentRunResult::NewRun { run } => run.run_id.clone(),
            _ => panic!("expected NewRun"),
        };

        // Terminate the run
        manager
            .persistence
            .update_run_status(&first_run_id, IndexRunStatus::Failed, None)
            .unwrap();

        // Different params after terminal → should create new run (stale record)
        let replay = manager
            .start_run_idempotent("repo-1", "ws-1", IndexRunMode::Incremental)
            .unwrap();
        match replay {
            IdempotentRunResult::NewRun { run } => {
                assert_ne!(run.run_id, first_run_id);
                assert_eq!(run.mode, IndexRunMode::Incremental);
            }
            IdempotentRunResult::ExistingRun { .. } => panic!("expected NewRun for stale record"),
        }
    }

    #[test]
    fn test_idempotent_orphaned_record_creates_new_run() {
        let (_dir, manager) = temp_run_manager();

        // Seed an orphaned idempotency record (run doesn't exist)
        let hash = compute_request_hash("repo-1", "ws-1", &IndexRunMode::Full);
        let record = IdempotencyRecord {
            operation: "index".to_string(),
            idempotency_key: "index::repo-1::ws-1".to_string(),
            request_hash: hash,
            status: IdempotencyStatus::Pending,
            result_ref: Some("nonexistent-run".to_string()),
            created_at_unix_ms: 1000,
            expires_at_unix_ms: None,
        };
        manager
            .persistence
            .save_idempotency_record(&record)
            .unwrap();

        // Same params replay with orphaned record → new run
        let result = manager
            .start_run_idempotent("repo-1", "ws-1", IndexRunMode::Full)
            .unwrap();
        match result {
            IdempotentRunResult::NewRun { run } => {
                assert_ne!(run.run_id, "nonexistent-run");
            }
            IdempotentRunResult::ExistingRun { .. } => {
                panic!("expected NewRun for orphaned record")
            }
        }
    }

    #[test]
    fn test_start_run_after_completed_run_succeeds() {
        let (_dir, manager) = temp_run_manager();
        let first_run = manager.start_run("repo-1", IndexRunMode::Full).unwrap();

        manager
            .persistence
            .update_run_status(&first_run.run_id, IndexRunStatus::Succeeded, None)
            .unwrap();

        let second_run = manager.start_run("repo-1", IndexRunMode::Full).unwrap();
        assert_ne!(first_run.run_id, second_run.run_id);
        assert_eq!(second_run.status, IndexRunStatus::Queued);
    }

    #[test]
    fn test_active_run_progress_snapshot_reflects_atomic_counters() {
        let (_dir, manager) = temp_run_manager();
        let token = CancellationToken::new();
        let handle = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .spawn(async {});

        let progress = Arc::new(PipelineProgress::new());
        progress
            .total_files
            .store(100, std::sync::atomic::Ordering::Relaxed);
        progress
            .files_processed
            .store(75, std::sync::atomic::Ordering::Relaxed);
        progress
            .files_failed
            .store(3, std::sync::atomic::Ordering::Relaxed);

        manager.register_active_run(
            "repo-progress",
            ActiveRun {
                run_id: "run-progress".to_string(),
                handle,
                cancellation_token: token,
                progress: Some(Arc::clone(&progress)),
                checkpoint_cursor_fn: None,
            },
        );

        let snapshot = manager.get_active_progress("repo-progress");
        assert!(snapshot.is_some());
        let snapshot = snapshot.unwrap();
        assert_eq!(snapshot.total_files, 100);
        assert_eq!(snapshot.files_processed, 75);
        assert_eq!(snapshot.files_failed, 3);

        // Verify no progress for unknown repo
        assert!(manager.get_active_progress("unknown-repo").is_none());
    }

    fn sample_run_with_status(status: IndexRunStatus) -> IndexRun {
        IndexRun {
            run_id: "run-health-test".into(),
            repo_id: "repo-1".into(),
            mode: IndexRunMode::Full,
            status,
            requested_at_unix_ms: 1000,
            started_at_unix_ms: Some(1001),
            finished_at_unix_ms: Some(2000),
            idempotency_key: None,
            request_hash: None,
            checkpoint_cursor: None,
            error_summary: None,
            not_yet_supported: None,
            prior_run_id: None,
            description: None,
        }
    }

    #[test]
    fn test_classify_health_succeeded_all_ok_returns_healthy() {
        let run = sample_run_with_status(IndexRunStatus::Succeeded);
        let summary = FileOutcomeSummary {
            total_committed: 10,
            processed_ok: 10,
            partial_parse: 0,
            failed: 0,
        };
        assert_eq!(
            classify_run_health(&run, Some(&summary)),
            RunHealth::Healthy
        );
    }

    #[test]
    fn test_classify_health_succeeded_with_partial_returns_degraded() {
        let run = sample_run_with_status(IndexRunStatus::Succeeded);
        let summary = FileOutcomeSummary {
            total_committed: 10,
            processed_ok: 8,
            partial_parse: 2,
            failed: 0,
        };
        assert_eq!(
            classify_run_health(&run, Some(&summary)),
            RunHealth::Degraded
        );
    }

    #[test]
    fn test_classify_health_failed_returns_unhealthy() {
        let run = sample_run_with_status(IndexRunStatus::Failed);
        assert_eq!(classify_run_health(&run, None), RunHealth::Unhealthy);
    }

    #[test]
    fn test_classify_health_interrupted_returns_unhealthy() {
        let run = sample_run_with_status(IndexRunStatus::Interrupted);
        assert_eq!(classify_run_health(&run, None), RunHealth::Unhealthy);
    }

    #[test]
    fn test_classify_health_cancelled_returns_healthy() {
        let run = sample_run_with_status(IndexRunStatus::Cancelled);
        assert_eq!(classify_run_health(&run, None), RunHealth::Healthy);
    }

    #[test]
    fn test_classify_health_running_returns_healthy() {
        let run = sample_run_with_status(IndexRunStatus::Running);
        assert_eq!(classify_run_health(&run, None), RunHealth::Healthy);
    }

    #[test]
    fn test_classify_health_aborted_returns_unhealthy() {
        let run = sample_run_with_status(IndexRunStatus::Aborted);
        assert_eq!(classify_run_health(&run, None), RunHealth::Unhealthy);
    }

    #[test]
    fn test_action_required_for_interrupted_run() {
        let run = sample_run_with_status(IndexRunStatus::Interrupted);
        let health = classify_run_health(&run, None);
        let msg = action_required_message(&run, &health);
        assert!(msg.is_some());
        assert!(msg.unwrap().contains("interrupted"));
    }

    #[test]
    fn test_action_required_for_healthy_run_is_none() {
        let run = sample_run_with_status(IndexRunStatus::Succeeded);
        let summary = FileOutcomeSummary {
            total_committed: 5,
            processed_ok: 5,
            partial_parse: 0,
            failed: 0,
        };
        let health = classify_run_health(&run, Some(&summary));
        assert!(action_required_message(&run, &health).is_none());
    }

    #[test]
    fn test_active_progress_snapshot_includes_phase() {
        let (_dir, manager) = temp_run_manager();
        let token = CancellationToken::new();
        let handle = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .spawn(async {});

        let progress = Arc::new(PipelineProgress::new());
        progress
            .total_files
            .store(50, std::sync::atomic::Ordering::Relaxed);
        progress
            .files_processed
            .store(25, std::sync::atomic::Ordering::Relaxed);
        progress.set_phase(RunPhase::Processing);

        manager.register_active_run(
            "repo-phase",
            ActiveRun {
                run_id: "run-phase".to_string(),
                handle,
                cancellation_token: token,
                progress: Some(Arc::clone(&progress)),
                checkpoint_cursor_fn: None,
            },
        );

        let snapshot = manager.get_active_progress("repo-phase").unwrap();
        assert_eq!(snapshot.phase, RunPhase::Processing);
        assert_eq!(snapshot.total_files, 50);
        assert_eq!(snapshot.files_processed, 25);
    }

    #[test]
    fn test_terminal_run_report_includes_final_progress_snapshot() {
        let (_dir, manager) = temp_run_manager();

        // Create a run that completes
        let run = manager
            .start_run("repo-terminal", IndexRunMode::Full)
            .unwrap();
        let run_id = run.run_id.clone();

        // Simulate run completion with file records
        let file_record = FileRecord {
            relative_path: "main.rs".into(),
            language: crate::domain::LanguageId::Rust,
            blob_id: "abc123".into(),
            byte_len: 100,
            content_hash: "hash123".into(),
            outcome: PersistedFileOutcome::Committed,
            symbols: vec![],
            run_id: run_id.clone(),
            repo_id: "repo-terminal".into(),
            committed_at_unix_ms: 1000,
        };
        manager
            .persistence
            .save_file_records(&run_id, &[file_record])
            .unwrap();
        manager
            .persistence
            .update_run_status(&run_id, IndexRunStatus::Succeeded, None)
            .unwrap();

        let report = manager.inspect_run(&run_id).unwrap();
        assert!(!report.is_active);
        assert!(report.progress.is_some());
        let progress = report.progress.unwrap();
        assert_eq!(progress.phase, RunPhase::Complete);
        assert_eq!(progress.total_files, 1);
        assert_eq!(progress.files_processed, 1);
        assert_eq!(progress.files_failed, 0);
    }

    #[tokio::test]
    async fn test_cancel_active_run_signals_token_and_returns_cancelled() {
        let (_dir, manager) = temp_run_manager();
        let run = manager
            .start_run("repo-cancel", IndexRunMode::Full)
            .unwrap();
        let run_id = run.run_id.clone();

        // Transition to Running
        manager
            .persistence
            .transition_to_running(&run_id, 1001)
            .unwrap();

        // Register an active run with a cancellation token
        let token = CancellationToken::new();
        let token_clone = token.clone();
        manager.register_active_run(
            "repo-cancel",
            ActiveRun {
                run_id: run_id.clone(),
                handle: tokio::spawn(async {}),
                cancellation_token: token,
                progress: None,
                checkpoint_cursor_fn: None,
            },
        );

        let report = manager.cancel_run(&run_id).unwrap();
        assert_eq!(report.run.status, IndexRunStatus::Cancelled);
        assert!(!report.is_active);
        assert!(token_clone.is_cancelled());
    }

    #[test]
    fn test_cancel_terminal_run_returns_current_report_without_mutation() {
        let (_dir, manager) = temp_run_manager();
        let run = manager.start_run("repo-term", IndexRunMode::Full).unwrap();
        let run_id = run.run_id.clone();

        // Complete the run
        manager
            .persistence
            .update_run_status_with_finish(&run_id, IndexRunStatus::Succeeded, None, 2000, None)
            .unwrap();

        let report = manager.cancel_run(&run_id).unwrap();
        assert_eq!(report.run.status, IndexRunStatus::Succeeded);
        assert!(!report.is_active);
    }

    #[test]
    fn test_cancel_nonexistent_run_returns_not_found() {
        let (_dir, manager) = temp_run_manager();

        let result = manager.cancel_run("nonexistent-run");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::error::TokenizorError::NotFound(_)
        ));
    }

    #[test]
    fn test_cancel_queued_run_without_active_entry_transitions_to_cancelled() {
        let (_dir, manager) = temp_run_manager();
        let run = manager
            .start_run("repo-queued", IndexRunMode::Full)
            .unwrap();
        let run_id = run.run_id.clone();

        // Run is Queued — no active entry registered yet
        let report = manager.cancel_run(&run_id).unwrap();
        assert_eq!(report.run.status, IndexRunStatus::Cancelled);
        assert!(!report.is_active);
    }

    #[tokio::test]
    async fn test_cancel_removes_from_active_runs() {
        let (_dir, manager) = temp_run_manager();
        let run = manager
            .start_run("repo-remove", IndexRunMode::Full)
            .unwrap();
        let run_id = run.run_id.clone();

        manager
            .persistence
            .transition_to_running(&run_id, 1001)
            .unwrap();

        let token = CancellationToken::new();
        manager.register_active_run(
            "repo-remove",
            ActiveRun {
                run_id: run_id.clone(),
                handle: tokio::spawn(async {}),
                cancellation_token: token,
                progress: None,
                checkpoint_cursor_fn: None,
            },
        );

        assert!(manager.has_active_run("repo-remove"));
        manager.cancel_run(&run_id).unwrap();
        assert!(!manager.has_active_run("repo-remove"));
    }

    #[tokio::test]
    async fn test_checkpoint_active_run_creates_and_persists() {
        let (_dir, manager) = temp_run_manager();
        let run = manager.start_run("repo-cp", IndexRunMode::Full).unwrap();
        let run_id = run.run_id.clone();
        manager
            .persistence
            .transition_to_running(&run_id, 1001)
            .unwrap();

        let progress = Arc::new(PipelineProgress::new());
        progress
            .files_processed
            .store(42, std::sync::atomic::Ordering::Relaxed);
        progress
            .symbols_extracted
            .store(150, std::sync::atomic::Ordering::Relaxed);

        let cursor_fn: Arc<dyn Fn() -> Option<String> + Send + Sync> =
            Arc::new(|| Some("src/main.rs".to_string()));

        manager.register_active_run(
            "repo-cp",
            ActiveRun {
                run_id: run_id.clone(),
                handle: tokio::spawn(async {}),
                cancellation_token: CancellationToken::new(),
                progress: Some(progress),
                checkpoint_cursor_fn: Some(cursor_fn),
            },
        );

        let checkpoint = manager.checkpoint_run(&run_id).unwrap();
        assert_eq!(checkpoint.run_id, run_id);
        assert_eq!(checkpoint.cursor, "src/main.rs");
        assert_eq!(checkpoint.files_processed, 42);
        assert_eq!(checkpoint.symbols_written, 150);
        assert!(checkpoint.created_at_unix_ms > 0);

        // Verify persisted
        let latest = manager
            .persistence()
            .get_latest_checkpoint(&run_id)
            .unwrap();
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().cursor, "src/main.rs");

        // Verify run's checkpoint_cursor updated
        let updated_run = manager.persistence().find_run(&run_id).unwrap().unwrap();
        assert_eq!(
            updated_run.checkpoint_cursor,
            Some("src/main.rs".to_string())
        );
    }

    #[test]
    fn test_checkpoint_terminal_run_returns_error() {
        let (_dir, manager) = temp_run_manager();
        let run = manager
            .start_run("repo-term-cp", IndexRunMode::Full)
            .unwrap();
        let run_id = run.run_id.clone();
        manager
            .persistence
            .update_run_status(&run_id, IndexRunStatus::Succeeded, None)
            .unwrap();

        let result = manager.checkpoint_run(&run_id);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TokenizorError::InvalidOperation(_)
        ));
    }

    #[test]
    fn test_checkpoint_nonexistent_run_returns_not_found() {
        let (_dir, manager) = temp_run_manager();

        let result = manager.checkpoint_run("nonexistent-run");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TokenizorError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_checkpoint_run_without_progress_returns_error() {
        let (_dir, manager) = temp_run_manager();
        let run = manager
            .start_run("repo-noprog", IndexRunMode::Full)
            .unwrap();
        let run_id = run.run_id.clone();
        manager
            .persistence
            .transition_to_running(&run_id, 1001)
            .unwrap();

        manager.register_active_run(
            "repo-noprog",
            ActiveRun {
                run_id: run_id.clone(),
                handle: tokio::spawn(async {}),
                cancellation_token: CancellationToken::new(),
                progress: None,
                checkpoint_cursor_fn: None,
            },
        );

        let result = manager.checkpoint_run(&run_id);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TokenizorError::InvalidOperation(_)
        ));
    }

    #[tokio::test]
    async fn test_checkpoint_run_without_cursor_returns_error() {
        let (_dir, manager) = temp_run_manager();
        let run = manager
            .start_run("repo-nocursor", IndexRunMode::Full)
            .unwrap();
        let run_id = run.run_id.clone();
        manager
            .persistence
            .transition_to_running(&run_id, 1001)
            .unwrap();

        let progress = Arc::new(PipelineProgress::new());
        progress
            .files_processed
            .store(10, std::sync::atomic::Ordering::Relaxed);

        // cursor_fn returns None (no committed work yet)
        let cursor_fn: Arc<dyn Fn() -> Option<String> + Send + Sync> = Arc::new(|| None);

        manager.register_active_run(
            "repo-nocursor",
            ActiveRun {
                run_id: run_id.clone(),
                handle: tokio::spawn(async {}),
                cancellation_token: CancellationToken::new(),
                progress: Some(progress),
                checkpoint_cursor_fn: Some(cursor_fn),
            },
        );

        let result = manager.checkpoint_run(&run_id);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TokenizorError::InvalidOperation(_)
        ));
    }

    fn temp_reindex_env() -> (
        tempfile::TempDir,
        Arc<RunManager>,
        tempfile::TempDir,
        Arc<dyn BlobStore>,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("registry.json");
        let persistence = RegistryPersistence::new(path);
        let manager = Arc::new(RunManager::new(persistence));
        let cas_dir = tempfile::tempdir().unwrap();
        let cas: Arc<dyn BlobStore> = Arc::new(crate::storage::LocalCasBlobStore::new(
            crate::config::BlobStoreConfig {
                root_dir: cas_dir.path().to_path_buf(),
            },
        ));
        cas.initialize().unwrap();
        (dir, manager, cas_dir, cas)
    }

    fn reindex_repo_root() -> tempfile::TempDir {
        let repo_dir = tempfile::tempdir().unwrap();
        std::fs::write(repo_dir.path().join("main.rs"), "fn main() {}").unwrap();
        repo_dir
    }

    #[tokio::test]
    async fn test_reindex_creates_run_with_prior_run_id() {
        let (_dir, manager, _cas_dir, cas) = temp_reindex_env();
        let repo_dir = reindex_repo_root();
        // Create and complete a prior run
        let prior = manager.start_run("repo-1", IndexRunMode::Full).unwrap();
        manager
            .persistence
            .update_run_status_with_finish(
                &prior.run_id,
                IndexRunStatus::Succeeded,
                None,
                2000,
                None,
            )
            .unwrap();

        let reindex = manager
            .reindex_repository("repo-1", None, None, repo_dir.path().to_path_buf(), cas)
            .unwrap();
        assert_eq!(reindex.mode, IndexRunMode::Reindex);
        assert_eq!(reindex.prior_run_id, Some(prior.run_id));
        assert_eq!(reindex.status, IndexRunStatus::Queued);
    }

    #[tokio::test]
    async fn test_reindex_with_active_run_returns_error() {
        let (_dir, manager, _cas_dir, cas) = temp_reindex_env();
        let repo_dir = reindex_repo_root();
        let _active = manager.start_run("repo-1", IndexRunMode::Full).unwrap();

        let result =
            manager.reindex_repository("repo-1", None, None, repo_dir.path().to_path_buf(), cas);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("active indexing run exists"));
    }

    #[tokio::test]
    async fn test_reindex_idempotent_replay_returns_same_run_while_queued() {
        let (_dir, manager, _cas_dir, cas) = temp_reindex_env();
        let repo_dir = reindex_repo_root();
        // Create and complete a prior run so reindex has a target
        let prior = manager.start_run("repo-1", IndexRunMode::Full).unwrap();
        manager
            .persistence
            .update_run_status_with_finish(
                &prior.run_id,
                IndexRunStatus::Succeeded,
                None,
                2000,
                None,
            )
            .unwrap();

        let first = manager
            .reindex_repository(
                "repo-1",
                None,
                None,
                repo_dir.path().to_path_buf(),
                cas.clone(),
            )
            .unwrap();
        // H1 fix: replay while the first reindex is still active — idempotency check
        // fires before active-run check, so the stored result is returned
        let replay = manager
            .reindex_repository("repo-1", None, None, repo_dir.path().to_path_buf(), cas)
            .unwrap();
        assert_eq!(
            first.run_id, replay.run_id,
            "idempotent replay should return same run"
        );
    }

    #[tokio::test]
    async fn test_reindex_conflicting_replay_returns_error() {
        let (_dir, manager, _cas_dir, cas) = temp_reindex_env();
        let repo_dir = reindex_repo_root();
        // Create an active run so the idempotency record references a non-terminal run
        let active_run = manager.start_run("repo-1", IndexRunMode::Reindex).unwrap();
        let record = IdempotencyRecord {
            operation: "reindex".to_string(),
            idempotency_key: "reindex::repo-1::".to_string(),
            request_hash: "different-hash-from-prior-request".to_string(),
            status: IdempotencyStatus::Pending,
            result_ref: Some(active_run.run_id.clone()),
            created_at_unix_ms: 1000,
            expires_at_unix_ms: None,
        };
        manager
            .persistence
            .save_idempotency_record(&record)
            .unwrap();

        let result =
            manager.reindex_repository("repo-1", None, None, repo_dir.path().to_path_buf(), cas);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, TokenizorError::ConflictingReplay(_)),
            "expected ConflictingReplay, got: {err:?}"
        );
        assert!(err.to_string().contains("conflicting replay"));
    }

    #[tokio::test]
    async fn test_reindex_no_prior_completed_run_sets_none() {
        let (_dir, manager, _cas_dir, cas) = temp_reindex_env();
        let repo_dir = reindex_repo_root();
        // No prior runs at all
        let reindex = manager
            .reindex_repository("repo-1", None, None, repo_dir.path().to_path_buf(), cas)
            .unwrap();
        assert_eq!(reindex.mode, IndexRunMode::Reindex);
        assert_eq!(reindex.prior_run_id, None);
    }

    #[tokio::test]
    async fn test_reindex_prior_run_auto_discovered() {
        let (_dir, manager, _cas_dir, cas) = temp_reindex_env();
        let repo_dir = reindex_repo_root();
        // Create multiple runs — only succeeded ones count
        let run1 = manager.start_run("repo-1", IndexRunMode::Full).unwrap();
        manager
            .persistence
            .update_run_status_with_finish(
                &run1.run_id,
                IndexRunStatus::Succeeded,
                None,
                2000,
                None,
            )
            .unwrap();

        let run2 = manager.start_run("repo-1", IndexRunMode::Full).unwrap();
        manager
            .persistence
            .update_run_status_with_finish(&run2.run_id, IndexRunStatus::Failed, None, 3000, None)
            .unwrap();

        let reindex = manager
            .reindex_repository("repo-1", None, None, repo_dir.path().to_path_buf(), cas)
            .unwrap();
        // Should pick run1 (succeeded), not run2 (failed)
        assert_eq!(reindex.prior_run_id, Some(run1.run_id));
    }

    #[tokio::test]
    async fn test_reindex_same_hash_terminal_run_creates_new_run() {
        let (_dir, manager, _cas_dir, cas) = temp_reindex_env();
        let repo_dir = reindex_repo_root();
        // Create and complete a prior run
        let prior = manager.start_run("repo-1", IndexRunMode::Full).unwrap();
        manager
            .persistence
            .update_run_status_with_finish(
                &prior.run_id,
                IndexRunStatus::Succeeded,
                None,
                2000,
                None,
            )
            .unwrap();

        // First reindex
        let first = manager
            .reindex_repository(
                "repo-1",
                None,
                None,
                repo_dir.path().to_path_buf(),
                cas.clone(),
            )
            .unwrap();
        let first_id = first.run_id.clone();

        // Terminate the reindex run
        manager
            .persistence
            .update_run_status_with_finish(&first_id, IndexRunStatus::Succeeded, None, 3000, None)
            .unwrap();
        manager.deregister_active_run("repo-1");

        // Same params replay after terminal → new run (stale record bypassed)
        let second = manager
            .reindex_repository("repo-1", None, None, repo_dir.path().to_path_buf(), cas)
            .unwrap();
        assert_ne!(
            second.run_id, first_id,
            "should be a new run, not the old one"
        );
        assert_eq!(second.mode, IndexRunMode::Reindex);
    }

    #[tokio::test]
    async fn test_reindex_different_hash_active_run_returns_conflicting_replay() {
        let (_dir, manager, _cas_dir, cas) = temp_reindex_env();
        let repo_dir = reindex_repo_root();
        // Seed an idempotency record with different hash + active run
        let run = manager.start_run("repo-1", IndexRunMode::Reindex).unwrap();
        let record = IdempotencyRecord {
            operation: "reindex".to_string(),
            idempotency_key: "reindex::repo-1::".to_string(),
            request_hash: "different-hash-from-new-request".to_string(),
            status: IdempotencyStatus::Pending,
            result_ref: Some(run.run_id.clone()),
            created_at_unix_ms: 1000,
            expires_at_unix_ms: None,
        };
        manager
            .persistence
            .save_idempotency_record(&record)
            .unwrap();

        let result =
            manager.reindex_repository("repo-1", None, None, repo_dir.path().to_path_buf(), cas);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TokenizorError::ConflictingReplay(_)
        ));
    }

    #[tokio::test]
    async fn test_reindex_different_hash_terminal_run_creates_new_run() {
        let (_dir, manager, _cas_dir, cas) = temp_reindex_env();
        let repo_dir = reindex_repo_root();
        // Seed a completed run + idempotency record with different hash
        let run = manager.start_run("repo-1", IndexRunMode::Reindex).unwrap();
        manager
            .persistence
            .update_run_status_with_finish(&run.run_id, IndexRunStatus::Succeeded, None, 2000, None)
            .unwrap();
        let record = IdempotencyRecord {
            operation: "reindex".to_string(),
            idempotency_key: "reindex::repo-1::".to_string(),
            request_hash: "old-hash-differs-from-new".to_string(),
            status: IdempotencyStatus::Pending,
            result_ref: Some(run.run_id.clone()),
            created_at_unix_ms: 1000,
            expires_at_unix_ms: None,
        };
        manager
            .persistence
            .save_idempotency_record(&record)
            .unwrap();

        // Different hash + terminal run → new run (stale record bypassed)
        let result = manager
            .reindex_repository("repo-1", None, None, repo_dir.path().to_path_buf(), cas)
            .unwrap();
        assert_ne!(result.run_id, run.run_id, "should be a new run");
        assert_eq!(result.mode, IndexRunMode::Reindex);
    }

    #[tokio::test]
    async fn test_reindex_orphaned_record_creates_new_run() {
        let (_dir, manager, _cas_dir, cas) = temp_reindex_env();
        let repo_dir = reindex_repo_root();
        // Seed an orphaned idempotency record (referenced run doesn't exist)
        let hash = compute_request_hash("repo-1", "", &IndexRunMode::Reindex);
        let record = IdempotencyRecord {
            operation: "reindex".to_string(),
            idempotency_key: "reindex::repo-1::".to_string(),
            request_hash: hash,
            status: IdempotencyStatus::Pending,
            result_ref: Some("nonexistent-run-id".to_string()),
            created_at_unix_ms: 1000,
            expires_at_unix_ms: None,
        };
        manager
            .persistence
            .save_idempotency_record(&record)
            .unwrap();

        let result = manager
            .reindex_repository("repo-1", None, None, repo_dir.path().to_path_buf(), cas)
            .unwrap();
        assert_ne!(result.run_id, "nonexistent-run-id");
        assert_eq!(result.mode, IndexRunMode::Reindex);
    }

    fn seed_repo(manager: &RunManager, repo_id: &str) {
        let repo = crate::domain::Repository {
            repo_id: repo_id.to_string(),
            kind: crate::domain::RepositoryKind::Git,
            root_uri: format!("/tmp/{repo_id}"),
            project_identity: format!("identity-{repo_id}"),
            project_identity_kind: crate::domain::ProjectIdentityKind::GitCommonDir,
            default_branch: None,
            last_known_revision: None,
            status: RepositoryStatus::Ready,
            invalidated_at_unix_ms: None,
            invalidation_reason: None,
            quarantined_at_unix_ms: None,
            quarantine_reason: None,
        };
        manager.persistence.save_repository(&repo).unwrap();
    }

    #[test]
    fn test_invalidate_transitions_repo_from_ready_to_invalidated() {
        let (_dir, manager) = temp_run_manager();
        seed_repo(&manager, "repo-1");

        let result = manager
            .invalidate_repository("repo-1", None, Some("stale data"))
            .unwrap();
        assert_eq!(result.repo_id, "repo-1");
        assert_eq!(result.previous_status, RepositoryStatus::Ready);
        assert!(result.invalidated_at_unix_ms > 0);
        assert_eq!(result.reason.as_deref(), Some("stale data"));
        assert_eq!(result.action_required, "re-index or repair required");

        let repo = manager
            .persistence
            .get_repository("repo-1")
            .unwrap()
            .unwrap();
        assert_eq!(repo.status, RepositoryStatus::Invalidated);
    }

    #[test]
    fn test_invalidate_unknown_repo_returns_not_found() {
        let (_dir, manager) = temp_run_manager();
        let result = manager.invalidate_repository("nonexistent", None, None);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TokenizorError::NotFound(_)));
    }

    #[test]
    fn test_invalidate_already_invalidated_returns_success() {
        let (_dir, manager) = temp_run_manager();
        seed_repo(&manager, "repo-1");

        let first = manager
            .invalidate_repository("repo-1", None, Some("reason-1"))
            .unwrap();
        assert_eq!(first.previous_status, RepositoryStatus::Ready);

        let second = manager
            .invalidate_repository("repo-1", None, Some("reason-2"))
            .unwrap();
        // Domain-level idempotency: already invalidated → success
        assert_eq!(second.previous_status, RepositoryStatus::Invalidated);
    }

    #[test]
    fn test_invalidate_with_active_run_returns_invalid_operation() {
        let (_dir, manager) = temp_run_manager();
        seed_repo(&manager, "repo-1");

        // Create a run to simulate active state
        let run = manager.start_run("repo-1", IndexRunMode::Full).unwrap();
        // Transition to Running in persistence
        manager
            .persistence
            .transition_to_running(&run.run_id, unix_timestamp_ms())
            .unwrap();

        let result = manager.invalidate_repository("repo-1", None, None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, TokenizorError::InvalidOperation(_)),
            "expected InvalidOperation, got: {err:?}"
        );
    }

    #[test]
    fn test_invalidate_idempotent_replay_returns_stored_result() {
        let (_dir, manager) = temp_run_manager();
        seed_repo(&manager, "repo-1");

        let first = manager
            .invalidate_repository("repo-1", None, Some("reason"))
            .unwrap();

        // Same request replayed
        let second = manager
            .invalidate_repository("repo-1", None, Some("reason"))
            .unwrap();

        assert_eq!(first.repo_id, second.repo_id);
        assert_eq!(second.action_required, "re-index or repair required");
    }

    #[test]
    fn test_invalidate_different_reason_after_invalidated_returns_success() {
        let (_dir, manager) = temp_run_manager();
        seed_repo(&manager, "repo-1");

        manager
            .invalidate_repository("repo-1", None, Some("reason-A"))
            .unwrap();

        // Different reason but repo already invalidated → domain-level idempotency
        // returns success (already invalidated)
        let result = manager
            .invalidate_repository("repo-1", None, Some("reason-B"))
            .unwrap();
        assert_eq!(result.previous_status, RepositoryStatus::Invalidated);
        assert_eq!(result.reason.as_deref(), Some("reason-A")); // keeps original reason
    }

    #[test]
    fn test_invalidate_pending_repo_works() {
        let (_dir, manager) = temp_run_manager();
        let repo = crate::domain::Repository {
            repo_id: "repo-1".to_string(),
            kind: crate::domain::RepositoryKind::Git,
            root_uri: "/tmp/repo-1".to_string(),
            project_identity: "identity".to_string(),
            project_identity_kind: crate::domain::ProjectIdentityKind::GitCommonDir,
            default_branch: None,
            last_known_revision: None,
            status: RepositoryStatus::Pending,
            invalidated_at_unix_ms: None,
            invalidation_reason: None,
            quarantined_at_unix_ms: None,
            quarantine_reason: None,
        };
        manager.persistence.save_repository(&repo).unwrap();

        let result = manager.invalidate_repository("repo-1", None, None).unwrap();
        assert_eq!(result.previous_status, RepositoryStatus::Pending);
    }

    #[test]
    fn test_invalidation_idempotency_key_distinct_from_reindex() {
        let (_dir, manager) = temp_run_manager();
        seed_repo(&manager, "repo-1");

        manager
            .invalidate_repository("repo-1", None, Some("test"))
            .unwrap();

        // Verify invalidation key is in its own key space
        let invalidate_record = manager
            .persistence
            .find_idempotency_record("invalidate::repo-1::")
            .unwrap();
        assert!(invalidate_record.is_some());
        assert_eq!(invalidate_record.unwrap().operation, "invalidate");

        // Reindex key space should be empty
        let reindex_record = manager
            .persistence
            .find_idempotency_record("reindex::repo-1::")
            .unwrap();
        assert!(reindex_record.is_none());
    }

    #[test]
    fn test_inspect_run_surfaces_repo_invalidation() {
        let (_dir, manager) = temp_run_manager();
        seed_repo(&manager, "repo-1");

        // Create and persist a completed run
        let run = manager.start_run("repo-1", IndexRunMode::Full).unwrap();
        manager
            .persistence
            .update_run_status(&run.run_id, IndexRunStatus::Succeeded, None)
            .unwrap();

        // Before invalidation: no invalidation note
        let report = manager.inspect_run(&run.run_id).unwrap();
        let action = report.action_required.as_deref().unwrap_or("");
        assert!(
            !action.contains("invalidated"),
            "should not mention invalidation before invalidation"
        );

        // Invalidate the repo
        manager
            .invalidate_repository("repo-1", None, Some("test"))
            .unwrap();

        // After invalidation: action_required includes invalidation note
        let report = manager.inspect_run(&run.run_id).unwrap();
        let action = report.action_required.as_deref().unwrap_or("");
        assert!(
            action.contains("invalidated"),
            "action_required should mention invalidation: got '{action}'"
        );
        assert!(
            action.contains("re-index or repair"),
            "action_required should mention recovery path: got '{action}'"
        );
    }

    #[test]
    fn test_list_runs_with_health_surfaces_repo_invalidation() {
        let (_dir, manager) = temp_run_manager();
        seed_repo(&manager, "repo-1");

        let run = manager.start_run("repo-1", IndexRunMode::Full).unwrap();
        manager
            .persistence
            .update_run_status(&run.run_id, IndexRunStatus::Succeeded, None)
            .unwrap();

        manager.invalidate_repository("repo-1", None, None).unwrap();

        let reports = manager.list_runs_with_health(Some("repo-1"), None).unwrap();
        assert_eq!(reports.len(), 1);
        let action = reports[0].action_required.as_deref().unwrap_or("");
        assert!(
            action.contains("invalidated"),
            "list_runs should surface invalidation: got '{action}'"
        );
    }

    #[test]
    fn test_invalidate_reapplies_after_stale_idempotency_record_same_reason() {
        let (_dir, manager) = temp_run_manager();
        seed_repo(&manager, "repo-1");

        // Invalidate with reason A
        let first = manager
            .invalidate_repository("repo-1", None, Some("reason-A"))
            .unwrap();
        assert_eq!(first.previous_status, RepositoryStatus::Ready);

        // Simulate re-index clearing invalidation
        manager
            .persistence
            .update_repository_status("repo-1", RepositoryStatus::Ready, None, None, None, None)
            .unwrap();

        // Re-invalidate with same reason — stale record should be ignored, re-applied
        let second = manager
            .invalidate_repository("repo-1", None, Some("reason-A"))
            .unwrap();
        assert_eq!(second.previous_status, RepositoryStatus::Ready);
        assert_eq!(second.reason.as_deref(), Some("reason-A"));
        assert!(second.invalidated_at_unix_ms > 0);

        let repo = manager
            .persistence
            .get_repository("repo-1")
            .unwrap()
            .unwrap();
        assert_eq!(repo.status, RepositoryStatus::Invalidated);
    }

    #[test]
    fn test_invalidate_reapplies_after_stale_idempotency_record_different_reason() {
        let (_dir, manager) = temp_run_manager();
        seed_repo(&manager, "repo-1");

        // Invalidate with reason A
        manager
            .invalidate_repository("repo-1", None, Some("reason-A"))
            .unwrap();

        // Simulate re-index clearing invalidation
        manager
            .persistence
            .update_repository_status("repo-1", RepositoryStatus::Ready, None, None, None, None)
            .unwrap();

        // Re-invalidate with different reason — stale record should be ignored
        let result = manager
            .invalidate_repository("repo-1", None, Some("reason-B"))
            .unwrap();
        assert_eq!(result.previous_status, RepositoryStatus::Ready);
        assert_eq!(result.reason.as_deref(), Some("reason-B"));

        let repo = manager
            .persistence
            .get_repository("repo-1")
            .unwrap()
            .unwrap();
        assert_eq!(repo.status, RepositoryStatus::Invalidated);
        assert_eq!(repo.invalidation_reason.as_deref(), Some("reason-B"));
    }
}
