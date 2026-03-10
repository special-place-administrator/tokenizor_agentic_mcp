use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::domain::{
    Checkpoint, ComponentHealth, DiscoveryManifest, FileHealthSummary, FileOutcome,
    FileOutcomeSummary, FileProcessingResult, FileRecord, HealthIssueCategory, IdempotencyRecord,
    IdempotencyStatus, IndexRun, IndexRunMode, IndexRunStatus, IntegrityEventKind,
    InvalidationResult, LanguageId, NextAction, OperationalEvent, OperationalEventFilter,
    OperationalEventKind, PersistedFileOutcome, ProjectIdentityKind, RecoveryStateKind,
    RepairEvent, RepairOutcome, RepairResult, RepairScope, Repository, RepositoryHealthReport,
    RepositoryKind, RepositoryStatus, ResumeRejectReason, ResumeRunOutcome, RunHealth,
    RunHealthSummary, RunPhase, RunProgressSnapshot, RunRecoveryState, RunStatusReport,
    StatusContext, classify_repository_action, classify_run_action, unix_timestamp_ms,
    STALE_QUEUED_ABORTED_SUMMARY,
};
use crate::error::{Result, TokenizorError};
use crate::indexing::pipeline::{IndexingPipeline, PipelineProgress, PipelineResumeState};
use crate::storage::BlobStore;
use crate::storage::digest_hex;
use crate::storage::registry_persistence::is_owned_registry_temp_artifact_path;
use crate::storage::{
    ControlPlane, LocalCasBlobStore, RegistryBackedControlPlane, RegistryPersistence, RegistryQuery,
};

pub struct ActiveRun {
    pub run_id: String,
    pub handle: JoinHandle<()>,
    pub cancellation_token: CancellationToken,
    pub progress: Option<Arc<PipelineProgress>>,
    pub checkpoint_cursor_fn: Option<Arc<dyn Fn() -> Option<String> + Send + Sync>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StartupRecoveryReport {
    pub transitioned_run_ids: Vec<String>,
    pub transitioned_runs: Vec<StartupRecoveredRunTransition>,
    pub cleaned_temp_artifacts: Vec<StartupRecoveredTempArtifact>,
    pub blocking_findings: Vec<StartupRecoveryFinding>,
    pub operator_guidance: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupRecoveredTempArtifact {
    pub surface: StartupCleanupSurface,
    pub path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartupCleanupSurface {
    RegistryTemp,
    CasTemp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupRecoveryFinding {
    pub name: String,
    pub detail: String,
    pub remediation: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupRecoveredRunTransition {
    pub run_id: String,
    pub repo_id: String,
    pub from_status: IndexRunStatus,
    pub to_status: IndexRunStatus,
}

const STALE_RUNNING_STARTUP_SWEEP_SUMMARY: &str = "stale run detected during startup sweep";
const STALE_QUEUED_INTERRUPTED_STARTUP_SWEEP_SUMMARY: &str =
    "stale queued run recovered as interrupted during startup sweep because durable work exists";

impl StartupCleanupSurface {
    fn label(&self) -> &'static str {
        match self {
            Self::RegistryTemp => "registry temp artifact",
            Self::CasTemp => "CAS temp artifact",
        }
    }

    fn check_name(&self) -> &'static str {
        match self {
            Self::RegistryTemp => "startup_recovery_registry_temp",
            Self::CasTemp => "startup_recovery_cas_temp",
        }
    }

    fn cleanup_remediation(&self) -> &'static str {
        match self {
            Self::RegistryTemp => {
                "Repair or migrate the registry state, or wait for the conflicting process to release the artifact, then restart Tokenizor."
            }
            Self::CasTemp => {
                "Repair the local CAS state, or wait for the conflicting process to release the artifact, then restart Tokenizor."
            }
        }
    }
}

impl StartupRecoveryReport {
    pub fn is_noop(&self) -> bool {
        self.transitioned_runs.is_empty()
            && self.cleaned_temp_artifacts.is_empty()
            && self.blocking_findings.is_empty()
    }

    pub fn has_blocking_findings(&self) -> bool {
        !self.blocking_findings.is_empty()
    }

    pub fn interrupted_run_count(&self) -> usize {
        self.transitioned_runs
            .iter()
            .filter(|transition| transition.to_status == IndexRunStatus::Interrupted)
            .count()
    }

    pub fn aborted_run_count(&self) -> usize {
        self.transitioned_runs
            .iter()
            .filter(|transition| transition.to_status == IndexRunStatus::Aborted)
            .count()
    }

    pub fn readiness_checks(&self) -> Vec<ComponentHealth> {
        let mut checks = Vec::new();

        if !self.transitioned_runs.is_empty() {
            let interrupted_ids =
                self.transitioned_run_ids_for_status(&IndexRunStatus::Interrupted);
            let aborted_ids = self.transitioned_run_ids_for_status(&IndexRunStatus::Aborted);
            let mut transition_groups = Vec::new();
            if !interrupted_ids.is_empty() {
                transition_groups.push(format!(
                    "interrupted={} ({})",
                    interrupted_ids.len(),
                    interrupted_ids.join(", ")
                ));
            }
            if !aborted_ids.is_empty() {
                transition_groups.push(format!(
                    "aborted={} ({})",
                    aborted_ids.len(),
                    aborted_ids.join(", ")
                ));
            }
            checks.push(ComponentHealth::warning(
                "startup_recovery_runs",
                HealthIssueCategory::Recovery,
                format!(
                    "startup sweep recovered {} stale run(s): {}",
                    self.transitioned_runs.len(),
                    transition_groups.join("; ")
                ),
                self.transition_guidance()
                    .unwrap_or("Inspect recovered runs before trusting prior results."),
            ));
        }

        if !self.cleaned_temp_artifacts.is_empty() {
            let mut registry_count = 0usize;
            let mut cas_count = 0usize;
            for artifact in &self.cleaned_temp_artifacts {
                match artifact.surface {
                    StartupCleanupSurface::RegistryTemp => registry_count += 1,
                    StartupCleanupSurface::CasTemp => cas_count += 1,
                }
            }

            let mut surfaces = Vec::new();
            if registry_count > 0 {
                surfaces.push(format!("registry={registry_count}"));
            }
            if cas_count > 0 {
                surfaces.push(format!("cas={cas_count}"));
            }

            checks.push(ComponentHealth::warning(
                "startup_recovery_cleanup",
                HealthIssueCategory::Recovery,
                format!(
                    "startup sweep removed {} stale temp artifact(s) from Tokenizor-owned paths ({})",
                    self.cleaned_temp_artifacts.len(),
                    surfaces.join(", ")
                ),
                "No immediate action is required. If stale temp artifacts recur, wait for active processes to exit and run repair before starting new mutations.",
            ));
        }

        checks.extend(self.blocking_findings.iter().map(|finding| {
            ComponentHealth::error(
                finding.name.clone(),
                HealthIssueCategory::Recovery,
                finding.detail.clone(),
                finding.remediation.clone(),
            )
        }));

        checks
    }

    fn push_guidance(&mut self, guidance: impl Into<String>) {
        let guidance = guidance.into();
        if !self
            .operator_guidance
            .iter()
            .any(|existing| existing == &guidance)
        {
            self.operator_guidance.push(guidance);
        }
    }

    fn push_run_transition(&mut self, transition: StartupRecoveredRunTransition) {
        self.transitioned_run_ids.push(transition.run_id.clone());
        self.transitioned_runs.push(transition);
    }

    fn push_blocking_finding(&mut self, finding: StartupRecoveryFinding) {
        self.push_guidance(finding.remediation.clone());
        self.blocking_findings.push(finding);
    }

    fn transitioned_run_ids_for_status(&self, status: &IndexRunStatus) -> Vec<String> {
        self.transitioned_runs
            .iter()
            .filter(|transition| &transition.to_status == status)
            .map(|transition| transition.run_id.clone())
            .collect()
    }

    fn transition_guidance(&self) -> Option<&'static str> {
        match (
            self.interrupted_run_count() > 0,
            self.aborted_run_count() > 0,
        ) {
            (true, true) => Some(
                "Inspect interrupted runs and resume if eligible; otherwise reindex or repair before trusting prior results. Start a fresh index for startup-aborted queued runs with no durable work.",
            ),
            (true, false) => Some(
                "Inspect interrupted runs and choose the next safe action: resume if eligible; otherwise reindex or repair before trusting prior results.",
            ),
            (false, true) => Some(
                "Startup recovery aborted stale queued runs with no durable work. Next safe action: start a fresh index or reindex; do not resume those runs.",
            ),
            (false, false) => None,
        }
    }
}

pub struct RunManager {
    control_plane: Arc<dyn ControlPlane>,
    persistence: Arc<RunManagerPersistenceAdapter>,
    registry_path: Option<PathBuf>,
    blob_root: Option<PathBuf>,
    active_runs: Mutex<HashMap<String, ActiveRun>>,
}

pub struct RunManagerPersistenceAdapter {
    control_plane: Arc<dyn ControlPlane>,
    registry: Arc<RegistryPersistence>,
}

impl RunManagerPersistenceAdapter {
    fn new(control_plane: Arc<dyn ControlPlane>, registry: Arc<RegistryPersistence>) -> Self {
        Self {
            control_plane,
            registry,
        }
    }

    fn mirrors_bootstrap_registry(&self) -> bool {
        self.control_plane.backend_name() != "local_registry"
    }

    fn uses_registry_mutable_state_fallback(&self) -> bool {
        self.control_plane.backend_name() == "in_memory"
    }

    fn mirrors_mutable_state_to_registry(&self) -> bool {
        self.control_plane.backend_name() == "in_memory"
    }

    pub fn find_run(&self, run_id: &str) -> Result<Option<IndexRun>> {
        match self.control_plane.find_run(run_id)? {
            Some(run) => Ok(Some(run)),
            None if self.uses_registry_mutable_state_fallback() => self.registry.find_run(run_id),
            None => Ok(None),
        }
    }

    pub fn find_runs_by_status(&self, status: &IndexRunStatus) -> Result<Vec<IndexRun>> {
        let mut runs = self.control_plane.find_runs_by_status(status)?;
        if self.uses_registry_mutable_state_fallback() {
            let registry_runs = self.registry.find_runs_by_status(status)?;
            let known_ids: std::collections::HashSet<String> =
                runs.iter().map(|r| r.run_id.clone()).collect();
            runs.extend(
                registry_runs
                    .into_iter()
                    .filter(|r| !known_ids.contains(&r.run_id)),
            );
        }
        Ok(runs)
    }

    pub fn list_runs(&self) -> Result<Vec<IndexRun>> {
        let mut runs = self.control_plane.list_runs()?;
        if self.uses_registry_mutable_state_fallback() {
            let registry_runs = self.registry.list_runs()?;
            let known_ids: std::collections::HashSet<String> =
                runs.iter().map(|r| r.run_id.clone()).collect();
            runs.extend(
                registry_runs
                    .into_iter()
                    .filter(|r| !known_ids.contains(&r.run_id)),
            );
        }
        Ok(runs)
    }

    pub fn get_runs_by_repo(&self, repo_id: &str) -> Result<Vec<IndexRun>> {
        if self.uses_registry_mutable_state_fallback() {
            self.registry.get_runs_by_repo(repo_id)
        } else {
            self.control_plane.get_runs_by_repo(repo_id)
        }
    }

    pub fn get_latest_completed_run(&self, repo_id: &str) -> Result<Option<IndexRun>> {
        if self.uses_registry_mutable_state_fallback() {
            self.registry.get_latest_completed_run(repo_id)
        } else {
            self.control_plane.get_latest_completed_run(repo_id)
        }
    }

    pub fn get_repository(&self, repo_id: &str) -> Result<Option<crate::domain::Repository>> {
        match self.control_plane.get_repository(repo_id)? {
            Some(repository) => Ok(Some(repository)),
            None => self.registry.get_repository(repo_id),
        }
    }

    pub fn get_file_records(&self, run_id: &str) -> Result<Vec<FileRecord>> {
        if self.uses_registry_mutable_state_fallback() {
            self.registry.get_file_records(run_id)
        } else {
            self.control_plane.get_file_records(run_id)
        }
    }

    pub fn get_latest_checkpoint(&self, run_id: &str) -> Result<Option<Checkpoint>> {
        match self.control_plane.get_latest_checkpoint(run_id)? {
            Some(checkpoint) => Ok(Some(checkpoint)),
            None if self.uses_registry_mutable_state_fallback() => {
                self.registry.get_latest_checkpoint(run_id)
            }
            None => Ok(None),
        }
    }

    pub fn find_idempotency_record(&self, key: &str) -> Result<Option<IdempotencyRecord>> {
        match self.control_plane.find_idempotency_record(key)? {
            Some(record) => Ok(Some(record)),
            None if self.uses_registry_mutable_state_fallback() => {
                self.registry.find_idempotency_record(key)
            }
            None => Ok(None),
        }
    }

    pub fn get_discovery_manifest(&self, run_id: &str) -> Result<Option<DiscoveryManifest>> {
        match self.control_plane.get_discovery_manifest(run_id)? {
            Some(manifest) => Ok(Some(manifest)),
            None if self.uses_registry_mutable_state_fallback() => {
                self.registry.get_discovery_manifest(run_id)
            }
            None => Ok(None),
        }
    }

    pub fn save_run(&self, run: &IndexRun) -> Result<()> {
        if self.mirrors_mutable_state_to_registry() {
            self.registry.save_run(run)?;
        }
        match self.control_plane.save_run(run) {
            Ok(()) | Err(TokenizorError::NotFound(_))
                if self.mirrors_mutable_state_to_registry() =>
            {
                Ok(())
            }
            other => other,
        }
    }

    pub fn update_run_status(
        &self,
        run_id: &str,
        status: IndexRunStatus,
        error_summary: Option<String>,
    ) -> Result<()> {
        if self.mirrors_mutable_state_to_registry() {
            self.registry
                .update_run_status(run_id, status.clone(), error_summary.clone())?;
        }
        match self
            .control_plane
            .update_run_status(run_id, status, error_summary)
        {
            Ok(()) | Err(TokenizorError::NotFound(_))
                if self.mirrors_mutable_state_to_registry() =>
            {
                Ok(())
            }
            other => other,
        }
    }

    pub fn transition_to_running(&self, run_id: &str, started_at_unix_ms: u64) -> Result<()> {
        if self.mirrors_mutable_state_to_registry() {
            self.registry
                .transition_to_running(run_id, started_at_unix_ms)?;
        }
        match self
            .control_plane
            .transition_to_running(run_id, started_at_unix_ms)
        {
            Ok(()) | Err(TokenizorError::NotFound(_))
                if self.mirrors_mutable_state_to_registry() =>
            {
                Ok(())
            }
            other => other,
        }
    }

    pub fn update_run_status_with_finish(
        &self,
        run_id: &str,
        status: IndexRunStatus,
        error_summary: Option<String>,
        finished_at_unix_ms: u64,
        not_yet_supported: Option<std::collections::BTreeMap<crate::domain::LanguageId, u64>>,
    ) -> Result<()> {
        if self.mirrors_mutable_state_to_registry() {
            self.registry.update_run_status_with_finish(
                run_id,
                status.clone(),
                error_summary.clone(),
                finished_at_unix_ms,
                not_yet_supported.clone(),
            )?;
        }
        match self.control_plane.update_run_status_with_finish(
            run_id,
            status,
            error_summary,
            finished_at_unix_ms,
            not_yet_supported,
        ) {
            Ok(()) | Err(TokenizorError::NotFound(_))
                if self.mirrors_mutable_state_to_registry() =>
            {
                Ok(())
            }
            other => other,
        }
    }

    pub fn cancel_run_if_active(&self, run_id: &str, finished_at_unix_ms: u64) -> Result<bool> {
        let changed = if self.mirrors_mutable_state_to_registry() {
            self.registry
                .cancel_run_if_active(run_id, finished_at_unix_ms)?
        } else {
            false
        };
        match self
            .control_plane
            .cancel_run_if_active(run_id, finished_at_unix_ms)
        {
            Ok(result) if self.mirrors_mutable_state_to_registry() => Ok(changed || result),
            Err(TokenizorError::NotFound(_)) if self.mirrors_mutable_state_to_registry() => {
                Ok(changed)
            }
            other => other,
        }
    }

