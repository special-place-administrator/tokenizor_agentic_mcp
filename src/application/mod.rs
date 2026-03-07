mod deployment;
mod health;
mod init;

use std::sync::Arc;

use crate::config::ServerConfig;
use std::path::PathBuf;

use crate::domain::{
    ActiveWorkspaceContext, ComponentHealth, DeploymentReport, HealthReport, InitializationReport,
    MigrationReport, RegistryView,
};
use crate::error::{Result, TokenizorError};
use crate::storage::{BlobStore, ControlPlane, LocalCasBlobStore, build_control_plane};

use self::deployment::DeploymentService;
use self::health::HealthService;
use self::init::InitializationService;

#[derive(Clone)]
pub struct ApplicationContext {
    config: ServerConfig,
    blob_store: Arc<dyn BlobStore>,
    control_plane: Arc<dyn ControlPlane>,
}

impl ApplicationContext {
    pub fn from_config(config: ServerConfig) -> Result<Self> {
        let blob_store: Arc<dyn BlobStore> =
            Arc::new(LocalCasBlobStore::new(config.blob_store.clone()));
        let control_plane = build_control_plane(&config.control_plane)?;

        Ok(Self {
            config,
            blob_store,
            control_plane,
        })
    }

    pub fn initialize_local_storage(&self) -> Result<ComponentHealth> {
        self.blob_store.initialize()
    }

    pub fn health_report(&self) -> Result<HealthReport> {
        HealthService::new(self.blob_store.as_ref(), self.control_plane.as_ref()).report()
    }

    pub fn deployment_report(&self) -> Result<DeploymentReport> {
        DeploymentService::new(
            &self.config,
            self.blob_store.as_ref(),
            self.control_plane.as_ref(),
        )
        .report()
    }

    pub fn bootstrap_report(&self) -> Result<DeploymentReport> {
        DeploymentService::new(
            &self.config,
            self.blob_store.as_ref(),
            self.control_plane.as_ref(),
        )
        .bootstrap()
    }

    pub fn initialize_repository(
        &self,
        target_path: Option<PathBuf>,
    ) -> Result<InitializationReport> {
        InitializationService::new(
            &self.config,
            self.blob_store.as_ref(),
            self.control_plane.as_ref(),
        )
        .initialize_repository(target_path)
    }

    pub fn attach_workspace(&self, target_path: Option<PathBuf>) -> Result<InitializationReport> {
        InitializationService::new(
            &self.config,
            self.blob_store.as_ref(),
            self.control_plane.as_ref(),
        )
        .attach_workspace(target_path)
    }

    pub fn inspect_registry(&self) -> Result<RegistryView> {
        InitializationService::new(
            &self.config,
            self.blob_store.as_ref(),
            self.control_plane.as_ref(),
        )
        .inspect_registry()
    }

    pub fn migrate_registry(
        &self,
        source_path: Option<PathBuf>,
        target_path: Option<PathBuf>,
    ) -> Result<MigrationReport> {
        InitializationService::new(
            &self.config,
            self.blob_store.as_ref(),
            self.control_plane.as_ref(),
        )
        .migrate_registry(source_path, target_path)
    }

    pub fn resolve_active_context(
        &self,
        target_path: Option<PathBuf>,
    ) -> Result<ActiveWorkspaceContext> {
        InitializationService::new(
            &self.config,
            self.blob_store.as_ref(),
            self.control_plane.as_ref(),
        )
        .resolve_active_context(target_path)
    }

    pub fn ensure_runtime_ready(&self) -> Result<DeploymentReport> {
        let report = self.deployment_report()?;

        if self.config.runtime.require_ready_control_plane && !report.is_ready() {
            return Err(TokenizorError::ControlPlane(format!(
                "runtime readiness is blocked: {}",
                report.blocking_summary()
            )));
        }

        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::config::ServerConfig;
    use crate::domain::{
        Checkpoint, ComponentHealth, HealthIssueCategory, IdempotencyRecord, IndexRun, Repository,
    };
    use crate::error::{Result, TokenizorError};
    use crate::storage::{BlobStore, ControlPlane, StoredBlob};

    use super::ApplicationContext;

    struct FakeBlobStore {
        root_dir: PathBuf,
        health: ComponentHealth,
        initialize_calls: AtomicUsize,
        health_check_calls: AtomicUsize,
    }

    impl FakeBlobStore {
        fn new(root_dir: impl Into<PathBuf>, health: ComponentHealth) -> Self {
            Self {
                root_dir: root_dir.into(),
                health,
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
            Err(TokenizorError::Storage(
                "initialize() should not run during runtime readiness".into(),
            ))
        }

        fn health_check(&self) -> Result<ComponentHealth> {
            self.health_check_calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.health.clone())
        }

        fn store_bytes(&self, _bytes: &[u8]) -> Result<StoredBlob> {
            unreachable!("store_bytes is not exercised by application tests")
        }

        fn read_bytes(&self, _blob_id: &str) -> Result<Vec<u8>> {
            unreachable!("read_bytes is not exercised by application tests")
        }
    }

    struct FakeControlPlane {
        deployment_checks: Vec<ComponentHealth>,
        deployment_check_calls: AtomicUsize,
        health_check_calls: AtomicUsize,
    }

    impl FakeControlPlane {
        fn new(deployment_checks: Vec<ComponentHealth>) -> Self {
            Self {
                deployment_checks,
                deployment_check_calls: AtomicUsize::new(0),
                health_check_calls: AtomicUsize::new(0),
            }
        }
    }

    impl ControlPlane for FakeControlPlane {
        fn backend_name(&self) -> &'static str {
            "fake_control_plane"
        }

        fn health_check(&self) -> Result<ComponentHealth> {
            self.health_check_calls.fetch_add(1, Ordering::SeqCst);
            Err(TokenizorError::ControlPlane(
                "health_check() should not run during runtime readiness".into(),
            ))
        }

        fn deployment_checks(&self) -> Result<Vec<ComponentHealth>> {
            self.deployment_check_calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.deployment_checks.clone())
        }

