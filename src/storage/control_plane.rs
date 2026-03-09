use std::collections::BTreeMap;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::registry_persistence::RegistryData;
use super::{RegistryPersistence, SdkSpacetimeStateStore, SpacetimeStateStore};
use crate::config::{
    ControlPlaneBackend, ControlPlaneConfig, SUPPORTED_SPACETIMEDB_SCHEMA_VERSION,
    SpacetimeDbConfig,
};
use crate::domain::{
    Checkpoint, ComponentHealth, DiscoveryManifest, FileRecord, HealthIssueCategory,
    IdempotencyRecord, IndexRun, IndexRunStatus, Repository, RepositoryStatus,
};
use crate::error::{Result, TokenizorError};

pub trait ControlPlane: Send + Sync {
    fn backend_name(&self) -> &'static str;
    fn health_check(&self) -> Result<ComponentHealth>;
    fn deployment_checks(&self) -> Result<Vec<ComponentHealth>>;
    fn find_run(&self, run_id: &str) -> Result<Option<IndexRun>>;
    fn find_runs_by_status(&self, status: &IndexRunStatus) -> Result<Vec<IndexRun>>;
    fn list_runs(&self) -> Result<Vec<IndexRun>>;
    fn get_runs_by_repo(&self, repo_id: &str) -> Result<Vec<IndexRun>>;
    fn get_latest_completed_run(&self, repo_id: &str) -> Result<Option<IndexRun>>;
    fn get_repository(&self, repo_id: &str) -> Result<Option<Repository>>;
    fn get_file_records(&self, run_id: &str) -> Result<Vec<FileRecord>>;
    fn get_latest_checkpoint(&self, run_id: &str) -> Result<Option<Checkpoint>>;
    fn find_idempotency_record(&self, key: &str) -> Result<Option<IdempotencyRecord>>;
    fn get_discovery_manifest(&self, run_id: &str) -> Result<Option<DiscoveryManifest>>;
    fn save_run(&self, run: &IndexRun) -> Result<()>;
    fn update_run_status(
        &self,
        run_id: &str,
        status: IndexRunStatus,
        error_summary: Option<String>,
    ) -> Result<()>;
    fn transition_to_running(&self, run_id: &str, started_at_unix_ms: u64) -> Result<()>;
    fn update_run_status_with_finish(
        &self,
        run_id: &str,
        status: IndexRunStatus,
        error_summary: Option<String>,
        finished_at_unix_ms: u64,
        not_yet_supported: Option<BTreeMap<crate::domain::LanguageId, u64>>,
    ) -> Result<()>;
    fn cancel_run_if_active(&self, run_id: &str, finished_at_unix_ms: u64) -> Result<bool>;
    fn save_file_records(&self, run_id: &str, records: &[FileRecord]) -> Result<()>;
    fn save_checkpoint(&self, checkpoint: &Checkpoint) -> Result<()>;
    fn save_repository(&self, repository: &Repository) -> Result<()>;
    fn update_repository_status(
        &self,
        repo_id: &str,
        status: RepositoryStatus,
        invalidated_at_unix_ms: Option<u64>,
        invalidation_reason: Option<String>,
        quarantined_at_unix_ms: Option<u64>,
        quarantine_reason: Option<String>,
    ) -> Result<()>;
    fn save_idempotency_record(&self, record: &IdempotencyRecord) -> Result<()>;
    fn save_discovery_manifest(&self, manifest: &DiscoveryManifest) -> Result<()>;
    fn migrate_mutable_state_from_registry(&self) -> Result<()> {
        Err(TokenizorError::InvalidOperation(format!(
            "control plane backend `{}` does not support local mutable-state migration; set TOKENIZOR_CONTROL_PLANE_BACKEND=spacetimedb and run `tokenizor_agentic_mcp migrate control-plane`",
            self.backend_name()
        )))
    }

    fn upsert_repository(&self, repository: Repository) -> Result<()> {
        self.save_repository(&repository)
    }

    fn create_index_run(&self, run: IndexRun) -> Result<()> {
        self.save_run(&run)
    }

    fn write_checkpoint(&self, checkpoint: Checkpoint) -> Result<()> {
        self.save_checkpoint(&checkpoint)
    }

    fn put_idempotency_record(&self, record: IdempotencyRecord) -> Result<()> {
        self.save_idempotency_record(&record)
    }
}

pub fn build_control_plane(
    config: &ControlPlaneConfig,
    registry: Arc<RegistryPersistence>,
) -> Result<Arc<dyn ControlPlane>> {
    match config.backend {
        ControlPlaneBackend::InMemory => Ok(Arc::new(InMemoryControlPlane::default())),
        ControlPlaneBackend::LocalRegistry => Ok(Arc::new(RegistryBackedControlPlane::new(
            Arc::clone(&registry),
        ))),
        ControlPlaneBackend::SpacetimeDb => Ok(Arc::new(SpacetimeControlPlane::new(
            config.spacetimedb.clone(),
            registry,
        ))),
    }
}

#[derive(Default)]
struct InMemoryState {
    repositories: BTreeMap<String, Repository>,
    runs: BTreeMap<String, IndexRun>,
    checkpoints: Vec<Checkpoint>,
    run_file_records: BTreeMap<String, Vec<FileRecord>>,
    idempotency_records: BTreeMap<String, IdempotencyRecord>,
    discovery_manifests: BTreeMap<String, DiscoveryManifest>,
}

#[derive(Default)]
pub struct InMemoryControlPlane {
    state: Mutex<InMemoryState>,
}

