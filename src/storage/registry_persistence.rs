use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::domain::{
    AuthorityMode, Checkpoint, FileRecord, IdempotencyRecord, IndexRun, IndexRunStatus,
    RegistryKind, Repository, Workspace,
};
use crate::error::{Result, TokenizorError};

fn default_control_plane_backend() -> String {
    "in_memory".to_string()
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub(crate) struct RegistryData {
    pub schema_version: u32,
    #[serde(default)]
    pub registry_kind: RegistryKind,
    #[serde(default)]
    pub authority_mode: AuthorityMode,
    #[serde(default = "default_control_plane_backend")]
    pub control_plane_backend: String,
    pub repositories: BTreeMap<String, Repository>,
    pub workspaces: BTreeMap<String, Workspace>,
    #[serde(default)]
    pub runs: Vec<IndexRun>,
    #[serde(default)]
    pub idempotency_records: Vec<IdempotencyRecord>,
    #[serde(default)]
    pub run_file_records: BTreeMap<String, Vec<FileRecord>>,
    #[serde(default)]
    pub checkpoints: Vec<Checkpoint>,
}

pub struct RegistryPersistence {
    path: PathBuf,
}

impl RegistryPersistence {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub(crate) fn load(&self) -> Result<RegistryData> {
        load_registry_data(&self.path)
    }

    pub fn save_run(&self, run: &IndexRun) -> Result<()> {
        self.read_modify_write(|data| {
            if let Some(existing) = data.runs.iter_mut().find(|r| r.run_id == run.run_id) {
                *existing = run.clone();
            } else {
                data.runs.push(run.clone());
            }
            Ok(())
        })
    }

    pub fn update_run_status(
        &self,
        run_id: &str,
        status: IndexRunStatus,
        error_summary: Option<String>,
    ) -> Result<()> {
        self.read_modify_write(|data| {
            let run = data
                .runs
                .iter_mut()
                .find(|r| r.run_id == run_id)
                .ok_or_else(|| {
                    TokenizorError::NotFound(format!(
                        "run `{run_id}` not found in registry"
                    ))
                })?;
            run.status = status.clone();
            if error_summary.is_some() {
                run.error_summary = error_summary.clone();
            }
            Ok(())
        })
    }

    pub fn transition_to_running(
        &self,
        run_id: &str,
        started_at_unix_ms: u64,
    ) -> Result<()> {
        self.read_modify_write(|data| {
            let run = data
                .runs
                .iter_mut()
                .find(|r| r.run_id == run_id)
                .ok_or_else(|| {
                    TokenizorError::NotFound(format!(
                        "run `{run_id}` not found in registry"
                    ))
                })?;
            // Skip if already terminal (e.g. cancelled before pipeline started)
            if run.status.is_terminal() {
                return Ok(());
            }
            run.status = IndexRunStatus::Running;
            run.started_at_unix_ms = Some(started_at_unix_ms);
            Ok(())
        })
    }

    pub fn update_run_status_with_finish(
        &self,
        run_id: &str,
        status: IndexRunStatus,
        error_summary: Option<String>,
        finished_at_unix_ms: u64,
        not_yet_supported: Option<BTreeMap<crate::domain::LanguageId, u64>>,
    ) -> Result<()> {
        self.read_modify_write(|data| {
            let run = data
                .runs
                .iter_mut()
                .find(|r| r.run_id == run_id)
                .ok_or_else(|| {
                    TokenizorError::NotFound(format!(
                        "run `{run_id}` not found in registry"
                    ))
                })?;
            run.status = status.clone();
            run.finished_at_unix_ms = Some(finished_at_unix_ms);
            if error_summary.is_some() {
                run.error_summary = error_summary.clone();
            }
            run.not_yet_supported = not_yet_supported.clone();
            Ok(())
        })
    }

    pub fn cancel_run_if_active(&self, run_id: &str, finished_at_unix_ms: u64) -> Result<bool> {
        let mut changed = false;
        self.read_modify_write(|data| {
            let run = data
                .runs
                .iter_mut()
                .find(|r| r.run_id == run_id)
                .ok_or_else(|| {
                    TokenizorError::NotFound(format!(
                        "run `{run_id}` not found in registry"
                    ))
                })?;
            if run.status.is_terminal() {
                return Ok(());
            }
            run.status = IndexRunStatus::Cancelled;
            run.finished_at_unix_ms = Some(finished_at_unix_ms);
            changed = true;
            Ok(())
        })?;
        Ok(changed)
    }

    /// List all persisted runs. Reads without acquiring the advisory lock.
    /// Callers in a concurrent environment should serialize at a higher level
    /// (e.g. `RunManager`'s `Mutex`) to avoid stale-read races.
    pub fn list_runs(&self) -> Result<Vec<IndexRun>> {
        let data = self.load()?;
        Ok(data.runs)
    }

    pub fn find_run(&self, run_id: &str) -> Result<Option<IndexRun>> {
        let data = self.load()?;
        Ok(data.runs.into_iter().find(|r| r.run_id == run_id))
    }

    pub fn find_runs_by_status(&self, status: &IndexRunStatus) -> Result<Vec<IndexRun>> {
        let data = self.load()?;
        Ok(data
            .runs
            .into_iter()
            .filter(|r| &r.status == status)
            .collect())
    }

    pub fn save_idempotency_record(&self, record: &IdempotencyRecord) -> Result<()> {
        self.read_modify_write(|data| {
            if let Some(existing) = data
                .idempotency_records
                .iter_mut()
                .find(|r| r.idempotency_key == record.idempotency_key)
            {
                *existing = record.clone();
            } else {
                data.idempotency_records.push(record.clone());
            }
            Ok(())
        })
    }

    pub fn find_idempotency_record(&self, key: &str) -> Result<Option<IdempotencyRecord>> {
        let data = self.load()?;
        Ok(data
            .idempotency_records
            .into_iter()
            .find(|r| r.idempotency_key == key))
    }

    pub fn save_file_records(&self, run_id: &str, records: &[FileRecord]) -> Result<()> {
        self.read_modify_write(|data| {
            data.run_file_records
                .insert(run_id.to_string(), records.to_vec());
            Ok(())
        })
    }

    pub fn get_file_records(&self, run_id: &str) -> Result<Vec<FileRecord>> {
        let data = self.load()?;
        Ok(data
            .run_file_records
            .get(run_id)
            .cloned()
            .unwrap_or_default())
    }

    pub fn save_checkpoint(&self, checkpoint: &Checkpoint) -> Result<()> {
        self.read_modify_write(|data| {
            let run = data
                .runs
                .iter_mut()
                .find(|r| r.run_id == checkpoint.run_id)
                .ok_or_else(|| {
                    TokenizorError::NotFound(format!(
                        "run `{}` not found in registry",
                        checkpoint.run_id
                    ))
                })?;
            if run.status.is_terminal() {
                return Err(TokenizorError::InvalidOperation(format!(
                    "cannot checkpoint run `{}` with terminal status `{:?}`",
                    checkpoint.run_id, run.status
                )));
            }
            run.checkpoint_cursor = Some(checkpoint.cursor.clone());
            data.checkpoints.push(checkpoint.clone());
            Ok(())
        })
    }

    pub fn get_latest_checkpoint(&self, run_id: &str) -> Result<Option<Checkpoint>> {
        let data = self.load()?;
        Ok(data
            .checkpoints
            .into_iter()
            .filter(|c| c.run_id == run_id)
            .max_by_key(|c| c.created_at_unix_ms))
    }

    fn read_modify_write(&self, modify: impl FnOnce(&mut RegistryData) -> Result<()>) -> Result<()> {
        let _lock = acquire_lock(&self.path)?;

        let mut data = load_registry_data(&self.path)?;
        self.verify_integrity(&data)?;
        modify(&mut data)?;
        save_registry_data(&self.path, &data)?;

        Ok(())
    }

    /// Structural integrity check on the registry data. Verifies schema_version
    /// consistency only. Project/workspace identity verification is the caller's
    /// responsibility — this layer is general-purpose and does not know which
    /// identity to expect.
    fn verify_integrity(&self, data: &RegistryData) -> Result<()> {
        if data.schema_version == 0 && data.repositories.is_empty() && data.workspaces.is_empty() {
            return Ok(());
        }

        if data.schema_version == 0 {
            return Err(TokenizorError::Integrity(format!(
                "registry at `{}` has schema_version 0 but contains data",
                self.path.display()
            )));
        }

        Ok(())
    }
}

fn lock_path(path: &Path) -> PathBuf {
    path.with_extension("persistence.lock")
}

struct PersistenceLock {
    _file: File,
}

fn acquire_lock(registry_path: &Path) -> Result<PersistenceLock> {
    let lock_file_path = lock_path(registry_path);
    if let Some(parent) = lock_file_path.parent() {
        fs::create_dir_all(parent).map_err(|error| TokenizorError::io(parent, error))?;
    }

    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_file_path)
        .map_err(|error| TokenizorError::io(&lock_file_path, error))?;

    file.lock_exclusive()
        .map_err(|error| TokenizorError::io(&lock_file_path, error))?;

    Ok(PersistenceLock { _file: file })
}

