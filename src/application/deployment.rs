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
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::config::ServerConfig;
    use crate::domain::{
        Checkpoint, ComponentHealth, HealthIssueCategory, IdempotencyRecord, IndexRun, Repository,
    };
    use crate::error::Result;
    use crate::storage::{BlobStore, ControlPlane, StoredBlob};

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
        deployment_check_calls: AtomicUsize,
        write_calls: AtomicUsize,
    }

    impl Default for FakeControlPlane {
        fn default() -> Self {
            Self {
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

        fn upsert_repository(&self, _repository: Repository) -> Result<()> {
            self.write_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn create_index_run(&self, _run: IndexRun) -> Result<()> {
            self.write_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn write_checkpoint(&self, _checkpoint: Checkpoint) -> Result<()> {
            self.write_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn put_idempotency_record(&self, _record: IdempotencyRecord) -> Result<()> {
            self.write_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
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