    pub fn save_file_records(&self, run_id: &str, records: &[FileRecord]) -> Result<()> {
        if self.mirrors_mutable_state_to_registry() {
            self.registry.save_file_records(run_id, records)?;
        }
        match self.control_plane.save_file_records(run_id, records) {
            Ok(()) | Err(TokenizorError::NotFound(_))
                if self.mirrors_mutable_state_to_registry() =>
            {
                Ok(())
            }
            other => other,
        }
    }

    pub fn save_checkpoint(&self, checkpoint: &Checkpoint) -> Result<()> {
        if self.mirrors_mutable_state_to_registry() {
            self.registry.save_checkpoint(checkpoint)?;
        }
        match self.control_plane.save_checkpoint(checkpoint) {
            Ok(()) | Err(TokenizorError::NotFound(_))
                if self.mirrors_mutable_state_to_registry() =>
            {
                Ok(())
            }
            other => other,
        }
    }

    pub fn save_repository(&self, repository: &crate::domain::Repository) -> Result<()> {
        self.control_plane.save_repository(repository)?;
        if self.mirrors_bootstrap_registry() {
            self.registry.save_repository(repository)?;
        }
        Ok(())
    }

    pub fn update_repository_status(
        &self,
        repo_id: &str,
        status: RepositoryStatus,
        invalidated_at_unix_ms: Option<u64>,
        invalidation_reason: Option<String>,
        quarantined_at_unix_ms: Option<u64>,
        quarantine_reason: Option<String>,
    ) -> Result<()> {
        let registry_status = status.clone();
        let registry_invalidation_reason = invalidation_reason.clone();
        let registry_quarantine_reason = quarantine_reason.clone();
        self.control_plane.update_repository_status(
            repo_id,
            status,
            invalidated_at_unix_ms,
            invalidation_reason,
            quarantined_at_unix_ms,
            quarantine_reason,
        )?;
        if self.mirrors_bootstrap_registry() {
            self.registry.update_repository_status(
                repo_id,
                registry_status,
                invalidated_at_unix_ms,
                registry_invalidation_reason,
                quarantined_at_unix_ms,
                registry_quarantine_reason,
            )?;
        }
        Ok(())
    }

    pub fn save_idempotency_record(&self, record: &IdempotencyRecord) -> Result<()> {
        if self.mirrors_mutable_state_to_registry() {
            self.registry.save_idempotency_record(record)?;
        }
        match self.control_plane.save_idempotency_record(record) {
            Ok(()) | Err(TokenizorError::NotFound(_))
                if self.mirrors_mutable_state_to_registry() =>
            {
                Ok(())
            }
            other => other,
        }
    }

    pub fn save_discovery_manifest(&self, manifest: &DiscoveryManifest) -> Result<()> {
        if self.mirrors_mutable_state_to_registry() {
            self.registry.save_discovery_manifest(manifest)?;
        }
        match self.control_plane.save_discovery_manifest(manifest) {
            Ok(()) | Err(TokenizorError::NotFound(_))
                if self.mirrors_mutable_state_to_registry() =>
            {
                Ok(())
            }
            other => other,
        }
    }

    pub fn save_repair_event(&self, event: &RepairEvent) -> Result<()> {
        if self.mirrors_mutable_state_to_registry() {
            self.registry.save_repair_event(event)?;
        }
        match self.control_plane.save_repair_event(event) {
            Ok(()) | Err(TokenizorError::NotFound(_))
                if self.mirrors_mutable_state_to_registry() =>
            {
                Ok(())
            }
            other => other,
        }
    }

    pub fn get_repair_events(&self, repo_id: &str) -> Result<Vec<RepairEvent>> {
        if self.uses_registry_mutable_state_fallback() {
            self.registry.get_repair_events(repo_id)
        } else {
            self.control_plane.get_repair_events(repo_id)
        }
    }

    pub fn save_operational_event(&self, event: &OperationalEvent) -> Result<()> {
        if self.mirrors_mutable_state_to_registry() {
            self.registry.save_operational_event(event)?;
        }
        match self.control_plane.save_operational_event(event) {
            Ok(()) | Err(TokenizorError::NotFound(_))
                if self.mirrors_mutable_state_to_registry() =>
            {
                Ok(())
            }
            other => other,
        }
    }

    pub fn get_operational_events(
        &self,
        repo_id: &str,
        filter: &OperationalEventFilter,
    ) -> Result<Vec<OperationalEvent>> {
        if self.uses_registry_mutable_state_fallback() {
            self.registry.get_operational_events(repo_id, filter)
        } else {
            self.control_plane.get_operational_events(repo_id, filter)
        }
    }
}

impl RegistryQuery for RunManagerPersistenceAdapter {
    fn get_repository(&self, repo_id: &str) -> Result<Option<crate::domain::Repository>> {
        RunManagerPersistenceAdapter::get_repository(self, repo_id)
    }

    fn get_runs_by_repo(&self, repo_id: &str) -> Result<Vec<IndexRun>> {
        RunManagerPersistenceAdapter::get_runs_by_repo(self, repo_id)
    }

    fn get_latest_completed_run(&self, repo_id: &str) -> Result<Option<IndexRun>> {
        RunManagerPersistenceAdapter::get_latest_completed_run(self, repo_id)
    }

    fn get_file_records(&self, run_id: &str) -> Result<Vec<FileRecord>> {
        RunManagerPersistenceAdapter::get_file_records(self, run_id)
    }
}

impl ControlPlane for RunManagerPersistenceAdapter {
    fn backend_name(&self) -> &'static str {
        self.control_plane.backend_name()
    }

    fn health_check(&self) -> Result<ComponentHealth> {
        self.control_plane.health_check()
    }

    fn deployment_checks(&self) -> Result<Vec<ComponentHealth>> {
        self.control_plane.deployment_checks()
    }

    fn find_run(&self, run_id: &str) -> Result<Option<IndexRun>> {
        RunManagerPersistenceAdapter::find_run(self, run_id)
    }

    fn find_runs_by_status(&self, status: &IndexRunStatus) -> Result<Vec<IndexRun>> {
        RunManagerPersistenceAdapter::find_runs_by_status(self, status)
    }

    fn list_runs(&self) -> Result<Vec<IndexRun>> {
        RunManagerPersistenceAdapter::list_runs(self)
    }

    fn get_runs_by_repo(&self, repo_id: &str) -> Result<Vec<IndexRun>> {
        RunManagerPersistenceAdapter::get_runs_by_repo(self, repo_id)
    }

    fn get_latest_completed_run(&self, repo_id: &str) -> Result<Option<IndexRun>> {
        RunManagerPersistenceAdapter::get_latest_completed_run(self, repo_id)
    }

    fn get_repository(&self, repo_id: &str) -> Result<Option<crate::domain::Repository>> {
        RunManagerPersistenceAdapter::get_repository(self, repo_id)
    }

    fn get_file_records(&self, run_id: &str) -> Result<Vec<FileRecord>> {
        RunManagerPersistenceAdapter::get_file_records(self, run_id)
    }

    fn get_latest_checkpoint(&self, run_id: &str) -> Result<Option<Checkpoint>> {
        RunManagerPersistenceAdapter::get_latest_checkpoint(self, run_id)
    }

    fn find_idempotency_record(&self, key: &str) -> Result<Option<IdempotencyRecord>> {
        RunManagerPersistenceAdapter::find_idempotency_record(self, key)
    }

    fn get_discovery_manifest(&self, run_id: &str) -> Result<Option<DiscoveryManifest>> {
        RunManagerPersistenceAdapter::get_discovery_manifest(self, run_id)
    }

    fn save_run(&self, run: &IndexRun) -> Result<()> {
        RunManagerPersistenceAdapter::save_run(self, run)
    }

    fn update_run_status(
        &self,
        run_id: &str,
        status: IndexRunStatus,
        error_summary: Option<String>,
    ) -> Result<()> {
        RunManagerPersistenceAdapter::update_run_status(self, run_id, status, error_summary)
    }

    fn transition_to_running(&self, run_id: &str, started_at_unix_ms: u64) -> Result<()> {
        RunManagerPersistenceAdapter::transition_to_running(self, run_id, started_at_unix_ms)
    }

    fn update_run_status_with_finish(
        &self,
        run_id: &str,
        status: IndexRunStatus,
        error_summary: Option<String>,
        finished_at_unix_ms: u64,
        not_yet_supported: Option<std::collections::BTreeMap<crate::domain::LanguageId, u64>>,
    ) -> Result<()> {
        RunManagerPersistenceAdapter::update_run_status_with_finish(
            self,
            run_id,
            status,
            error_summary,
            finished_at_unix_ms,
            not_yet_supported,
        )
    }

    fn cancel_run_if_active(&self, run_id: &str, finished_at_unix_ms: u64) -> Result<bool> {
        RunManagerPersistenceAdapter::cancel_run_if_active(self, run_id, finished_at_unix_ms)
    }

    fn save_file_records(&self, run_id: &str, records: &[FileRecord]) -> Result<()> {
        RunManagerPersistenceAdapter::save_file_records(self, run_id, records)
    }

    fn save_checkpoint(&self, checkpoint: &Checkpoint) -> Result<()> {
        RunManagerPersistenceAdapter::save_checkpoint(self, checkpoint)
    }

    fn save_repository(&self, repository: &crate::domain::Repository) -> Result<()> {
        RunManagerPersistenceAdapter::save_repository(self, repository)
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
        RunManagerPersistenceAdapter::update_repository_status(
            self,
            repo_id,
            status,
            invalidated_at_unix_ms,
            invalidation_reason,
            quarantined_at_unix_ms,
            quarantine_reason,
        )
    }

    fn save_idempotency_record(&self, record: &IdempotencyRecord) -> Result<()> {
        RunManagerPersistenceAdapter::save_idempotency_record(self, record)
    }

    fn save_discovery_manifest(&self, manifest: &DiscoveryManifest) -> Result<()> {
        RunManagerPersistenceAdapter::save_discovery_manifest(self, manifest)
    }

    fn save_repair_event(&self, event: &RepairEvent) -> Result<()> {
        RunManagerPersistenceAdapter::save_repair_event(self, event)
    }

    fn get_repair_events(&self, repo_id: &str) -> Result<Vec<RepairEvent>> {
        RunManagerPersistenceAdapter::get_repair_events(self, repo_id)
    }

    fn save_operational_event(&self, event: &OperationalEvent) -> Result<()> {
        RunManagerPersistenceAdapter::save_operational_event(self, event)
    }

    fn get_operational_events(
        &self,
        repo_id: &str,
        filter: &OperationalEventFilter,
    ) -> Result<Vec<OperationalEvent>> {
        RunManagerPersistenceAdapter::get_operational_events(self, repo_id, filter)
    }
}

impl RunManager {
    pub fn new(persistence: RegistryPersistence) -> Self {
        Self::with_optional_blob_root(persistence, None)
    }

    pub fn with_blob_root(persistence: RegistryPersistence, blob_root: PathBuf) -> Self {
        Self::with_optional_blob_root(persistence, Some(blob_root))
    }

    fn with_optional_blob_root(
        persistence: RegistryPersistence,
        blob_root: Option<PathBuf>,
    ) -> Self {
        let registry_path = persistence.path().to_path_buf();
        let persistence = Arc::new(persistence);
        let control_plane: Arc<dyn ControlPlane> =
            Arc::new(RegistryBackedControlPlane::new(Arc::clone(&persistence)));
        Self::with_services(control_plane, persistence, Some(registry_path), blob_root)
    }

    pub fn with_services(
        control_plane: Arc<dyn ControlPlane>,
        registry: Arc<RegistryPersistence>,
        registry_path: Option<PathBuf>,
        blob_root: Option<PathBuf>,
    ) -> Self {
        let persistence = Arc::new(RunManagerPersistenceAdapter::new(
            Arc::clone(&control_plane),
            registry,
        ));
        let control_plane: Arc<dyn ControlPlane> = persistence.clone();
        Self {
            control_plane,
            persistence,
            registry_path,
            blob_root,
            active_runs: Mutex::new(HashMap::new()),
        }
    }

    pub fn startup_sweep(&self) -> Result<StartupRecoveryReport> {
        let mut report = StartupRecoveryReport::default();

        let mut running_runs = match self
            .control_plane
            .find_runs_by_status(&IndexRunStatus::Running)
        {
            Ok(runs) => runs,
            Err(error) => {
                report.push_blocking_finding(StartupRecoveryFinding {
                    name: "startup_recovery_runs".to_string(),
                    detail: format!(
                        "startup sweep could not inspect persisted runs before mutating work: {error}"
                    ),
                    remediation:
                        "Repair or migrate the registry state before starting new mutations."
                            .to_string(),
                });
                return Ok(report);
            }
        };
        let mut queued_runs = match self
            .control_plane
            .find_runs_by_status(&IndexRunStatus::Queued)
        {
            Ok(runs) => runs,
            Err(error) => {
                report.push_blocking_finding(StartupRecoveryFinding {
                    name: "startup_recovery_runs".to_string(),
                    detail: format!(
                        "startup sweep could not inspect persisted runs before mutating work: {error}"
                    ),
                    remediation:
                        "Repair or migrate the registry state before starting new mutations."
                            .to_string(),
                });
                return Ok(report);
            }
        };
        running_runs.sort_by(|left, right| left.run_id.cmp(&right.run_id));
        queued_runs.sort_by(|left, right| left.run_id.cmp(&right.run_id));

        for run in running_runs {
            self.apply_startup_run_transition(
                &run,
                IndexRunStatus::Interrupted,
                STALE_RUNNING_STARTUP_SWEEP_SUMMARY,
                &mut report,
            );
        }

        for run in queued_runs {
            let (target_status, error_summary) = match self.classify_stale_queued_run(&run) {
                Ok(result) => result,
                Err(error) => {
                    warn!(
                        run_id = %run.run_id,
                        repo_id = %run.repo_id,
                        ?error,
                        "startup sweep: failed to inspect stale queued run durability"
                    );
                    report.push_blocking_finding(StartupRecoveryFinding {
                        name: "startup_recovery_runs".to_string(),
                        detail: format!(
                            "startup sweep could not inspect durable state for stale queued run `{}`: {error}",
                            run.run_id
                        ),
                        remediation:
                            "Repair or migrate the registry state before starting new mutations."
                                .to_string(),
                    });
                    continue;
                }
            };
            self.apply_startup_run_transition(&run, target_status, error_summary, &mut report);
        }

        if report.interrupted_run_count() > 0 {
            report.push_guidance(
                "Inspect interrupted runs and choose the next safe action: resume if eligible; otherwise reindex or repair before trusting prior results.",
            );
        }
        if report.aborted_run_count() > 0 {
            report.push_guidance(
                "Startup recovery aborted stale queued runs with no durable work. Next safe action: start a fresh index or reindex; do not resume those runs.",
            );
        }

        if let Some(registry_path) = &self.registry_path {
            sweep_owned_temp_artifacts(
                registry_path.parent(),
                |path| is_owned_registry_temp_artifact_path(registry_path, path),
                StartupCleanupSurface::RegistryTemp,
                &mut report,
            );
        }

        if let Some(blob_root) = &self.blob_root {
            let cas_temp_dir = LocalCasBlobStore::temp_dir_from_root(blob_root);
            sweep_owned_temp_artifacts(
                Some(cas_temp_dir.as_path()),
                |path| LocalCasBlobStore::is_owned_temp_blob_path(blob_root, path),
                StartupCleanupSurface::CasTemp,
                &mut report,
            );
        }

        if !report.cleaned_temp_artifacts.is_empty() {
            report.push_guidance(
                "Startup recovery removed stale Tokenizor temp artifacts. If they recur, wait for active processes to exit and run repair before starting new mutations.",
            );
        }

        let transition_count = report.transitioned_runs.len();
        if transition_count > 0 {
            let actions: Vec<String> = report
                .transitioned_runs
                .iter()
                .map(|t| format!("{}:{:?}->{:?}", t.run_id, t.from_status, t.to_status))
                .collect();
            // Emit per distinct repo_id
            let mut seen_repos = std::collections::HashSet::new();
            for t in &report.transitioned_runs {
                if seen_repos.insert(t.repo_id.clone()) {
                    if let Err(e) = self
                        .control_plane
                        .save_operational_event(&OperationalEvent {
                            repo_id: t.repo_id.clone(),
                            kind: OperationalEventKind::StartupSweepCompleted {
                                stale_runs_found: transition_count,
                                actions_taken: actions.clone(),
                            },
                            timestamp_unix_ms: unix_timestamp_ms(),
                        })
                    {
                        warn!(repo_id = %t.repo_id, error = %e, "failed to record startup sweep event");
                    }
                }
            }
        }

        Ok(report)
    }