fn load_registry_data(path: &Path) -> Result<RegistryData> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| {
            TokenizorError::Serialization(format!(
                "failed to deserialize registry `{}`: {error}",
                path.display()
            ))
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(RegistryData::default())
        }
        Err(error) => Err(TokenizorError::io(path, error)),
    }
}

fn save_registry_data(path: &Path, data: &RegistryData) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        TokenizorError::Storage(format!(
            "registry path `{}` is missing a parent directory",
            path.display()
        ))
    })?;

    fs::create_dir_all(parent).map_err(|error| TokenizorError::io(parent, error))?;

    let bytes = serde_json::to_vec_pretty(data).map_err(|error| {
        TokenizorError::Serialization(format!(
            "failed to serialize registry `{}`: {error}",
            path.display()
        ))
    })?;

    let temp_path = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("registry.json"),
        crate::domain::unix_timestamp_ms()
    ));

    let mut file =
        File::create(&temp_path).map_err(|error| TokenizorError::io(&temp_path, error))?;
    file.write_all(&bytes)
        .map_err(|error| TokenizorError::io(&temp_path, error))?;
    file.sync_all()
        .map_err(|error| TokenizorError::io(&temp_path, error))?;
    drop(file);

    atomic_replace(&temp_path, path)?;
    sync_parent_dir(parent)?;
    Ok(())
}

