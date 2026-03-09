use crate::config::ServerConfig;
use crate::domain::{ComponentHealth, DeploymentReport, HealthIssueCategory};
use crate::error::Result;
use crate::storage::{BlobStore, ControlPlane};

pub struct DeploymentService<'a> {
    config: &'a ServerConfig,
    blob_store: &'a dyn BlobStore,
    control_plane: &'a dyn ControlPlane,
}

impl<'a> DeploymentService<'a> {
    pub fn new(
        config: &'a ServerConfig,
        blob_store: &'a dyn BlobStore,
        control_plane: &'a dyn ControlPlane,
    ) -> Self {
        Self {
            config,
            blob_store,
            control_plane,
        }
    }

    pub fn report(&self) -> Result<DeploymentReport> {
        let mut checks = self.control_plane.deployment_checks()?;

        if self.blob_store.root_dir().as_os_str().is_empty() {
            checks.push(ComponentHealth::error(
                "blob_store_root",
                HealthIssueCategory::Configuration,
                "blob store root path is empty",
                "Set TOKENIZOR_BLOB_ROOT to a valid writable directory before running doctor or init.",
            ));
        }

        checks.push(self.blob_store.health_check()?);

        Ok(DeploymentReport::new(
            self.config.control_plane.backend.as_str(),
            self.blob_store.root_dir().to_path_buf(),
            checks,
        ))
    }