    fn classify_stale_queued_run(&self, run: &IndexRun) -> Result<(IndexRunStatus, &'static str)> {
        let has_checkpoint = self
            .control_plane
            .get_latest_checkpoint(&run.run_id)?
            .is_some();
        let has_durable_records = if has_checkpoint {
            false
        } else {
            !self.control_plane.get_file_records(&run.run_id)?.is_empty()
        };

        if has_checkpoint || has_durable_records {
            Ok((
                IndexRunStatus::Interrupted,
                STALE_QUEUED_INTERRUPTED_STARTUP_SWEEP_SUMMARY,
            ))
        } else {
            Ok((
                IndexRunStatus::Aborted,
                STALE_QUEUED_ABORTED_SUMMARY,
            ))
        }
    }

    fn apply_startup_run_transition(
        &self,
        run: &IndexRun,
        to_status: IndexRunStatus,
        error_summary: &'static str,
        report: &mut StartupRecoveryReport,
    ) {
        let update_result = match &to_status {
            IndexRunStatus::Aborted => self.control_plane.update_run_status_with_finish(
                &run.run_id,
                to_status.clone(),
                Some(error_summary.to_string()),
                unix_timestamp_ms(),
                None,
            ),
            _ => self.control_plane.update_run_status(
                &run.run_id,
                to_status.clone(),
                Some(error_summary.to_string()),
            ),
        };

        match update_result {
            Ok(()) => {
                if let Err(e) = self.control_plane.save_operational_event(&OperationalEvent {
                    repo_id: run.repo_id.clone(),
                    kind: OperationalEventKind::RunInterrupted {
                        run_id: run.run_id.clone(),
                        reason: error_summary.to_string(),
                    },
                    timestamp_unix_ms: unix_timestamp_ms(),
                }) {
                    warn!(run_id = %run.run_id, error = %e, "failed to record run interrupted event");
                }
                info!(
                    run_id = %run.run_id,
                    repo_id = %run.repo_id,
                    from_status = ?run.status,
                    to_status = ?to_status,
                    reason = error_summary,
                    "startup sweep: transitioned stale run"
                );
                report.push_run_transition(StartupRecoveredRunTransition {
                    run_id: run.run_id.clone(),
                    repo_id: run.repo_id.clone(),
                    from_status: run.status.clone(),
                    to_status,
                });
            }
            Err(error) => {
                warn!(
                    run_id = %run.run_id,
                    repo_id = %run.repo_id,
                    from_status = ?run.status,
                    to_status = ?to_status,
                    ?error,
                    "startup sweep: failed to transition stale run"
                );
                report.push_blocking_finding(StartupRecoveryFinding {
                    name: "startup_recovery_runs".to_string(),
                    detail: format!(
                        "startup sweep could not transition stale run `{}` from {:?} to {:?}: {error}",
                        run.run_id,
                        run.status,
                        to_status
                    ),
                    remediation:
                        "Repair or migrate the registry state before starting new mutations."
                            .to_string(),
                });
            }
        }
    }

    pub fn start_run(&self, repo_id: &str, mode: IndexRunMode) -> Result<IndexRun> {
        let active_runs = self.active_runs.lock().unwrap_or_else(|e| e.into_inner());
        if active_runs.contains_key(repo_id) {
            return Err(TokenizorError::InvalidArgument(format!(
                "an active indexing run already exists for repository `{repo_id}`"
            )));
        }
        drop(active_runs);

        let persisted_active = self.control_plane.list_runs()?;
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
            recovery_state: None,
        };

        self.control_plane.save_run(&run)?;

        self.control_plane
            .save_operational_event(&OperationalEvent {
                repo_id: repo_id.to_string(),
                kind: OperationalEventKind::RunStarted {
                    run_id: run.run_id.clone(),
                    mode: run.mode.clone(),
                },
                timestamp_unix_ms: requested_at,
            })?;

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

    fn persist_durable_file_record(&self, record: &FileRecord) -> Result<()> {
        self.control_plane
            .save_file_records(&record.run_id, std::slice::from_ref(record))
    }

    fn save_recovery_state(
        &self,
        run: &IndexRun,
        state: Option<RunRecoveryState>,
        error_summary: Option<String>,
    ) -> Result<IndexRun> {
        let mut updated = run.clone();
        updated.recovery_state = state;
        updated.error_summary = error_summary;
        self.control_plane.save_run(&updated)?;
        Ok(updated)
    }

