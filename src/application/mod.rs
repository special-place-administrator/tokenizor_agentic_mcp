mod deployment;
mod health;
mod init;
pub mod run_manager;
pub mod search;

use std::sync::Arc;

use crate::config::ServerConfig;
use std::path::PathBuf;

use tracing::{info, warn};

use crate::domain::{
    ActiveWorkspaceContext, BatchRetrievalRequest, ComponentHealth, DeploymentReport,
    FileOutlineResponse, GetSymbolsResponse, HealthReport, IndexRun, IndexRunMode,
    InitializationReport, InvalidationResult, MigrationReport, RegistryView, RepoOutlineResponse,
    ResultEnvelope, ResumeRunOutcome, SearchResultItem, SymbolKind, SymbolSearchResponse,
    VerifiedSourceResponse,
};
use crate::error::{Result, TokenizorError};
use crate::indexing::pipeline::PipelineProgress;
use crate::storage::{
    BlobStore, ControlPlane, LocalCasBlobStore, RegistryPersistence, build_control_plane,
};

use self::deployment::DeploymentService;
use self::health::HealthService;
use self::init::InitializationService;
pub use self::run_manager::RunManager;
use self::run_manager::StartupRecoveryReport;

#[derive(Clone)]
pub struct ApplicationContext {
    config: ServerConfig,
    blob_store: Arc<dyn BlobStore>,
    control_plane: Arc<dyn ControlPlane>,
    run_manager: Arc<RunManager>,
    startup_recovery: StartupRecoveryReport,
}

impl ApplicationContext {
    pub fn from_config(config: ServerConfig) -> Result<Self> {
        let blob_store: Arc<dyn BlobStore> =
            Arc::new(LocalCasBlobStore::new(config.blob_store.clone()));
        let registry_path = config
            .blob_store
            .root_dir
            .join("control-plane")
            .join("project-workspace-registry.json");
        let registry = Arc::new(RegistryPersistence::new(registry_path.clone()));
        let control_plane = build_control_plane(&config.control_plane, Arc::clone(&registry))?;
        let run_manager = Arc::new(RunManager::with_services(
            Arc::clone(&control_plane),
            registry,
            Some(registry_path),
            Some(config.blob_store.root_dir.clone()),
        ));

        let startup_recovery = run_manager.startup_sweep()?;
        info!(
            transitioned_runs = startup_recovery.transitioned_run_ids.len(),
            interrupted_runs = startup_recovery.interrupted_run_count(),
            aborted_runs = startup_recovery.aborted_run_count(),
            cleaned_temp_artifacts = startup_recovery.cleaned_temp_artifacts.len(),
            blocking_findings = startup_recovery.blocking_findings.len(),
            guidance = ?startup_recovery.operator_guidance,
            "startup sweep completed"
        );
        if startup_recovery.has_blocking_findings() {
            warn!(
                summary = %startup_recovery
                    .blocking_findings
                    .iter()
                    .map(|finding| format!("{}: {}", finding.name, finding.detail))
                    .collect::<Vec<_>>()
                    .join("; "),
                "startup sweep detected blocking recovery findings"
            );
        }

        Ok(Self {
            config,
            blob_store,
            control_plane,
            run_manager,
            startup_recovery,
        })
    }

    pub fn initialize_local_storage(&self) -> Result<ComponentHealth> {
        self.blob_store.initialize()
    }

    pub fn health_report(&self) -> Result<HealthReport> {
        HealthService::new(self.blob_store.as_ref(), self.control_plane.as_ref()).report()
    }

    pub fn deployment_report(&self) -> Result<DeploymentReport> {
        let base = DeploymentService::new(
            &self.config,
            self.blob_store.as_ref(),
            self.control_plane.as_ref(),
        )
        .report()?;
        Ok(self.merge_startup_recovery_checks(base))
    }