        fn upsert_repository(&self, _repository: Repository) -> Result<()> {
            unreachable!("writes are not exercised by application tests")
        }

        fn create_index_run(&self, _run: IndexRun) -> Result<()> {
            unreachable!("writes are not exercised by application tests")
        }

        fn write_checkpoint(&self, _checkpoint: Checkpoint) -> Result<()> {
            unreachable!("writes are not exercised by application tests")
        }

        fn put_idempotency_record(&self, _record: IdempotencyRecord) -> Result<()> {
            unreachable!("writes are not exercised by application tests")
        }
    }

    fn application_with_checks(
        mut config: ServerConfig,
        deployment_checks: Vec<ComponentHealth>,
    ) -> (
        ApplicationContext,
        Arc<FakeBlobStore>,
        Arc<FakeControlPlane>,
    ) {
        config.blob_store.root_dir = PathBuf::from(".tokenizor");

        let blob_store = Arc::new(FakeBlobStore::new(
            ".tokenizor",
            ComponentHealth::ok(
                "blob_store",
                HealthIssueCategory::Storage,
                "blob store health checked",
            ),
        ));
        let control_plane = Arc::new(FakeControlPlane::new(deployment_checks));
        let application = ApplicationContext {
            config,
            blob_store: blob_store.clone(),
            control_plane: control_plane.clone(),
        };

        (application, blob_store, control_plane)
    }

    #[test]
    fn ensure_runtime_ready_blocks_on_deployment_report_errors() {
        let config = ServerConfig::default();
        let blocking_check = ComponentHealth::error(
            "spacetimedb_cli",
            HealthIssueCategory::Dependency,
            "`spacetimedb` is not installed",
            "Install the SpacetimeDB CLI and ensure it is on PATH.",
        );
        let (application, blob_store, control_plane) =
            application_with_checks(config, vec![blocking_check]);

        let error = application
            .ensure_runtime_ready()
            .expect_err("runtime readiness should fail");

        assert!(
            error
                .to_string()
                .contains("runtime readiness is blocked: spacetimedb_cli")
        );
        assert!(
            error
                .to_string()
                .contains("Install the SpacetimeDB CLI and ensure it is on PATH.")
        );
        assert_eq!(blob_store.initialize_calls.load(Ordering::SeqCst), 0);
        assert_eq!(blob_store.health_check_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            control_plane.deployment_check_calls.load(Ordering::SeqCst),
            1
        );
        assert_eq!(control_plane.health_check_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn ensure_runtime_ready_returns_report_when_prerequisites_are_ready() {
        let config = ServerConfig::default();
        let (application, blob_store, control_plane) = application_with_checks(
            config,
            vec![ComponentHealth::warning(
                "spacetimedb_schema_compatibility",
                HealthIssueCategory::Compatibility,
                "published schema cannot be verified yet",
                "Treat this as an operator warning only.",
            )],
        );

        let report = application
            .ensure_runtime_ready()
            .expect("runtime readiness should succeed");

        assert!(report.is_ready());
        assert_eq!(report.checks.len(), 2);
        assert_eq!(blob_store.initialize_calls.load(Ordering::SeqCst), 0);
        assert_eq!(blob_store.health_check_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            control_plane.deployment_check_calls.load(Ordering::SeqCst),
            1
        );
        assert_eq!(control_plane.health_check_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn ensure_runtime_ready_allows_explicit_opt_out() {
        let mut config = ServerConfig::default();
        config.runtime.require_ready_control_plane = false;

        let (application, _blob_store, control_plane) = application_with_checks(
            config,
            vec![ComponentHealth::error(
                "spacetimedb_endpoint",
                HealthIssueCategory::Dependency,
                "SpacetimeDB endpoint http://127.0.0.1:3007 is not reachable",
                "Start the local SpacetimeDB runtime or correct TOKENIZOR_SPACETIMEDB_ENDPOINT before retrying.",
            )],
        );

        let report = application
            .ensure_runtime_ready()
            .expect("explicit opt-out should bypass the runtime gate");

        assert!(!report.is_ready());
        assert_eq!(
            control_plane.deployment_check_calls.load(Ordering::SeqCst),
            1
        );
        assert_eq!(control_plane.health_check_calls.load(Ordering::SeqCst), 0);
    }
}