    fn resume_rejected(
        &self,
        run: &IndexRun,
        reason: ResumeRejectReason,
        next_action: NextAction,
        detail: impl Into<String>,
    ) -> Result<ResumeRunOutcome> {
        let detail = detail.into();
        let updated = self.save_recovery_state(
            run,
            Some(RunRecoveryState {
                state: RecoveryStateKind::ResumeRejected,
                rejection_reason: Some(reason.clone()),
                next_action: Some(next_action.clone()),
                detail: Some(detail.clone()),
                updated_at_unix_ms: unix_timestamp_ms(),
            }),
            run.error_summary.clone(),
        )?;

        Ok(ResumeRunOutcome::Rejected {
            run: updated,
            reason,
            next_action,
            detail,
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

        if let Some(existing) = self
            .control_plane
            .find_idempotency_record(&idempotency_key)?
        {
            let run_id = existing.result_ref.as_deref().unwrap_or("");
            let referenced_run = self.control_plane.find_run(run_id)?;
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
        self.control_plane.save_idempotency_record(&record)?;

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

        if let Some(existing) = self
            .control_plane
            .find_idempotency_record(&idempotency_key)?
        {
            let run_id = existing.result_ref.as_deref().unwrap_or("");
            let referenced_run = self.control_plane.find_run(run_id)?;
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

        let persisted_active = self.control_plane.list_runs()?;
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
            .control_plane
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
            recovery_state: None,
        };

        self.control_plane.save_run(&run)?;

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
        self.control_plane.save_idempotency_record(&record)?;

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
            .control_plane
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
        if let Some(_stale) = self
            .control_plane
            .find_idempotency_record(&idempotency_key)?
        {
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

        let persisted_active = self.control_plane.list_runs()?;
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

        self.control_plane.update_repository_status(
            repo_id,
            RepositoryStatus::Invalidated,
            Some(now),
            reason.map(|r| r.to_string()),
            None,
            None,
        )?;

        self.control_plane
            .save_operational_event(&OperationalEvent {
                repo_id: repo_id.to_string(),
                kind: OperationalEventKind::RepositoryStatusChanged {
                    previous: previous_status.clone(),
                    current: RepositoryStatus::Invalidated,
                    trigger: reason.unwrap_or("invalidation requested").to_string(),
                },
                timestamp_unix_ms: now,
            })?;

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
        self.control_plane.save_idempotency_record(&record)?;

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
        // Ensure a repository entry exists in the control plane so retrieval
        // tools can find it. This is idempotent — if the repo already exists,
        // save_repository overwrites with the same data.
        if self.control_plane.get_repository(repo_id)?.is_none() {
            let is_git = repo_root.join(".git").exists();
            let (project_identity, project_identity_kind) = if is_git {
                (
                    repo_root.join(".git").to_string_lossy().to_string(),
                    ProjectIdentityKind::GitCommonDir,
                )
            } else {
                (
                    repo_root.to_string_lossy().to_string(),
                    ProjectIdentityKind::LocalRootPath,
                )
            };
            let repo = Repository {
                repo_id: repo_id.to_string(),
                kind: if is_git {
                    RepositoryKind::Git
                } else {
                    RepositoryKind::Local
                },
                root_uri: repo_root.to_string_lossy().to_string(),
                project_identity,
                project_identity_kind,
                default_branch: None,
                last_known_revision: None,
                status: RepositoryStatus::Ready,
                invalidated_at_unix_ms: None,
                invalidation_reason: None,
                quarantined_at_unix_ms: None,
                quarantine_reason: None,
            };
            self.control_plane.save_repository(&repo)?;
            info!(repo_id = %repo_id, root = %repo_root.display(), "auto-registered repository in control plane");
        }

        let run = self.start_run(repo_id, mode)?;
        let progress = self.spawn_pipeline_for_run(&run, repo_root, blob_store);
        Ok((run, progress))
    }

    pub fn resume_run(
        self: &Arc<Self>,
        run_id: &str,
        repo_root: PathBuf,
        blob_store: Arc<dyn BlobStore>,
    ) -> Result<ResumeRunOutcome> {
        let run = self
            .control_plane
            .find_run(run_id)?
            .ok_or_else(|| TokenizorError::NotFound(format!("run '{run_id}' not found")))?;

        if run.status != IndexRunStatus::Interrupted {
            let next_action = match run.status {
                IndexRunStatus::Queued | IndexRunStatus::Running => NextAction::Wait,
                _ => NextAction::Reindex,
            };
            return self.resume_rejected(
                &run,
                ResumeRejectReason::RunNotInterrupted,
                next_action,
                format!(
                    "run `{run_id}` is not interrupted and cannot resume from a checkpoint while status is `{:?}`",
                    run.status
                ),
            );
        }

        if self.has_active_run(&run.repo_id) {
            return self.resume_rejected(
                &run,
                ResumeRejectReason::ActiveRunConflict,
                NextAction::Wait,
                format!(
                    "repository `{}` already has an active indexing run in memory",
                    run.repo_id
                ),
            );
        }

        let persisted_active = self.control_plane.list_runs()?;
        if persisted_active.iter().any(|candidate| {
            candidate.repo_id == run.repo_id
                && candidate.run_id != run.run_id
                && matches!(
                    candidate.status,
                    IndexRunStatus::Queued | IndexRunStatus::Running
                )
        }) {
            return self.resume_rejected(
                &run,
                ResumeRejectReason::ActiveRunConflict,
                NextAction::Wait,
                format!(
                    "repository `{}` already has another persisted active run",
                    run.repo_id
                ),
            );
        }

        if let Some(repo) = self.control_plane.get_repository(&run.repo_id)? {
            match repo.status {
                RepositoryStatus::Invalidated => {
                    return self.resume_rejected(
                        &run,
                        ResumeRejectReason::RepositoryInvalidated,
                        NextAction::Reindex,
                        "repository indexed state has been invalidated; deterministic re-index is the safe fallback",
                    );
                }
                RepositoryStatus::Failed => {
                    return self.resume_rejected(
                        &run,
                        ResumeRejectReason::RepositoryFailed,
                        NextAction::Repair,
                        "repository is in failed state; repair is required before trusting partial run outputs",
                    );
                }
                RepositoryStatus::Degraded => {
                    return self.resume_rejected(
                        &run,
                        ResumeRejectReason::RepositoryDegraded,
                        NextAction::Repair,
                        "repository is in degraded state; repair is required before resuming interrupted work",
                    );
                }
                RepositoryStatus::Quarantined => {
                    return self.resume_rejected(
                        &run,
                        ResumeRejectReason::RepositoryQuarantined,
                        NextAction::Repair,
                        "repository is quarantined; repair is required before resuming interrupted work",
                    );
                }
                RepositoryStatus::Pending | RepositoryStatus::Ready => {}
            }
        }

        let checkpoint = match self.control_plane.get_latest_checkpoint(&run.run_id)? {
            Some(checkpoint) => checkpoint,
            None => {
                return self.resume_rejected(
                    &run,
                    ResumeRejectReason::MissingCheckpoint,
                    NextAction::Reindex,
                    format!("run `{run_id}` has no persisted checkpoint to resume from"),
                );
            }
        };

        if checkpoint.cursor.trim().is_empty() {
            return self.resume_rejected(
                &run,
                ResumeRejectReason::EmptyCheckpointCursor,
                NextAction::Reindex,
                format!("run `{run_id}` has a checkpoint with an empty cursor"),
            );
        }

        let manifest = match self.control_plane.get_discovery_manifest(&run.run_id)? {
            Some(manifest) => manifest,
            None => {
                return self.resume_rejected(
                    &run,
                    ResumeRejectReason::MissingDiscoveryManifest,
                    NextAction::Reindex,
                    format!(
                        "run `{run_id}` has no persisted discovery manifest; start a fresh re-index to re-establish deterministic resume boundaries"
                    ),
                );
            }
        };
        if manifest.run_id != run.run_id {
            return self.resume_rejected(
                &run,
                ResumeRejectReason::CorruptDiscoveryManifest,
                NextAction::Reindex,
                format!(
                    "persisted discovery manifest run_id `{}` does not match resume target `{}`",
                    manifest.run_id, run.run_id
                ),
            );
        }
        let manifest_paths = match validate_discovery_manifest(&manifest) {
            Ok(paths) => paths,
            Err(detail) => {
                return self.resume_rejected(
                    &run,
                    ResumeRejectReason::CorruptDiscoveryManifest,
                    NextAction::Reindex,
                    detail,
                );
            }
        };
        let cursor_index = match manifest_paths
            .iter()
            .position(|path| path == &checkpoint.cursor)
        {
            Some(index) => index,
            None => {
                return self.resume_rejected(
                    &run,
                    ResumeRejectReason::CheckpointCursorMissing,
                    NextAction::Reindex,
                    format!(
                        "checkpoint cursor `{}` is missing from the persisted discovery manifest",
                        checkpoint.cursor
                    ),
                );
            }
        };

        let durable_records = self.control_plane.get_file_records(&run.run_id)?;
        let durable_paths: std::collections::HashSet<&str> = durable_records
            .iter()
            .map(|record| record.relative_path.as_str())
            .collect();
        if let Some(missing_path) = manifest_paths
            .iter()
            .take(cursor_index + 1)
            .find(|path| !durable_paths.contains(path.as_str()))
            .cloned()
        {
            return self.resume_rejected(
                &run,
                ResumeRejectReason::MissingDurableOutputs,
                NextAction::Reindex,
                format!(
                    "checkpoint cursor `{}` is ahead of durable file record `{missing_path}`",
                    checkpoint.cursor
                ),
            );
        }

        let resumed_run = self.save_recovery_state(
            &run,
            Some(RunRecoveryState {
                state: RecoveryStateKind::Resumed,
                rejection_reason: None,
                next_action: None,
                detail: Some(format!(
                    "resumed from persisted discovery manifest at checkpoint `{}` after skipping {} durable files",
                    checkpoint.cursor,
                    cursor_index + 1
                )),
                updated_at_unix_ms: unix_timestamp_ms(),
            }),
            None,
        )?;

        // Record RunStarted event for the resume transition to Running
        self.control_plane
            .save_operational_event(&OperationalEvent {
                repo_id: resumed_run.repo_id.clone(),
                kind: OperationalEventKind::RunStarted {
                    run_id: resumed_run.run_id.clone(),
                    mode: resumed_run.mode.clone(),
                },
                timestamp_unix_ms: unix_timestamp_ms(),
            })?;

        self.spawn_pipeline_for_run_with_resume(
            &resumed_run,
            repo_root,
            blob_store,
            Some(PipelineResumeState {
                cursor: checkpoint.cursor.clone(),
                total_files: manifest_paths.len() as u64,
                files_processed: checkpoint.files_processed,
                symbols_extracted: checkpoint.symbols_written,
                files_failed: checkpoint.files_failed,
                manifest_paths: manifest_paths.clone(),
            }),
        );

        Ok(ResumeRunOutcome::Resumed {
            run: resumed_run,
            checkpoint,
            durable_files_skipped: (cursor_index + 1) as u64,
        })
    }

    fn spawn_pipeline_for_run(
        self: &Arc<Self>,
        run: &IndexRun,
        repo_root: PathBuf,
        blob_store: Arc<dyn BlobStore>,
    ) -> Arc<PipelineProgress> {
        self.spawn_pipeline_for_run_with_resume(run, repo_root, blob_store, None)
    }

    fn spawn_pipeline_for_run_with_resume(
        self: &Arc<Self>,
        run: &IndexRun,
        repo_root: PathBuf,
        blob_store: Arc<dyn BlobStore>,
        resume_from: Option<PipelineResumeState>,
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

        let manager_for_records = Arc::clone(self);
        let durable_record_callback = Box::new(move |record: &FileRecord| {
            manager_for_records.persist_durable_file_record(record)
        });
        let manager_for_manifest = Arc::clone(self);
        let discovery_manifest_callback = Box::new(move |manifest: &DiscoveryManifest| {
            manager_for_manifest
                .control_plane
                .save_discovery_manifest(manifest)
        });
        let manager_for_integrity = Arc::clone(self);
        let run_id_for_integrity = run_id.clone();
        let repo_id_for_integrity = repo_id_owned.clone();
        let integrity_event_callback =
            Box::new(move |relative_path: &str, reason: &str| {
                manager_for_integrity
                    .control_plane
                    .save_operational_event(&OperationalEvent {
                        repo_id: repo_id_for_integrity.clone(),
                        kind: OperationalEventKind::IntegrityEvent {
                            run_id: Some(run_id_for_integrity.clone()),
                            relative_path: Some(relative_path.to_string()),
                            kind: IntegrityEventKind::Quarantined,
                            detail: reason.to_string(),
                        },
                        timestamp_unix_ms: unix_timestamp_ms(),
                    })
            });

        let mut pipeline = IndexingPipeline::new(run_id.clone(), repo_root, token.clone())
            .with_cas(blob_store, repo_id_owned.clone())
            .with_discovery_manifest_callback(discovery_manifest_callback)
            .with_durable_record_callback(durable_record_callback)
            .with_integrity_event_callback(integrity_event_callback)
            .with_checkpoint_callback(checkpoint_callback, 100);
        if let Some(resume_state) = resume_from {
            pipeline = pipeline.with_resume_state(resume_state);
        }
        let progress = pipeline.progress();
        let tracker = pipeline.checkpoint_tracker();

        let manager = Arc::clone(self);

        let handle = tokio::spawn(async move {
            // Transition to Running with start timestamp (skips if already terminal)
            if let Err(e) = manager
                .control_plane
                .transition_to_running(&run_id, unix_timestamp_ms())
            {
                error!(run_id = %run_id, error = %e, "failed to transition to Running");
                manager.deregister_active_run(&repo_id_owned);
                return;
            }

            // If already cancelled before pipeline starts, skip execution
            let already_terminal = match manager.control_plane.find_run(&run_id) {
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
            let should_clear_invalidation =
                run_completion_clears_repository_invalidation(&result.status, &result.results);
            let final_error_summary = result.error_summary;
            let files_processed_count = result.results.len();

            // Check if the run was already cancelled (or otherwise made terminal)
            // by cancel_run() before we update status — prevents overwriting Cancelled
            let already_terminal = match manager.control_plane.find_run(&run_id) {
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
                let error_summary_clone = final_error_summary.clone();
                if let Err(e) = manager.control_plane.update_run_status_with_finish(
                    &run_id,
                    result.status.clone(),
                    final_error_summary,
                    finished_at,
                    not_yet_supported,
                ) {
                    error!(run_id = %run_id, error = %e, "failed to update final run status");
                }

                if let Err(e) =
                    manager
                        .control_plane
                        .save_operational_event(&OperationalEvent {
                            repo_id: repo_id_owned.clone(),
                            kind: OperationalEventKind::RunCompleted {
                                run_id: run_id.clone(),
                                status: result.status.clone(),
                                files_processed: files_processed_count,
                                error_summary: error_summary_clone,
                            },
                            timestamp_unix_ms: finished_at,
                        })
                {
                    warn!(run_id = %run_id, error = %e, "failed to record run completion event");
                }

                // Only fully healthy succeeded runs should clear repository invalidation.
                if should_clear_invalidation {
                    if let Ok(Some(repo)) = manager.control_plane.get_repository(&repo_id_owned) {
                        if repo.status == RepositoryStatus::Invalidated {
                            if let Err(e) = manager.control_plane.update_repository_status(
                                &repo_id_owned,
                                RepositoryStatus::Ready,
                                None,
                                None,
                                None,
                                None,
                            ) {
                                warn!(repo_id = %repo_id_owned, error = %e, "failed to clear invalidation after successful run");
                            } else {
                                if let Err(e) = manager.control_plane.save_operational_event(&OperationalEvent {
                                    repo_id: repo_id_owned.clone(),
                                    kind: OperationalEventKind::RepositoryStatusChanged {
                                        previous: RepositoryStatus::Invalidated,
                                        current: RepositoryStatus::Ready,
                                        trigger: "successful reindex cleared invalidation".to_string(),
                                    },
                                    timestamp_unix_ms: unix_timestamp_ms(),
                                }) {
                                    warn!(repo_id = %repo_id_owned, error = %e, "failed to record status change event");
                                }
                                info!(repo_id = %repo_id_owned, "cleared invalidation after successful run");
                            }
                        }
                    }
                } else if result.status == IndexRunStatus::Succeeded {
                    info!(
                        repo_id = %repo_id_owned,
                        run_id = %run_id,
                        "leaving repository invalidated because reindex completed with degraded file outcomes"
                    );
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

    pub fn persistence(&self) -> &RunManagerPersistenceAdapter {
        self.persistence.as_ref()
    }

    pub fn registry_query(&self) -> &dyn RegistryQuery {
        self.persistence.as_ref()
    }

    pub fn inspect_run(&self, run_id: &str) -> Result<RunStatusReport> {
        let run = self
            .control_plane
            .find_run(run_id)?
            .ok_or_else(|| TokenizorError::NotFound(format!("run '{run_id}' not found")))?;

        self.build_run_report(run)
    }

    pub fn cancel_run(&self, run_id: &str) -> Result<RunStatusReport> {
        let run = self
            .control_plane
            .find_run(run_id)?
            .ok_or_else(|| TokenizorError::NotFound(format!("run '{run_id}' not found")))?;

        // AC #2: terminal runs return current report without mutation
        if run.status.is_terminal() {
            return self.inspect_run(run_id);
        }

        // Signal cancellation token, capture progress, and remove from active_runs
        // Drop Mutex guard before calling persistence methods
        let files_processed_at_cancel = {
            let mut active_runs = self.active_runs.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(active_run) = active_runs.remove(&run.repo_id) {
                active_run.cancellation_token.cancel();
                debug!(run_id = %run_id, repo_id = %run.repo_id, "cancellation token signaled");
                active_run
                    .progress
                    .as_ref()
                    .map(|p| {
                        p.files_processed
                            .load(std::sync::atomic::Ordering::Relaxed)
                    })
                    .unwrap_or(0)
            } else {
                0
            }
        };

        // Atomic, race-safe persistence update
        let changed = self
            .control_plane
            .cancel_run_if_active(run_id, unix_timestamp_ms())?;

        if changed {
            self.control_plane.save_operational_event(&OperationalEvent {
                repo_id: run.repo_id.clone(),
                kind: OperationalEventKind::RunCompleted {
                    run_id: run_id.to_string(),
                    status: IndexRunStatus::Cancelled,
                    files_processed: files_processed_at_cancel as usize,
                    error_summary: Some("cancelled by user".to_string()),
                },
                timestamp_unix_ms: unix_timestamp_ms(),
            })?;
            info!(run_id = %run_id, "run cancelled");
        } else {
            debug!(run_id = %run_id, "cancel_run: run became terminal before persistence update");
        }
        self.inspect_run(run_id)
    }

    pub fn checkpoint_run(&self, run_id: &str) -> Result<Checkpoint> {
        let run = self
            .control_plane
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
            let files_failed = progress
                .files_failed
                .load(std::sync::atomic::Ordering::Relaxed);

            let cursor = active.checkpoint_cursor_fn.as_ref().and_then(|f| f());

            ((files_processed, symbols_extracted, files_failed), cursor)
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
            files_failed: progress.2,
            created_at_unix_ms: unix_timestamp_ms(),
        };

        self.control_plane.save_checkpoint(&checkpoint)?;

        self.control_plane
            .save_operational_event(&OperationalEvent {
                repo_id: run.repo_id.clone(),
                kind: OperationalEventKind::CheckpointCreated {
                    run_id: run_id.to_string(),
                    cursor: checkpoint.cursor.clone(),
                    files_committed: checkpoint.files_processed as usize, // u64→usize safe for file counts
                },
                timestamp_unix_ms: checkpoint.created_at_unix_ms,
            })?;

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
            Some(s) => self.control_plane.find_runs_by_status(s)?,
            None => self.control_plane.list_runs()?,
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
        let all_runs = self.control_plane.list_runs().unwrap_or_default();
        let mut sorted = all_runs;
        // Sort by requested_at (not started_at) because started_at is Option<u64>
        // and may be None for Queued runs. requested_at is always set.
        sorted.sort_by(|a, b| b.requested_at_unix_ms.cmp(&a.requested_at_unix_ms));
        sorted.into_iter().take(limit).map(|r| r.run_id).collect()
    }

    fn build_run_report(&self, run: IndexRun) -> Result<RunStatusReport> {
        let has_active_run = self.has_active_run(&run.repo_id);
        let is_active = has_active_run
            && (run.status == IndexRunStatus::Running
                || matches!(
                    run.recovery_state.as_ref().map(|state| &state.state),
                    Some(RecoveryStateKind::Resumed)
                ));

        let progress = if is_active {
            self.get_active_progress(&run.repo_id)
        } else {
            None
        };

        let file_outcome_summary = if run.status.is_terminal() {
            let records = self.control_plane.get_file_records(&run.run_id)?;
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
        let mut classification =
            classify_run_action(&run, &health, run.recovery_state.as_ref());

        // Surface repo-level invalidation overlay
        if let Ok(Some(repo)) = self.control_plane.get_repository(&run.repo_id) {
            if repo.status == RepositoryStatus::Invalidated {
                let invalidation_note =
                    "repository indexed state has been invalidated — re-index or repair required";
                classification.detail = if classification.action_required {
                    format!("{}. {invalidation_note}", classification.detail)
                } else {
                    invalidation_note.to_string()
                };
                classification.action_required = true;
            }
        }

        let next_action = classification.next_action.clone();
        let action_required = if classification.action_required {
            Some(classification.detail.clone())
        } else {
            None
        };

        Ok(RunStatusReport {
            run,
            health,
            is_active,
            progress,
            file_outcome_summary,
            classification,
            next_action,
            action_required,
        })
    }


    pub fn repair_repository(
        self: &Arc<Self>,
        repo_id: &str,
        scope: RepairScope,
        repo_root: PathBuf,
        blob_store: Arc<dyn BlobStore>,
    ) -> Result<RepairResult> {
        let repo = self
            .control_plane
            .get_repository(repo_id)?
            .ok_or_else(|| TokenizorError::NotFound(format!("repository not found: {repo_id}")))?;

        let previous_status = repo.status.clone();

        if matches!(previous_status, RepositoryStatus::Ready | RepositoryStatus::Pending) {
            let result = RepairResult {
                repo_id: repo_id.to_string(),
                scope: scope.clone(),
                previous_status: previous_status.clone(),
                outcome: RepairOutcome::AlreadyHealthy,
                next_action: None,
                detail: "repository is already in a healthy state".to_string(),
                recorded_at_unix_ms: unix_timestamp_ms(),
            };
            self.record_repair_event(&result)?;
            return Ok(result);
        }

        let outcome = match &scope {
            RepairScope::Repository => {
                self.repair_repository_state(repo_id, &previous_status, &repo_root, blob_store)?
            }
            RepairScope::Run { run_id } => {
                self.repair_run_state(run_id, repo_root.clone(), blob_store.clone())?
            }
            RepairScope::File {
                run_id,
                relative_path,
            } => self.repair_file_state(run_id, relative_path, &repo_root, repo_id)?,
        };

        let next_action = match &outcome {
            RepairOutcome::RequiresReindex => Some(NextAction::Reindex),
            RepairOutcome::InProgress { .. } => Some(NextAction::Wait),
            RepairOutcome::CannotRestore { .. } => Some(NextAction::Repair),
            _ => None,
        };

        let detail = match &outcome {
            RepairOutcome::Restored => "repair successfully restored trusted state".to_string(),
            RepairOutcome::AlreadyHealthy => "no repair needed".to_string(),
            RepairOutcome::CannotRestore { reason } => {
                format!("repair cannot restore trust: {reason}")
            }
            RepairOutcome::RequiresReindex => {
                "repair requires a full reindex to restore trust".to_string()
            }
            RepairOutcome::InProgress { run_id } => format!("repair spawned run {run_id}"),
        };

        let result = RepairResult {
            repo_id: repo_id.to_string(),
            scope,
            previous_status,
            outcome,
            next_action,
            detail,
            recorded_at_unix_ms: unix_timestamp_ms(),
        };

        self.record_repair_event(&result)?;
        Ok(result)
    }

    fn record_repair_event(&self, result: &RepairResult) -> Result<()> {
        let event = RepairEvent {
            repo_id: result.repo_id.clone(),
            scope: result.scope.clone(),
            previous_status: result.previous_status.clone(),
            outcome: result.outcome.clone(),
            detail: result.detail.clone(),
            timestamp_unix_ms: result.recorded_at_unix_ms,
        };
        self.control_plane.save_repair_event(&event)
    }

    fn repair_repository_state(
        self: &Arc<Self>,
        repo_id: &str,
        status: &RepositoryStatus,
        repo_root: &Path,
        blob_store: Arc<dyn BlobStore>,
    ) -> Result<RepairOutcome> {
        match status {
            RepositoryStatus::Degraded => {
                let latest_run = self.control_plane.get_latest_completed_run(repo_id)?;
                if let Some(run) = latest_run {
                    let records = self.control_plane.get_file_records(&run.run_id)?;
                    let failed_count = records
                        .iter()
                        .filter(|r| matches!(r.outcome, PersistedFileOutcome::Failed { .. }))
                        .count();
                    if failed_count == 0 {
                        self.control_plane.update_repository_status(
                            repo_id,
                            RepositoryStatus::Ready,
                            None,
                            None,
                            None,
                            None,
                        )?;
                        self.control_plane
                            .save_operational_event(&OperationalEvent {
                                repo_id: repo_id.to_string(),
                                kind: OperationalEventKind::RepositoryStatusChanged {
                                    previous: RepositoryStatus::Degraded,
                                    current: RepositoryStatus::Ready,
                                    trigger: "repair: no failed files found".to_string(),
                                },
                                timestamp_unix_ms: unix_timestamp_ms(),
                            })?;
                        return Ok(RepairOutcome::Restored);
                    }
                }
                match self.reindex_repository(
                    repo_id,
                    None,
                    Some("repair: degraded repository with failed files"),
                    repo_root.to_path_buf(),
                    blob_store,
                ) {
                    Ok(run) => Ok(RepairOutcome::InProgress {
                        run_id: run.run_id,
                    }),
                    Err(e) => Ok(RepairOutcome::CannotRestore {
                        reason: format!("reindex failed to start: {e}"),
                    }),
                }
            }
            RepositoryStatus::Quarantined => {
                let latest_run = self.control_plane.get_latest_completed_run(repo_id)?;
                if let Some(run) = latest_run {
                    let records = self.control_plane.get_file_records(&run.run_id)?;
                    let quarantined: Vec<_> = records
                        .iter()
                        .filter(|r| {
                            matches!(r.outcome, PersistedFileOutcome::Quarantined { .. })
                        })
                        .collect();
                    if quarantined.is_empty() {
                        self.control_plane.update_repository_status(
                            repo_id,
                            RepositoryStatus::Ready,
                            None,
                            None,
                            None,
                            None,
                        )?;
                        self.control_plane
                            .save_operational_event(&OperationalEvent {
                                repo_id: repo_id.to_string(),
                                kind: OperationalEventKind::RepositoryStatusChanged {
                                    previous: RepositoryStatus::Quarantined,
                                    current: RepositoryStatus::Ready,
                                    trigger: "repair: no quarantined files found".to_string(),
                                },
                                timestamp_unix_ms: unix_timestamp_ms(),
                            })?;
                        return Ok(RepairOutcome::Restored);
                    }

                    let mut verified_indices = Vec::new();
                    let mut failed_paths = Vec::new();
                    for (i, record) in quarantined.iter().enumerate() {
                        if verify_file_against_source(record, repo_root) {
                            verified_indices.push(i);
                        } else {
                            failed_paths.push(record.relative_path.clone());
                            if let Err(e) = self.control_plane.save_operational_event(
                                &OperationalEvent {
                                    repo_id: repo_id.to_string(),
                                    kind: OperationalEventKind::IntegrityEvent {
                                        run_id: Some(run.run_id.clone()),
                                        relative_path: Some(record.relative_path.clone()),
                                        kind: IntegrityEventKind::SuspectDetected,
                                        detail: "quarantined file failed re-verification during repair".to_string(),
                                    },
                                    timestamp_unix_ms: unix_timestamp_ms(),
                                },
                            ) {
                                warn!(repo_id = %repo_id, path = %record.relative_path, error = %e, "failed to record suspect detected event");
                            }
                        }
                    }

                    if failed_paths.is_empty() {
                        let updated_records: Vec<FileRecord> = quarantined
                            .iter()
                            .map(|r| {
                                let mut updated = (*r).clone();
                                updated.outcome = PersistedFileOutcome::Committed;
                                updated
                            })
                            .collect();
                        self.control_plane
                            .save_file_records(&run.run_id, &updated_records)?;
                        self.control_plane.update_repository_status(
                            repo_id,
                            RepositoryStatus::Ready,
                            None,
                            None,
                            None,
                            None,
                        )?;
                        self.control_plane
                            .save_operational_event(&OperationalEvent {
                                repo_id: repo_id.to_string(),
                                kind: OperationalEventKind::RepositoryStatusChanged {
                                    previous: RepositoryStatus::Quarantined,
                                    current: RepositoryStatus::Ready,
                                    trigger: "repair: all quarantined files re-verified"
                                        .to_string(),
                                },
                                timestamp_unix_ms: unix_timestamp_ms(),
                            })?;
                        return Ok(RepairOutcome::Restored);
                    }

                    if !verified_indices.is_empty() {
                        let verified_records: Vec<FileRecord> = verified_indices
                            .iter()
                            .map(|&i| {
                                let mut updated = quarantined[i].clone();
                                updated.outcome = PersistedFileOutcome::Committed;
                                updated
                            })
                            .collect();
                        self.control_plane
                            .save_file_records(&run.run_id, &verified_records)?;
                        self.control_plane.update_repository_status(
                            repo_id,
                            RepositoryStatus::Degraded,
                            None,
                            None,
                            None,
                            None,
                        )?;
                        self.control_plane
                            .save_operational_event(&OperationalEvent {
                                repo_id: repo_id.to_string(),
                                kind: OperationalEventKind::RepositoryStatusChanged {
                                    previous: RepositoryStatus::Quarantined,
                                    current: RepositoryStatus::Degraded,
                                    trigger: "repair: partial re-verification".to_string(),
                                },
                                timestamp_unix_ms: unix_timestamp_ms(),
                            })?;
                    }

                    Ok(RepairOutcome::CannotRestore {
                        reason: format!(
                            "{} of {} quarantined files could not be re-verified: {}",
                            failed_paths.len(),
                            quarantined.len(),
                            failed_paths.join(", ")
                        ),
                    })
                } else {
                    Ok(RepairOutcome::CannotRestore {
                        reason: "no completed run found for quarantined repository".to_string(),
                    })
                }
            }
            RepositoryStatus::Failed | RepositoryStatus::Invalidated => {
                match self.reindex_repository(
                    repo_id,
                    None,
                    Some("repair: restoring from failed/invalidated state"),
                    repo_root.to_path_buf(),
                    blob_store,
                ) {
                    Ok(run) => Ok(RepairOutcome::InProgress {
                        run_id: run.run_id,
                    }),
                    Err(e) => Ok(RepairOutcome::CannotRestore {
                        reason: format!("reindex failed to start: {e}"),
                    }),
                }
            }
            _ => Ok(RepairOutcome::AlreadyHealthy),
        }
    }

    fn repair_run_state(
        self: &Arc<Self>,
        run_id: &str,
        repo_root: PathBuf,
        blob_store: Arc<dyn BlobStore>,
    ) -> Result<RepairOutcome> {
        let run = self
            .control_plane
            .find_run(run_id)?
            .ok_or_else(|| TokenizorError::NotFound(format!("run not found: {run_id}")))?;

        match run.status {
            IndexRunStatus::Interrupted => {
                match self.resume_run(run_id, repo_root, blob_store) {
                    Ok(ResumeRunOutcome::Resumed { run, .. }) => {
                        Ok(RepairOutcome::InProgress {
                            run_id: run.run_id,
                        })
                    }
                    Ok(ResumeRunOutcome::Rejected {
                        reason, detail, ..
                    }) => {
                        match reason {
                            ResumeRejectReason::MissingCheckpoint
                            | ResumeRejectReason::EmptyCheckpointCursor
                            | ResumeRejectReason::MissingDiscoveryManifest
                            | ResumeRejectReason::CorruptDiscoveryManifest
                            | ResumeRejectReason::MissingDurableOutputs
                            | ResumeRejectReason::CheckpointCursorMissing => {
                                Ok(RepairOutcome::RequiresReindex)
                            }
                            _ => Ok(RepairOutcome::CannotRestore {
                                reason: detail,
                            }),
                        }
                    }
                    Err(e) => Ok(RepairOutcome::CannotRestore {
                        reason: format!("resume failed: {e}"),
                    }),
                }
            }
            IndexRunStatus::Failed => Ok(RepairOutcome::RequiresReindex),
            IndexRunStatus::Cancelled
            | IndexRunStatus::Aborted
            | IndexRunStatus::Succeeded => Ok(RepairOutcome::AlreadyHealthy),
            IndexRunStatus::Queued | IndexRunStatus::Running => Ok(RepairOutcome::CannotRestore {
                reason: "run is still active; wait for completion or cancel first".to_string(),
            }),
        }
    }

    fn repair_file_state(
        &self,
        run_id: &str,
        relative_path: &str,
        repo_root: &Path,
        repo_id: &str,
    ) -> Result<RepairOutcome> {
        let records = self.control_plane.get_file_records(run_id)?;
        let record = records
            .iter()
            .find(|r| r.relative_path == relative_path)
            .ok_or_else(|| {
                TokenizorError::NotFound(format!(
                    "file '{relative_path}' not found in run '{run_id}'"
                ))
            })?;

        match &record.outcome {
            PersistedFileOutcome::Quarantined { .. } => {
                if verify_file_against_source(record, repo_root) {
                    let mut updated = record.clone();
                    updated.outcome = PersistedFileOutcome::Committed;
                    self.control_plane
                        .save_file_records(run_id, &[updated])?;
                    let all_records = self.control_plane.get_file_records(run_id)?;
                    let has_quarantined = all_records.iter().any(|r| {
                        matches!(r.outcome, PersistedFileOutcome::Quarantined { .. })
                    });
                    let has_failed = all_records.iter().any(|r| {
                        matches!(r.outcome, PersistedFileOutcome::Failed { .. })
                    });
                    if !has_quarantined && !has_failed {
                        self.control_plane.update_repository_status(
                            repo_id,
                            RepositoryStatus::Ready,
                            None,
                            None,
                            None,
                            None,
                        )?;
                    }
                    Ok(RepairOutcome::Restored)
                } else {
                    Ok(RepairOutcome::CannotRestore {
                        reason: "source file has diverged from indexed state; reindex required"
                            .to_string(),
                    })
                }
            }
            PersistedFileOutcome::Failed { .. } => Ok(RepairOutcome::RequiresReindex),
            PersistedFileOutcome::Committed | PersistedFileOutcome::EmptySymbols => {
                Ok(RepairOutcome::AlreadyHealthy)
            }
        }
    }

    // --- Repository health inspection (Story 4.5) ---

    pub fn inspect_repository_health(&self, repo_id: &str) -> Result<RepositoryHealthReport> {
        let repo = self
            .control_plane
            .get_repository(repo_id)?
            .ok_or_else(|| TokenizorError::NotFound(format!("repository not found: {repo_id}")))?;

        let latest_run = self.control_plane.get_latest_completed_run(repo_id)?;
        let active_run_id = self.get_active_run_id(repo_id);
        let repair_events = self.control_plane.get_repair_events(repo_id)?;
        let recent_repairs: Vec<RepairEvent> = repair_events.into_iter().rev().take(10).collect();

        let classification = classify_repository_action(
            &repo.status,
            latest_run.is_some(),
            active_run_id.is_some(),
            &repo.invalidation_reason,
            &repo.quarantine_reason,
        );

        let action_required = classification.action_required;
        let next_action = classification.next_action.clone();
        let status_detail = classification.detail.clone();

        let file_health = if let Some(ref run) = latest_run {
            let records = self.control_plane.get_file_records(&run.run_id)?;
            Some(Self::compute_file_health_summary(&records))
        } else {
            None
        };

        let run_summary = latest_run.map(|run| RunHealthSummary {
            run_id: run.run_id,
            status: run.status,
            mode: run.mode,
            started_at_unix_ms: run.started_at_unix_ms.unwrap_or(run.requested_at_unix_ms),
            completed_at_unix_ms: run.finished_at_unix_ms,
        });

        let invalidation_context = if repo.status == RepositoryStatus::Invalidated {
            repo.invalidation_reason.map(|reason| StatusContext {
                reason,
                occurred_at_unix_ms: repo.invalidated_at_unix_ms.unwrap_or(0),
            })
        } else {
            None
        };

        let quarantine_context = if repo.status == RepositoryStatus::Quarantined {
            repo.quarantine_reason.map(|reason| StatusContext {
                reason,
                occurred_at_unix_ms: repo.quarantined_at_unix_ms.unwrap_or(0),
            })
        } else {
            None
        };

        Ok(RepositoryHealthReport {
            repo_id: repo_id.to_string(),
            status: repo.status,
            classification,
            action_required,
            next_action,
            status_detail,
            file_health,
            latest_run: run_summary,
            active_run_id,
            recent_repairs,
            invalidation_context,
            quarantine_context,
            checked_at_unix_ms: unix_timestamp_ms(),
        })
    }

    // --- Operational history (Story 4.6) ---

    pub fn get_operational_history(
        &self,
        repo_id: &str,
        filter: &OperationalEventFilter,
    ) -> Result<Vec<OperationalEvent>> {
        // Validate repo exists
        self.control_plane
            .get_repository(repo_id)?
            .ok_or_else(|| TokenizorError::NotFound(format!("repository not found: {repo_id}")))?;
        self.control_plane
            .get_operational_events(repo_id, filter)
    }

    fn compute_file_health_summary(records: &[FileRecord]) -> FileHealthSummary {
        let mut committed = 0usize;
        let mut quarantined = 0usize;
        let mut failed = 0usize;
        let mut empty_symbols = 0usize;

        for record in records {
            match &record.outcome {
                PersistedFileOutcome::Committed => committed += 1,
                PersistedFileOutcome::EmptySymbols => empty_symbols += 1,
                PersistedFileOutcome::Failed { .. } => failed += 1,
                PersistedFileOutcome::Quarantined { .. } => quarantined += 1,
            }
        }

        FileHealthSummary {
            total_files: committed + quarantined + failed + empty_symbols,
            committed,
            quarantined,
            failed,
            empty_symbols,
        }
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

fn run_completion_clears_repository_invalidation(
    status: &IndexRunStatus,
    results: &[FileProcessingResult],
) -> bool {
    // A successful reindex clears invalidation even when some files have
    // partial parses — those are normal tree-sitter behaviour, not a sign
    // of untrusted state.  Only hard failures block the clear.
    *status == IndexRunStatus::Succeeded
        && !results
            .iter()
            .any(|result| matches!(result.outcome, FileOutcome::Failed { .. }))
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

fn validate_discovery_manifest(
    manifest: &DiscoveryManifest,
) -> std::result::Result<Vec<String>, String> {
    if manifest.run_id.trim().is_empty() {
        return Err("persisted discovery manifest is missing its run id".to_string());
    }
    if manifest.relative_paths.is_empty() {
        return Err(format!(
            "persisted discovery manifest for run `{}` contains no indexable paths",
            manifest.run_id
        ));
    }

    let mut seen = std::collections::BTreeSet::new();
    for relative_path in &manifest.relative_paths {
        if relative_path.trim().is_empty() {
            return Err(format!(
                "persisted discovery manifest for run `{}` contains an empty relative path",
                manifest.run_id
            ));
        }
        let extension = Path::new(relative_path)
            .extension()
            .and_then(|ext| ext.to_str())
            .ok_or_else(|| {
                format!(
                    "persisted discovery manifest path `{relative_path}` is missing a supported file extension"
                )
            })?;
        let Some(language) = LanguageId::from_extension(extension) else {
            return Err(format!(
                "persisted discovery manifest path `{relative_path}` uses unsupported extension `{extension}`"
            ));
        };
        if language.support_tier() == crate::domain::SupportTier::Unsupported {
            return Err(format!(
                "persisted discovery manifest path `{relative_path}` resolved to unsupported language `{:?}`",
                language
            ));
        }
        if !seen.insert(relative_path.clone()) {
            return Err(format!(
                "persisted discovery manifest for run `{}` contains duplicate path `{relative_path}`",
                manifest.run_id
            ));
        }
    }

    let mut expected_order = manifest.relative_paths.clone();
    expected_order.sort_by(|left, right| {
        left.to_lowercase()
            .cmp(&right.to_lowercase())
            .then_with(|| left.cmp(right))
    });
    if expected_order != manifest.relative_paths {
        return Err(format!(
            "persisted discovery manifest for run `{}` is not in deterministic path order",
            manifest.run_id
        ));
    }

    Ok(manifest.relative_paths.clone())
}

fn sweep_owned_temp_artifacts(
    scan_dir: Option<&Path>,
    owns_path: impl Fn(&Path) -> bool,
    surface: StartupCleanupSurface,
    report: &mut StartupRecoveryReport,
) {
    let Some(scan_dir) = scan_dir else {
        return;
    };

    let entries = match fs::read_dir(scan_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            report.push_blocking_finding(StartupRecoveryFinding {
                name: surface.check_name().to_string(),
                detail: format!(
                    "startup sweep could not inspect {}s in `{}`: {}",
                    surface.label(),
                    scan_dir.display(),
                    error
                ),
                remediation: surface.cleanup_remediation().to_string(),
            });
            return;
        }
    };

    let mut candidates = Vec::new();
    for entry in entries {
        match entry {
            Ok(entry) => {
                let path = entry.path();
                if owns_path(&path) {
                    candidates.push(path);
                }
            }
            Err(error) => {
                report.push_blocking_finding(StartupRecoveryFinding {
                    name: surface.check_name().to_string(),
                    detail: format!(
                        "startup sweep could not enumerate {}s in `{}`: {}",
                        surface.label(),
                        scan_dir.display(),
                        error
                    ),
                    remediation: surface.cleanup_remediation().to_string(),
                });
            }
        }
    }

    candidates.sort_by(|left, right| left.to_string_lossy().cmp(&right.to_string_lossy()));

    for path in candidates {
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                report.push_blocking_finding(StartupRecoveryFinding {
                    name: surface.check_name().to_string(),
                    detail: format!(
                        "startup sweep could not stat {} `{}`: {}",
                        surface.label(),
                        path.display(),
                        error
                    ),
                    remediation: surface.cleanup_remediation().to_string(),
                });
                continue;
            }
        };

        if !metadata.is_file() {
            report.push_blocking_finding(StartupRecoveryFinding {
                name: surface.check_name().to_string(),
                detail: format!(
                    "startup sweep found a malformed {} at `{}`; expected a file",
                    surface.label(),
                    path.display()
                ),
                remediation: surface.cleanup_remediation().to_string(),
            });
            continue;
        }

        match fs::remove_file(&path) {
            Ok(()) => {
                debug!(
                    surface = surface.check_name(),
                    path = %path.display(),
                    "startup sweep: removed stale temp artifact"
                );
                report
                    .cleaned_temp_artifacts
                    .push(StartupRecoveredTempArtifact {
                        surface: surface.clone(),
                        path,
                    });
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                report.push_blocking_finding(StartupRecoveryFinding {
                    name: surface.check_name().to_string(),
                    detail: format!(
                        "startup sweep could not remove {} `{}`: {}",
                        surface.label(),
                        path.display(),
                        error
                    ),
                    remediation: surface.cleanup_remediation().to_string(),
                });
            }
        }
    }
}

fn verify_file_against_source(file_record: &FileRecord, repo_root: &Path) -> bool {
    let source_path = repo_root.join(&file_record.relative_path);
    match fs::read(&source_path) {
        Ok(bytes) => {
            let current_hash = digest_hex(&bytes);
            current_hash == file_record.content_hash
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ActionCondition;
    use crate::storage::InMemoryControlPlane;

    struct AuthoritativeInMemoryControlPlane {
        backing: InMemoryControlPlane,
    }

    impl Default for AuthoritativeInMemoryControlPlane {
        fn default() -> Self {
            Self {
                backing: InMemoryControlPlane::default(),
            }
        }
    }

    impl ControlPlane for AuthoritativeInMemoryControlPlane {
        fn backend_name(&self) -> &'static str {
            "spacetimedb"
        }

        fn health_check(&self) -> Result<ComponentHealth> {
            self.backing.health_check()
        }

        fn deployment_checks(&self) -> Result<Vec<ComponentHealth>> {
            self.backing.deployment_checks()
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

        fn get_repository(&self, repo_id: &str) -> Result<Option<crate::domain::Repository>> {
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

        fn save_repository(&self, repository: &crate::domain::Repository) -> Result<()> {
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

        fn save_repair_event(&self, event: &RepairEvent) -> Result<()> {
            self.backing.save_repair_event(event)
        }

        fn get_repair_events(&self, repo_id: &str) -> Result<Vec<RepairEvent>> {
            self.backing.get_repair_events(repo_id)
        }

        fn save_operational_event(&self, event: &OperationalEvent) -> Result<()> {
            self.backing.save_operational_event(event)
        }

        fn get_operational_events(
            &self,
            repo_id: &str,
            filter: &OperationalEventFilter,
        ) -> Result<Vec<OperationalEvent>> {
            self.backing.get_operational_events(repo_id, filter)
        }
    }

    fn temp_run_manager() -> (tempfile::TempDir, RunManager) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("registry.json");
        let persistence = RegistryPersistence::new(path);
        let manager = RunManager::new(persistence);
        (dir, manager)
    }

    #[test]
    fn test_persistence_adapter_does_not_write_mutable_run_state_to_registry() {
        let dir = tempfile::tempdir().unwrap();
        let registry =
            std::sync::Arc::new(RegistryPersistence::new(dir.path().join("registry.json")));
        let adapter = RunManagerPersistenceAdapter::new(
            std::sync::Arc::new(AuthoritativeInMemoryControlPlane::default()),
            std::sync::Arc::clone(&registry),
        );

        let run = IndexRun {
            run_id: "run-1".to_string(),
            repo_id: "repo-1".to_string(),
            mode: IndexRunMode::Full,
            status: IndexRunStatus::Running,
            requested_at_unix_ms: 1000,
            started_at_unix_ms: Some(1001),
            finished_at_unix_ms: None,
            idempotency_key: Some("idem-1".to_string()),
            request_hash: Some("hash-1".to_string()),
            checkpoint_cursor: None,
            error_summary: None,
            not_yet_supported: None,
            prior_run_id: None,
            description: Some("adapter test".to_string()),
            recovery_state: None,
        };
        let file_record = FileRecord {
            relative_path: "src/lib.rs".to_string(),
            language: LanguageId::Rust,
            blob_id: "blob-1".to_string(),
            byte_len: 12,
            content_hash: "deadbeef".to_string(),
            outcome: PersistedFileOutcome::Committed,
            symbols: Vec::new(),
            run_id: run.run_id.clone(),
            repo_id: run.repo_id.clone(),
            committed_at_unix_ms: 1002,
        };
        let checkpoint = Checkpoint {
            run_id: run.run_id.clone(),
            cursor: file_record.relative_path.clone(),
            files_processed: 1,
            symbols_written: 0,
            files_failed: 0,
            created_at_unix_ms: 1003,
        };
        let idempotency = IdempotencyRecord {
            operation: "index_repository".to_string(),
            idempotency_key: "idem-1".to_string(),
            request_hash: "hash-1".to_string(),
            status: IdempotencyStatus::Succeeded,
            result_ref: Some(run.run_id.clone()),
            created_at_unix_ms: 1004,
            expires_at_unix_ms: None,
        };
        let manifest = DiscoveryManifest {
            run_id: run.run_id.clone(),
            discovered_at_unix_ms: 1005,
            relative_paths: vec!["src/lib.rs".to_string()],
        };

        adapter.save_run(&run).unwrap();
        adapter
            .save_file_records(&run.run_id, std::slice::from_ref(&file_record))
            .unwrap();
        adapter.save_checkpoint(&checkpoint).unwrap();
        adapter.save_idempotency_record(&idempotency).unwrap();
        adapter.save_discovery_manifest(&manifest).unwrap();

        let registry_data = registry.load().unwrap();
        assert!(registry_data.runs.is_empty());
        assert!(registry_data.run_file_records.is_empty());
        assert!(registry_data.checkpoints.is_empty());
        assert!(registry_data.idempotency_records.is_empty());
        assert!(registry_data.discovery_manifests.is_empty());
    }

    #[test]
    fn test_persistence_adapter_preserves_repository_bootstrap_mirror() {
        let dir = tempfile::tempdir().unwrap();
        let registry =
            std::sync::Arc::new(RegistryPersistence::new(dir.path().join("registry.json")));
        let adapter = RunManagerPersistenceAdapter::new(
            std::sync::Arc::new(InMemoryControlPlane::default()),
            std::sync::Arc::clone(&registry),
        );

        let repository = crate::domain::Repository {
            repo_id: "repo-1".to_string(),
            kind: crate::domain::RepositoryKind::Local,
            root_uri: "file:///repo-1".to_string(),
            project_identity: "repo-1".to_string(),
            project_identity_kind: crate::domain::ProjectIdentityKind::LocalRootPath,
            default_branch: Some("main".to_string()),
            last_known_revision: None,
            status: RepositoryStatus::Ready,
            invalidated_at_unix_ms: None,
            invalidation_reason: None,
            quarantined_at_unix_ms: None,
            quarantine_reason: None,
        };

        adapter.save_repository(&repository).unwrap();

        let persisted = registry.get_repository(&repository.repo_id).unwrap();
        assert_eq!(persisted, Some(repository));
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

        let persisted = manager
            .persistence()
            .find_run(&run.run_id)
            .unwrap()
            .unwrap();
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
                recovery_state: None,
            };
            persistence.save_run(&run).unwrap();
        }

        let persistence = RegistryPersistence::new(path);
        let manager = RunManager::new(persistence);
        let recovery = manager.startup_sweep().unwrap();

        assert_eq!(recovery.transitioned_run_ids, vec!["stale-run".to_string()]);
        assert_eq!(
            recovery.transitioned_runs,
            vec![StartupRecoveredRunTransition {
                run_id: "stale-run".to_string(),
                repo_id: "repo-1".to_string(),
                from_status: IndexRunStatus::Running,
                to_status: IndexRunStatus::Interrupted,
            }]
        );
        assert!(recovery.cleaned_temp_artifacts.is_empty());
        assert!(recovery.blocking_findings.is_empty());
        assert!(
            recovery
                .operator_guidance
                .iter()
                .any(|message| message.contains("resume") || message.contains("repair"))
        );
        let run = manager
            .persistence()
            .find_run("stale-run")
            .unwrap()
            .unwrap();
        assert_eq!(run.status, IndexRunStatus::Interrupted);
        assert!(run.error_summary.is_some());
    }

    #[test]
    fn test_startup_sweep_transitions_stale_queued_run_with_checkpoint_to_interrupted() {
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
                recovery_state: None,
            })
            .unwrap();
        persistence
            .save_checkpoint(&Checkpoint {
                run_id: "queued-run".to_string(),
                cursor: "src/lib.rs".to_string(),
                files_processed: 1,
                symbols_written: 3,
                files_failed: 0,
                created_at_unix_ms: 1001,
            })
            .unwrap();

        let manager = RunManager::new(RegistryPersistence::new(path));
        let recovery = manager.startup_sweep().unwrap();

        assert_eq!(
            recovery.transitioned_run_ids,
            vec!["queued-run".to_string()]
        );
        assert_eq!(recovery.interrupted_run_count(), 1);
        assert_eq!(recovery.aborted_run_count(), 0);
        assert!(recovery.cleaned_temp_artifacts.is_empty());
        assert!(recovery.blocking_findings.is_empty());

        let run = manager
            .persistence()
            .find_run("queued-run")
            .unwrap()
            .unwrap();
        assert_eq!(run.status, IndexRunStatus::Interrupted);
        assert_eq!(
            run.error_summary.as_deref(),
            Some(STALE_QUEUED_INTERRUPTED_STARTUP_SWEEP_SUMMARY)
        );
    }

    #[test]
    fn test_startup_sweep_transitions_stale_queued_run_with_durable_file_records() {
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
                recovery_state: None,
            })
            .unwrap();
        persistence
            .save_file_records(
                "queued-run",
                &[FileRecord {
                    relative_path: "src/main.rs".into(),
                    language: crate::domain::LanguageId::Rust,
                    blob_id: "blob-1".into(),
                    byte_len: 42,
                    content_hash: "hash-1".into(),
                    outcome: PersistedFileOutcome::Committed,
                    symbols: vec![],
                    run_id: "queued-run".into(),
                    repo_id: "repo-1".into(),
                    committed_at_unix_ms: 1001,
                }],
            )
            .unwrap();

        let manager = RunManager::new(RegistryPersistence::new(path));
        let recovery = manager.startup_sweep().unwrap();

        assert_eq!(
            recovery.transitioned_run_ids,
            vec!["queued-run".to_string()]
        );
        assert_eq!(recovery.interrupted_run_count(), 1);
        assert_eq!(recovery.aborted_run_count(), 0);

        let run = manager
            .persistence()
            .find_run("queued-run")
            .unwrap()
            .unwrap();
        assert_eq!(run.status, IndexRunStatus::Interrupted);
        assert_eq!(
            run.error_summary.as_deref(),
            Some(STALE_QUEUED_INTERRUPTED_STARTUP_SWEEP_SUMMARY)
        );
    }

    #[test]
    fn test_startup_sweep_aborts_stale_queued_run_without_durable_progress() {
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
                recovery_state: None,
            })
            .unwrap();

        let manager = RunManager::new(RegistryPersistence::new(path));
        let recovery = manager.startup_sweep().unwrap();

        assert_eq!(
            recovery.transitioned_run_ids,
            vec!["queued-run".to_string()]
        );
        assert_eq!(recovery.interrupted_run_count(), 0);
        assert_eq!(recovery.aborted_run_count(), 1);
        assert!(
            recovery
                .operator_guidance
                .iter()
                .any(|message| message.contains("do not resume"))
        );

        let run = manager
            .persistence()
            .find_run("queued-run")
            .unwrap()
            .unwrap();
        assert_eq!(run.status, IndexRunStatus::Aborted);
        assert!(run.finished_at_unix_ms.is_some());
        assert_eq!(
            run.error_summary.as_deref(),
            Some(STALE_QUEUED_ABORTED_SUMMARY)
        );

        let replacement = manager.start_run("repo-1", IndexRunMode::Full).unwrap();
        assert_eq!(replacement.repo_id, "repo-1");
        assert_eq!(replacement.status, IndexRunStatus::Queued);
    }

    #[test]
    fn test_startup_sweep_ignores_terminal_runs_without_recovery_work() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("registry.json");

        let persistence = RegistryPersistence::new(path.clone());
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
                recovery_state: None,
            })
            .unwrap();

        let manager = RunManager::new(RegistryPersistence::new(path));
        let recovery = manager.startup_sweep().unwrap();

        assert!(recovery.transitioned_run_ids.is_empty());
        assert!(recovery.transitioned_runs.is_empty());
        assert!(recovery.cleaned_temp_artifacts.is_empty());
        assert!(recovery.blocking_findings.is_empty());
    }

    #[test]
    fn test_startup_sweep_cleans_owned_temp_artifacts_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let registry_path = dir
            .path()
            .join("control-plane")
            .join("project-workspace-registry.json");
        let blob_root = dir.path().join("blob-root");
        let temp_dir = blob_root.join("temp");

        std::fs::create_dir_all(registry_path.parent().unwrap()).unwrap();
        std::fs::create_dir_all(&temp_dir).unwrap();

        let registry_temp = registry_path
            .parent()
            .unwrap()
            .join(".project-workspace-registry.json.123.tmp");
        let cas_temp = temp_dir.join(format!("{}.456.42.tmp", "a".repeat(64)));
        let ignored = registry_path.parent().unwrap().join(".other.json.123.tmp");

        std::fs::write(&registry_temp, b"stale-registry").unwrap();
        std::fs::write(&cas_temp, b"stale-cas").unwrap();
        std::fs::write(&ignored, b"keep-me").unwrap();

        let manager =
            RunManager::with_blob_root(RegistryPersistence::new(registry_path), blob_root);
        let first = manager.startup_sweep().unwrap();

        assert_eq!(first.cleaned_temp_artifacts.len(), 2);
        assert!(!registry_temp.exists());
        assert!(!cas_temp.exists());
        assert!(ignored.exists());
        assert!(
            first
                .operator_guidance
                .iter()
                .any(|message| message.contains("wait") || message.contains("repair"))
        );

        let second = manager.startup_sweep().unwrap();
        assert!(second.transitioned_run_ids.is_empty());
        assert!(second.cleaned_temp_artifacts.is_empty());
        assert!(second.blocking_findings.is_empty());
    }

    #[test]
    fn test_startup_sweep_reports_blocking_findings_for_malformed_temp_artifacts() {
        let dir = tempfile::tempdir().unwrap();
        let registry_path = dir
            .path()
            .join("control-plane")
            .join("project-workspace-registry.json");
        let blob_root = dir.path().join("blob-root");
        let temp_dir = blob_root.join("temp");

        std::fs::create_dir_all(registry_path.parent().unwrap()).unwrap();
        std::fs::create_dir_all(&temp_dir).unwrap();

        let registry_temp_dir = registry_path
            .parent()
            .unwrap()
            .join(".project-workspace-registry.json.123.tmp");
        let cas_temp_dir = temp_dir.join(format!("{}.456.42.tmp", "b".repeat(64)));

        std::fs::create_dir_all(&registry_temp_dir).unwrap();
        std::fs::create_dir_all(&cas_temp_dir).unwrap();

        let manager =
            RunManager::with_blob_root(RegistryPersistence::new(registry_path), blob_root);
        let recovery = manager.startup_sweep().unwrap();

        assert!(recovery.cleaned_temp_artifacts.is_empty());
        assert_eq!(recovery.blocking_findings.len(), 2);
        assert!(
            recovery
                .blocking_findings
                .iter()
                .all(|finding| finding.remediation.contains("repair")
                    || finding.remediation.contains("wait"))
        );
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
            recovery_state: None,
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

    fn sample_file_processing_result(outcome: FileOutcome) -> FileProcessingResult {
        FileProcessingResult {
            relative_path: "src/lib.rs".to_string(),
            language: LanguageId::Rust,
            outcome,
            symbols: vec![],
            byte_len: 16,
            content_hash: "abc123".to_string(),
        }
    }

    #[test]
    fn test_succeeded_run_clears_invalidation_when_all_results_are_processed() {
        let results = vec![sample_file_processing_result(FileOutcome::Processed)];
        assert!(run_completion_clears_repository_invalidation(
            &IndexRunStatus::Succeeded,
            &results
        ));
    }

    #[test]
    fn test_succeeded_run_clears_invalidation_when_some_results_are_partial() {
        let results = vec![sample_file_processing_result(FileOutcome::PartialParse {
            warning: "parser recovered".to_string(),
        })];
        assert!(run_completion_clears_repository_invalidation(
            &IndexRunStatus::Succeeded,
            &results
        ));
    }

    #[test]
    fn test_succeeded_run_does_not_clear_invalidation_when_any_result_failed() {
        let results = vec![sample_file_processing_result(FileOutcome::Failed {
            error: "boom".to_string(),
        })];
        assert!(!run_completion_clears_repository_invalidation(
            &IndexRunStatus::Succeeded,
            &results
        ));
    }

    #[test]
    fn test_action_required_for_interrupted_run() {
        let run = sample_run_with_status(IndexRunStatus::Interrupted);
        let health = classify_run_health(&run, None);
        let classification = classify_run_action(&run, &health, run.recovery_state.as_ref());
        assert!(classification.action_required);
        assert_eq!(classification.condition, ActionCondition::Interrupted);
        assert!(classification.detail.contains("interrupted"));
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
        let classification = classify_run_action(&run, &health, run.recovery_state.as_ref());
        assert!(!classification.action_required);
    }

    #[test]
    fn test_action_required_for_resume_rejected_run_mentions_next_action() {
        let mut run = sample_run_with_status(IndexRunStatus::Interrupted);
        run.recovery_state = Some(RunRecoveryState {
            state: RecoveryStateKind::ResumeRejected,
            rejection_reason: Some(ResumeRejectReason::MissingCheckpoint),
            next_action: Some(NextAction::Reindex),
            detail: Some("run has no persisted checkpoint".to_string()),
            updated_at_unix_ms: 1234,
        });
        let health = classify_run_health(&run, None);
        let classification = classify_run_action(&run, &health, run.recovery_state.as_ref());
        assert!(classification.action_required);
        assert!(classification.detail.contains("rejected"));
        assert_eq!(classification.next_action, Some(NextAction::Reindex));
    }

    #[test]
    fn test_action_required_for_startup_aborted_queued_run_mentions_fresh_index() {
        let mut run = sample_run_with_status(IndexRunStatus::Aborted);
        run.error_summary = Some(STALE_QUEUED_ABORTED_SUMMARY.to_string());
        let health = classify_run_health(&run, None);
        let classification = classify_run_action(&run, &health, run.recovery_state.as_ref());
        assert!(classification.action_required);
        assert_eq!(classification.next_action, Some(NextAction::Reindex));
        assert!(classification.detail.contains("fresh index") || classification.detail.contains("reindex") || classification.detail.contains("Reindex"));
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
        manager.persistence().save_repository(&repo).unwrap();
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
            .control_plane
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
        manager.persistence().save_repository(&repo).unwrap();

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
            .control_plane
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
            .control_plane
            .get_repository("repo-1")
            .unwrap()
            .unwrap();
        assert_eq!(repo.status, RepositoryStatus::Invalidated);
        assert_eq!(repo.invalidation_reason.as_deref(), Some("reason-B"));
    }

    fn seed_repo_with_status(manager: &RunManager, repo_id: &str, status: RepositoryStatus) {
        let repo = crate::domain::Repository {
            repo_id: repo_id.to_string(),
            kind: crate::domain::RepositoryKind::Git,
            root_uri: format!("/tmp/{repo_id}"),
            project_identity: format!("identity-{repo_id}"),
            project_identity_kind: crate::domain::ProjectIdentityKind::GitCommonDir,
            default_branch: None,
            last_known_revision: None,
            status,
            invalidated_at_unix_ms: None,
            invalidation_reason: None,
            quarantined_at_unix_ms: None,
            quarantine_reason: None,
        };
        manager.persistence().save_repository(&repo).unwrap();
    }

    #[test]
    fn test_repair_ready_repository_returns_already_healthy() {
        let (_dir, manager, _cas_dir, cas) = temp_reindex_env();
        seed_repo_with_status(&manager, "repo-1", RepositoryStatus::Ready);

        let result = manager
            .repair_repository(
                "repo-1",
                RepairScope::Repository,
                PathBuf::from("/tmp/repo-1"),
                cas,
            )
            .unwrap();

        assert_eq!(result.outcome, RepairOutcome::AlreadyHealthy);
        assert_eq!(result.previous_status, RepositoryStatus::Ready);
        assert!(result.next_action.is_none());
    }

    #[test]
    fn test_repair_pending_repository_returns_already_healthy() {
        let (_dir, manager, _cas_dir, cas) = temp_reindex_env();
        seed_repo_with_status(&manager, "repo-1", RepositoryStatus::Pending);

        let result = manager
            .repair_repository(
                "repo-1",
                RepairScope::Repository,
                PathBuf::from("/tmp/repo-1"),
                cas,
            )
            .unwrap();

        assert_eq!(result.outcome, RepairOutcome::AlreadyHealthy);
    }

    #[test]
    fn test_repair_degraded_no_failed_files_marks_ready() {
        let (_dir, manager, _cas_dir, cas) = temp_reindex_env();
        seed_repo_with_status(&manager, "repo-1", RepositoryStatus::Degraded);

        let run = IndexRun {
            run_id: "run-d1".to_string(),
            repo_id: "repo-1".to_string(),
            mode: IndexRunMode::Full,
            status: IndexRunStatus::Succeeded,
            requested_at_unix_ms: 1000,
            started_at_unix_ms: Some(1001),
            finished_at_unix_ms: Some(1002),
            idempotency_key: None,
            request_hash: None,
            checkpoint_cursor: None,
            error_summary: None,
            not_yet_supported: None,
            prior_run_id: None,
            description: None,
            recovery_state: None,
        };
        manager.persistence().save_run(&run).unwrap();

        let record = FileRecord {
            relative_path: "src/lib.rs".to_string(),
            language: LanguageId::Rust,
            blob_id: "blob-1".to_string(),
            byte_len: 10,
            content_hash: "aabb".to_string(),
            outcome: PersistedFileOutcome::Committed,
            symbols: Vec::new(),
            run_id: "run-d1".to_string(),
            repo_id: "repo-1".to_string(),
            committed_at_unix_ms: 1002,
        };
        manager
            .persistence()
            .save_file_records("run-d1", &[record])
            .unwrap();

        let result = manager
            .repair_repository(
                "repo-1",
                RepairScope::Repository,
                PathBuf::from("/tmp/repo-1"),
                cas,
            )
            .unwrap();

        assert_eq!(result.outcome, RepairOutcome::Restored);
        let repo = manager
            .control_plane
            .get_repository("repo-1")
            .unwrap()
            .unwrap();
        assert_eq!(repo.status, RepositoryStatus::Ready);
    }

    #[tokio::test]
    async fn test_repair_failed_repository_delegates_to_reindex() {
        let (dir, manager, _cas_dir, cas) = temp_reindex_env();
        seed_repo_with_status(&manager, "repo-1", RepositoryStatus::Failed);

        let result = manager
            .repair_repository(
                "repo-1",
                RepairScope::Repository,
                dir.path().to_path_buf(),
                cas,
            )
            .unwrap();

        match &result.outcome {
            RepairOutcome::InProgress { run_id } => {
                assert!(!run_id.is_empty());
            }
            RepairOutcome::CannotRestore { .. } => {
                // acceptable if reindex fails to start in test env
            }
            other => panic!("expected InProgress or CannotRestore, got {other:?}"),
        }
    }

    #[test]
    fn test_repair_records_event() {
        let (_dir, manager, _cas_dir, cas) = temp_reindex_env();
        seed_repo_with_status(&manager, "repo-1", RepositoryStatus::Ready);

        manager
            .repair_repository(
                "repo-1",
                RepairScope::Repository,
                PathBuf::from("/tmp/repo-1"),
                cas,
            )
            .unwrap();

        let events = manager
            .control_plane
            .get_repair_events("repo-1")
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].repo_id, "repo-1");
        assert_eq!(events[0].outcome, RepairOutcome::AlreadyHealthy);
    }