    pub fn bootstrap_report(&self) -> Result<DeploymentReport> {
        let base = DeploymentService::new(
            &self.config,
            self.blob_store.as_ref(),
            self.control_plane.as_ref(),
        )
        .bootstrap()?;
        Ok(self.merge_startup_recovery_checks(base))
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

    pub fn migrate_control_plane_mutable_state(&self) -> Result<DeploymentReport> {
        self.control_plane.migrate_mutable_state_from_registry()?;
        self.deployment_report()
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

    pub fn start_indexing(&self, repo_id: &str, mode: IndexRunMode) -> Result<IndexRun> {
        self.run_manager.start_run(repo_id, mode)
    }

    pub fn launch_indexing(
        &self,
        repo_id: &str,
        mode: IndexRunMode,
        repo_root: PathBuf,
    ) -> Result<(IndexRun, Arc<PipelineProgress>)> {
        self.run_manager
            .launch_run(repo_id, mode, repo_root, self.blob_store.clone())
    }

    pub fn reindex_repository(
        &self,
        repo_id: &str,
        workspace_id: Option<&str>,
        reason: Option<&str>,
        repo_root: PathBuf,
    ) -> Result<IndexRun> {
        self.run_manager.reindex_repository(
            repo_id,
            workspace_id,
            reason,
            repo_root,
            self.blob_store.clone(),
        )
    }

    pub fn resume_index_run(&self, run_id: &str, repo_root: PathBuf) -> Result<ResumeRunOutcome> {
        self.run_manager
            .resume_run(run_id, repo_root, self.blob_store.clone())
    }

    pub fn invalidate_repository(
        &self,
        repo_id: &str,
        workspace_id: Option<&str>,
        reason: Option<&str>,
    ) -> Result<InvalidationResult> {
        self.run_manager
            .invalidate_repository(repo_id, workspace_id, reason)
    }

    pub fn search_text(
        &self,
        repo_id: &str,
        query: &str,
    ) -> Result<ResultEnvelope<Vec<SearchResultItem>>> {
        search::search_text(
            repo_id,
            query,
            self.run_manager.registry_query(),
            &self.run_manager,
            self.blob_store.as_ref(),
        )
    }

    pub fn search_symbols(
        &self,
        repo_id: &str,
        query: &str,
        kind_filter: Option<SymbolKind>,
    ) -> Result<ResultEnvelope<SymbolSearchResponse>> {
        search::search_symbols(
            repo_id,
            query,
            kind_filter,
            self.run_manager.registry_query(),
            &self.run_manager,
        )
    }

    pub fn get_file_outline(
        &self,
        repo_id: &str,
        relative_path: &str,
    ) -> Result<ResultEnvelope<FileOutlineResponse>> {
        search::get_file_outline(
            repo_id,
            relative_path,
            self.run_manager.registry_query(),
            &self.run_manager,
        )
    }

    pub fn get_repo_outline(&self, repo_id: &str) -> Result<ResultEnvelope<RepoOutlineResponse>> {
        search::get_repo_outline(
            repo_id,
            self.run_manager.registry_query(),
            &self.run_manager,
        )
    }

    pub fn get_symbol(
        &self,
        repo_id: &str,
        relative_path: &str,
        symbol_name: &str,
        kind_filter: Option<SymbolKind>,
    ) -> Result<ResultEnvelope<VerifiedSourceResponse>> {
        search::get_symbol(
            repo_id,
            relative_path,
            symbol_name,
            kind_filter,
            self.run_manager.registry_query(),
            &self.run_manager,
            self.blob_store.as_ref(),
        )
    }

    pub fn get_symbols(
        &self,
        repo_id: &str,
        requests: &[BatchRetrievalRequest],
    ) -> Result<ResultEnvelope<GetSymbolsResponse>> {
        search::get_symbols(
            repo_id,
            requests,
            self.run_manager.registry_query(),
            &self.run_manager,
            self.blob_store.as_ref(),
        )
    }

    pub fn run_manager(&self) -> &Arc<RunManager> {
        &self.run_manager
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

    fn merge_startup_recovery_checks(&self, base: DeploymentReport) -> DeploymentReport {
        let mut checks = base.checks;
        checks.extend(self.startup_recovery.readiness_checks());
        DeploymentReport::new(base.control_plane_backend, base.blob_root, checks)
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::config::ServerConfig;
    use crate::domain::{
        Checkpoint, ComponentHealth, DiscoveryManifest, FileRecord, HealthIssueCategory,
        IdempotencyRecord, IndexRun, IndexRunStatus, Repository, RepositoryStatus,
    };
    use crate::error::{Result, TokenizorError};
    use crate::storage::{
        BlobStore, ControlPlane, InMemoryControlPlane, RegistryPersistence, StoredBlob,
    };

    use super::run_manager::{
        StartupCleanupSurface, StartupRecoveredTempArtifact, StartupRecoveryFinding,
        StartupRecoveryReport,
    };
    use super::{ApplicationContext, RunManager};

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
        backing: InMemoryControlPlane,
        deployment_checks: Vec<ComponentHealth>,
        deployment_check_calls: AtomicUsize,
        health_check_calls: AtomicUsize,
    }

    impl FakeControlPlane {
        fn new(deployment_checks: Vec<ComponentHealth>) -> Self {
            Self {
                backing: InMemoryControlPlane::default(),
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
            self.backing.save_run(run)
        }

        fn update_run_status(
            &self,
            run_id: &str,
            status: IndexRunStatus,
            error_summary: Option<String>,
        ) -> Result<()> {
            self.backing
                .update_run_status(run_id, status, error_summary)
        }

        fn transition_to_running(&self, run_id: &str, started_at_unix_ms: u64) -> Result<()> {
            self.backing
                .transition_to_running(run_id, started_at_unix_ms)
        }

        fn update_run_status_with_finish(
            &self,
            run_id: &str,
            status: IndexRunStatus,
            error_summary: Option<String>,
            finished_at_unix_ms: u64,
            not_yet_supported: Option<std::collections::BTreeMap<crate::domain::LanguageId, u64>>,
        ) -> Result<()> {
            self.backing.update_run_status_with_finish(
                run_id,
                status,
                error_summary,
                finished_at_unix_ms,
                not_yet_supported,
            )
        }

        fn cancel_run_if_active(&self, run_id: &str, finished_at_unix_ms: u64) -> Result<bool> {
            self.backing
                .cancel_run_if_active(run_id, finished_at_unix_ms)
        }

        fn save_file_records(&self, run_id: &str, records: &[FileRecord]) -> Result<()> {
            self.backing.save_file_records(run_id, records)
        }

        fn save_checkpoint(&self, checkpoint: &Checkpoint) -> Result<()> {
            self.backing.save_checkpoint(checkpoint)
        }

        fn save_repository(&self, repository: &Repository) -> Result<()> {
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
            self.backing.save_idempotency_record(record)
        }

        fn save_discovery_manifest(&self, manifest: &DiscoveryManifest) -> Result<()> {
            self.backing.save_discovery_manifest(manifest)
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
        let temp_dir = tempfile::tempdir().unwrap();
        let registry_path = temp_dir.path().join("test-registry.json");
        let persistence = RegistryPersistence::new(registry_path);
        let run_manager = Arc::new(RunManager::new(persistence));
        let application = ApplicationContext {
            config,
            blob_store: blob_store.clone(),
            control_plane: control_plane.clone(),
            run_manager,
            startup_recovery: StartupRecoveryReport::default(),
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

    #[test]
    fn deployment_report_includes_startup_recovery_warnings() {
        let config = ServerConfig::default();
        let (mut application, _blob_store, _control_plane) =
            application_with_checks(config, vec![]);
        application.startup_recovery = StartupRecoveryReport {
            transitioned_run_ids: vec!["run-1".into()],
            transitioned_runs: vec![crate::application::run_manager::StartupRecoveredRunTransition {
                run_id: "run-1".into(),
                repo_id: "repo-1".into(),
                from_status: crate::domain::IndexRunStatus::Running,
                to_status: crate::domain::IndexRunStatus::Interrupted,
            }],
            cleaned_temp_artifacts: vec![StartupRecoveredTempArtifact {
                surface: StartupCleanupSurface::RegistryTemp,
                path: PathBuf::from(
                    ".tokenizor/control-plane/.project-workspace-registry.json.1.tmp",
                ),
            }],
            blocking_findings: Vec::new(),
            operator_guidance: vec![
                "Inspect interrupted runs and choose the next safe action: reindex or repair before trusting prior results.".into(),
            ],
        };

        let report = application.deployment_report().unwrap();

        assert!(report.is_ready());
        assert!(
            report
                .checks
                .iter()
                .any(|check| check.category == HealthIssueCategory::Recovery)
        );
    }

    #[test]
    fn ensure_runtime_ready_blocks_on_startup_recovery_errors() {
        let config = ServerConfig::default();
        let (mut application, _blob_store, _control_plane) =
            application_with_checks(config, vec![]);
        application.startup_recovery = StartupRecoveryReport {
            transitioned_run_ids: Vec::new(),
            transitioned_runs: Vec::new(),
            cleaned_temp_artifacts: Vec::new(),
            blocking_findings: vec![StartupRecoveryFinding {
                name: "startup_recovery_registry_temp".into(),
                detail: "registry temp artifact could not be cleaned safely".into(),
                remediation: "Repair or migrate the registry state, or wait for the conflicting process to release the artifact, then restart Tokenizor.".into(),
            }],
            operator_guidance: vec![
                "Repair or migrate the registry state, or wait for the conflicting process to release the artifact, then restart Tokenizor.".into(),
            ],
        };

        let error = application
            .ensure_runtime_ready()
            .expect_err("runtime readiness should fail on startup recovery blockers");

        assert!(error.to_string().contains("startup_recovery_registry_temp"));
        assert!(error.to_string().contains("Repair or migrate"));
    }
}
