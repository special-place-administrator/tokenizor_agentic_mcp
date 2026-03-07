use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::domain::{
    IdempotencyRecord, IdempotencyStatus, IndexRun, IndexRunMode, IndexRunStatus,
    unix_timestamp_ms,
};
use crate::storage::BlobStore;
use crate::error::{Result, TokenizorError};
use crate::indexing::pipeline::{IndexingPipeline, PipelineProgress};
use crate::storage::RegistryPersistence;
use crate::storage::digest_hex;

pub struct ActiveRun {
    pub handle: JoinHandle<()>,
    pub cancellation_token: CancellationToken,
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

    pub fn start_run_idempotent(
        &self,
        repo_id: &str,
        workspace_id: &str,
        mode: IndexRunMode,
    ) -> Result<IdempotentRunResult> {
        let idempotency_key = format!("index::{repo_id}::{workspace_id}");
        let request_hash = compute_request_hash(repo_id, workspace_id, &mode);

        if let Some(existing) = self.persistence.find_idempotency_record(&idempotency_key)? {
            if existing.request_hash == request_hash {
                info!(
                    idempotency_key = %idempotency_key,
                    "idempotent replay detected, returning stored result"
                );
                return Ok(IdempotentRunResult::ExistingRun {
                    run_id: existing.result_ref.unwrap_or_default(),
                });
            } else {
                return Err(TokenizorError::InvalidArgument(format!(
                    "conflicting replay for idempotency key `{idempotency_key}`: \
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

    pub fn launch_run(
        self: &Arc<Self>,
        repo_id: &str,
        mode: IndexRunMode,
        repo_root: PathBuf,
        blob_store: Arc<dyn BlobStore>,
    ) -> Result<(IndexRun, Arc<PipelineProgress>)> {
        let run = self.start_run(repo_id, mode)?;
        let run_id = run.run_id.clone();
        let repo_id_owned = repo_id.to_string();

        let pipeline = IndexingPipeline::new(run_id.clone(), repo_root)
            .with_cas(blob_store, repo_id_owned.clone());
        let progress = pipeline.progress();

        let manager = Arc::clone(self);
        let token = CancellationToken::new();

        let handle = tokio::spawn(async move {
            // Transition to Running with start timestamp
            if let Err(e) = manager.persistence.transition_to_running(
                &run_id,
                unix_timestamp_ms(),
            ) {
                error!(run_id = %run_id, error = %e, "failed to transition to Running");
                return;
            }

            let result = pipeline.execute().await;

            // Batch-save file records to registry
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
                } else {
                    info!(
                        run_id = %run_id,
                        records = record_count,
                        "file records saved to registry"
                    );
                }
            }

            let finished_at = unix_timestamp_ms();
            if let Err(e) = manager.persistence.update_run_status_with_finish(
                &run_id,
                result.status.clone(),
                result.error_summary,
                finished_at,
            ) {
                error!(run_id = %run_id, error = %e, "failed to update final run status");
            }

            // Deregister active run
            manager.deregister_active_run(&repo_id_owned);
        });

        self.register_active_run(
            repo_id,
            ActiveRun {
                handle,
                cancellation_token: token,
            },
        );

        Ok((run, progress))
    }

    pub fn deregister_active_run(&self, repo_id: &str) {
        let mut active_runs = self.active_runs.lock().unwrap_or_else(|e| e.into_inner());
        active_runs.remove(repo_id);
    }

    pub fn persistence(&self) -> &RegistryPersistence {
        &self.persistence
    }
}

#[derive(Debug)]
pub enum IdempotentRunResult {
    NewRun { run: IndexRun },
    ExistingRun { run_id: String },
}

fn compute_request_hash(repo_id: &str, workspace_id: &str, mode: &IndexRunMode) -> String {
    let mode_str = match mode {
        IndexRunMode::Full => "full",
        IndexRunMode::Incremental => "incremental",
        IndexRunMode::Repair => "repair",
        IndexRunMode::Verify => "verify",
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
    };
    let input = format!("{repo_id}:{mode_str}:{requested_at_unix_ms}");
    digest_hex(input.as_bytes())
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
            };
            persistence.save_run(&run).unwrap();
        }

        let persistence = RegistryPersistence::new(path);
        let manager = RunManager::new(persistence);
        let transitioned = manager.startup_sweep().unwrap();

        assert_eq!(transitioned, vec!["stale-run".to_string()]);
        let run = manager
            .persistence
            .find_run("stale-run")
            .unwrap()
            .unwrap();
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
                handle,
                cancellation_token: token,
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
        let err = result.unwrap_err().to_string();
        assert!(err.contains("conflicting replay"));
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
}