    #[tokio::test]
    async fn test_repair_never_silently_marks_healthy_on_failed_files() {
        let (dir, manager, _cas_dir, cas) = temp_reindex_env();
        seed_repo_with_status(&manager, "repo-1", RepositoryStatus::Degraded);

        let run = IndexRun {
            run_id: "run-f1".to_string(),
            repo_id: "repo-1".to_string(),
            mode: IndexRunMode::Full,
            status: IndexRunStatus::Succeeded,
            requested_at_unix_ms: 1000,
            started_at_unix_ms: Some(1001),
            finished_at_unix_ms: Some(1002),
            idempotency_key: None,
            request_hash: None,
            checkpoint_cursor: None,
            error_summary: None,
            not_yet_supported: None,
            prior_run_id: None,
            description: None,
            recovery_state: None,
        };
        manager.persistence().save_run(&run).unwrap();

        let record = FileRecord {
            relative_path: "src/broken.rs".to_string(),
            language: LanguageId::Rust,
            blob_id: "blob-broken".to_string(),
            byte_len: 10,
            content_hash: "ffff".to_string(),
            outcome: PersistedFileOutcome::Failed {
                error: "parse error".to_string(),
            },
            symbols: Vec::new(),
            run_id: "run-f1".to_string(),
            repo_id: "repo-1".to_string(),
            committed_at_unix_ms: 1002,
        };
        manager
            .persistence()
            .save_file_records("run-f1", &[record])
            .unwrap();

        let result = manager
            .repair_repository(
                "repo-1",
                RepairScope::Repository,
                dir.path().to_path_buf(),
                cas,
            )
            .unwrap();

        // Must NOT silently restore to Ready when failed files exist
        assert_ne!(result.outcome, RepairOutcome::Restored);
        assert_ne!(result.outcome, RepairOutcome::AlreadyHealthy);
    }