impl ControlPlane for InMemoryControlPlane {
    fn backend_name(&self) -> &'static str {
        "in_memory"
    }

    fn health_check(&self) -> Result<ComponentHealth> {
        Ok(ComponentHealth::ok(
            "control_plane",
            HealthIssueCategory::Configuration,
            "using the in-memory control plane configured for tests or disposable local sessions",
        ))
    }

    fn deployment_checks(&self) -> Result<Vec<ComponentHealth>> {
        Ok(vec![ComponentHealth::warning(
            "control_plane_backend",
            HealthIssueCategory::Configuration,
            "the configured control plane backend is in-memory, so run metadata will not be durable and repository/workspace registration remains in the local bootstrap registry",
            "Set TOKENIZOR_CONTROL_PLANE_BACKEND=spacetimedb when you need authoritative durable control-plane state.",
        )])
    }

    fn find_run(&self, run_id: &str) -> Result<Option<IndexRun>> {
        let state = self.state.lock().map_err(|_| {
            TokenizorError::ControlPlane("in-memory control plane lock poisoned".into())
        })?;
        Ok(state.runs.get(run_id).cloned())
    }

    fn find_runs_by_status(&self, status: &IndexRunStatus) -> Result<Vec<IndexRun>> {
        let state = self.state.lock().map_err(|_| {
            TokenizorError::ControlPlane("in-memory control plane lock poisoned".into())
        })?;
        Ok(state
            .runs
            .values()
            .filter(|run| &run.status == status)
            .cloned()
            .collect())
    }

    fn list_runs(&self) -> Result<Vec<IndexRun>> {
        let state = self.state.lock().map_err(|_| {
            TokenizorError::ControlPlane("in-memory control plane lock poisoned".into())
        })?;
        let mut runs: Vec<_> = state.runs.values().cloned().collect();
        runs.sort_by(|left, right| right.requested_at_unix_ms.cmp(&left.requested_at_unix_ms));
        Ok(runs)
    }

    fn get_runs_by_repo(&self, repo_id: &str) -> Result<Vec<IndexRun>> {
        let state = self.state.lock().map_err(|_| {
            TokenizorError::ControlPlane("in-memory control plane lock poisoned".into())
        })?;
        let mut runs: Vec<_> = state
            .runs
            .values()
            .filter(|run| run.repo_id == repo_id)
            .cloned()
            .collect();
        runs.sort_by(|left, right| right.requested_at_unix_ms.cmp(&left.requested_at_unix_ms));
        Ok(runs)
    }

    fn get_latest_completed_run(&self, repo_id: &str) -> Result<Option<IndexRun>> {
        let state = self.state.lock().map_err(|_| {
            TokenizorError::ControlPlane("in-memory control plane lock poisoned".into())
        })?;
        Ok(state
            .runs
            .values()
            .filter(|run| run.repo_id == repo_id && run.status == IndexRunStatus::Succeeded)
            .max_by_key(|run| run.requested_at_unix_ms)
            .cloned())
    }

    fn get_repository(&self, repo_id: &str) -> Result<Option<Repository>> {
        let state = self.state.lock().map_err(|_| {
            TokenizorError::ControlPlane("in-memory control plane lock poisoned".into())
        })?;
        Ok(state.repositories.get(repo_id).cloned())
    }

    fn get_file_records(&self, run_id: &str) -> Result<Vec<FileRecord>> {
        let state = self.state.lock().map_err(|_| {
            TokenizorError::ControlPlane("in-memory control plane lock poisoned".into())
        })?;
        Ok(state
            .run_file_records
            .get(run_id)
            .cloned()
            .unwrap_or_default())
    }

    fn get_latest_checkpoint(&self, run_id: &str) -> Result<Option<Checkpoint>> {
        let state = self.state.lock().map_err(|_| {
            TokenizorError::ControlPlane("in-memory control plane lock poisoned".into())
        })?;
        Ok(state
            .checkpoints
            .iter()
            .filter(|checkpoint| checkpoint.run_id == run_id)
            .max_by_key(|checkpoint| checkpoint.created_at_unix_ms)
            .cloned())
    }

    fn find_idempotency_record(&self, key: &str) -> Result<Option<IdempotencyRecord>> {
        let state = self.state.lock().map_err(|_| {
            TokenizorError::ControlPlane("in-memory control plane lock poisoned".into())
        })?;
        Ok(state.idempotency_records.get(key).cloned())
    }

    fn get_discovery_manifest(&self, run_id: &str) -> Result<Option<DiscoveryManifest>> {
        let state = self.state.lock().map_err(|_| {
            TokenizorError::ControlPlane("in-memory control plane lock poisoned".into())
        })?;
        Ok(state.discovery_manifests.get(run_id).cloned())
    }

    fn save_run(&self, run: &IndexRun) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| {
            TokenizorError::ControlPlane("in-memory control plane lock poisoned".into())
        })?;
        state.runs.insert(run.run_id.clone(), run.clone());
        Ok(())
    }

    fn update_run_status(
        &self,
        run_id: &str,
        status: IndexRunStatus,
        error_summary: Option<String>,
    ) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| {
            TokenizorError::ControlPlane("in-memory control plane lock poisoned".into())
        })?;
        let run = state
            .runs
            .get_mut(run_id)
            .ok_or_else(|| TokenizorError::NotFound(format!("run `{run_id}` not found")))?;
        run.status = status;
        run.error_summary = error_summary;
        Ok(())
    }

    fn transition_to_running(&self, run_id: &str, started_at_unix_ms: u64) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| {
            TokenizorError::ControlPlane("in-memory control plane lock poisoned".into())
        })?;
        let run = state
            .runs
            .get_mut(run_id)
            .ok_or_else(|| TokenizorError::NotFound(format!("run `{run_id}` not found")))?;
        if run.status.is_terminal() && run.status != IndexRunStatus::Interrupted {
            return Ok(());
        }
        run.status = IndexRunStatus::Running;
        if run.started_at_unix_ms.is_none() {
            run.started_at_unix_ms = Some(started_at_unix_ms);
        }
        Ok(())
    }

    fn update_run_status_with_finish(
        &self,
        run_id: &str,
        status: IndexRunStatus,
        error_summary: Option<String>,
        finished_at_unix_ms: u64,
        not_yet_supported: Option<BTreeMap<crate::domain::LanguageId, u64>>,
    ) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| {
            TokenizorError::ControlPlane("in-memory control plane lock poisoned".into())
        })?;
        let run = state
            .runs
            .get_mut(run_id)
            .ok_or_else(|| TokenizorError::NotFound(format!("run `{run_id}` not found")))?;
        run.status = status;
        run.finished_at_unix_ms = Some(finished_at_unix_ms);
        run.error_summary = error_summary;
        run.not_yet_supported = not_yet_supported;
        Ok(())
    }

    fn cancel_run_if_active(&self, run_id: &str, finished_at_unix_ms: u64) -> Result<bool> {
        let mut state = self.state.lock().map_err(|_| {
            TokenizorError::ControlPlane("in-memory control plane lock poisoned".into())
        })?;
        let run = state
            .runs
            .get_mut(run_id)
            .ok_or_else(|| TokenizorError::NotFound(format!("run `{run_id}` not found")))?;
        if run.status.is_terminal() {
            return Ok(false);
        }
        run.status = IndexRunStatus::Cancelled;
        run.finished_at_unix_ms = Some(finished_at_unix_ms);
        Ok(true)
    }

    fn save_file_records(&self, run_id: &str, records: &[FileRecord]) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| {
            TokenizorError::ControlPlane("in-memory control plane lock poisoned".into())
        })?;
        let existing = state
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
        let mut merged_records: Vec<_> = merged.into_values().collect();
        merged_records.sort_by(|left, right| {
            left.relative_path
                .to_lowercase()
                .cmp(&right.relative_path.to_lowercase())
                .then_with(|| left.relative_path.cmp(&right.relative_path))
        });
        *existing = merged_records;
        Ok(())
    }

    fn save_checkpoint(&self, checkpoint: &Checkpoint) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| {
            TokenizorError::ControlPlane("in-memory control plane lock poisoned".into())
        })?;
        let run = state.runs.get_mut(&checkpoint.run_id).ok_or_else(|| {
            TokenizorError::NotFound(format!("run `{}` not found", checkpoint.run_id))
        })?;
        if run.status.is_terminal() {
            return Err(TokenizorError::InvalidOperation(format!(
                "cannot checkpoint run `{}` with terminal status `{:?}`",
                checkpoint.run_id, run.status
            )));
        }
        run.checkpoint_cursor = Some(checkpoint.cursor.clone());
        state.checkpoints.push(checkpoint.clone());
        Ok(())
    }

    fn save_repository(&self, repository: &Repository) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| {
            TokenizorError::ControlPlane("in-memory control plane lock poisoned".into())
        })?;
        state
            .repositories
            .insert(repository.repo_id.clone(), repository.clone());
        Ok(())
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
        let mut state = self.state.lock().map_err(|_| {
            TokenizorError::ControlPlane("in-memory control plane lock poisoned".into())
        })?;
        let repository = state
            .repositories
            .get_mut(repo_id)
            .ok_or_else(|| TokenizorError::NotFound(format!("repository not found: {repo_id}")))?;
        repository.status = status;
        repository.invalidated_at_unix_ms = invalidated_at_unix_ms;
        repository.invalidation_reason = invalidation_reason;
        repository.quarantined_at_unix_ms = quarantined_at_unix_ms;
        repository.quarantine_reason = quarantine_reason;
        Ok(())
    }

    fn save_idempotency_record(&self, record: &IdempotencyRecord) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| {
            TokenizorError::ControlPlane("in-memory control plane lock poisoned".into())
        })?;
        state
            .idempotency_records
            .insert(record.idempotency_key.clone(), record.clone());
        Ok(())
    }

    fn save_discovery_manifest(&self, manifest: &DiscoveryManifest) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| {
            TokenizorError::ControlPlane("in-memory control plane lock poisoned".into())
        })?;
        state
            .discovery_manifests
            .insert(manifest.run_id.clone(), manifest.clone());
        Ok(())
    }
}

pub struct RegistryBackedControlPlane {
    persistence: Arc<RegistryPersistence>,
}

impl RegistryBackedControlPlane {
    pub fn new(persistence: Arc<RegistryPersistence>) -> Self {
        Self { persistence }
    }
}