fn atomic_replace(source: &Path, destination: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        atomic_replace_windows(source, destination)
    }

    #[cfg(not(windows))]
    {
        fs::rename(source, destination).map_err(|error| TokenizorError::io(destination, error))
    }
}

#[cfg(windows)]
fn atomic_replace_windows(source: &Path, destination: &Path) -> Result<()> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    type Bool = i32;
    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    unsafe extern "system" {
        fn MoveFileExW(
            lpExistingFileName: *const u16,
            lpNewFileName: *const u16,
            dwFlags: u32,
        ) -> Bool;
    }

    fn wide(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain(std::iter::once(0)).collect()
    }

    let source_wide = wide(source.as_os_str());
    let destination_wide = wide(destination.as_os_str());
    // SAFETY: Both `source_wide` and `destination_wide` are null-terminated
    // UTF-16 slices produced by `OsStr::encode_wide` with an appended NUL.
    // The pointers remain valid for the duration of the FFI call because the
    // owning `Vec<u16>` values live until after `MoveFileExW` returns.
    let result = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };

    if result != 0 {
        Ok(())
    } else {
        Err(TokenizorError::Storage(format!(
            "failed to atomically replace registry `{}` with `{}`",
            destination.display(),
            source.display()
        )))
    }
}

fn sync_parent_dir(path: &Path) -> Result<()> {
    #[cfg(not(windows))]
    {
        let dir = File::open(path).map_err(|error| TokenizorError::io(path, error))?;
        dir.sync_all()
            .map_err(|error| TokenizorError::io(path, error))?;
    }

    #[cfg(windows)]
    {
        let _ = path;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{IndexRunMode, IdempotencyStatus};
    use std::fs;

    fn temp_registry() -> (tempfile::TempDir, RegistryPersistence) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("registry.json");
        let persistence = RegistryPersistence::new(path);
        (dir, persistence)
    }

    fn sample_run(run_id: &str, repo_id: &str, status: IndexRunStatus) -> IndexRun {
        IndexRun {
            run_id: run_id.to_string(),
            repo_id: repo_id.to_string(),
            mode: IndexRunMode::Full,
            status,
            requested_at_unix_ms: 1000,
            started_at_unix_ms: None,
            finished_at_unix_ms: None,
            idempotency_key: None,
            request_hash: None,
            checkpoint_cursor: None,
            error_summary: None,
            not_yet_supported: None,
        }
    }

    fn sample_idempotency_record(key: &str, hash: &str) -> IdempotencyRecord {
        IdempotencyRecord {
            operation: "index".to_string(),
            idempotency_key: key.to_string(),
            request_hash: hash.to_string(),
            status: IdempotencyStatus::Succeeded,
            result_ref: Some("run-123".to_string()),
            created_at_unix_ms: 1000,
            expires_at_unix_ms: None,
        }
    }

    #[test]
    fn test_load_returns_default_when_file_missing() {
        let (_dir, persistence) = temp_registry();
        let data = persistence.load().unwrap();
        assert!(data.repositories.is_empty());
        assert!(data.workspaces.is_empty());
        assert!(data.runs.is_empty());
        assert!(data.idempotency_records.is_empty());
    }

    #[test]
    fn test_save_run_creates_registry_file() {
        let (_dir, persistence) = temp_registry();
        let run = sample_run("run-1", "repo-1", IndexRunStatus::Queued);
        persistence.save_run(&run).unwrap();

        let runs = persistence.list_runs().unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].run_id, "run-1");
        assert_eq!(runs[0].status, IndexRunStatus::Queued);
    }

    #[test]
    fn test_save_run_updates_existing_run() {
        let (_dir, persistence) = temp_registry();
        let run = sample_run("run-1", "repo-1", IndexRunStatus::Queued);
        persistence.save_run(&run).unwrap();

        let mut updated = run.clone();
        updated.status = IndexRunStatus::Running;
        updated.started_at_unix_ms = Some(2000);
        persistence.save_run(&updated).unwrap();

        let runs = persistence.list_runs().unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, IndexRunStatus::Running);
        assert_eq!(runs[0].started_at_unix_ms, Some(2000));
    }

    #[test]
    fn test_update_run_status() {
        let (_dir, persistence) = temp_registry();
        let run = sample_run("run-1", "repo-1", IndexRunStatus::Running);
        persistence.save_run(&run).unwrap();

        persistence
            .update_run_status(
                "run-1",
                IndexRunStatus::Interrupted,
                Some("process exited".to_string()),
            )
            .unwrap();

        let found = persistence.find_run("run-1").unwrap().unwrap();
        assert_eq!(found.status, IndexRunStatus::Interrupted);
        assert_eq!(found.error_summary, Some("process exited".to_string()));
    }

    #[test]
    fn test_update_run_status_returns_not_found_for_missing_run() {
        let (_dir, persistence) = temp_registry();
        let result = persistence.update_run_status(
            "nonexistent",
            IndexRunStatus::Interrupted,
            Some("should fail".to_string()),
        );
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not found"));
        assert!(err.contains("nonexistent"));
    }

    #[test]
    fn test_find_run_returns_none_for_missing() {
        let (_dir, persistence) = temp_registry();
        assert!(persistence.find_run("nonexistent").unwrap().is_none());
    }

    #[test]
    fn test_find_runs_by_status() {
        let (_dir, persistence) = temp_registry();
        persistence
            .save_run(&sample_run("run-1", "repo-1", IndexRunStatus::Running))
            .unwrap();
        persistence
            .save_run(&sample_run("run-2", "repo-2", IndexRunStatus::Queued))
            .unwrap();
        persistence
            .save_run(&sample_run("run-3", "repo-3", IndexRunStatus::Running))
            .unwrap();

        let running = persistence
            .find_runs_by_status(&IndexRunStatus::Running)
            .unwrap();
        assert_eq!(running.len(), 2);
        assert!(running.iter().all(|r| r.status == IndexRunStatus::Running));
    }

    #[test]
    fn test_save_idempotency_record_roundtrip() {
        let (_dir, persistence) = temp_registry();
        let record = sample_idempotency_record("key-1", "hash-abc");
        persistence.save_idempotency_record(&record).unwrap();

        let found = persistence
            .find_idempotency_record("key-1")
            .unwrap()
            .unwrap();
        assert_eq!(found.request_hash, "hash-abc");
        assert_eq!(found.result_ref, Some("run-123".to_string()));
    }

    #[test]
    fn test_save_idempotency_record_updates_existing() {
        let (_dir, persistence) = temp_registry();
        persistence
            .save_idempotency_record(&sample_idempotency_record("key-1", "hash-abc"))
            .unwrap();

        let mut updated = sample_idempotency_record("key-1", "hash-xyz");
        updated.status = IdempotencyStatus::Failed;
        persistence.save_idempotency_record(&updated).unwrap();

        let found = persistence
            .find_idempotency_record("key-1")
            .unwrap()
            .unwrap();
        assert_eq!(found.request_hash, "hash-xyz");
        assert_eq!(found.status, IdempotencyStatus::Failed);
    }

    #[test]
    fn test_find_idempotency_record_returns_none_for_missing() {
        let (_dir, persistence) = temp_registry();
        assert!(persistence
            .find_idempotency_record("nonexistent")
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_roundtrip_preserves_project_workspace_data() {
        let (_dir, persistence) = temp_registry();

        let mut data = RegistryData {
            schema_version: 2,
            registry_kind: RegistryKind::LocalBootstrapProjectWorkspace,
            authority_mode: AuthorityMode::LocalBootstrapOnly,
            control_plane_backend: "in_memory".to_string(),
            ..RegistryData::default()
        };
        data.repositories.insert(
            "proj-1".to_string(),
            Repository {
                repo_id: "proj-1".to_string(),
                kind: crate::domain::RepositoryKind::Git,
                root_uri: "/tmp/test".to_string(),
                project_identity: "identity-1".to_string(),
                project_identity_kind: crate::domain::ProjectIdentityKind::GitCommonDir,
                default_branch: None,
                last_known_revision: None,
                status: crate::domain::RepositoryStatus::Ready,
            },
        );

        save_registry_data(&persistence.path, &data).unwrap();

        let run = sample_run("run-1", "proj-1", IndexRunStatus::Queued);
        persistence.save_run(&run).unwrap();

        let loaded = persistence.load().unwrap();
        assert_eq!(loaded.schema_version, 2);
        assert_eq!(loaded.repositories.len(), 1);
        assert!(loaded.repositories.contains_key("proj-1"));
        assert_eq!(loaded.runs.len(), 1);
    }

    #[test]
    fn test_backward_compatible_deserialization_of_epic1_registry() {
        let (_dir, persistence) = temp_registry();

        let epic1_json = serde_json::json!({
            "schema_version": 2,
            "registry_kind": "local_bootstrap_project_workspace",
            "authority_mode": "local_bootstrap_only",
            "control_plane_backend": "in_memory",
            "repositories": {
                "proj-1": {
                    "repo_id": "proj-1",
                    "kind": "git",
                    "root_uri": "/tmp/test",
                    "project_identity": "identity-1",
                    "project_identity_kind": "git_common_dir",
                    "default_branch": null,
                    "last_known_revision": null,
                    "status": "ready"
                }
            },
            "workspaces": {
                "ws-1": {
                    "workspace_id": "ws-1",
                    "repo_id": "proj-1",
                    "root_uri": "/tmp/test",
                    "status": "active"
                }
            }
        });

        fs::write(
            &persistence.path,
            serde_json::to_vec_pretty(&epic1_json).unwrap(),
        )
        .unwrap();

        let data = persistence.load().unwrap();
        assert_eq!(data.schema_version, 2);
        assert_eq!(data.repositories.len(), 1);
        assert_eq!(data.workspaces.len(), 1);
        assert!(data.runs.is_empty());
        assert!(data.idempotency_records.is_empty());
    }

    #[test]
    fn test_backward_compatible_deserialization_of_epic1_fixture() {
        let fixture = include_str!("../../tests/fixtures/epic1-registry.json");
        let data: RegistryData = serde_json::from_str(fixture).unwrap();
        assert_eq!(data.schema_version, 2);
        assert!(!data.repositories.is_empty());
        assert!(!data.workspaces.is_empty());
        assert!(data.runs.is_empty());
        assert!(data.idempotency_records.is_empty());
    }

    #[test]
    fn test_registry_survives_process_restart_simulation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("registry.json");

        {
            let persistence = RegistryPersistence::new(path.clone());
            let run = sample_run("run-1", "repo-1", IndexRunStatus::Running);
            persistence.save_run(&run).unwrap();
        }

        {
            let persistence = RegistryPersistence::new(path);
            let run = persistence.find_run("run-1").unwrap().unwrap();
            assert_eq!(run.run_id, "run-1");
            assert_eq!(run.status, IndexRunStatus::Running);
            assert_eq!(run.repo_id, "repo-1");
        }
    }

    #[test]
    fn test_integrity_check_rejects_corrupt_schema() {
        let (_dir, persistence) = temp_registry();

        let mut data = RegistryData::default();
        data.schema_version = 0;
        data.repositories.insert(
            "proj-1".to_string(),
            Repository {
                repo_id: "proj-1".to_string(),
                kind: crate::domain::RepositoryKind::Git,
                root_uri: "/tmp".to_string(),
                project_identity: String::new(),
                project_identity_kind: crate::domain::ProjectIdentityKind::LegacyRootUri,
                default_branch: None,
                last_known_revision: None,
                status: crate::domain::RepositoryStatus::Ready,
            },
        );
        save_registry_data(&persistence.path, &data).unwrap();

        let result = persistence.save_run(&sample_run("run-1", "repo-1", IndexRunStatus::Queued));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("schema_version 0"));
    }

    #[test]
    fn test_concurrent_writes_do_not_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("registry.json");

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
        let threads: Vec<_> = (0..4)
            .map(|i| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let persistence = RegistryPersistence::new(path);
                    barrier.wait();
                    let run = sample_run(
                        &format!("run-{i}"),
                        &format!("repo-{i}"),
                        IndexRunStatus::Queued,
                    );
                    persistence.save_run(&run).unwrap();
                })
            })
            .collect();

        for thread in threads {
            thread.join().unwrap();
        }

        let persistence = RegistryPersistence::new(path);
        let runs = persistence.list_runs().unwrap();
        assert_eq!(runs.len(), 4);
    }

    fn sample_file_record(path: &str, run_id: &str) -> FileRecord {
        use crate::domain::{LanguageId, PersistedFileOutcome, SymbolKind, SymbolRecord};
        FileRecord {
            relative_path: path.to_string(),
            language: LanguageId::Rust,
            blob_id: "deadbeef".to_string(),
            byte_len: 100,
            content_hash: "deadbeef".to_string(),
            outcome: PersistedFileOutcome::Committed,
            symbols: vec![SymbolRecord {
                name: "main".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 50),
                line_range: (1, 3),
            }],
            run_id: run_id.to_string(),
            repo_id: "repo-1".to_string(),
            committed_at_unix_ms: 1700000000000,
        }
    }

    #[test]
    fn test_save_file_records_roundtrip() {
        let (_dir, persistence) = temp_registry();
        let records = vec![
            sample_file_record("src/main.rs", "run-1"),
            sample_file_record("src/lib.rs", "run-1"),
        ];

        persistence.save_file_records("run-1", &records).unwrap();
        let loaded = persistence.get_file_records("run-1").unwrap();

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded, records);
    }

    #[test]
    fn test_get_file_records_returns_empty_for_missing_run() {
        let (_dir, persistence) = temp_registry();
        let loaded = persistence.get_file_records("nonexistent-run").unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn test_backward_compat_deserialization_missing_run_file_records() {
        let (_dir, persistence) = temp_registry();
        let run = sample_run("run-1", "repo-1", IndexRunStatus::Queued);
        persistence.save_run(&run).unwrap();

        let data = persistence.load().unwrap();
        assert!(data.run_file_records.is_empty());
    }

    #[test]
    fn test_save_file_records_does_not_clobber_existing_runs() {
        let (_dir, persistence) = temp_registry();
        let run = sample_run("run-1", "repo-1", IndexRunStatus::Running);
        persistence.save_run(&run).unwrap();

        let records = vec![sample_file_record("src/main.rs", "run-1")];
        persistence.save_file_records("run-1", &records).unwrap();

        let loaded_run = persistence.find_run("run-1").unwrap();
        assert!(loaded_run.is_some());
        let loaded_records = persistence.get_file_records("run-1").unwrap();
        assert_eq!(loaded_records.len(), 1);
    }

    #[test]
    fn test_epic1_fixture_backward_compat_with_run_file_records() {
        let fixture = include_str!("../../tests/fixtures/epic1-registry.json");
        let data: RegistryData = serde_json::from_str(fixture).unwrap();
        assert!(data.run_file_records.is_empty());
    }

    #[test]
    fn test_cancel_run_if_active_transitions_running_to_cancelled() {
        let (_dir, persistence) = temp_registry();
        let run = sample_run("run-1", "repo-1", IndexRunStatus::Running);
        persistence.save_run(&run).unwrap();

        let changed = persistence.cancel_run_if_active("run-1", 5000).unwrap();
        assert!(changed);

        let updated = persistence.find_run("run-1").unwrap().unwrap();
        assert_eq!(updated.status, IndexRunStatus::Cancelled);
        assert_eq!(updated.finished_at_unix_ms, Some(5000));
    }

    #[test]
    fn test_cancel_run_if_active_returns_false_for_terminal_run() {
        let (_dir, persistence) = temp_registry();
        let run = sample_run("run-1", "repo-1", IndexRunStatus::Succeeded);
        persistence.save_run(&run).unwrap();

        let changed = persistence.cancel_run_if_active("run-1", 5000).unwrap();
        assert!(!changed);

        let updated = persistence.find_run("run-1").unwrap().unwrap();
        assert_eq!(updated.status, IndexRunStatus::Succeeded);
        assert_eq!(updated.finished_at_unix_ms, None);
    }

    #[test]
    fn test_cancel_run_if_active_returns_not_found_for_missing_run() {
        let (_dir, persistence) = temp_registry();

        let result = persistence.cancel_run_if_active("nonexistent", 5000);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, crate::error::TokenizorError::NotFound(_)));
    }

    #[test]
    fn test_backward_compat_deserialization_missing_checkpoints() {
        let (_dir, persistence) = temp_registry();
        let run = sample_run("run-1", "repo-1", IndexRunStatus::Queued);
        persistence.save_run(&run).unwrap();

        // Reload — registry was saved with checkpoints field (as empty vec via Default)
        // but let's also test raw JSON without the field
        let json_without_checkpoints = serde_json::json!({
            "schema_version": 2,
            "registry_kind": "local_bootstrap_project_workspace",
            "authority_mode": "local_bootstrap_only",
            "control_plane_backend": "in_memory",
            "repositories": {},
            "workspaces": {},
            "runs": [],
            "idempotency_records": [],
            "run_file_records": {}
        });
        fs::write(
            &persistence.path,
            serde_json::to_vec_pretty(&json_without_checkpoints).unwrap(),
        )
        .unwrap();

        let data = persistence.load().unwrap();
        assert!(data.checkpoints.is_empty());
    }

    #[test]
    fn test_epic1_fixture_backward_compat_with_checkpoints() {
        let fixture = include_str!("../../tests/fixtures/epic1-registry.json");
        let data: RegistryData = serde_json::from_str(fixture).unwrap();
        assert!(data.checkpoints.is_empty());
    }

    fn sample_checkpoint(run_id: &str, cursor: &str, created_at: u64) -> Checkpoint {
        Checkpoint {
            run_id: run_id.to_string(),
            cursor: cursor.to_string(),
            files_processed: 10,
            symbols_written: 50,
            created_at_unix_ms: created_at,
        }
    }

    #[test]
    fn test_save_checkpoint_persists_and_updates_run_cursor() {
        let (_dir, persistence) = temp_registry();
        let run = sample_run("run-1", "repo-1", IndexRunStatus::Running);
        persistence.save_run(&run).unwrap();

        let checkpoint = sample_checkpoint("run-1", "src/main.rs", 2000);
        persistence.save_checkpoint(&checkpoint).unwrap();

        // Verify checkpoint persisted
        let data = persistence.load().unwrap();
        assert_eq!(data.checkpoints.len(), 1);
        assert_eq!(data.checkpoints[0].cursor, "src/main.rs");
        assert_eq!(data.checkpoints[0].run_id, "run-1");

        // Verify run's checkpoint_cursor was updated
        let loaded_run = persistence.find_run("run-1").unwrap().unwrap();
        assert_eq!(loaded_run.checkpoint_cursor, Some("src/main.rs".to_string()));
    }

    #[test]
    fn test_save_checkpoint_rejects_terminal_run() {
        let (_dir, persistence) = temp_registry();
        let run = sample_run("run-1", "repo-1", IndexRunStatus::Succeeded);
        persistence.save_run(&run).unwrap();

        let checkpoint = sample_checkpoint("run-1", "src/main.rs", 2000);
        let result = persistence.save_checkpoint(&checkpoint);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::InvalidOperation(_)));

        // Verify no orphan checkpoint was created
        let data = persistence.load().unwrap();
        assert!(data.checkpoints.is_empty());
    }

    #[test]
    fn test_save_checkpoint_rejects_missing_run() {
        let (_dir, persistence) = temp_registry();

        let checkpoint = sample_checkpoint("nonexistent", "src/main.rs", 2000);
        let result = persistence.save_checkpoint(&checkpoint);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TokenizorError::NotFound(_)));
    }

    #[test]
    fn test_get_latest_checkpoint_returns_most_recent() {
        let (_dir, persistence) = temp_registry();
        let run = sample_run("run-1", "repo-1", IndexRunStatus::Running);
        persistence.save_run(&run).unwrap();

        persistence
            .save_checkpoint(&sample_checkpoint("run-1", "src/a.rs", 1000))
            .unwrap();
        persistence
            .save_checkpoint(&sample_checkpoint("run-1", "src/b.rs", 2000))
            .unwrap();
        persistence
            .save_checkpoint(&sample_checkpoint("run-1", "src/c.rs", 3000))
            .unwrap();

        let latest = persistence
            .get_latest_checkpoint("run-1")
            .unwrap()
            .unwrap();
        assert_eq!(latest.cursor, "src/c.rs");
        assert_eq!(latest.created_at_unix_ms, 3000);
    }

    #[test]
    fn test_get_latest_checkpoint_returns_none_for_no_checkpoints() {
        let (_dir, persistence) = temp_registry();
        let run = sample_run("run-1", "repo-1", IndexRunStatus::Running);
        persistence.save_run(&run).unwrap();

        let latest = persistence.get_latest_checkpoint("run-1").unwrap();
        assert!(latest.is_none());
    }
}