    #[test]
    fn test_repair_run_scope_succeeded_returns_healthy() {
        let (_dir, manager, _cas_dir, cas) = temp_reindex_env();
        seed_repo_with_status(&manager, "repo-1", RepositoryStatus::Degraded);

        let run = IndexRun {
            run_id: "run-ok".to_string(),
            repo_id: "repo-1".to_string(),
            mode: IndexRunMode::Full,
            status: IndexRunStatus::Succeeded,
            requested_at_unix_ms: 1000,
            started_at_unix_ms: Some(1001),
            finished_at_unix_ms: Some(1002),
            idempotency_key: None,
            request_hash: None,
            checkpoint_cursor: None,
            error_summary: None,
            not_yet_supported: None,
            prior_run_id: None,
            description: None,
            recovery_state: None,
        };
        manager.persistence().save_run(&run).unwrap();

        let result = manager
            .repair_repository(
                "repo-1",
                RepairScope::Run {
                    run_id: "run-ok".to_string(),
                },
                PathBuf::from("/tmp/repo-1"),
                cas,
            )
            .unwrap();

        assert_eq!(result.outcome, RepairOutcome::AlreadyHealthy);
    }

    #[test]
    fn test_repair_quarantined_all_verified_marks_ready() {
        let (_dir, manager, _cas_dir, cas) = temp_reindex_env();
        seed_repo_with_status(&manager, "repo-q1", RepositoryStatus::Quarantined);

        let repo_dir = tempfile::tempdir().unwrap();
        let file_content = b"fn quarantined() {}";
        let file_hash = digest_hex(file_content);
        std::fs::create_dir_all(repo_dir.path().join("src")).unwrap();
        std::fs::write(repo_dir.path().join("src/q.rs"), file_content).unwrap();

        let run = IndexRun {
            run_id: "run-q1".to_string(),
            repo_id: "repo-q1".to_string(),
            mode: IndexRunMode::Full,
            status: IndexRunStatus::Succeeded,
            requested_at_unix_ms: 1000,
            started_at_unix_ms: Some(1001),
            finished_at_unix_ms: Some(1002),
            idempotency_key: None,
            request_hash: None,
            checkpoint_cursor: None,
            error_summary: None,
            not_yet_supported: None,
            prior_run_id: None,
            description: None,
            recovery_state: None,
        };
        manager.persistence().save_run(&run).unwrap();

        let record = FileRecord {
            relative_path: "src/q.rs".to_string(),
            language: LanguageId::Rust,
            blob_id: "blob-q1".to_string(),
            byte_len: file_content.len() as u64,
            content_hash: file_hash,
            outcome: PersistedFileOutcome::Quarantined {
                reason: "test".to_string(),
            },
            symbols: Vec::new(),
            run_id: "run-q1".to_string(),
            repo_id: "repo-q1".to_string(),
            committed_at_unix_ms: 1002,
        };
        manager
            .persistence()
            .save_file_records("run-q1", &[record])
            .unwrap();

        let result = manager
            .repair_repository(
                "repo-q1",
                RepairScope::Repository,
                repo_dir.path().to_path_buf(),
                cas,
            )
            .unwrap();

        assert_eq!(result.outcome, RepairOutcome::Restored);
        let repo = manager
            .control_plane
            .get_repository("repo-q1")
            .unwrap()
            .unwrap();
        assert_eq!(repo.status, RepositoryStatus::Ready);
    }