impl ControlPlane for RegistryBackedControlPlane {
    fn backend_name(&self) -> &'static str {
        "local_registry"
    }

    fn health_check(&self) -> Result<ComponentHealth> {
        Ok(ComponentHealth::warning(
            "control_plane",
            HealthIssueCategory::Configuration,
            "using the local registry-backed control plane for compatibility and development flows",
            "Use TOKENIZOR_CONTROL_PLANE_BACKEND=spacetimedb after mutable run state has been migrated to the authoritative control plane.",
        ))
    }

    fn deployment_checks(&self) -> Result<Vec<ComponentHealth>> {
        Ok(vec![ComponentHealth::warning(
            "control_plane_backend",
            HealthIssueCategory::Configuration,
            "the configured control plane backend is the local registry compatibility adapter; mutable run state is not yet stored in SpacetimeDB",
            "Migrate mutable run state and switch to TOKENIZOR_CONTROL_PLANE_BACKEND=spacetimedb when the authoritative control plane is ready.",
        )])
    }

    fn find_run(&self, run_id: &str) -> Result<Option<IndexRun>> {
        self.persistence.find_run(run_id)
    }

    fn find_runs_by_status(&self, status: &IndexRunStatus) -> Result<Vec<IndexRun>> {
        self.persistence.find_runs_by_status(status)
    }

    fn list_runs(&self) -> Result<Vec<IndexRun>> {
        self.persistence.list_runs()
    }

    fn get_runs_by_repo(&self, repo_id: &str) -> Result<Vec<IndexRun>> {
        self.persistence.get_runs_by_repo(repo_id)
    }

    fn get_latest_completed_run(&self, repo_id: &str) -> Result<Option<IndexRun>> {
        self.persistence.get_latest_completed_run(repo_id)
    }

    fn get_repository(&self, repo_id: &str) -> Result<Option<Repository>> {
        self.persistence.get_repository(repo_id)
    }

    fn get_file_records(&self, run_id: &str) -> Result<Vec<FileRecord>> {
        self.persistence.get_file_records(run_id)
    }

    fn get_latest_checkpoint(&self, run_id: &str) -> Result<Option<Checkpoint>> {
        self.persistence.get_latest_checkpoint(run_id)
    }

    fn find_idempotency_record(&self, key: &str) -> Result<Option<IdempotencyRecord>> {
        self.persistence.find_idempotency_record(key)
    }

    fn get_discovery_manifest(&self, run_id: &str) -> Result<Option<DiscoveryManifest>> {
        self.persistence.get_discovery_manifest(run_id)
    }

    fn save_run(&self, run: &IndexRun) -> Result<()> {
        self.persistence.save_run(run)
    }

    fn update_run_status(
        &self,
        run_id: &str,
        status: IndexRunStatus,
        error_summary: Option<String>,
    ) -> Result<()> {
        self.persistence
            .update_run_status(run_id, status, error_summary)
    }

    fn transition_to_running(&self, run_id: &str, started_at_unix_ms: u64) -> Result<()> {
        self.persistence
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
        self.persistence.update_run_status_with_finish(
            run_id,
            status,
            error_summary,
            finished_at_unix_ms,
            not_yet_supported,
        )
    }

    fn cancel_run_if_active(&self, run_id: &str, finished_at_unix_ms: u64) -> Result<bool> {
        self.persistence
            .cancel_run_if_active(run_id, finished_at_unix_ms)
    }

    fn save_file_records(&self, run_id: &str, records: &[FileRecord]) -> Result<()> {
        self.persistence.save_file_records(run_id, records)
    }

    fn save_checkpoint(&self, checkpoint: &Checkpoint) -> Result<()> {
        self.persistence.save_checkpoint(checkpoint)
    }

    fn save_repository(&self, repository: &Repository) -> Result<()> {
        self.persistence.save_repository(repository)
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
        self.persistence.update_repository_status(
            repo_id,
            status,
            invalidated_at_unix_ms,
            invalidation_reason,
            quarantined_at_unix_ms,
            quarantine_reason,
        )
    }

    fn save_idempotency_record(&self, record: &IdempotencyRecord) -> Result<()> {
        self.persistence.save_idempotency_record(record)
    }

    fn save_discovery_manifest(&self, manifest: &DiscoveryManifest) -> Result<()> {
        self.persistence.save_discovery_manifest(manifest)
    }
}

trait SpacetimeRuntimeProbe: Send + Sync {
    fn cli_available(&self, cli_path: &str) -> Result<bool>;
    fn endpoint_reachable(&self, endpoint: &str, timeout: Duration) -> Result<bool>;
    fn path_exists(&self, path: &Path) -> bool;
}

#[derive(Default)]
struct SystemSpacetimeRuntimeProbe;

impl SpacetimeRuntimeProbe for SystemSpacetimeRuntimeProbe {
    fn cli_available(&self, cli_path: &str) -> Result<bool> {
        match Command::new(cli_path)
            .arg("--help")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
        {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(TokenizorError::ControlPlane(format!(
                "failed to invoke `{cli_path}`: {error}"
            ))),
        }
    }

