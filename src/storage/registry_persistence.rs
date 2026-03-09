use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::domain::{
    AuthorityMode, Checkpoint, DiscoveryManifest, FileRecord, IdempotencyRecord, IndexRun,
    IndexRunStatus, RegistryKind, Repository, Workspace,
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
    #[serde(default)]
    pub discovery_manifests: BTreeMap<String, DiscoveryManifest>,
}

pub struct RegistryPersistence {
    path: PathBuf,
}

pub trait RegistryQuery: Send + Sync {
    fn get_repository(&self, repo_id: &str) -> Result<Option<Repository>>;
    fn get_runs_by_repo(&self, repo_id: &str) -> Result<Vec<IndexRun>>;
    fn get_latest_completed_run(&self, repo_id: &str) -> Result<Option<IndexRun>>;
    fn get_file_records(&self, run_id: &str) -> Result<Vec<FileRecord>>;
}

impl RegistryPersistence {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn load(&self) -> Result<RegistryData> {
        load_registry_data(&self.path)
    }

    pub(crate) fn clear_mutable_state(&self) -> Result<()> {
        self.read_modify_write(|data| {
            data.runs.clear();
            data.idempotency_records.clear();
            data.run_file_records.clear();
            data.checkpoints.clear();
            data.discovery_manifests.clear();
            Ok(())
        })
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
                    TokenizorError::NotFound(format!("run `{run_id}` not found in registry"))
                })?;
            run.status = status.clone();
            run.error_summary = error_summary.clone();
            Ok(())
        })
    }

    pub fn transition_to_running(&self, run_id: &str, started_at_unix_ms: u64) -> Result<()> {
        self.read_modify_write(|data| {
            let run = data
                .runs
                .iter_mut()
                .find(|r| r.run_id == run_id)
                .ok_or_else(|| {
                    TokenizorError::NotFound(format!("run `{run_id}` not found in registry"))
                })?;
            // Skip terminal runs unless they are being explicitly resumed from Interrupted.
            if run.status.is_terminal() && run.status != IndexRunStatus::Interrupted {
                return Ok(());
            }
            run.status = IndexRunStatus::Running;
            if run.started_at_unix_ms.is_none() {
                run.started_at_unix_ms = Some(started_at_unix_ms);
            }
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
                    TokenizorError::NotFound(format!("run `{run_id}` not found in registry"))
                })?;
            run.status = status.clone();
            run.finished_at_unix_ms = Some(finished_at_unix_ms);
            run.error_summary = error_summary.clone();
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
                    TokenizorError::NotFound(format!("run `{run_id}` not found in registry"))
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

    pub fn get_repository(&self, repo_id: &str) -> Result<Option<Repository>> {
        let data = self.load()?;
        Ok(data.repositories.get(repo_id).cloned())
    }

    pub fn save_repository(&self, repo: &Repository) -> Result<()> {
        self.read_modify_write(|data| {
            if data.schema_version == 0 {
                data.schema_version = 2;
            }
            data.repositories.insert(repo.repo_id.clone(), repo.clone());
            Ok(())
        })
    }

    pub fn update_repository_status(
        &self,
        repo_id: &str,
        status: crate::domain::RepositoryStatus,
        invalidated_at_unix_ms: Option<u64>,
        invalidation_reason: Option<String>,
        quarantined_at_unix_ms: Option<u64>,
        quarantine_reason: Option<String>,
    ) -> Result<()> {
        self.read_modify_write(|data| {
            let repo = data.repositories.get_mut(repo_id).ok_or_else(|| {
                TokenizorError::NotFound(format!("repository not found: {repo_id}"))
            })?;
            repo.status = status;
            repo.invalidated_at_unix_ms = invalidated_at_unix_ms;
            repo.invalidation_reason = invalidation_reason;
            repo.quarantined_at_unix_ms = quarantined_at_unix_ms;
            repo.quarantine_reason = quarantine_reason;
            Ok(())
        })
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

    pub fn get_latest_completed_run(&self, repo_id: &str) -> Result<Option<IndexRun>> {
        let data = self.load()?;
        Ok(data
            .runs
            .into_iter()
            .filter(|r| r.repo_id == repo_id && r.status == IndexRunStatus::Succeeded)
            .max_by_key(|r| r.requested_at_unix_ms))
    }

    pub fn get_runs_by_repo(&self, repo_id: &str) -> Result<Vec<IndexRun>> {
        let data = self.load()?;
        let mut runs: Vec<IndexRun> = data
            .runs
            .into_iter()
            .filter(|r| r.repo_id == repo_id)
            .collect();
        runs.sort_by(|a, b| b.requested_at_unix_ms.cmp(&a.requested_at_unix_ms));
        Ok(runs)
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
            let existing = data
                .run_file_records
                .entry(run_id.to_string())
                .or_insert_with(Vec::new);

            let mut merged = BTreeMap::new();
            for record in existing.iter().cloned() {
                merged.insert(record.relative_path.clone(), record);
            }
            for record in records.iter().cloned() {
                merged.insert(record.relative_path.clone(), record);
            }

            let mut merged_records: Vec<FileRecord> = merged.into_values().collect();
            merged_records.sort_by(|left, right| {
                left.relative_path
                    .to_lowercase()
                    .cmp(&right.relative_path.to_lowercase())
                    .then_with(|| left.relative_path.cmp(&right.relative_path))
            });
            *existing = merged_records;
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

    pub fn save_discovery_manifest(&self, manifest: &DiscoveryManifest) -> Result<()> {
        self.read_modify_write(|data| {
            data.discovery_manifests
                .insert(manifest.run_id.clone(), manifest.clone());
            Ok(())
        })
    }

    pub fn get_discovery_manifest(&self, run_id: &str) -> Result<Option<DiscoveryManifest>> {
        let data = self.load()?;
        Ok(data.discovery_manifests.get(run_id).cloned())
    }

    fn read_modify_write(
        &self,
        modify: impl FnOnce(&mut RegistryData) -> Result<()>,
    ) -> Result<()> {
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

impl RegistryQuery for RegistryPersistence {
    fn get_repository(&self, repo_id: &str) -> Result<Option<Repository>> {
        RegistryPersistence::get_repository(self, repo_id)
    }

    fn get_runs_by_repo(&self, repo_id: &str) -> Result<Vec<IndexRun>> {
        RegistryPersistence::get_runs_by_repo(self, repo_id)
    }

    fn get_latest_completed_run(&self, repo_id: &str) -> Result<Option<IndexRun>> {
        RegistryPersistence::get_latest_completed_run(self, repo_id)
    }

    fn get_file_records(&self, run_id: &str) -> Result<Vec<FileRecord>> {
        RegistryPersistence::get_file_records(self, run_id)
    }
}

fn lock_path(path: &Path) -> PathBuf {
    path.with_extension("persistence.lock")
}

pub(crate) fn is_owned_registry_temp_artifact_path(registry_path: &Path, candidate: &Path) -> bool {
    if candidate.parent() != registry_path.parent() {
        return false;
    }

    let registry_name = registry_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "registry.json".to_string());
    let Some(candidate_name) = candidate
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
    else {
        return false;
    };

    let prefix = format!(".{registry_name}.");
    if !candidate_name.starts_with(&prefix) || !candidate_name.ends_with(".tmp") {
        return false;
    }

    let middle = &candidate_name[prefix.len()..candidate_name.len() - ".tmp".len()];
    !middle.is_empty() && middle.chars().all(|ch| ch.is_ascii_digit())
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
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(RegistryData::default()),
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
    use crate::domain::{IdempotencyStatus, IndexRunMode};
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
            prior_run_id: None,
            description: None,
            recovery_state: None,
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
        assert!(
            persistence
                .find_idempotency_record("nonexistent")
                .unwrap()
                .is_none()
        );
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
                invalidated_at_unix_ms: None,
                invalidation_reason: None,
                quarantined_at_unix_ms: None,
                quarantine_reason: None,
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
                invalidated_at_unix_ms: None,
                invalidation_reason: None,
                quarantined_at_unix_ms: None,
                quarantine_reason: None,
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
            sample_file_record("src/lib.rs", "run-1"),
            sample_file_record("src/main.rs", "run-1"),
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
    fn test_save_file_records_upserts_by_relative_path() {
        let (_dir, persistence) = temp_registry();

        let original = sample_file_record("src/main.rs", "run-1");
        persistence
            .save_file_records("run-1", std::slice::from_ref(&original))
            .unwrap();

        let mut updated = sample_file_record("src/main.rs", "run-1");
        updated.byte_len = 200;
        updated.content_hash = "updated".to_string();
        persistence
            .save_file_records("run-1", std::slice::from_ref(&updated))
            .unwrap();

        let loaded = persistence.get_file_records("run-1").unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].byte_len, 200);
        assert_eq!(loaded[0].content_hash, "updated");
    }

    #[test]
    fn test_transition_to_running_preserves_original_started_at() {
        let (_dir, persistence) = temp_registry();
        let mut run = sample_run("run-1", "repo-1", IndexRunStatus::Interrupted);
        run.started_at_unix_ms = Some(1111);
        persistence.save_run(&run).unwrap();

        persistence.transition_to_running("run-1", 2222).unwrap();

        let loaded = persistence.find_run("run-1").unwrap().unwrap();
        assert_eq!(loaded.status, IndexRunStatus::Running);
        assert_eq!(loaded.started_at_unix_ms, Some(1111));
    }

    #[test]
    fn test_update_run_status_with_finish_clears_prior_error_summary() {
        let (_dir, persistence) = temp_registry();
        let run = sample_run("run-1", "repo-1", IndexRunStatus::Interrupted);
        persistence.save_run(&run).unwrap();
        persistence
            .update_run_status(
                "run-1",
                IndexRunStatus::Interrupted,
                Some("stale run detected during startup sweep".to_string()),
            )
            .unwrap();

        persistence
            .update_run_status_with_finish("run-1", IndexRunStatus::Succeeded, None, 2000, None)
            .unwrap();

        let loaded = persistence.find_run("run-1").unwrap().unwrap();
        assert_eq!(loaded.status, IndexRunStatus::Succeeded);
        assert_eq!(loaded.error_summary, None);
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
        assert_eq!(
            loaded_run.checkpoint_cursor,
            Some("src/main.rs".to_string())
        );
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

        let latest = persistence.get_latest_checkpoint("run-1").unwrap().unwrap();
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

    #[test]
    fn test_get_latest_completed_run_returns_succeeded_run() {
        let (_dir, persistence) = temp_registry();
        persistence
            .save_run(&sample_run("run-1", "repo-1", IndexRunStatus::Running))
            .unwrap();
        let mut succeeded = sample_run("run-2", "repo-1", IndexRunStatus::Succeeded);
        succeeded.requested_at_unix_ms = 2000;
        persistence.save_run(&succeeded).unwrap();
        persistence
            .save_run(&sample_run("run-3", "repo-1", IndexRunStatus::Failed))
            .unwrap();

        let result = persistence.get_latest_completed_run("repo-1").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().run_id, "run-2");
    }

    #[test]
    fn test_get_latest_completed_run_returns_none_when_no_completed() {
        let (_dir, persistence) = temp_registry();
        persistence
            .save_run(&sample_run("run-1", "repo-1", IndexRunStatus::Running))
            .unwrap();
        persistence
            .save_run(&sample_run("run-2", "repo-1", IndexRunStatus::Failed))
            .unwrap();

        let result = persistence.get_latest_completed_run("repo-1").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_latest_completed_run_filters_by_repo_id() {
        let (_dir, persistence) = temp_registry();
        let mut run_a = sample_run("run-a", "repo-a", IndexRunStatus::Succeeded);
        run_a.requested_at_unix_ms = 3000;
        persistence.save_run(&run_a).unwrap();
        persistence
            .save_run(&sample_run("run-b", "repo-b", IndexRunStatus::Succeeded))
            .unwrap();

        let result = persistence.get_latest_completed_run("repo-a").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().run_id, "run-a");

        let result_b = persistence.get_latest_completed_run("repo-b").unwrap();
        assert!(result_b.is_some());
        assert_eq!(result_b.unwrap().run_id, "run-b");
    }

    #[test]
    fn test_get_runs_by_repo_returns_all_sorted_descending() {
        let (_dir, persistence) = temp_registry();
        let mut run1 = sample_run("run-1", "repo-1", IndexRunStatus::Succeeded);
        run1.requested_at_unix_ms = 1000;
        let mut run2 = sample_run("run-2", "repo-1", IndexRunStatus::Running);
        run2.requested_at_unix_ms = 3000;
        let mut run3 = sample_run("run-3", "repo-1", IndexRunStatus::Failed);
        run3.requested_at_unix_ms = 2000;
        persistence.save_run(&run1).unwrap();
        persistence.save_run(&run2).unwrap();
        persistence.save_run(&run3).unwrap();

        let runs = persistence.get_runs_by_repo("repo-1").unwrap();
        assert_eq!(runs.len(), 3);
        assert_eq!(runs[0].run_id, "run-2"); // 3000 — most recent
        assert_eq!(runs[1].run_id, "run-3"); // 2000
        assert_eq!(runs[2].run_id, "run-1"); // 1000 — oldest
    }

    #[test]
    fn test_get_runs_by_repo_returns_empty_for_unknown_repo() {
        let (_dir, persistence) = temp_registry();
        persistence
            .save_run(&sample_run("run-1", "repo-1", IndexRunStatus::Succeeded))
            .unwrap();

        let runs = persistence.get_runs_by_repo("unknown-repo").unwrap();
        assert!(runs.is_empty());
    }

    #[test]
    fn test_idempotency_record_supports_reindex_operation() {
        let (_dir, persistence) = temp_registry();
        let record = IdempotencyRecord {
            operation: "reindex".to_string(),
            idempotency_key: "reindex::repo-1::ws-1".to_string(),
            request_hash: "hash-123".to_string(),
            status: IdempotencyStatus::Pending,
            result_ref: Some("run-reindex-1".to_string()),
            created_at_unix_ms: 1000,
            expires_at_unix_ms: None,
        };
        persistence.save_idempotency_record(&record).unwrap();

        let found = persistence
            .find_idempotency_record("reindex::repo-1::ws-1")
            .unwrap()
            .unwrap();
        assert_eq!(found.operation, "reindex");
        assert_eq!(found.request_hash, "hash-123");
        assert_eq!(found.result_ref, Some("run-reindex-1".to_string()));
    }

    #[test]
    fn test_reindex_idempotency_key_distinct_from_index() {
        let (_dir, persistence) = temp_registry();
        let index_record = IdempotencyRecord {
            operation: "index".to_string(),
            idempotency_key: "index::repo-1::ws-1".to_string(),
            request_hash: "hash-index".to_string(),
            status: IdempotencyStatus::Succeeded,
            result_ref: Some("run-index-1".to_string()),
            created_at_unix_ms: 1000,
            expires_at_unix_ms: None,
        };
        let reindex_record = IdempotencyRecord {
            operation: "reindex".to_string(),
            idempotency_key: "reindex::repo-1::ws-1".to_string(),
            request_hash: "hash-reindex".to_string(),
            status: IdempotencyStatus::Pending,
            result_ref: Some("run-reindex-1".to_string()),
            created_at_unix_ms: 2000,
            expires_at_unix_ms: None,
        };
        persistence.save_idempotency_record(&index_record).unwrap();
        persistence
            .save_idempotency_record(&reindex_record)
            .unwrap();

        let found_index = persistence
            .find_idempotency_record("index::repo-1::ws-1")
            .unwrap()
            .unwrap();
        let found_reindex = persistence
            .find_idempotency_record("reindex::repo-1::ws-1")
            .unwrap()
            .unwrap();
        assert_eq!(found_index.operation, "index");
        assert_eq!(found_reindex.operation, "reindex");
        assert_ne!(found_index.idempotency_key, found_reindex.idempotency_key);
    }

    fn sample_repo(repo_id: &str) -> Repository {
        Repository {
            repo_id: repo_id.to_string(),
            kind: crate::domain::RepositoryKind::Git,
            root_uri: format!("/tmp/{repo_id}"),
            project_identity: format!("identity-{repo_id}"),
            project_identity_kind: crate::domain::ProjectIdentityKind::GitCommonDir,
            default_branch: None,
            last_known_revision: None,
            status: crate::domain::RepositoryStatus::Ready,
            invalidated_at_unix_ms: None,
            invalidation_reason: None,
            quarantined_at_unix_ms: None,
            quarantine_reason: None,
        }
    }

    fn registry_with_repo(persistence: &RegistryPersistence, repo_id: &str) {
        let mut data = RegistryData {
            schema_version: 2,
            ..RegistryData::default()
        };
        data.repositories
            .insert(repo_id.to_string(), sample_repo(repo_id));
        save_registry_data(&persistence.path, &data).unwrap();
    }

    #[test]
    fn test_get_repository_returns_existing_repo() {
        let (_dir, persistence) = temp_registry();
        registry_with_repo(&persistence, "repo-1");

        let repo = persistence.get_repository("repo-1").unwrap();
        assert!(repo.is_some());
        let repo = repo.unwrap();
        assert_eq!(repo.repo_id, "repo-1");
        assert_eq!(repo.status, crate::domain::RepositoryStatus::Ready);
    }

    #[test]
    fn test_get_repository_returns_none_for_unknown() {
        let (_dir, persistence) = temp_registry();
        registry_with_repo(&persistence, "repo-1");

        let repo = persistence.get_repository("nonexistent").unwrap();
        assert!(repo.is_none());
    }

    #[test]
    fn test_update_repository_status_transitions_to_invalidated() {
        let (_dir, persistence) = temp_registry();
        registry_with_repo(&persistence, "repo-1");

        persistence
            .update_repository_status(
                "repo-1",
                crate::domain::RepositoryStatus::Invalidated,
                Some(1709827200000),
                Some("stale data".to_string()),
                None,
                None,
            )
            .unwrap();

        let repo = persistence.get_repository("repo-1").unwrap().unwrap();
        assert_eq!(repo.status, crate::domain::RepositoryStatus::Invalidated);
        assert_eq!(repo.invalidated_at_unix_ms, Some(1709827200000));
        assert_eq!(repo.invalidation_reason.as_deref(), Some("stale data"));
    }

    #[test]
    fn test_update_repository_status_returns_not_found_for_unknown() {
        let (_dir, persistence) = temp_registry();
        registry_with_repo(&persistence, "repo-1");

        let result = persistence.update_repository_status(
            "nonexistent",
            crate::domain::RepositoryStatus::Invalidated,
            None,
            None,
            None,
            None,
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, TokenizorError::NotFound(_)),
            "expected NotFound, got: {err:?}"
        );
    }

    #[test]
    fn test_update_repository_status_clears_invalidation_on_ready() {
        let (_dir, persistence) = temp_registry();
        let mut data = RegistryData {
            schema_version: 2,
            ..RegistryData::default()
        };
        let mut repo = sample_repo("repo-1");
        repo.status = crate::domain::RepositoryStatus::Invalidated;
        repo.invalidated_at_unix_ms = Some(1709827200000);
        repo.invalidation_reason = Some("old reason".to_string());
        data.repositories.insert("repo-1".to_string(), repo);
        save_registry_data(&persistence.path, &data).unwrap();

        persistence
            .update_repository_status(
                "repo-1",
                crate::domain::RepositoryStatus::Ready,
                None,
                None,
                None,
                None,
            )
            .unwrap();

        let repo = persistence.get_repository("repo-1").unwrap().unwrap();
        assert_eq!(repo.status, crate::domain::RepositoryStatus::Ready);
        assert!(repo.invalidated_at_unix_ms.is_none());
        assert!(repo.invalidation_reason.is_none());
        assert!(repo.quarantined_at_unix_ms.is_none());
        assert!(repo.quarantine_reason.is_none());
    }

    #[test]
    fn test_update_repository_status_transitions_to_quarantined() {
        let (_dir, persistence) = temp_registry();
        registry_with_repo(&persistence, "repo-1");

        persistence
            .update_repository_status(
                "repo-1",
                crate::domain::RepositoryStatus::Quarantined,
                None,
                None,
                Some(1709827300000),
                Some("blob verification failed repeatedly".to_string()),
            )
            .unwrap();

        let repo = persistence.get_repository("repo-1").unwrap().unwrap();
        assert_eq!(repo.status, crate::domain::RepositoryStatus::Quarantined);
        assert_eq!(repo.quarantined_at_unix_ms, Some(1709827300000));
        assert_eq!(
            repo.quarantine_reason.as_deref(),
            Some("blob verification failed repeatedly")
        );
        assert!(repo.invalidated_at_unix_ms.is_none());
        assert!(repo.invalidation_reason.is_none());
    }

    #[test]
    fn test_update_repository_status_clears_quarantine_on_ready() {
        let (_dir, persistence) = temp_registry();
        let mut data = RegistryData {
            schema_version: 2,
            ..RegistryData::default()
        };
        let mut repo = sample_repo("repo-1");
        repo.status = crate::domain::RepositoryStatus::Quarantined;
        repo.quarantined_at_unix_ms = Some(1709827300000);
        repo.quarantine_reason = Some("quarantine reason".to_string());
        data.repositories.insert("repo-1".to_string(), repo);
        save_registry_data(&persistence.path, &data).unwrap();

        persistence
            .update_repository_status(
                "repo-1",
                crate::domain::RepositoryStatus::Ready,
                None,
                None,
                None,
                None,
            )
            .unwrap();

        let repo = persistence.get_repository("repo-1").unwrap().unwrap();
        assert_eq!(repo.status, crate::domain::RepositoryStatus::Ready);
        assert!(repo.quarantined_at_unix_ms.is_none());
        assert!(repo.quarantine_reason.is_none());
    }

    #[test]
    fn test_owned_registry_temp_artifact_matches_only_registry_sibling_pattern() {
        let registry = PathBuf::from(".tokenizor/control-plane/project-workspace-registry.json");
        let owned = registry
            .parent()
            .unwrap()
            .join(".project-workspace-registry.json.1234567890.tmp");
        let wrong_name = registry
            .parent()
            .unwrap()
            .join(".other-registry.json.1234567890.tmp");
        let nested = registry
            .parent()
            .unwrap()
            .join("nested")
            .join(".project-workspace-registry.json.1234567890.tmp");

        assert!(is_owned_registry_temp_artifact_path(&registry, &owned));
        assert!(!is_owned_registry_temp_artifact_path(
            &registry,
            &wrong_name
        ));
        assert!(!is_owned_registry_temp_artifact_path(&registry, &nested));

        let non_numeric_middle = registry
            .parent()
            .unwrap()
            .join(".project-workspace-registry.json.backup.tmp");
        assert!(!is_owned_registry_temp_artifact_path(
            &registry,
            &non_numeric_middle
        ));
    }
}