    #[test]
    fn test_repair_quarantined_partial_failure_reports_cannot_restore() {
        let (_dir, manager, _cas_dir, cas) = temp_reindex_env();
        seed_repo_with_status(&manager, "repo-q2", RepositoryStatus::Quarantined);

        let repo_dir = tempfile::tempdir().unwrap();

        // File A: matching content
        let file_a_content = b"fn file_a() {}";
        let file_a_hash = digest_hex(file_a_content);
        std::fs::create_dir_all(repo_dir.path().join("src")).unwrap();
        std::fs::write(repo_dir.path().join("src/a.rs"), file_a_content).unwrap();

        // File B: mismatched content (don't write file at all so hash won't match)

        let run = IndexRun {
            run_id: "run-q2".to_string(),
            repo_id: "repo-q2".to_string(),
            mode: IndexRunMode::Full,
            status: IndexRunStatus::Succeeded,
            requested_at_unix_ms: 1000,
            started_at_unix_ms: Some(1001),
            finished_at_unix_ms: Some(1002),
            idempotency_key: None,
            request_hash: None,
            checkpoint_cursor: None,
            error_summary: None,
            not_yet_supported: None,
            prior_run_id: None,
            description: None,
            recovery_state: None,
        };
        manager.persistence().save_run(&run).unwrap();

        let record_a = FileRecord {
            relative_path: "src/a.rs".to_string(),
            language: LanguageId::Rust,
            blob_id: "blob-a".to_string(),
            byte_len: file_a_content.len() as u64,
            content_hash: file_a_hash,
            outcome: PersistedFileOutcome::Quarantined {
                reason: "test".to_string(),
            },
            symbols: Vec::new(),
            run_id: "run-q2".to_string(),
            repo_id: "repo-q2".to_string(),
            committed_at_unix_ms: 1002,
        };
        let record_b = FileRecord {
            relative_path: "src/b.rs".to_string(),
            language: LanguageId::Rust,
            blob_id: "blob-b".to_string(),
            byte_len: 10,
            content_hash: "nonexistent-hash".to_string(),
            outcome: PersistedFileOutcome::Quarantined {
                reason: "test".to_string(),
            },
            symbols: Vec::new(),
            run_id: "run-q2".to_string(),
            repo_id: "repo-q2".to_string(),
            committed_at_unix_ms: 1002,
        };
        manager
            .persistence()
            .save_file_records("run-q2", &[record_a, record_b])
            .unwrap();

        let result = manager
            .repair_repository(
                "repo-q2",
                RepairScope::Repository,
                repo_dir.path().to_path_buf(),
                cas,
            )
            .unwrap();

        assert!(
            matches!(result.outcome, RepairOutcome::CannotRestore { .. }),
            "expected CannotRestore, got {:?}",
            result.outcome
        );
        let repo = manager
            .control_plane
            .get_repository("repo-q2")
            .unwrap()
            .unwrap();
        assert_eq!(repo.status, RepositoryStatus::Degraded);
    }

    #[tokio::test]
    async fn test_repair_invalidated_repository_delegates_to_reindex() {
        let (dir, manager, _cas_dir, cas) = temp_reindex_env();
        seed_repo_with_status(&manager, "repo-inv", RepositoryStatus::Invalidated);

        let result = manager
            .repair_repository(
                "repo-inv",
                RepairScope::Repository,
                dir.path().to_path_buf(),
                cas,
            )
            .unwrap();

        match &result.outcome {
            RepairOutcome::InProgress { run_id } => {
                assert!(!run_id.is_empty());
            }
            RepairOutcome::CannotRestore { .. } => {
                // acceptable if reindex fails to start in test env
            }
            other => panic!("expected InProgress or CannotRestore, got {other:?}"),
        }
    }