    pub fn bootstrap(&self) -> Result<DeploymentReport> {
        let mut checks = self.control_plane.deployment_checks()?;
        checks.push(self.blob_store.initialize()?);

        Ok(DeploymentReport::new(
            self.config.control_plane.backend.as_str(),
            self.blob_store.root_dir().to_path_buf(),
            checks,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::config::ServerConfig;
    use crate::domain::{
        Checkpoint, ComponentHealth, DiscoveryManifest, FileRecord, HealthIssueCategory,
        IdempotencyRecord, IndexRun, IndexRunStatus, Repository, RepositoryStatus,
    };
    use crate::error::Result;
    use crate::storage::{BlobStore, ControlPlane, InMemoryControlPlane, StoredBlob};

    use super::DeploymentService;

    struct FakeBlobStore {
        root_dir: PathBuf,
        initialize_calls: AtomicUsize,
        health_check_calls: AtomicUsize,
    }

    impl FakeBlobStore {
        fn new(root_dir: impl Into<PathBuf>) -> Self {
            Self {
                root_dir: root_dir.into(),
                initialize_calls: AtomicUsize::new(0),
                health_check_calls: AtomicUsize::new(0),
            }
        }
    }

    impl BlobStore for FakeBlobStore {
        fn backend_name(&self) -> &'static str {
            "fake_blob_store"
        }

        fn root_dir(&self) -> &Path {
            &self.root_dir
        }

        fn initialize(&self) -> Result<ComponentHealth> {
            self.initialize_calls.fetch_add(1, Ordering::SeqCst);
            Ok(ComponentHealth::ok(
                "blob_store",
                HealthIssueCategory::Storage,
                "blob store initialized",
            ))
        }

        fn health_check(&self) -> Result<ComponentHealth> {
            self.health_check_calls.fetch_add(1, Ordering::SeqCst);
            Ok(ComponentHealth::ok(
                "blob_store",
                HealthIssueCategory::Storage,
                "blob store health checked",
            ))
        }

        fn store_bytes(&self, _bytes: &[u8]) -> Result<StoredBlob> {
            unreachable!("store_bytes is not exercised by deployment tests")
        }

        fn read_bytes(&self, _blob_id: &str) -> Result<Vec<u8>> {
            unreachable!("read_bytes is not exercised by deployment tests")
        }
    }

    struct FakeControlPlane {
        backing: InMemoryControlPlane,
        deployment_check_calls: AtomicUsize,
        write_calls: AtomicUsize,
    }

    impl Default for FakeControlPlane {
        fn default() -> Self {
            Self {
                backing: InMemoryControlPlane::default(),
                deployment_check_calls: AtomicUsize::new(0),
                write_calls: AtomicUsize::new(0),
            }
        }
    }

    impl ControlPlane for FakeControlPlane {
        fn backend_name(&self) -> &'static str {
            "fake_control_plane"
        }

        fn health_check(&self) -> Result<ComponentHealth> {
            unreachable!("health_check is not exercised by deployment tests")
        }

        fn deployment_checks(&self) -> Result<Vec<ComponentHealth>> {
            self.deployment_check_calls.fetch_add(1, Ordering::SeqCst);
            Ok(vec![ComponentHealth::ok(
                "control_plane",
                HealthIssueCategory::Dependency,
                "control plane checked",
            )])
        }

        fn find_run(&self, run_id: &str) -> Result<Option<IndexRun>> {
            self.backing.find_run(run_id)
        }

        fn find_runs_by_status(&self, status: &IndexRunStatus) -> Result<Vec<IndexRun>> {
            self.backing.find_runs_by_status(status)
        }

        fn list_runs(&self) -> Result<Vec<IndexRun>> {
            self.backing.list_runs()
        }

        fn get_runs_by_repo(&self, repo_id: &str) -> Result<Vec<IndexRun>> {
            self.backing.get_runs_by_repo(repo_id)
        }

        fn get_latest_completed_run(&self, repo_id: &str) -> Result<Option<IndexRun>> {
            self.backing.get_latest_completed_run(repo_id)
        }

        fn get_repository(&self, repo_id: &str) -> Result<Option<Repository>> {
            self.backing.get_repository(repo_id)
        }

        fn get_file_records(&self, run_id: &str) -> Result<Vec<FileRecord>> {
            self.backing.get_file_records(run_id)
        }

        fn get_latest_checkpoint(&self, run_id: &str) -> Result<Option<Checkpoint>> {
            self.backing.get_latest_checkpoint(run_id)
        }

        fn find_idempotency_record(&self, key: &str) -> Result<Option<IdempotencyRecord>> {
            self.backing.find_idempotency_record(key)
        }

        fn get_discovery_manifest(&self, run_id: &str) -> Result<Option<DiscoveryManifest>> {
            self.backing.get_discovery_manifest(run_id)
        }

        fn save_run(&self, run: &IndexRun) -> Result<()> {
            self.write_calls.fetch_add(1, Ordering::SeqCst);
            self.backing.save_run(run)
        }

        fn update_run_status(
            &self,
            run_id: &str,
            status: IndexRunStatus,
            error_summary: Option<String>,
        ) -> Result<()> {
            self.write_calls.fetch_add(1, Ordering::SeqCst);
            self.backing
                .update_run_status(run_id, status, error_summary)
        }

        fn transition_to_running(&self, run_id: &str, started_at_unix_ms: u64) -> Result<()> {
            self.write_calls.fetch_add(1, Ordering::SeqCst);
            self.backing
                .transition_to_running(run_id, started_at_unix_ms)
        }

        fn update_run_status_with_finish(
            &self,
            run_id: &str,
            status: IndexRunStatus,
            error_summary: Option<String>,
            finished_at_unix_ms: u64,
            not_yet_supported: Option<BTreeMap<crate::domain::LanguageId, u64>>,
        ) -> Result<()> {
            self.write_calls.fetch_add(1, Ordering::SeqCst);
            self.backing.update_run_status_with_finish(
                run_id,
                status,
                error_summary,
                finished_at_unix_ms,
                not_yet_supported,
            )
        }

        fn cancel_run_if_active(&self, run_id: &str, finished_at_unix_ms: u64) -> Result<bool> {
            self.write_calls.fetch_add(1, Ordering::SeqCst);
            self.backing
                .cancel_run_if_active(run_id, finished_at_unix_ms)
        }

        fn save_file_records(&self, run_id: &str, records: &[FileRecord]) -> Result<()> {
            self.write_calls.fetch_add(1, Ordering::SeqCst);
            self.backing.save_file_records(run_id, records)
        }

        fn save_checkpoint(&self, checkpoint: &Checkpoint) -> Result<()> {
            self.write_calls.fetch_add(1, Ordering::SeqCst);
            self.backing.save_checkpoint(checkpoint)
        }

        fn save_repository(&self, repository: &Repository) -> Result<()> {
            self.write_calls.fetch_add(1, Ordering::SeqCst);
            self.backing.save_repository(repository)
        }

        fn update_repository_status(
            &self,
            repo_id: &str,
            status: RepositoryStatus,
            invalidated_at_unix_ms: Option<u64>,
            invalidation_reason: Option<String>,
            quarantined_at_unix_ms: Option<u64>,
            quarantine_reason: Option<String>,
        ) -> Result<()> {
            self.write_calls.fetch_add(1, Ordering::SeqCst);
            self.backing.update_repository_status(
                repo_id,
                status,
                invalidated_at_unix_ms,
                invalidation_reason,
                quarantined_at_unix_ms,
                quarantine_reason,
            )
        }

        fn save_idempotency_record(&self, record: &IdempotencyRecord) -> Result<()> {
            self.write_calls.fetch_add(1, Ordering::SeqCst);
            self.backing.save_idempotency_record(record)
        }

        fn save_discovery_manifest(&self, manifest: &DiscoveryManifest) -> Result<()> {
            self.write_calls.fetch_add(1, Ordering::SeqCst);
            self.backing.save_discovery_manifest(manifest)
        }
    }

    #[test]
    fn report_uses_only_read_only_checks() {
        let config = ServerConfig::default();
        let blob_store = FakeBlobStore::new(".tokenizor");
        let control_plane = FakeControlPlane::default();
        let service = DeploymentService::new(&config, &blob_store, &control_plane);

        let report = service.report().expect("deployment report should succeed");

        assert!(report.is_ready());
        assert_eq!(blob_store.initialize_calls.load(Ordering::SeqCst), 0);
        assert_eq!(blob_store.health_check_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            control_plane.deployment_check_calls.load(Ordering::SeqCst),
            1
        );
        assert_eq!(control_plane.write_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn bootstrap_uses_initialization_path() {
        let config = ServerConfig::default();
        let blob_store = FakeBlobStore::new(".tokenizor");
        let control_plane = FakeControlPlane::default();
        let service = DeploymentService::new(&config, &blob_store, &control_plane);

        let report = service
            .bootstrap()
            .expect("bootstrap report should succeed");

        assert!(report.is_ready());
        assert_eq!(blob_store.initialize_calls.load(Ordering::SeqCst), 1);
        assert_eq!(blob_store.health_check_calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            control_plane.deployment_check_calls.load(Ordering::SeqCst),
            1
        );
    }
}