    fn endpoint_reachable(&self, endpoint: &str, timeout: Duration) -> Result<bool> {
        let authority = authority_from_endpoint(endpoint)?;
        let addresses = authority.to_socket_addrs().map_err(|error| {
            TokenizorError::Config(format!(
                "invalid SpacetimeDB endpoint `{endpoint}`: {error}"
            ))
        })?;

        for address in addresses {
            if TcpStream::connect_timeout(&address, timeout).is_ok() {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn path_exists(&self, path: &Path) -> bool {
        path.exists()
    }
}

const SPACETIMEDB_MUTABLE_STATE_MIGRATION_CHECK: &str = "spacetimedb_mutable_state_migration";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct LocalMutableStateSummary {
    run_count: usize,
    checkpoint_count: usize,
    file_record_count: usize,
    idempotency_record_count: usize,
    discovery_manifest_count: usize,
}

impl LocalMutableStateSummary {
    fn from_registry(data: &RegistryData) -> Self {
        Self {
            run_count: data.runs.len(),
            checkpoint_count: data.checkpoints.len(),
            file_record_count: data.run_file_records.values().map(std::vec::Vec::len).sum(),
            idempotency_record_count: data.idempotency_records.len(),
            discovery_manifest_count: data.discovery_manifests.len(),
        }
    }

    fn has_any(&self) -> bool {
        self.run_count > 0
            || self.checkpoint_count > 0
            || self.file_record_count > 0
            || self.idempotency_record_count > 0
            || self.discovery_manifest_count > 0
    }

    fn describe(&self) -> String {
        let mut parts = Vec::new();
        if self.run_count > 0 {
            parts.push(format!("runs={}", self.run_count));
        }
        if self.checkpoint_count > 0 {
            parts.push(format!("checkpoints={}", self.checkpoint_count));
        }
        if self.file_record_count > 0 {
            parts.push(format!("file_records={}", self.file_record_count));
        }
        if self.idempotency_record_count > 0 {
            parts.push(format!(
                "idempotency_records={}",
                self.idempotency_record_count
            ));
        }
        if self.discovery_manifest_count > 0 {
            parts.push(format!(
                "discovery_manifests={}",
                self.discovery_manifest_count
            ));
        }

        if parts.is_empty() {
            "no mutable registry state".to_string()
        } else {
            parts.join(", ")
        }
    }
}

pub struct SpacetimeControlPlane {
    config: SpacetimeDbConfig,
    registry: Arc<RegistryPersistence>,
    runtime_probe: Arc<dyn SpacetimeRuntimeProbe>,
    store: Arc<dyn SpacetimeStateStore>,
}

impl SpacetimeControlPlane {
    pub fn new(config: SpacetimeDbConfig, registry: Arc<RegistryPersistence>) -> Self {
        let store = Arc::new(SdkSpacetimeStateStore::new(
            config.endpoint.clone(),
            config.database.clone(),
        ));
        Self::with_dependencies(
            config,
            registry,
            Arc::new(SystemSpacetimeRuntimeProbe),
            store,
        )
    }

    fn with_dependencies(
        config: SpacetimeDbConfig,
        registry: Arc<RegistryPersistence>,
        runtime_probe: Arc<dyn SpacetimeRuntimeProbe>,
        store: Arc<dyn SpacetimeStateStore>,
    ) -> Self {
        Self {
            config,
            registry,
            runtime_probe,
            store,
        }
    }

    fn local_mutable_state_summary(&self) -> Result<LocalMutableStateSummary> {
        Ok(LocalMutableStateSummary::from_registry(
            &self.registry.load()?,
        ))
    }

    fn migration_remediation(&self) -> &'static str {
        "Run `tokenizor_agentic_mcp migrate control-plane` before starting new mutations, or clear the obsolete local run-state snapshot if it is no longer needed."
    }

    fn ensure_mutable_state_ready(&self) -> Result<()> {
        let summary = self.local_mutable_state_summary()?;
        if !summary.has_any() {
            return Ok(());
        }

        let store_has_state = self.store.has_any_mutable_state()?;
        let detail = if store_has_state {
            "SpacetimeDB already contains mutable operational state, so continuing would create an unsafe mixed-state authority boundary"
        } else {
            "SpacetimeDB does not yet contain the migrated mutable operational state"
        };

        Err(TokenizorError::RequestGated {
            gate_error: format!(
                "local registry still holds legacy mutable state ({}) and {detail}. {}",
                summary.describe(),
                self.migration_remediation()
            ),
        })
    }

    fn migration_check(&self) -> ComponentHealth {
        let summary = match self.local_mutable_state_summary() {
            Ok(summary) => summary,
            Err(error) => {
                return ComponentHealth::error(
                    SPACETIMEDB_MUTABLE_STATE_MIGRATION_CHECK,
                    HealthIssueCategory::Recovery,
                    format!("failed to inspect local mutable registry state: {error}"),
                    self.migration_remediation(),
                );
            }
        };

        if !summary.has_any() {
            return ComponentHealth::ok(
                SPACETIMEDB_MUTABLE_STATE_MIGRATION_CHECK,
                HealthIssueCategory::Recovery,
                "no local mutable run state is waiting for migration",
            );
        }

        match self.store.has_any_mutable_state() {
            Ok(true) => ComponentHealth::error(
                SPACETIMEDB_MUTABLE_STATE_MIGRATION_CHECK,
                HealthIssueCategory::Recovery,
                format!(
                    "local registry still holds mutable state ({}) while SpacetimeDB also contains mutable state",
                    summary.describe()
                ),
                self.migration_remediation(),
            ),
            Ok(false) => ComponentHealth::error(
                SPACETIMEDB_MUTABLE_STATE_MIGRATION_CHECK,
                HealthIssueCategory::Recovery,
                format!(
                    "local registry still holds mutable state ({}) that has not been migrated to SpacetimeDB",
                    summary.describe()
                ),
                self.migration_remediation(),
            ),
            Err(error) => ComponentHealth::error(
                SPACETIMEDB_MUTABLE_STATE_MIGRATION_CHECK,
                HealthIssueCategory::Recovery,
                format!("failed to verify SpacetimeDB mutable state before migration: {error}"),
                self.migration_remediation(),
            ),
        }
    }

    fn load_run_for_update(&self, run_id: &str) -> Result<IndexRun> {
        self.store.find_run(run_id)?.ok_or_else(|| {
            TokenizorError::NotFound(format!("run `{run_id}` not found in control plane"))
        })
    }

    fn load_repository_for_update(&self, repo_id: &str) -> Result<Repository> {
        self.store
            .get_repository(repo_id)?
            .ok_or_else(|| TokenizorError::NotFound(format!("repository not found: {repo_id}")))
    }

    fn migrate_mutable_state_from_registry_inner(&self) -> Result<()> {
        let data = self.registry.load()?;

        for repository in data.repositories.values() {
            self.store.save_repository(repository)?;
        }
        for run in &data.runs {
            self.store.save_run(run)?;
        }
        for (run_id, records) in &data.run_file_records {
            self.store.save_file_records(run_id, records)?;
        }
        for checkpoint in &data.checkpoints {
            self.store.save_checkpoint(checkpoint)?;
        }
        for record in &data.idempotency_records {
            self.store.save_idempotency_record(record)?;
        }
        for manifest in data.discovery_manifests.values() {
            self.store.save_discovery_manifest(manifest)?;
        }

        self.registry.clear_mutable_state()
    }

    pub fn migrate_mutable_state_from_registry(&self) -> Result<()> {
        self.migrate_mutable_state_from_registry_inner()
    }

    fn cli_check(&self) -> ComponentHealth {
        if self.config.cli_path.trim().is_empty() {
            return ComponentHealth::error(
                "spacetimedb_cli",
                HealthIssueCategory::Configuration,
                "SpacetimeDB CLI path is empty",
                "Set TOKENIZOR_SPACETIMEDB_CLI to the SpacetimeDB CLI binary name or absolute path.",
            );
        }

        match self.runtime_probe.cli_available(&self.config.cli_path) {
            Ok(true) => ComponentHealth::ok(
                "spacetimedb_cli",
                HealthIssueCategory::Dependency,
                format!(
                    "`{}` is available for operator commands",
                    self.config.cli_path
                ),
            ),
            Ok(false) => ComponentHealth::error(
                "spacetimedb_cli",
                HealthIssueCategory::Dependency,
                format!(
                    "`{}` is not installed or not available on PATH",
                    self.config.cli_path
                ),
                "Install the SpacetimeDB CLI and ensure the configured path resolves before running doctor or init.",
            ),
            Err(error) => ComponentHealth::error(
                "spacetimedb_cli",
                HealthIssueCategory::Dependency,
                error.to_string(),
                "Verify the configured CLI path points to an executable SpacetimeDB binary.",
            ),
        }
    }

    fn endpoint_check(&self, component_name: &str) -> ComponentHealth {
        if self.config.endpoint.trim().is_empty() {
            return ComponentHealth::error(
                component_name,
                HealthIssueCategory::Configuration,
                "SpacetimeDB endpoint is empty",
                "Set TOKENIZOR_SPACETIMEDB_ENDPOINT to the local SpacetimeDB HTTP endpoint, such as http://127.0.0.1:3007.",
            );
        }

        match self
            .runtime_probe
            .endpoint_reachable(&self.config.endpoint, Duration::from_millis(500))
        {
            Ok(true) => ComponentHealth::ok(
                component_name,
                HealthIssueCategory::Dependency,
                format!("SpacetimeDB endpoint {} is reachable", self.config.endpoint),
            ),
            Ok(false) => ComponentHealth::error(
                component_name,
                HealthIssueCategory::Dependency,
                format!(
                    "SpacetimeDB endpoint {} is not reachable",
                    self.config.endpoint
                ),
                "Start the local SpacetimeDB runtime or correct TOKENIZOR_SPACETIMEDB_ENDPOINT before retrying.",
            ),
            Err(TokenizorError::Config(detail)) => ComponentHealth::error(
                component_name,
                HealthIssueCategory::Configuration,
                detail,
                "Fix TOKENIZOR_SPACETIMEDB_ENDPOINT so it contains a valid host and optional port.",
            ),
            Err(error) => ComponentHealth::error(
                component_name,
                HealthIssueCategory::Dependency,
                error.to_string(),
                "Verify the local SpacetimeDB runtime is reachable and the endpoint is resolvable from this machine.",
            ),
        }
    }

    fn database_check(&self) -> ComponentHealth {
        if self.config.database.trim().is_empty() {
            ComponentHealth::error(
                "spacetimedb_database",
                HealthIssueCategory::Configuration,
                "SpacetimeDB database name is empty",
                "Set TOKENIZOR_SPACETIMEDB_DATABASE to the target database name before running doctor or init.",
            )
        } else {
            ComponentHealth::ok(
                "spacetimedb_database",
                HealthIssueCategory::Configuration,
                format!("database `{}` is configured", self.config.database),
            )
        }
    }

    fn module_path_check(&self) -> ComponentHealth {
        if self.config.module_path.as_os_str().is_empty() {
            return ComponentHealth::error(
                "spacetimedb_module_path",
                HealthIssueCategory::Configuration,
                "SpacetimeDB module path is empty",
                "Set TOKENIZOR_SPACETIMEDB_MODULE_PATH to the local module directory before running bootstrap flows.",
            );
        }

        if self.runtime_probe.path_exists(&self.config.module_path) {
            ComponentHealth::ok(
                "spacetimedb_module_path",
                HealthIssueCategory::Bootstrap,
                format!(
                    "module path {} is present",
                    self.config.module_path.display()
                ),
            )
        } else {
            ComponentHealth::error(
                "spacetimedb_module_path",
                HealthIssueCategory::Bootstrap,
                format!(
                    "module path {} does not exist",
                    self.config.module_path.display()
                ),
                "Build or place the SpacetimeDB module at the configured path, or update TOKENIZOR_SPACETIMEDB_MODULE_PATH.",
            )
        }
    }

    fn schema_compatibility_check(&self) -> ComponentHealth {
        if self.config.schema_version != SUPPORTED_SPACETIMEDB_SCHEMA_VERSION {
            return ComponentHealth::error(
                "spacetimedb_schema_compatibility",
                HealthIssueCategory::Compatibility,
                format!(
                    "configured schema version {} does not match Tokenizor's supported schema version {}",
                    self.config.schema_version, SUPPORTED_SPACETIMEDB_SCHEMA_VERSION
                ),
                format!(
                    "Set TOKENIZOR_SPACETIMEDB_SCHEMA_VERSION={} or upgrade Tokenizor to a build that supports schema version {}.",
                    SUPPORTED_SPACETIMEDB_SCHEMA_VERSION, self.config.schema_version
                ),
            );
        }

        ComponentHealth::warning(
            "spacetimedb_schema_compatibility",
            HealthIssueCategory::Compatibility,
            format!(
                "configured schema version {} matches Tokenizor's current expectation, but doctor cannot yet verify the published module schema",
                self.config.schema_version
            ),
            "Treat this as an operator warning only; if startup still fails later, re-run doctor after the compatibility probe is expanded.",
        )
    }
}

impl ControlPlane for SpacetimeControlPlane {
    fn backend_name(&self) -> &'static str {
        "spacetimedb"
    }

    fn health_check(&self) -> Result<ComponentHealth> {
        Ok(self.endpoint_check("control_plane"))
    }

    fn deployment_checks(&self) -> Result<Vec<ComponentHealth>> {
        Ok(vec![
            self.database_check(),
            self.schema_compatibility_check(),
            self.cli_check(),
            self.endpoint_check("spacetimedb_endpoint"),
            self.module_path_check(),
            self.migration_check(),
        ])
    }

    fn find_run(&self, run_id: &str) -> Result<Option<IndexRun>> {
        self.ensure_mutable_state_ready()?;
        self.store.find_run(run_id)
    }

    fn find_runs_by_status(&self, status: &IndexRunStatus) -> Result<Vec<IndexRun>> {
        self.ensure_mutable_state_ready()?;
        self.store.find_runs_by_status(status)
    }

    fn list_runs(&self) -> Result<Vec<IndexRun>> {
        self.ensure_mutable_state_ready()?;
        self.store.list_runs()
    }

    fn get_runs_by_repo(&self, repo_id: &str) -> Result<Vec<IndexRun>> {
        self.ensure_mutable_state_ready()?;
        self.store.get_runs_by_repo(repo_id)
    }

    fn get_latest_completed_run(&self, repo_id: &str) -> Result<Option<IndexRun>> {
        self.ensure_mutable_state_ready()?;
        self.store.get_latest_completed_run(repo_id)
    }

    fn get_repository(&self, repo_id: &str) -> Result<Option<Repository>> {
        self.store.get_repository(repo_id)
    }

    fn get_file_records(&self, run_id: &str) -> Result<Vec<FileRecord>> {
        self.ensure_mutable_state_ready()?;
        self.store.get_file_records(run_id)
    }

    fn get_latest_checkpoint(&self, run_id: &str) -> Result<Option<Checkpoint>> {
        self.ensure_mutable_state_ready()?;
        self.store.get_latest_checkpoint(run_id)
    }

    fn find_idempotency_record(&self, key: &str) -> Result<Option<IdempotencyRecord>> {
        self.ensure_mutable_state_ready()?;
        self.store.find_idempotency_record(key)
    }

    fn get_discovery_manifest(&self, run_id: &str) -> Result<Option<DiscoveryManifest>> {
        self.ensure_mutable_state_ready()?;
        self.store.get_discovery_manifest(run_id)
    }

    fn save_run(&self, run: &IndexRun) -> Result<()> {
        self.ensure_mutable_state_ready()?;
        self.store.save_run(run)
    }

    fn update_run_status(
        &self,
        run_id: &str,
        status: IndexRunStatus,
        error_summary: Option<String>,
    ) -> Result<()> {
        self.ensure_mutable_state_ready()?;
        let mut run = self.load_run_for_update(run_id)?;
        run.status = status;
        run.error_summary = error_summary;
        self.store.save_run(&run)
    }

    fn transition_to_running(&self, run_id: &str, started_at_unix_ms: u64) -> Result<()> {
        self.ensure_mutable_state_ready()?;
        let mut run = self.load_run_for_update(run_id)?;
        if run.status.is_terminal() && run.status != IndexRunStatus::Interrupted {
            return Ok(());
        }
        run.status = IndexRunStatus::Running;
        if run.started_at_unix_ms.is_none() {
            run.started_at_unix_ms = Some(started_at_unix_ms);
        }
        self.store.save_run(&run)
    }

    fn update_run_status_with_finish(
        &self,
        run_id: &str,
        status: IndexRunStatus,
        error_summary: Option<String>,
        finished_at_unix_ms: u64,
        not_yet_supported: Option<BTreeMap<crate::domain::LanguageId, u64>>,
    ) -> Result<()> {
        self.ensure_mutable_state_ready()?;
        let mut run = self.load_run_for_update(run_id)?;
        run.status = status;
        run.finished_at_unix_ms = Some(finished_at_unix_ms);
        run.error_summary = error_summary;
        run.not_yet_supported = not_yet_supported;
        self.store.save_run(&run)
    }

    fn cancel_run_if_active(&self, run_id: &str, finished_at_unix_ms: u64) -> Result<bool> {
        self.ensure_mutable_state_ready()?;
        let mut run = self.load_run_for_update(run_id)?;
        if run.status.is_terminal() {
            return Ok(false);
        }
        run.status = IndexRunStatus::Cancelled;
        run.finished_at_unix_ms = Some(finished_at_unix_ms);
        self.store.save_run(&run)?;
        Ok(true)
    }

    fn save_file_records(&self, run_id: &str, records: &[FileRecord]) -> Result<()> {
        self.ensure_mutable_state_ready()?;
        self.store.save_file_records(run_id, records)
    }

    fn save_checkpoint(&self, checkpoint: &Checkpoint) -> Result<()> {
        self.ensure_mutable_state_ready()?;
        let mut run = self.load_run_for_update(&checkpoint.run_id)?;
        if run.status.is_terminal() {
            return Err(TokenizorError::InvalidOperation(format!(
                "cannot checkpoint run `{}` with terminal status `{:?}`",
                checkpoint.run_id, run.status
            )));
        }
        self.store.save_checkpoint(checkpoint)?;
        run.checkpoint_cursor = Some(checkpoint.cursor.clone());
        self.store.save_run(&run)
    }

    fn save_repository(&self, repository: &Repository) -> Result<()> {
        self.store.save_repository(repository)
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
        let mut repository = self.load_repository_for_update(repo_id)?;
        repository.status = status;
        repository.invalidated_at_unix_ms = invalidated_at_unix_ms;
        repository.invalidation_reason = invalidation_reason;
        repository.quarantined_at_unix_ms = quarantined_at_unix_ms;
        repository.quarantine_reason = quarantine_reason;
        self.store.save_repository(&repository)
    }

    fn save_idempotency_record(&self, record: &IdempotencyRecord) -> Result<()> {
        self.ensure_mutable_state_ready()?;
        self.store.save_idempotency_record(record)
    }

    fn save_discovery_manifest(&self, manifest: &DiscoveryManifest) -> Result<()> {
        self.ensure_mutable_state_ready()?;
        self.store.save_discovery_manifest(manifest)
    }

    fn migrate_mutable_state_from_registry(&self) -> Result<()> {
        self.migrate_mutable_state_from_registry_inner()
    }
}

fn authority_from_endpoint(endpoint: &str) -> Result<String> {
    let without_scheme = endpoint.split("://").nth(1).unwrap_or(endpoint);
    let authority = without_scheme.split('/').next().unwrap_or_default().trim();

    if authority.is_empty() {
        return Err(TokenizorError::Config(format!(
            "SpacetimeDB endpoint `{endpoint}` is missing a host"
        )));
    }

    if authority.contains(':') {
        Ok(authority.to_string())
    } else if endpoint.starts_with("https://") {
        Ok(format!("{authority}:443"))
    } else {
        Ok(format!("{authority}:80"))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ControlPlane, SPACETIMEDB_MUTABLE_STATE_MIGRATION_CHECK, SpacetimeControlPlane,
        SpacetimeRuntimeProbe, authority_from_endpoint,
    };
    use crate::config::{SUPPORTED_SPACETIMEDB_SCHEMA_VERSION, SpacetimeDbConfig};
    use crate::domain::{
        Checkpoint, ComponentHealth, DiscoveryManifest, FileRecord, HealthIssueCategory,
        HealthSeverity, HealthStatus, IdempotencyRecord, IndexRun, IndexRunMode, IndexRunStatus,
        LanguageId, PersistedFileOutcome, ProjectIdentityKind, Repository, RepositoryKind,
        RepositoryStatus,
    };
    use crate::error::{Result, TokenizorError};
    use crate::storage::{RegistryPersistence, SpacetimeStateStore};
    use std::collections::{BTreeMap, HashSet};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tempfile::TempDir;

    struct FakeProbe {
        cli_available: bool,
        cli_error: Option<String>,
        endpoint_reachable: bool,
        endpoint_config_error: Option<String>,
        endpoint_probe_error: Option<String>,
        existing_paths: HashSet<PathBuf>,
    }

    impl Default for FakeProbe {
        fn default() -> Self {
            let mut existing_paths = HashSet::new();
            existing_paths.insert(PathBuf::from("spacetime/tokenizor"));

            Self {
                cli_available: true,
                cli_error: None,
                endpoint_reachable: true,
                endpoint_config_error: None,
                endpoint_probe_error: None,
                existing_paths,
            }
        }
    }

    impl SpacetimeRuntimeProbe for FakeProbe {
        fn cli_available(&self, _cli_path: &str) -> Result<bool> {
            if let Some(message) = &self.cli_error {
                Err(TokenizorError::ControlPlane(message.clone()))
            } else {
                Ok(self.cli_available)
            }
        }

        fn endpoint_reachable(&self, _endpoint: &str, _timeout: Duration) -> Result<bool> {
            if let Some(message) = &self.endpoint_config_error {
                Err(TokenizorError::Config(message.clone()))
            } else if let Some(message) = &self.endpoint_probe_error {
                Err(TokenizorError::ControlPlane(message.clone()))
            } else {
                Ok(self.endpoint_reachable)
            }
        }

        fn path_exists(&self, path: &Path) -> bool {
            self.existing_paths.contains(path)
        }
    }

    #[derive(Default)]
    struct FakeSpacetimeState {
        runs: Vec<IndexRun>,
        repositories: BTreeMap<String, Repository>,
        file_records: BTreeMap<String, Vec<FileRecord>>,
        checkpoints: Vec<Checkpoint>,
        idempotency_records: Vec<IdempotencyRecord>,
        discovery_manifests: BTreeMap<String, DiscoveryManifest>,
    }

    #[derive(Default)]
    struct FakeSpacetimeStore {
        state: Mutex<FakeSpacetimeState>,
    }

    impl FakeSpacetimeStore {
        fn with_state<T>(&self, action: impl FnOnce(&mut FakeSpacetimeState) -> T) -> T {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            action(&mut state)
        }
    }

    impl SpacetimeStateStore for FakeSpacetimeStore {
        fn find_run(&self, run_id: &str) -> Result<Option<IndexRun>> {
            Ok(
                self.with_state(|state| {
                    state.runs.iter().find(|run| run.run_id == run_id).cloned()
                }),
            )
        }

        fn find_runs_by_status(&self, status: &IndexRunStatus) -> Result<Vec<IndexRun>> {
            Ok(self.with_state(|state| {
                state
                    .runs
                    .iter()
                    .filter(|run| &run.status == status)
                    .cloned()
                    .collect()
            }))
        }

        fn list_runs(&self) -> Result<Vec<IndexRun>> {
            Ok(self.with_state(|state| state.runs.clone()))
        }

        fn get_runs_by_repo(&self, repo_id: &str) -> Result<Vec<IndexRun>> {
            Ok(self.with_state(|state| {
                let mut runs: Vec<IndexRun> = state
                    .runs
                    .iter()
                    .filter(|run| run.repo_id == repo_id)
                    .cloned()
                    .collect();
                runs.sort_by(|left, right| {
                    right.requested_at_unix_ms.cmp(&left.requested_at_unix_ms)
                });
                runs
            }))
        }

        fn get_latest_completed_run(&self, repo_id: &str) -> Result<Option<IndexRun>> {
            Ok(self.with_state(|state| {
                state
                    .runs
                    .iter()
                    .filter(|run| run.repo_id == repo_id && run.status == IndexRunStatus::Succeeded)
                    .max_by_key(|run| run.requested_at_unix_ms)
                    .cloned()
            }))
        }

        fn get_repository(&self, repo_id: &str) -> Result<Option<Repository>> {
            Ok(self.with_state(|state| state.repositories.get(repo_id).cloned()))
        }

        fn get_file_records(&self, run_id: &str) -> Result<Vec<FileRecord>> {
            Ok(
                self.with_state(|state| {
                    state.file_records.get(run_id).cloned().unwrap_or_default()
                }),
            )
        }

        fn get_latest_checkpoint(&self, run_id: &str) -> Result<Option<Checkpoint>> {
            Ok(self.with_state(|state| {
                state
                    .checkpoints
                    .iter()
                    .filter(|checkpoint| checkpoint.run_id == run_id)
                    .max_by_key(|checkpoint| checkpoint.created_at_unix_ms)
                    .cloned()
            }))
        }

        fn find_idempotency_record(&self, key: &str) -> Result<Option<IdempotencyRecord>> {
            Ok(self.with_state(|state| {
                state
                    .idempotency_records
                    .iter()
                    .find(|record| record.idempotency_key == key)
                    .cloned()
            }))
        }

        fn get_discovery_manifest(&self, run_id: &str) -> Result<Option<DiscoveryManifest>> {
            Ok(self.with_state(|state| state.discovery_manifests.get(run_id).cloned()))
        }

        fn save_run(&self, run: &IndexRun) -> Result<()> {
            self.with_state(|state| {
                if let Some(existing) = state
                    .runs
                    .iter_mut()
                    .find(|existing| existing.run_id == run.run_id)
                {
                    *existing = run.clone();
                } else {
                    state.runs.push(run.clone());
                }
            });
            Ok(())
        }

        fn save_file_records(&self, run_id: &str, records: &[FileRecord]) -> Result<()> {
            self.with_state(|state| {
                let entry = state
                    .file_records
                    .entry(run_id.to_string())
                    .or_insert_with(Vec::new);
                let mut merged = BTreeMap::new();
                for record in entry.iter().cloned() {
                    merged.insert(record.relative_path.clone(), record);
                }
                for record in records.iter().cloned() {
                    merged.insert(record.relative_path.clone(), record);
                }
                let mut merged_records: Vec<FileRecord> = merged.into_values().collect();
                merged_records.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
                *entry = merged_records;
            });
            Ok(())
        }

        fn save_checkpoint(&self, checkpoint: &Checkpoint) -> Result<()> {
            self.with_state(|state| state.checkpoints.push(checkpoint.clone()));
            Ok(())
        }

        fn save_repository(&self, repository: &Repository) -> Result<()> {
            self.with_state(|state| {
                state
                    .repositories
                    .insert(repository.repo_id.clone(), repository.clone());
            });
            Ok(())
        }

        fn save_idempotency_record(&self, record: &IdempotencyRecord) -> Result<()> {
            self.with_state(|state| {
                if let Some(existing) = state
                    .idempotency_records
                    .iter_mut()
                    .find(|existing| existing.idempotency_key == record.idempotency_key)
                {
                    *existing = record.clone();
                } else {
                    state.idempotency_records.push(record.clone());
                }
            });
            Ok(())
        }

        fn save_discovery_manifest(&self, manifest: &DiscoveryManifest) -> Result<()> {
            self.with_state(|state| {
                state
                    .discovery_manifests
                    .insert(manifest.run_id.clone(), manifest.clone());
            });
            Ok(())
        }

        fn has_any_mutable_state(&self) -> Result<bool> {
            Ok(self.with_state(|state| {
                !state.runs.is_empty()
                    || !state.file_records.is_empty()
                    || !state.checkpoints.is_empty()
                    || !state.idempotency_records.is_empty()
                    || !state.discovery_manifests.is_empty()
            }))
        }
    }

    fn base_config() -> SpacetimeDbConfig {
        SpacetimeDbConfig {
            cli_path: "spacetimedb".to_string(),
            endpoint: "http://127.0.0.1:3007".to_string(),
            database: "tokenizor".to_string(),
            module_path: PathBuf::from("spacetime/tokenizor"),
            schema_version: SUPPORTED_SPACETIMEDB_SCHEMA_VERSION,
        }
    }

    fn temp_registry() -> (TempDir, Arc<RegistryPersistence>) {
        let temp_dir = tempfile::tempdir().expect("temp dir should create");
        let registry = Arc::new(RegistryPersistence::new(
            temp_dir.path().join("registry.json"),
        ));
        (temp_dir, registry)
    }

    fn control_plane_with_probe(
        config: SpacetimeDbConfig,
        probe: FakeProbe,
    ) -> (TempDir, SpacetimeControlPlane) {
        let (temp_dir, registry) = temp_registry();
        let control_plane = SpacetimeControlPlane::with_dependencies(
            config,
            registry,
            Arc::new(probe),
            Arc::new(FakeSpacetimeStore::default()),
        );
        (temp_dir, control_plane)
    }

    fn control_plane_with_store(
        config: SpacetimeDbConfig,
        registry: Arc<RegistryPersistence>,
        probe: FakeProbe,
        store: Arc<dyn SpacetimeStateStore>,
    ) -> SpacetimeControlPlane {
        SpacetimeControlPlane::with_dependencies(config, registry, Arc::new(probe), store)
    }

    fn sample_repository(repo_id: &str) -> Repository {
        Repository {
            repo_id: repo_id.to_string(),
            kind: RepositoryKind::Local,
            root_uri: format!("file:///tmp/{repo_id}"),
            project_identity: repo_id.to_string(),
            project_identity_kind: ProjectIdentityKind::LocalRootPath,
            default_branch: Some("main".to_string()),
            last_known_revision: Some("abc123".to_string()),
            status: RepositoryStatus::Ready,
            invalidated_at_unix_ms: None,
            invalidation_reason: None,
            quarantined_at_unix_ms: None,
            quarantine_reason: None,
        }
    }

    fn sample_run(run_id: &str, repo_id: &str, status: IndexRunStatus) -> IndexRun {
        IndexRun {
            run_id: run_id.to_string(),
            repo_id: repo_id.to_string(),
            mode: IndexRunMode::Full,
            status,
            requested_at_unix_ms: 1_700_000_000_000,
            started_at_unix_ms: Some(1_700_000_000_100),
            finished_at_unix_ms: None,
            idempotency_key: Some(format!("key-{run_id}")),
            request_hash: Some("hash-1".to_string()),
            checkpoint_cursor: None,
            error_summary: None,
            not_yet_supported: None,
            prior_run_id: None,
            description: Some("test run".to_string()),
            recovery_state: None,
        }
    }

    fn sample_file_record(run_id: &str, repo_id: &str, relative_path: &str) -> FileRecord {
        FileRecord {
            relative_path: relative_path.to_string(),
            language: LanguageId::Rust,
            blob_id: "blob-1".to_string(),
            byte_len: 12,
            content_hash: "deadbeef".to_string(),
            outcome: PersistedFileOutcome::Committed,
            symbols: Vec::new(),
            run_id: run_id.to_string(),
            repo_id: repo_id.to_string(),
            committed_at_unix_ms: 1_700_000_000_200,
        }
    }

    fn sample_checkpoint(run_id: &str, cursor: &str) -> Checkpoint {
        Checkpoint {
            run_id: run_id.to_string(),
            cursor: cursor.to_string(),
            files_processed: 1,
            symbols_written: 2,
            created_at_unix_ms: 1_700_000_000_300,
        }
    }

    fn sample_idempotency_record(run_id: &str) -> IdempotencyRecord {
        IdempotencyRecord {
            operation: "index_repository".to_string(),
            idempotency_key: format!("key-{run_id}"),
            request_hash: "hash-1".to_string(),
            status: crate::domain::IdempotencyStatus::Succeeded,
            result_ref: Some(run_id.to_string()),
            created_at_unix_ms: 1_700_000_000_400,
            expires_at_unix_ms: None,
        }
    }

    fn sample_discovery_manifest(run_id: &str) -> DiscoveryManifest {
        DiscoveryManifest {
            run_id: run_id.to_string(),
            discovered_at_unix_ms: 1_700_000_000_500,
            relative_paths: vec!["src/lib.rs".to_string(), "src/main.rs".to_string()],
        }
    }

    fn find_check<'a>(checks: &'a [ComponentHealth], name: &str) -> &'a ComponentHealth {
        checks
            .iter()
            .find(|check| check.name == name)
            .expect("expected check to be present")
    }

    #[test]
    fn derives_default_http_port() {
        assert_eq!(
            authority_from_endpoint("http://127.0.0.1").expect("authority should parse"),
            "127.0.0.1:80"
        );
    }

    #[test]
    fn preserves_explicit_port() {
        assert_eq!(
            authority_from_endpoint("http://127.0.0.1:3007/v1").expect("authority should parse"),
            "127.0.0.1:3007"
        );
    }

    #[test]
    fn reports_missing_cli_as_dependency_error() {
        let probe = FakeProbe {
            cli_available: false,
            ..FakeProbe::default()
        };
        let (_temp_dir, control_plane) = control_plane_with_probe(base_config(), probe);

        let checks = control_plane
            .deployment_checks()
            .expect("deployment checks should succeed");
        let cli = find_check(&checks, "spacetimedb_cli");

        assert_eq!(cli.status, HealthStatus::Unavailable);
        assert_eq!(cli.category, HealthIssueCategory::Dependency);
        assert_eq!(cli.severity, HealthSeverity::Error);
        assert!(
            cli.remediation
                .as_deref()
                .expect("remediation should be present")
                .contains("Install the SpacetimeDB CLI")
        );
    }

    #[test]
    fn reports_unreachable_endpoint_as_dependency_error() {
        let probe = FakeProbe {
            endpoint_reachable: false,
            ..FakeProbe::default()
        };
        let (_temp_dir, control_plane) = control_plane_with_probe(base_config(), probe);

        let checks = control_plane
            .deployment_checks()
            .expect("deployment checks should succeed");
        let endpoint = find_check(&checks, "spacetimedb_endpoint");

        assert_eq!(endpoint.status, HealthStatus::Unavailable);
        assert_eq!(endpoint.category, HealthIssueCategory::Dependency);
        assert_eq!(endpoint.severity, HealthSeverity::Error);
        assert!(
            endpoint
                .remediation
                .as_deref()
                .expect("remediation should be present")
                .contains("Start the local SpacetimeDB runtime")
        );
    }

    #[test]
    fn reports_empty_database_as_configuration_error() {
        let mut config = base_config();
        config.database.clear();
        let (_temp_dir, control_plane) = control_plane_with_probe(config, FakeProbe::default());

        let checks = control_plane
            .deployment_checks()
            .expect("deployment checks should succeed");
        let database = find_check(&checks, "spacetimedb_database");

        assert_eq!(database.status, HealthStatus::Unavailable);
        assert_eq!(database.category, HealthIssueCategory::Configuration);
        assert_eq!(database.severity, HealthSeverity::Error);
    }

    #[test]
    fn reports_missing_module_path_as_bootstrap_error() {
        let (_temp_dir, control_plane) = control_plane_with_probe(
            base_config(),
            FakeProbe {
                existing_paths: HashSet::new(),
                ..FakeProbe::default()
            },
        );

        let checks = control_plane
            .deployment_checks()
            .expect("deployment checks should succeed");
        let module_path = find_check(&checks, "spacetimedb_module_path");

        assert_eq!(module_path.status, HealthStatus::Unavailable);
        assert_eq!(module_path.category, HealthIssueCategory::Bootstrap);
        assert_eq!(module_path.severity, HealthSeverity::Error);
    }

    #[test]
    fn reports_schema_version_mismatch_as_compatibility_error() {
        let mut config = base_config();
        config.schema_version = SUPPORTED_SPACETIMEDB_SCHEMA_VERSION + 1;
        let (_temp_dir, control_plane) = control_plane_with_probe(config, FakeProbe::default());

        let checks = control_plane
            .deployment_checks()
            .expect("deployment checks should succeed");
        let compatibility = find_check(&checks, "spacetimedb_schema_compatibility");

        assert_eq!(compatibility.status, HealthStatus::Unavailable);
        assert_eq!(compatibility.category, HealthIssueCategory::Compatibility);
        assert_eq!(compatibility.severity, HealthSeverity::Error);
    }

    #[test]
    fn reports_schema_verification_gap_as_non_blocking_warning() {
        let (_temp_dir, control_plane) =
            control_plane_with_probe(base_config(), FakeProbe::default());

        let checks = control_plane
            .deployment_checks()
            .expect("deployment checks should succeed");
        let compatibility = find_check(&checks, "spacetimedb_schema_compatibility");

        assert_eq!(compatibility.status, HealthStatus::Degraded);
        assert_eq!(compatibility.category, HealthIssueCategory::Compatibility);
        assert_eq!(compatibility.severity, HealthSeverity::Warning);
        assert!(
            compatibility
                .remediation
                .as_deref()
                .expect("remediation should be present")
                .contains("operator warning")
        );
    }

    #[test]
    fn reports_local_mutable_state_needing_migration_as_recovery_error() {
        let (_temp_dir, registry) = temp_registry();
        let repo = sample_repository("repo-1");
        let run = sample_run("run-1", &repo.repo_id, IndexRunStatus::Queued);
        registry
            .save_repository(&repo)
            .expect("repo should persist");
        registry.save_run(&run).expect("run should persist");

        let control_plane = control_plane_with_store(
            base_config(),
            registry,
            FakeProbe::default(),
            Arc::new(FakeSpacetimeStore::default()),
        );

        let checks = control_plane
            .deployment_checks()
            .expect("deployment checks should succeed");
        let migration = find_check(&checks, SPACETIMEDB_MUTABLE_STATE_MIGRATION_CHECK);

        assert_eq!(migration.status, HealthStatus::Unavailable);
        assert_eq!(migration.category, HealthIssueCategory::Recovery);
        assert!(migration.detail.contains("runs=1"));
        assert!(
            migration
                .remediation
                .as_deref()
                .expect("remediation should be present")
                .contains("migrate control-plane")
        );
    }

    #[test]
    fn blocks_authoritative_run_queries_until_local_mutable_state_is_migrated() {
        let (_temp_dir, registry) = temp_registry();
        let repo = sample_repository("repo-1");
        let run = sample_run("run-1", &repo.repo_id, IndexRunStatus::Queued);
        registry
            .save_repository(&repo)
            .expect("repo should persist");
        registry.save_run(&run).expect("run should persist");

        let control_plane = control_plane_with_store(
            base_config(),
            registry,
            FakeProbe::default(),
            Arc::new(FakeSpacetimeStore::default()),
        );

        let error = control_plane
            .list_runs()
            .expect_err("mixed-state reads should be gated");
        assert!(matches!(error, TokenizorError::RequestGated { .. }));
        assert!(error.to_string().contains("migrate control-plane"));
    }

    #[test]
    fn migrates_local_mutable_state_into_spacetimedb_and_clears_registry_mutable_sections() {
        let (_temp_dir, registry) = temp_registry();
        let repo = sample_repository("repo-1");
        let mut run = sample_run("run-1", &repo.repo_id, IndexRunStatus::Running);
        let file_record = sample_file_record(&run.run_id, &run.repo_id, "src/lib.rs");
        let checkpoint = sample_checkpoint(&run.run_id, "src/lib.rs");
        run.checkpoint_cursor = Some(checkpoint.cursor.clone());
        let idempotency = sample_idempotency_record(&run.run_id);
        let manifest = sample_discovery_manifest(&run.run_id);

        registry
            .save_repository(&repo)
            .expect("repo should persist");
        registry.save_run(&run).expect("run should persist");
        registry
            .save_file_records(&run.run_id, std::slice::from_ref(&file_record))
            .expect("file records should persist");
        registry
            .save_checkpoint(&checkpoint)
            .expect("checkpoint should persist");
        registry
            .save_idempotency_record(&idempotency)
            .expect("idempotency record should persist");
        registry
            .save_discovery_manifest(&manifest)
            .expect("manifest should persist");

        let control_plane = control_plane_with_store(
            base_config(),
            Arc::clone(&registry),
            FakeProbe::default(),
            Arc::new(FakeSpacetimeStore::default()),
        );

        control_plane
            .migrate_mutable_state_from_registry()
            .expect("migration should succeed");

        assert_eq!(
            control_plane
                .find_run(&run.run_id)
                .expect("control-plane lookup should succeed"),
            Some(run.clone())
        );
        assert_eq!(
            control_plane
                .get_file_records(&run.run_id)
                .expect("file records should load"),
            vec![file_record]
        );
        assert_eq!(
            control_plane
                .get_latest_checkpoint(&run.run_id)
                .expect("checkpoint should load"),
            Some(checkpoint)
        );
        assert_eq!(
            control_plane
                .find_idempotency_record(&idempotency.idempotency_key)
                .expect("idempotency record should load"),
            Some(idempotency)
        );
        assert_eq!(
            control_plane
                .get_discovery_manifest(&run.run_id)
                .expect("manifest should load"),
            Some(manifest)
        );
        assert_eq!(
            control_plane
                .get_repository(&repo.repo_id)
                .expect("repository should load"),
            Some(repo)
        );

        let registry_data = registry.load().expect("registry should still load");
        assert!(registry_data.runs.is_empty());
        assert!(registry_data.run_file_records.is_empty());
        assert!(registry_data.checkpoints.is_empty());
        assert!(registry_data.idempotency_records.is_empty());
        assert!(registry_data.discovery_manifests.is_empty());
        assert_eq!(registry_data.repositories.len(), 1);
    }

    #[test]
    fn reports_configuration_derived_findings_before_runtime_probe_findings() {
        let (_temp_dir, control_plane) = control_plane_with_probe(
            base_config(),
            FakeProbe {
                cli_available: false,
                endpoint_reachable: false,
                existing_paths: HashSet::new(),
                ..FakeProbe::default()
            },
        );

        let checks = control_plane
            .deployment_checks()
            .expect("deployment checks should succeed");
        let names = checks
            .iter()
            .map(|check| check.name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "spacetimedb_database",
                "spacetimedb_schema_compatibility",
                "spacetimedb_cli",
                "spacetimedb_endpoint",
                "spacetimedb_module_path",
                "spacetimedb_mutable_state_migration",
            ]
        );
    }
}