    #[test]
    fn test_repair_interrupted_run_delegates_to_resume() {
        let (_dir, manager, _cas_dir, cas) = temp_reindex_env();
        seed_repo_with_status(&manager, "repo-int", RepositoryStatus::Degraded);

        let run = IndexRun {
            run_id: "run-int".to_string(),
            repo_id: "repo-int".to_string(),
            mode: IndexRunMode::Full,
            status: IndexRunStatus::Interrupted,
            requested_at_unix_ms: 1000,
            started_at_unix_ms: Some(1001),
            finished_at_unix_ms: Some(1002),
            idempotency_key: None,
            request_hash: None,
            checkpoint_cursor: None,
            error_summary: None,
            not_yet_supported: None,
            prior_run_id: None,
            description: None,
            recovery_state: None,
        };
        manager.persistence().save_run(&run).unwrap();

        let result = manager
            .repair_repository(
                "repo-int",
                RepairScope::Run {
                    run_id: "run-int".to_string(),
                },
                PathBuf::from("/tmp/repo-int"),
                cas,
            )
            .unwrap();

        assert!(
            matches!(
                result.outcome,
                RepairOutcome::RequiresReindex | RepairOutcome::CannotRestore { .. }
            ),
            "expected RequiresReindex or CannotRestore, got {:?}",
            result.outcome
        );
    }

    #[test]
    fn test_repair_failed_run_returns_requires_reindex() {
        let (_dir, manager, _cas_dir, cas) = temp_reindex_env();
        seed_repo_with_status(&manager, "repo-fr", RepositoryStatus::Degraded);

        let run = IndexRun {
            run_id: "run-fr".to_string(),
            repo_id: "repo-fr".to_string(),
            mode: IndexRunMode::Full,
            status: IndexRunStatus::Failed,
            requested_at_unix_ms: 1000,
            started_at_unix_ms: Some(1001),
            finished_at_unix_ms: Some(1002),
            idempotency_key: None,
            request_hash: None,
            checkpoint_cursor: None,
            error_summary: Some("test failure".to_string()),
            not_yet_supported: None,
            prior_run_id: None,
            description: None,
            recovery_state: None,
        };
        manager.persistence().save_run(&run).unwrap();

        let result = manager
            .repair_repository(
                "repo-fr",
                RepairScope::Run {
                    run_id: "run-fr".to_string(),
                },
                PathBuf::from("/tmp/repo-fr"),
                cas,
            )
            .unwrap();

        assert_eq!(result.outcome, RepairOutcome::RequiresReindex);
    }

    #[test]
    fn test_repair_quarantined_file_unquarantined_on_verify() {
        let (_dir, manager, _cas_dir, cas) = temp_reindex_env();
        seed_repo_with_status(&manager, "repo-qf1", RepositoryStatus::Quarantined);

        let repo_dir = tempfile::tempdir().unwrap();
        let file_content = b"fn verified() {}";
        let file_hash = digest_hex(file_content);
        std::fs::create_dir_all(repo_dir.path().join("src")).unwrap();
        std::fs::write(repo_dir.path().join("src/v.rs"), file_content).unwrap();

        let run = IndexRun {
            run_id: "run-qf1".to_string(),
            repo_id: "repo-qf1".to_string(),
            mode: IndexRunMode::Full,
            status: IndexRunStatus::Succeeded,
            requested_at_unix_ms: 1000,
            started_at_unix_ms: Some(1001),
            finished_at_unix_ms: Some(1002),
            idempotency_key: None,
            request_hash: None,
            checkpoint_cursor: None,
            error_summary: None,
            not_yet_supported: None,
            prior_run_id: None,
            description: None,
            recovery_state: None,
        };
        manager.persistence().save_run(&run).unwrap();

        let record = FileRecord {
            relative_path: "src/v.rs".to_string(),
            language: LanguageId::Rust,
            blob_id: "blob-qf1".to_string(),
            byte_len: file_content.len() as u64,
            content_hash: file_hash,
            outcome: PersistedFileOutcome::Quarantined {
                reason: "test".to_string(),
            },
            symbols: Vec::new(),
            run_id: "run-qf1".to_string(),
            repo_id: "repo-qf1".to_string(),
            committed_at_unix_ms: 1002,
        };
        manager
            .persistence()
            .save_file_records("run-qf1", &[record])
            .unwrap();

        let result = manager
            .repair_repository(
                "repo-qf1",
                RepairScope::File {
                    run_id: "run-qf1".to_string(),
                    relative_path: "src/v.rs".to_string(),
                },
                repo_dir.path().to_path_buf(),
                cas,
            )
            .unwrap();

        assert_eq!(result.outcome, RepairOutcome::Restored);

        // Verify the file record outcome was updated to Committed
        let records = manager
            .persistence()
            .get_file_records("run-qf1")
            .unwrap();
        let updated = records
            .iter()
            .find(|r| r.relative_path == "src/v.rs")
            .unwrap();
        assert_eq!(updated.outcome, PersistedFileOutcome::Committed);
    }

    #[test]
    fn test_repair_quarantined_file_cannot_restore_on_source_drift() {
        let (_dir, manager, _cas_dir, cas) = temp_reindex_env();
        seed_repo_with_status(&manager, "repo-qf2", RepositoryStatus::Quarantined);

        let repo_dir = tempfile::tempdir().unwrap();
        // Write DIFFERENT content than what the hash says
        std::fs::create_dir_all(repo_dir.path().join("src")).unwrap();
        std::fs::write(repo_dir.path().join("src/d.rs"), b"fn drifted() {}").unwrap();

        let run = IndexRun {
            run_id: "run-qf2".to_string(),
            repo_id: "repo-qf2".to_string(),
            mode: IndexRunMode::Full,
            status: IndexRunStatus::Succeeded,
            requested_at_unix_ms: 1000,
            started_at_unix_ms: Some(1001),
            finished_at_unix_ms: Some(1002),
            idempotency_key: None,
            request_hash: None,
            checkpoint_cursor: None,
            error_summary: None,
            not_yet_supported: None,
            prior_run_id: None,
            description: None,
            recovery_state: None,
        };
        manager.persistence().save_run(&run).unwrap();

        let record = FileRecord {
            relative_path: "src/d.rs".to_string(),
            language: LanguageId::Rust,
            blob_id: "blob-qf2".to_string(),
            byte_len: 10,
            content_hash: "aabbccdd".to_string(),
            outcome: PersistedFileOutcome::Quarantined {
                reason: "test".to_string(),
            },
            symbols: Vec::new(),
            run_id: "run-qf2".to_string(),
            repo_id: "repo-qf2".to_string(),
            committed_at_unix_ms: 1002,
        };
        manager
            .persistence()
            .save_file_records("run-qf2", &[record])
            .unwrap();

        let result = manager
            .repair_repository(
                "repo-qf2",
                RepairScope::File {
                    run_id: "run-qf2".to_string(),
                    relative_path: "src/d.rs".to_string(),
                },
                repo_dir.path().to_path_buf(),
                cas,
            )
            .unwrap();

        assert!(
            matches!(result.outcome, RepairOutcome::CannotRestore { .. }),
            "expected CannotRestore, got {:?}",
            result.outcome
        );
    }

    // --- Health inspection unit tests (Story 4.5) ---

    fn seed_repo_with_full_state(
        manager: &RunManager,
        repo_id: &str,
        status: RepositoryStatus,
        invalidation_reason: Option<String>,
        invalidated_at_unix_ms: Option<u64>,
        quarantine_reason: Option<String>,
        quarantined_at_unix_ms: Option<u64>,
    ) {
        let repo = crate::domain::Repository {
            repo_id: repo_id.to_string(),
            kind: crate::domain::RepositoryKind::Git,
            root_uri: format!("/tmp/{repo_id}"),
            project_identity: format!("identity-{repo_id}"),
            project_identity_kind: crate::domain::ProjectIdentityKind::GitCommonDir,
            default_branch: None,
            last_known_revision: None,
            status,
            invalidated_at_unix_ms,
            invalidation_reason,
            quarantined_at_unix_ms,
            quarantine_reason,
        };
        manager.persistence().save_repository(&repo).unwrap();
    }

    fn seed_completed_run(manager: &RunManager, repo_id: &str, run_id: &str) {
        let run = IndexRun {
            run_id: run_id.to_string(),
            repo_id: repo_id.to_string(),
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
            recovery_state: None,
        };
        manager.persistence().save_run(&run).unwrap();
    }

    fn seed_file_records(
        manager: &RunManager,
        run_id: &str,
        repo_id: &str,
        records: Vec<(String, PersistedFileOutcome)>,
    ) {
        let file_records: Vec<FileRecord> = records
            .into_iter()
            .map(|(path, outcome)| FileRecord {
                relative_path: path,
                language: LanguageId::Rust,
                blob_id: format!("blob-{run_id}"),
                byte_len: 100,
                content_hash: "hash".to_string(),
                outcome,
                symbols: Vec::new(),
                run_id: run_id.to_string(),
                repo_id: repo_id.to_string(),
                committed_at_unix_ms: 2000,
            })
            .collect();
        manager
            .persistence()
            .save_file_records(run_id, &file_records)
            .unwrap();
    }

    #[test]
    fn test_inspect_health_ready_reports_healthy() {
        let (_dir, manager) = temp_run_manager();
        seed_repo_with_status(&manager, "repo-h1", RepositoryStatus::Ready);
        seed_completed_run(&manager, "repo-h1", "run-h1");

        let report = manager.inspect_repository_health("repo-h1").unwrap();

        assert_eq!(report.status, RepositoryStatus::Ready);
        assert!(!report.action_required);
        assert!(report.next_action.is_none());
        assert!(report.status_detail.starts_with("Repository is healthy"));
    }

    #[test]
    fn test_inspect_health_pending_no_runs_reports_never_indexed() {
        let (_dir, manager) = temp_run_manager();
        seed_repo_with_status(&manager, "repo-h2", RepositoryStatus::Pending);

        let report = manager.inspect_repository_health("repo-h2").unwrap();

        assert_eq!(report.status, RepositoryStatus::Pending);
        assert!(report.action_required);
        assert_eq!(report.next_action, Some(NextAction::Reindex));
        assert!(report.status_detail.contains("never been indexed"));
    }

    #[test]
    fn test_inspect_health_pending_active_run_reports_processing() {
        // Test the Pending + active_run classification branch directly,
        // since injecting a live active run requires full pipeline setup.
        let classification = classify_repository_action(
            &RepositoryStatus::Pending,
            false, // no completed run
            true,  // has active run
            &None,
            &None,
        );

        assert!(!classification.action_required);
        assert_eq!(classification.next_action, Some(NextAction::Wait));
        assert!(classification.detail.contains("in progress"));
    }

    #[test]
    fn test_inspect_health_degraded_reports_repair_needed() {
        let (_dir, manager) = temp_run_manager();
        seed_repo_with_status(&manager, "repo-h4", RepositoryStatus::Degraded);

        let report = manager.inspect_repository_health("repo-h4").unwrap();

        assert_eq!(report.status, RepositoryStatus::Degraded);
        assert!(report.action_required);
        assert_eq!(report.next_action, Some(NextAction::Repair));
        assert!(report.status_detail.contains("degraded"));
    }

    #[test]
    fn test_inspect_health_failed_reports_repair_needed() {
        let (_dir, manager) = temp_run_manager();
        seed_repo_with_status(&manager, "repo-h5", RepositoryStatus::Failed);

        let report = manager.inspect_repository_health("repo-h5").unwrap();

        assert_eq!(report.status, RepositoryStatus::Failed);
        assert!(report.action_required);
        assert_eq!(report.next_action, Some(NextAction::Repair));
        assert!(report.status_detail.contains("failed"));
    }

    #[test]
    fn test_inspect_health_invalidated_reports_reindex_needed() {
        let (_dir, manager) = temp_run_manager();
        seed_repo_with_full_state(
            &manager,
            "repo-h6",
            RepositoryStatus::Invalidated,
            Some("stale data after branch switch".to_string()),
            Some(5000),
            None,
            None,
        );

        let report = manager.inspect_repository_health("repo-h6").unwrap();

        assert_eq!(report.status, RepositoryStatus::Invalidated);
        assert!(report.action_required);
        assert_eq!(report.next_action, Some(NextAction::Reindex));
        assert!(report.status_detail.contains("invalidated"));
        assert!(report.status_detail.contains("stale data after branch switch"));
        let ctx = report.invalidation_context.unwrap();
        assert_eq!(ctx.reason, "stale data after branch switch");
        assert_eq!(ctx.occurred_at_unix_ms, 5000);
    }

    #[test]
    fn test_inspect_health_quarantined_reports_repair_needed() {
        let (_dir, manager) = temp_run_manager();
        seed_repo_with_full_state(
            &manager,
            "repo-h7",
            RepositoryStatus::Quarantined,
            None,
            None,
            Some("quarantine policy triggered".to_string()),
            Some(6000),
        );

        let report = manager.inspect_repository_health("repo-h7").unwrap();

        assert_eq!(report.status, RepositoryStatus::Quarantined);
        assert!(report.action_required);
        assert_eq!(report.next_action, Some(NextAction::Repair));
        assert!(report.status_detail.contains("quarantined"));
        assert!(report
            .status_detail
            .contains("quarantine policy triggered"));
        let ctx = report.quarantine_context.unwrap();
        assert_eq!(ctx.reason, "quarantine policy triggered");
        assert_eq!(ctx.occurred_at_unix_ms, 6000);
    }

    #[test]
    fn test_inspect_health_includes_file_health_summary() {
        let (_dir, manager) = temp_run_manager();
        seed_repo_with_status(&manager, "repo-h8", RepositoryStatus::Ready);
        seed_completed_run(&manager, "repo-h8", "run-h8");
        seed_file_records(
            &manager,
            "run-h8",
            "repo-h8",
            vec![
                ("src/a.rs".to_string(), PersistedFileOutcome::Committed),
                ("src/b.rs".to_string(), PersistedFileOutcome::Committed),
                ("src/c.rs".to_string(), PersistedFileOutcome::EmptySymbols),
                (
                    "src/d.rs".to_string(),
                    PersistedFileOutcome::Failed {
                        error: "parse error".to_string(),
                    },
                ),
                (
                    "src/e.rs".to_string(),
                    PersistedFileOutcome::Quarantined {
                        reason: "suspect".to_string(),
                    },
                ),
            ],
        );

        let report = manager.inspect_repository_health("repo-h8").unwrap();

        let file_health = report.file_health.unwrap();
        assert_eq!(file_health.total_files, 5);
        assert_eq!(file_health.committed, 2);
        assert_eq!(file_health.empty_symbols, 1);
        assert_eq!(file_health.failed, 1);
        assert_eq!(file_health.quarantined, 1);
    }

    #[test]
    fn test_inspect_health_includes_latest_run_summary() {
        let (_dir, manager) = temp_run_manager();
        seed_repo_with_status(&manager, "repo-h9", RepositoryStatus::Ready);
        seed_completed_run(&manager, "repo-h9", "run-h9");

        let report = manager.inspect_repository_health("repo-h9").unwrap();

        let run_summary = report.latest_run.unwrap();
        assert_eq!(run_summary.run_id, "run-h9");
        assert_eq!(run_summary.status, IndexRunStatus::Succeeded);
        assert_eq!(run_summary.mode, IndexRunMode::Full);
        assert_eq!(run_summary.started_at_unix_ms, 1001);
        assert_eq!(run_summary.completed_at_unix_ms, Some(2000));
    }

    #[test]
    fn test_inspect_health_includes_recent_repairs() {
        let (_dir, manager) = temp_run_manager();
        seed_repo_with_status(&manager, "repo-h10", RepositoryStatus::Ready);

        let event = RepairEvent {
            repo_id: "repo-h10".to_string(),
            scope: RepairScope::Repository,
            previous_status: RepositoryStatus::Degraded,
            outcome: RepairOutcome::Restored,
            detail: "repaired degraded state".to_string(),
            timestamp_unix_ms: 3000,
        };
        manager.persistence().save_repair_event(&event).unwrap();

        let report = manager.inspect_repository_health("repo-h10").unwrap();

        assert_eq!(report.recent_repairs.len(), 1);
        assert_eq!(report.recent_repairs[0].detail, "repaired degraded state");
    }

    #[test]
    fn test_inspect_health_not_found_returns_error() {
        let (_dir, manager) = temp_run_manager();

        let result = manager.inspect_repository_health("nonexistent-repo");

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("not found"));
    }

    #[test]
    fn test_inspect_health_explicit_healthy_never_silent() {
        let (_dir, manager) = temp_run_manager();
        seed_repo_with_status(&manager, "repo-h12", RepositoryStatus::Ready);

        let report = manager.inspect_repository_health("repo-h12").unwrap();

        assert!(!report.status_detail.is_empty());
        assert!(report.status_detail.starts_with("Repository is healthy"));
        assert!(!report.action_required);
    }
}
