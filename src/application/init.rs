use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::config::ServerConfig;
use crate::domain::{
    ActiveWorkspaceContext, AuthorityMode, ContextResolutionMode, InitializationReport,
    MigrationEntityKind, MigrationIssue, MigrationMode, MigrationRecord, MigrationReport,
    MigrationRequest, ProjectIdentityKind, RegisteredProject, RegistrationAction,
    RegistrationResult, RegistryKind, RegistryView, Repository, RepositoryKind, RepositoryStatus,
    Workspace, WorkspaceStatus,
};
use crate::error::{Result, TokenizorError};
use crate::storage::{BlobStore, ControlPlane, digest_hex};

use super::deployment::DeploymentService;

/// Schema v1: original bootstrap registry with repositories and workspaces only.
/// Missing `project_identity`, `project_identity_kind`, `registry_kind`,
/// `authority_mode`, and `control_plane_backend` fields.
const LEGACY_REGISTRY_SCHEMA_VERSION: u32 = 1;
/// Schema v2: adds canonical project identity fields (`project_identity`,
/// `project_identity_kind`) to repository records and provenance metadata
/// (`registry_kind`, `authority_mode`, `control_plane_backend`) to the snapshot.
const CURRENT_REGISTRY_SCHEMA_VERSION: u32 = 2;
const REGISTRY_LOCK_RETRY_DELAY_MS: u64 = 25;
const REGISTRY_LOCK_TIMEOUT_MS: u64 = 5_000;
const REGISTRY_LOCK_STALE_AFTER_MS: u64 = 30_000;

pub struct InitializationService<'a> {
    config: &'a ServerConfig,
    blob_store: &'a dyn BlobStore,
    control_plane: &'a dyn ControlPlane,
}

impl<'a> InitializationService<'a> {
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

    pub fn initialize_repository(
        &self,
        target_path: Option<PathBuf>,
    ) -> Result<InitializationReport> {
        self.register_workspace(target_path, RegistrationMode::InitializeOrAttach)
    }

    pub fn attach_workspace(&self, target_path: Option<PathBuf>) -> Result<InitializationReport> {
        self.register_workspace(target_path, RegistrationMode::AttachOnly)
    }

    fn register_workspace(
        &self,
        target_path: Option<PathBuf>,
        mode: RegistrationMode,
    ) -> Result<InitializationReport> {
        let deployment =
            DeploymentService::new(self.config, self.blob_store, self.control_plane).bootstrap()?;
        let current_dir = std::env::current_dir()
            .map_err(|error| TokenizorError::io(PathBuf::from("."), error))?;
        let resolved = resolve_repository_target(target_path.as_deref(), &current_dir)?;
        let registry_path = registry_path(self.blob_store.root_dir());
        let _lock = acquire_registry_lock(&registry_path)?;
        let mut snapshot = load_snapshot(&registry_path)?;
        snapshot = snapshot.with_runtime_provenance(self.config.control_plane.backend.as_str());
        let (repository, repository_action) =
            resolve_repository_registration(&mut snapshot, &resolved, mode)?;
        let workspace = build_workspace(&resolved, &repository);
        let workspace_action = upsert_workspace(&mut snapshot, workspace.clone())?;

        save_snapshot(&registry_path, &snapshot)?;

        Ok(InitializationReport::new(
            resolved.input_path.display().to_string(),
            registry_path.display().to_string(),
            RegistrationResult {
                action: repository_action,
                record: repository,
            },
            RegistrationResult {
                action: workspace_action,
                record: workspace,
            },
            deployment,
        ))
    }

    pub fn inspect_registry(&self) -> Result<RegistryView> {
        let registry_path = registry_path(self.blob_store.root_dir());
        let snapshot = load_snapshot(&registry_path)?;
        Ok(build_registry_view(&registry_path, snapshot))
    }

    pub fn migrate_registry(
        &self,
        source_path: Option<PathBuf>,
        target_path: Option<PathBuf>,
    ) -> Result<MigrationReport> {
        DeploymentService::new(self.config, self.blob_store, self.control_plane).bootstrap()?;

        let current_dir = std::env::current_dir()
            .map_err(|error| TokenizorError::io(PathBuf::from("."), error))?;
        let request = resolve_migration_request(
            source_path.as_deref(),
            target_path.as_deref(),
            &current_dir,
        )?;
        let registry_path = registry_path(self.blob_store.root_dir());
        let _lock = acquire_registry_lock(&registry_path)?;
        let mut snapshot = load_snapshot(&registry_path)?;
        let mut tracker = MigrationAccumulator::new(
            registry_path.display().to_string(),
            request.report_request(),
        );

        if let Some(update) = request.update.as_ref() {
            apply_explicit_path_update(&mut snapshot, update, &mut tracker)?;
        }

        apply_repository_migrations(&mut snapshot, &mut tracker);
        scan_workspace_state(&snapshot, &mut tracker);

        if tracker.changed() {
            snapshot = snapshot.with_runtime_provenance(self.config.control_plane.backend.as_str());
            save_snapshot(&registry_path, &snapshot)?;
        }

        Ok(tracker.finish())
    }

    pub fn resolve_active_context(
        &self,
        target_path: Option<PathBuf>,
    ) -> Result<ActiveWorkspaceContext> {
        let current_dir = std::env::current_dir()
            .map_err(|error| TokenizorError::io(PathBuf::from("."), error))?;
        let request = resolve_context_request(target_path.as_deref(), &current_dir)?;
        let registry_path = registry_path(self.blob_store.root_dir());
        let snapshot = load_snapshot(&registry_path)?;
        resolve_active_context_from_snapshot(&registry_path, snapshot, request)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ResolvedRepositoryTarget {
    input_path: PathBuf,
    workspace_root: PathBuf,
    repository_root: PathBuf,
    repository_kind: RepositoryKind,
    project_identity: String,
    project_identity_kind: ProjectIdentityKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RegistrationMode {
    InitializeOrAttach,
    AttachOnly,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ResolvedGitProjectIdentity {
    repository_root: PathBuf,
    project_identity: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ContextResolutionRequest {
    requested_path: PathBuf,
    resolution_mode: ContextResolutionMode,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
struct RegistrySnapshot {
    schema_version: u32,
    #[serde(default)]
    registry_kind: RegistryKind,
    #[serde(default)]
    authority_mode: AuthorityMode,
    #[serde(default = "default_control_plane_backend")]
    control_plane_backend: String,
    repositories: BTreeMap<String, Repository>,
    workspaces: BTreeMap<String, Workspace>,
}

impl RegistrySnapshot {
    fn with_runtime_provenance(mut self, backend: &str) -> Self {
        self.schema_version = CURRENT_REGISTRY_SCHEMA_VERSION;
        self.registry_kind = RegistryKind::LocalBootstrapProjectWorkspace;
        self.authority_mode = AuthorityMode::LocalBootstrapOnly;
        self.control_plane_backend = backend.to_string();
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProjectIdentityEvidence {
    path: PathBuf,
    project_identity: String,
    repository_root: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MigrationPathUpdate {
    source_path: PathBuf,
    target: ResolvedRepositoryTarget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MigrationRequestState {
    mode: MigrationMode,
    update: Option<MigrationPathUpdate>,
}

impl MigrationRequestState {
    fn report_request(&self) -> MigrationRequest {
        MigrationRequest {
            mode: self.mode.clone(),
            source_path: self
                .update
                .as_ref()
                .map(|update| update.source_path.display().to_string()),
            target_path: self
                .update
                .as_ref()
                .map(|update| update.target.workspace_root.display().to_string()),
        }
    }
}

#[derive(Clone, Copy)]
enum MigrationBucket {
    Migrated,
    Updated,
    Unchanged,
}

struct MigrationAccumulator {
    registry_path: String,
    request: MigrationRequest,
    changed: bool,
    migrated: Vec<MigrationRecord>,
    updated: Vec<MigrationRecord>,
    unchanged: Vec<MigrationRecord>,
    unresolved: Vec<MigrationIssue>,
    touched_repositories: BTreeSet<String>,
    touched_workspaces: BTreeSet<String>,
}

impl MigrationAccumulator {
    fn new(registry_path: String, request: MigrationRequest) -> Self {
        Self {
            registry_path,
            request,
            changed: false,
            migrated: Vec::new(),
            updated: Vec::new(),
            unchanged: Vec::new(),
            unresolved: Vec::new(),
            touched_repositories: BTreeSet::new(),
            touched_workspaces: BTreeSet::new(),
        }
    }

    fn changed(&self) -> bool {
        self.changed
    }

    fn finish(self) -> MigrationReport {
        MigrationReport::new(
            self.registry_path,
            self.request,
            self.changed,
            self.migrated,
            self.updated,
            self.unchanged,
            self.unresolved,
        )
    }

    fn repository_touched(&self, repo_id: &str) -> bool {
        self.touched_repositories.contains(repo_id)
    }

    fn workspace_touched(&self, workspace_id: &str) -> bool {
        self.touched_workspaces.contains(workspace_id)
    }

    fn add_repository_record(
        &mut self,
        bucket: MigrationBucket,
        repository: &Repository,
        previous_path: Option<String>,
        current_path: Option<String>,
        detail: impl Into<String>,
    ) {
        let record = MigrationRecord {
            entity_kind: MigrationEntityKind::Repository,
            entity_id: repository.repo_id.clone(),
            previous_path,
            current_path,
            detail: detail.into(),
        };

        self.touched_repositories.insert(repository.repo_id.clone());

        match bucket {
            MigrationBucket::Migrated => {
                self.changed = true;
                self.migrated.push(record);
            }
            MigrationBucket::Updated => {
                self.changed = true;
                self.updated.push(record);
            }
            MigrationBucket::Unchanged => self.unchanged.push(record),
        }
    }

    fn add_workspace_record(
        &mut self,
        bucket: MigrationBucket,
        workspace_id: &str,
        previous_path: Option<String>,
        current_path: Option<String>,
        detail: impl Into<String>,
    ) {
        let record = MigrationRecord {
            entity_kind: MigrationEntityKind::Workspace,
            entity_id: workspace_id.to_string(),
            previous_path,
            current_path,
            detail: detail.into(),
        };

        self.touched_workspaces.insert(workspace_id.to_string());

        match bucket {
            MigrationBucket::Migrated => {
                self.changed = true;
                self.migrated.push(record);
            }
            MigrationBucket::Updated => {
                self.changed = true;
                self.updated.push(record);
            }
            MigrationBucket::Unchanged => self.unchanged.push(record),
        }
    }

    fn add_repository_issue(
        &mut self,
        repository: &Repository,
        detail: impl Into<String>,
        guidance: impl Into<String>,
        candidate_paths: Vec<String>,
    ) {
        self.touched_repositories.insert(repository.repo_id.clone());
        self.unresolved.push(MigrationIssue {
            entity_kind: MigrationEntityKind::Repository,
            entity_id: Some(repository.repo_id.clone()),
            path: Some(repository.root_uri.clone()),
            detail: detail.into(),
            guidance: guidance.into(),
            candidate_paths: normalize_candidate_paths(candidate_paths),
        });
    }

    fn add_workspace_issue(
        &mut self,
        workspace: &Workspace,
        detail: impl Into<String>,
        guidance: impl Into<String>,
        candidate_paths: Vec<String>,
    ) {
        self.touched_workspaces
            .insert(workspace.workspace_id.clone());
        self.unresolved.push(MigrationIssue {
            entity_kind: MigrationEntityKind::Workspace,
            entity_id: Some(workspace.workspace_id.clone()),
            path: Some(workspace.root_uri.clone()),
            detail: detail.into(),
            guidance: guidance.into(),
            candidate_paths: normalize_candidate_paths(candidate_paths),
        });
    }

    fn add_general_issue(
        &mut self,
        entity_kind: MigrationEntityKind,
        path: Option<String>,
        detail: impl Into<String>,
        guidance: impl Into<String>,
        candidate_paths: Vec<String>,
    ) {
        self.unresolved.push(MigrationIssue {
            entity_kind,
            entity_id: None,
            path,
            detail: detail.into(),
            guidance: guidance.into(),
            candidate_paths: normalize_candidate_paths(candidate_paths),
        });
    }
}

fn default_control_plane_backend() -> String {
    "unknown".to_string()
}

fn normalize_candidate_paths(paths: Vec<String>) -> Vec<String> {
    paths
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn resolve_migration_request(
    source_path: Option<&Path>,
    target_path: Option<&Path>,
    current_dir: &Path,
) -> Result<MigrationRequestState> {
    match (source_path, target_path) {
        (None, None) => Ok(MigrationRequestState {
            mode: MigrationMode::Scan,
            update: None,
        }),
        (Some(source_path), Some(target_path)) => Ok(MigrationRequestState {
            mode: MigrationMode::Update,
            update: Some(MigrationPathUpdate {
                source_path: resolve_update_source_path(source_path, current_dir)?,
                target: resolve_repository_target(Some(target_path), current_dir)?,
            }),
        }),
        _ => Err(TokenizorError::InvalidArgument(
            "migrate expects either no path arguments or an explicit `<from-path> <to-path>` pair"
                .to_string(),
        )),
    }
}

fn resolve_update_source_path(path: &Path, current_dir: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        current_dir.join(path)
    };

    if absolute.exists() {
        Ok(normalize_path(
            fs::canonicalize(&absolute).map_err(|error| TokenizorError::io(&absolute, error))?,
        ))
    } else {
        Ok(normalize_path(absolute))
    }
}

fn build_registry_view(path: &Path, snapshot: RegistrySnapshot) -> RegistryView {
    let mut workspaces_by_repo: BTreeMap<String, Vec<Workspace>> = BTreeMap::new();
    let mut orphan_workspaces = Vec::new();

    let mut sorted_workspaces = snapshot.workspaces.into_values().collect::<Vec<_>>();
    sorted_workspaces.sort_by(|left, right| {
        left.root_uri
            .cmp(&right.root_uri)
            .then(left.workspace_id.cmp(&right.workspace_id))
    });

    let mut sorted_repositories = snapshot.repositories.into_values().collect::<Vec<_>>();
    sorted_repositories.sort_by(|left, right| {
        left.root_uri
            .cmp(&right.root_uri)
            .then(left.repo_id.cmp(&right.repo_id))
    });

    for workspace in sorted_workspaces {
        if sorted_repositories
            .iter()
            .any(|repository| repository.repo_id == workspace.repo_id)
        {
            workspaces_by_repo
                .entry(workspace.repo_id.clone())
                .or_default()
                .push(workspace);
        } else {
            orphan_workspaces.push(workspace);
        }
    }

    let projects = sorted_repositories
        .into_iter()
        .map(|repository| RegisteredProject {
            workspaces: workspaces_by_repo
                .remove(&repository.repo_id)
                .unwrap_or_default(),
            repository,
        })
        .collect::<Vec<_>>();

    RegistryView::new(
        path.display().to_string(),
        snapshot.registry_kind,
        snapshot.authority_mode,
        snapshot.control_plane_backend,
        projects,
        orphan_workspaces,
    )
}

fn registry_path(blob_root: &Path) -> PathBuf {
    blob_root
        .join("control-plane")
        .join("project-workspace-registry.json")
}

fn resolve_context_request(
    target_path: Option<&Path>,
    current_dir: &Path,
) -> Result<ContextResolutionRequest> {
    let input_path = target_path
        .map(PathBuf::from)
        .unwrap_or_else(|| current_dir.to_path_buf());
    let canonical_input = match fs::canonicalize(&input_path) {
        Ok(path) => normalize_path(path),
        Err(error) if target_path.is_some() && error.kind() == std::io::ErrorKind::NotFound => {
            return Err(TokenizorError::InvalidArgument(format!(
                "explicit override `{}` is not within a registered workspace",
                input_path.display()
            )));
        }
        Err(error) => return Err(TokenizorError::io(input_path.clone(), error)),
    };

    if !canonical_input.is_dir() {
        return Err(TokenizorError::InvalidArgument(format!(
            "context resolution target `{}` must be a directory",
            canonical_input.display()
        )));
    }

    Ok(ContextResolutionRequest {
        requested_path: canonical_input,
        resolution_mode: if target_path.is_some() {
            ContextResolutionMode::ExplicitOverride
        } else {
            ContextResolutionMode::CurrentDirectory
        },
    })
}

fn resolve_repository_target(
    target_path: Option<&Path>,
    current_dir: &Path,
) -> Result<ResolvedRepositoryTarget> {
    let input_path = target_path
        .map(PathBuf::from)
        .unwrap_or_else(|| current_dir.to_path_buf());
    let canonical_input = normalize_path(
        fs::canonicalize(&input_path)
            .map_err(|error| TokenizorError::io(input_path.clone(), error))?,
    );

    if !canonical_input.is_dir() {
        return Err(TokenizorError::InvalidArgument(format!(
            "initialization target `{}` must be a directory",
            canonical_input.display()
        )));
    }

    let workspace_root = find_git_root(&canonical_input).unwrap_or_else(|| canonical_input.clone());

    let (repository_root, repository_kind, project_identity, project_identity_kind) =
        if is_git_root_marker(&workspace_root) {
            let git_identity = resolve_git_project_identity(&workspace_root)?;
            (
                git_identity.repository_root,
                RepositoryKind::Git,
                git_identity.project_identity,
                ProjectIdentityKind::GitCommonDir,
            )
        } else {
            (
                workspace_root.clone(),
                RepositoryKind::Local,
                workspace_root.display().to_string(),
                ProjectIdentityKind::LocalRootPath,
            )
        };

    Ok(ResolvedRepositoryTarget {
        input_path: canonical_input.clone(),
        workspace_root,
        repository_root,
        repository_kind,
        project_identity,
        project_identity_kind,
    })
}

// Bootstrap-era Git project identity rule:
// use the normalized path to the repository's shared Git common directory.
// Main checkouts use `<repo>/.git`; linked worktrees resolve through their
// `.git` file/admin directory back to that same common directory.
fn resolve_git_project_identity(workspace_root: &Path) -> Result<ResolvedGitProjectIdentity> {
    let git_admin_dir = resolve_git_admin_dir(workspace_root)?;
    let common_git_dir = resolve_git_common_dir(&git_admin_dir)?;
    let repository_root = project_root_from_git_common_dir(&common_git_dir);

    Ok(ResolvedGitProjectIdentity {
        repository_root,
        project_identity: common_git_dir.display().to_string(),
    })
}

fn resolve_git_admin_dir(workspace_root: &Path) -> Result<PathBuf> {
    let git_marker = workspace_root.join(".git");

    if git_marker.is_dir() {
        return Ok(normalize_path(
            fs::canonicalize(&git_marker)
                .map_err(|error| TokenizorError::io(&git_marker, error))?,
        ));
    }

    if git_marker.is_file() {
        let contents = fs::read_to_string(&git_marker)
            .map_err(|error| TokenizorError::io(&git_marker, error))?;
        let gitdir = contents
            .lines()
            .next()
            .and_then(|line| line.trim().strip_prefix("gitdir:"))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                TokenizorError::InvalidArgument(format!(
                    "workspace `{}` has an invalid .git file; expected `gitdir:` metadata",
                    workspace_root.display()
                ))
            })?;
        let gitdir_path = PathBuf::from(gitdir);
        let resolved = if gitdir_path.is_absolute() {
            gitdir_path
        } else {
            workspace_root.join(gitdir_path)
        };

        return Ok(normalize_path(
            fs::canonicalize(&resolved).map_err(|error| TokenizorError::io(&resolved, error))?,
        ));
    }

    Err(TokenizorError::InvalidArgument(format!(
        "workspace `{}` is missing a valid Git marker",
        workspace_root.display()
    )))
}

fn resolve_git_common_dir(git_admin_dir: &Path) -> Result<PathBuf> {
    let commondir_path = git_admin_dir.join("commondir");
    if commondir_path.is_file() {
        let contents = fs::read_to_string(&commondir_path)
            .map_err(|error| TokenizorError::io(&commondir_path, error))?;
        let common_dir = contents
            .lines()
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                TokenizorError::InvalidArgument(format!(
                    "Git admin directory `{}` has an empty `commondir` file",
                    git_admin_dir.display()
                ))
            })?;
        let common_dir_path = PathBuf::from(common_dir);
        let resolved = if common_dir_path.is_absolute() {
            common_dir_path
        } else {
            git_admin_dir.join(common_dir_path)
        };

        return Ok(normalize_path(
            fs::canonicalize(&resolved).map_err(|error| TokenizorError::io(&resolved, error))?,
        ));
    }

    if git_admin_dir
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        == Some("worktrees")
    {
        if let Some(common_dir) = git_admin_dir.parent().and_then(|parent| parent.parent()) {
            return Ok(normalize_path(common_dir.to_path_buf()));
        }
    }

    Ok(normalize_path(git_admin_dir.to_path_buf()))
}

fn project_root_from_git_common_dir(common_git_dir: &Path) -> PathBuf {
    if common_git_dir.file_name().and_then(|name| name.to_str()) == Some(".git") {
        return common_git_dir
            .parent()
            .map(Path::to_path_buf)
            .map(normalize_path)
            .unwrap_or_else(|| normalize_path(common_git_dir.to_path_buf()));
    }

    normalize_path(common_git_dir.to_path_buf())
}

fn resolve_active_context_from_snapshot(
    registry_path: &Path,
    snapshot: RegistrySnapshot,
    request: ContextResolutionRequest,
) -> Result<ActiveWorkspaceContext> {
    let matched_workspaces = snapshot
        .workspaces
        .values()
        .filter(|workspace| requested_path_matches_workspace(&request.requested_path, workspace))
        .cloned()
        .collect::<Vec<_>>();

    if matched_workspaces.is_empty() {
        let source = match request.resolution_mode {
            ContextResolutionMode::CurrentDirectory => "current directory",
            ContextResolutionMode::ExplicitOverride => "explicit override",
        };
        return Err(TokenizorError::InvalidArgument(format!(
            "{source} `{}` is not within a registered workspace",
            request.requested_path.display()
        )));
    }

    if matched_workspaces.len() > 1 {
        let workspace_descriptors = matched_workspaces
            .iter()
            .map(|workspace| format!("{} ({})", workspace.workspace_id, workspace.root_uri))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(TokenizorError::InvalidArgument(format!(
            "requested context `{}` conflicts with multiple registered workspaces: {workspace_descriptors}",
            request.requested_path.display()
        )));
    }

    let workspace = matched_workspaces.into_iter().next().ok_or_else(|| {
        TokenizorError::Storage(
            "matched_workspaces was unexpectedly empty after non-empty guard".into(),
        )
    })?;
    let repository = snapshot
        .repositories
        .get(&workspace.repo_id)
        .cloned()
        .ok_or_else(|| {
            TokenizorError::Storage(format!(
                "workspace `{}` references missing repository `{}` in local bootstrap registry",
                workspace.workspace_id, workspace.repo_id
            ))
        })?;

    Ok(ActiveWorkspaceContext::new(
        request.requested_path.display().to_string(),
        request.resolution_mode,
        registry_path.display().to_string(),
        snapshot.registry_kind,
        snapshot.authority_mode,
        snapshot.control_plane_backend,
        repository,
        workspace,
    ))
}

fn requested_path_matches_workspace(requested_path: &Path, workspace: &Workspace) -> bool {
    let workspace_root = PathBuf::from(&workspace.root_uri);
    requested_path == workspace_root || requested_path.starts_with(&workspace_root)
}

fn find_git_root(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|candidate| is_git_root_marker(candidate))
        .map(PathBuf::from)
        .map(normalize_path)
}

fn is_git_root_marker(path: &Path) -> bool {
    let git_marker = path.join(".git");

    if git_marker.is_dir() {
        return git_marker.join("HEAD").is_file();
    }

    if git_marker.is_file() {
        return fs::read_to_string(&git_marker)
            .map(|contents| {
                contents
                    .lines()
                    .next()
                    .map(|line| line.trim_start().starts_with("gitdir:"))
                    .unwrap_or(false)
            })
            .unwrap_or(false);
    }

    false
}

fn normalize_path(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let raw = path.display().to_string();
        if let Some(stripped) = raw.strip_prefix(r"\\?\") {
            PathBuf::from(stripped)
        } else {
            path
        }
    }

    #[cfg(not(windows))]
    {
        path
    }
}

fn is_directory_path(path: &str) -> bool {
    Path::new(path).is_dir()
}

fn workspace_id_for_root_uri(root_uri: &str) -> String {
    format!("workspace_{}", digest_hex(root_uri.as_bytes()))
}

fn workspace_paths_for_repo(snapshot: &RegistrySnapshot, repo_id: &str) -> Vec<String> {
    snapshot
        .workspaces
        .values()
        .filter(|workspace| workspace.repo_id == repo_id)
        .map(|workspace| workspace.root_uri.clone())
        .collect()
}

fn collect_git_identity_evidence(
    snapshot: &RegistrySnapshot,
    repository: &Repository,
) -> Vec<ProjectIdentityEvidence> {
    let mut evidence = BTreeMap::new();

    let mut candidate_paths = vec![repository.root_uri.clone()];
    candidate_paths.extend(workspace_paths_for_repo(snapshot, &repository.repo_id));

    for candidate_path in candidate_paths {
        let path = PathBuf::from(&candidate_path);
        if !path.is_dir() {
            continue;
        }

        if let Ok(git_identity) = resolve_git_project_identity(&path) {
            let normalized = normalize_path(path);
            evidence.insert(
                normalized.display().to_string(),
                ProjectIdentityEvidence {
                    path: normalized,
                    project_identity: git_identity.project_identity,
                    repository_root: git_identity.repository_root,
                },
            );
        }
    }

    evidence.into_values().collect()
}

fn unique_git_project_identity(evidence: &[ProjectIdentityEvidence]) -> Option<String> {
    let identities = evidence
        .iter()
        .map(|candidate| candidate.project_identity.clone())
        .collect::<BTreeSet<_>>();

    if identities.len() == 1 {
        identities.into_iter().next()
    } else {
        None
    }
}

fn preferred_repository_root_from_evidence(evidence: &[ProjectIdentityEvidence]) -> Option<String> {
    let candidates = evidence
        .iter()
        .map(|candidate| {
            if candidate.repository_root.is_dir() {
                normalize_path(candidate.repository_root.clone())
            } else {
                normalize_path(candidate.path.clone())
            }
        })
        .collect::<BTreeSet<_>>();

    if candidates.len() == 1 {
        candidates
            .into_iter()
            .next()
            .map(|path| path.display().to_string())
    } else {
        None
    }
}

fn preferred_repository_root_from_target(target: &ResolvedRepositoryTarget) -> String {
    if target.repository_root.is_dir() {
        target.repository_root.display().to_string()
    } else {
        target.workspace_root.display().to_string()
    }
}

fn build_repository(target: &ResolvedRepositoryTarget) -> Repository {
    let root_uri = target.repository_root.display().to_string();

    Repository {
        repo_id: format!("repo_{}", digest_hex(target.project_identity.as_bytes())),
        kind: target.repository_kind.clone(),
        root_uri,
        project_identity: target.project_identity.clone(),
        project_identity_kind: target.project_identity_kind.clone(),
        default_branch: None,
        last_known_revision: None,
        status: RepositoryStatus::Ready,
    }
}

fn build_workspace(target: &ResolvedRepositoryTarget, repository: &Repository) -> Workspace {
    let root_uri = target.workspace_root.display().to_string();

    Workspace {
        workspace_id: workspace_id_for_root_uri(&root_uri),
        repo_id: repository.repo_id.clone(),
        root_uri,
        status: WorkspaceStatus::Active,
    }
}

fn apply_explicit_path_update(
    snapshot: &mut RegistrySnapshot,
    update: &MigrationPathUpdate,
    tracker: &mut MigrationAccumulator,
) -> Result<()> {
    let source_path = update.source_path.display().to_string();
    let target_workspace_root = update.target.workspace_root.display().to_string();
    let target_repository_root = preferred_repository_root_from_target(&update.target);

    let matching_repo_ids = snapshot
        .repositories
        .values()
        .filter(|repository| repository.root_uri == source_path)
        .map(|repository| repository.repo_id.clone())
        .collect::<Vec<_>>();
    let matching_workspace_ids = snapshot
        .workspaces
        .values()
        .filter(|workspace| workspace.root_uri == source_path)
        .map(|workspace| workspace.workspace_id.clone())
        .collect::<Vec<_>>();

    if matching_repo_ids.is_empty() && matching_workspace_ids.is_empty() {
        return report_idempotent_or_unmatched_update(
            snapshot,
            update,
            &source_path,
            &target_repository_root,
            &target_workspace_root,
            tracker,
        );
    }

    let repo_ids = matching_repo_ids
        .iter()
        .cloned()
        .chain(matching_workspace_ids.iter().filter_map(|workspace_id| {
            snapshot
                .workspaces
                .get(workspace_id)
                .map(|workspace| workspace.repo_id.clone())
        }))
        .collect::<BTreeSet<_>>();

    if repo_ids.len() != 1 || matching_repo_ids.len() > 1 || matching_workspace_ids.len() > 1 {
        let candidate_paths = matching_repo_ids
            .iter()
            .filter_map(|repo_id| snapshot.repositories.get(repo_id))
            .map(|repository| repository.root_uri.clone())
            .chain(matching_workspace_ids.iter().filter_map(|workspace_id| {
                snapshot
                    .workspaces
                    .get(workspace_id)
                    .map(|workspace| workspace.root_uri.clone())
            }))
            .collect::<Vec<_>>();
        tracker.add_general_issue(
            MigrationEntityKind::Workspace,
            Some(source_path.clone()),
            format!(
                "registered state for `{source_path}` is ambiguous and cannot be updated safely"
            ),
            format!(
                "Run `cargo run -- inspect` to review the conflicting records before retrying `cargo run -- migrate {source_path} <current-path>`."
            ),
            candidate_paths,
        );
        return Ok(());
    }

    let repo_id = repo_ids.into_iter().next().ok_or_else(|| {
        TokenizorError::Storage(
            "repo_ids was unexpectedly empty after single-element guard".into(),
        )
    })?;
    let repository = snapshot
        .repositories
        .get(&repo_id)
        .cloned()
        .ok_or_else(|| {
            TokenizorError::Storage(format!(
                "workspace update references missing repository `{repo_id}`"
            ))
        })?;

    if let Some(()) = validate_path_update_target(
        snapshot,
        &repository,
        update,
        &source_path,
        &target_repository_root,
        &target_workspace_root,
        &matching_workspace_ids,
        tracker,
    )? {
        return Ok(());
    }

    commit_path_update(
        snapshot,
        repository,
        update,
        &source_path,
        &target_repository_root,
        &target_workspace_root,
        &matching_workspace_ids,
        tracker,
    )
}

/// Handles the case where no registered repository or workspace matches the source path.
/// Reports idempotent "already applied" records or an unmatched-source issue.
fn report_idempotent_or_unmatched_update(
    snapshot: &RegistrySnapshot,
    update: &MigrationPathUpdate,
    source_path: &str,
    target_repository_root: &str,
    target_workspace_root: &str,
    tracker: &mut MigrationAccumulator,
) -> Result<()> {
    let repo_matches = snapshot
        .repositories
        .values()
        .filter(|repository| {
            repository.root_uri == target_repository_root
                && repository.kind == update.target.repository_kind
                && (repository.kind != RepositoryKind::Git
                    || repository.project_identity == update.target.project_identity)
        })
        .cloned()
        .collect::<Vec<_>>();
    let workspace_matches = snapshot
        .workspaces
        .values()
        .filter(|workspace| workspace.root_uri == target_workspace_root)
        .cloned()
        .collect::<Vec<_>>();

    if let Some(repository) = repo_matches.first() {
        tracker.add_repository_record(
            MigrationBucket::Unchanged,
            repository,
            Some(source_path.to_string()),
            Some(repository.root_uri.clone()),
            "requested repository update was already applied",
        );
    }
    if let Some(workspace) = workspace_matches.first() {
        tracker.add_workspace_record(
            MigrationBucket::Unchanged,
            &workspace.workspace_id,
            Some(source_path.to_string()),
            Some(workspace.root_uri.clone()),
            "requested workspace update was already applied",
        );
    }

    if repo_matches.is_empty() && workspace_matches.is_empty() {
        tracker.add_general_issue(
            MigrationEntityKind::Workspace,
            Some(source_path.to_string()),
            format!(
                "no registered repository or workspace path matches `{source_path}`"
            ),
            format!(
                "Run `cargo run -- inspect` to confirm the current registry state, then rerun `cargo run -- migrate {source_path} <current-path>` with the old registered path."
            ),
            vec![target_workspace_root.to_string(), target_repository_root.to_string()],
        );
    }

    Ok(())
}

/// Validates that the resolved update target is compatible with the matched repository.
/// Returns `Ok(Some(()))` if a conflict was detected and reported (caller should return early),
/// or `Ok(None)` if validation passed and the update can proceed.
fn validate_path_update_target(
    snapshot: &RegistrySnapshot,
    repository: &Repository,
    update: &MigrationPathUpdate,
    source_path: &str,
    target_repository_root: &str,
    target_workspace_root: &str,
    matching_workspace_ids: &[String],
    tracker: &mut MigrationAccumulator,
) -> Result<Option<()>> {
    if repository.kind != update.target.repository_kind {
        tracker.add_repository_issue(
            repository,
            format!(
                "requested update would change repository `{}` from {:?} to {:?}",
                repository.repo_id, repository.kind, update.target.repository_kind
            ),
            "Update the target path to the same repository kind or migrate the registration explicitly by creating a fresh project entry.".to_string(),
            workspace_paths_for_repo(snapshot, &repository.repo_id),
        );
        return Ok(Some(()));
    }

    if repository.kind == RepositoryKind::Git
        && !repository.project_identity.is_empty()
        && repository.project_identity_kind != ProjectIdentityKind::LegacyRootUri
        && repository.project_identity != update.target.project_identity
    {
        tracker.add_repository_issue(
            repository,
            format!(
                "requested update for `{source_path}` does not prove the same canonical Git identity"
            ),
            format!(
                "Verify the replacement path and rerun `cargo run -- migrate {source_path} <current-path>` with a checkout or worktree that resolves to the same Git project."
            ),
            workspace_paths_for_repo(snapshot, &repository.repo_id),
        );
        return Ok(Some(()));
    }

    let duplicate_identity_paths = snapshot
        .repositories
        .values()
        .filter(|other| {
            other.repo_id != repository.repo_id
                && other.kind == update.target.repository_kind
                && !other.project_identity.is_empty()
                && other.project_identity == update.target.project_identity
        })
        .flat_map(|other| {
            std::iter::once(other.root_uri.clone())
                .chain(workspace_paths_for_repo(snapshot, &other.repo_id))
        })
        .collect::<Vec<_>>();
    if !duplicate_identity_paths.is_empty() {
        tracker.add_repository_issue(
            repository,
            format!(
                "requested update for `{source_path}` would collide with another registered project that already owns the same canonical identity"
            ),
            "Resolve the duplicate project registrations explicitly before retrying migration; Tokenizor will not merge projects silently.".to_string(),
            duplicate_identity_paths,
        );
        return Ok(Some(()));
    }

    let workspace_count = workspace_paths_for_repo(snapshot, &repository.repo_id).len();
    let should_update_repository_root = repository.root_uri == source_path
        || (!is_directory_path(&repository.root_uri)
            && matching_workspace_ids.len() == 1
            && workspace_count == 1);
    let repository_root_conflict = should_update_repository_root
        && snapshot.repositories.values().any(|other| {
            other.repo_id != repository.repo_id && other.root_uri == target_repository_root
        });
    if repository_root_conflict {
        tracker.add_repository_issue(
            repository,
            format!(
                "requested repository root `{target_repository_root}` is already registered to another project"
            ),
            "Choose the correct existing project or reconcile the conflicting registration before retrying migration.".to_string(),
            vec![target_repository_root.to_string()],
        );
        return Ok(Some(()));
    }

    let target_workspace_conflict = snapshot.workspaces.values().any(|workspace| {
        !matching_workspace_ids.contains(&workspace.workspace_id)
            && workspace.root_uri == target_workspace_root
    });
    if target_workspace_conflict {
        let candidate_paths = snapshot
            .workspaces
            .values()
            .filter(|workspace| workspace.root_uri == target_workspace_root)
            .map(|workspace| workspace.root_uri.clone())
            .collect::<Vec<_>>();
        tracker.add_repository_issue(
            repository,
            format!(
                "requested workspace path `{target_workspace_root}` is already registered"
            ),
            "Choose a different replacement path or inspect the existing workspace registration before retrying migration.".to_string(),
            candidate_paths,
        );
        return Ok(Some(()));
    }

    Ok(None)
}

/// Applies the validated repository identity/root update and workspace path re-keying,
/// then records the changes in the migration tracker.
fn commit_path_update(
    snapshot: &mut RegistrySnapshot,
    repository: Repository,
    update: &MigrationPathUpdate,
    source_path: &str,
    target_repository_root: &str,
    target_workspace_root: &str,
    matching_workspace_ids: &[String],
    tracker: &mut MigrationAccumulator,
) -> Result<()> {
    let workspace_count = workspace_paths_for_repo(snapshot, &repository.repo_id).len();
    let should_update_repository_root = repository.root_uri == source_path
        || (!is_directory_path(&repository.root_uri)
            && matching_workspace_ids.len() == 1
            && workspace_count == 1);

    let mut updated_repository = repository.clone();
    let repository_previous_path = updated_repository.root_uri.clone();
    let mut repository_changed = false;
    let repository_requires_identity_migration = updated_repository.project_identity.is_empty()
        || updated_repository.project_identity_kind == ProjectIdentityKind::LegacyRootUri;

    match updated_repository.kind {
        RepositoryKind::Local => {
            if updated_repository.project_identity != update.target.project_identity {
                updated_repository.project_identity = update.target.project_identity.clone();
                repository_changed = true;
            }
            if updated_repository.project_identity_kind != ProjectIdentityKind::LocalRootPath {
                updated_repository.project_identity_kind = ProjectIdentityKind::LocalRootPath;
                repository_changed = true;
            }
        }
        RepositoryKind::Git => {
            if updated_repository.project_identity != update.target.project_identity {
                updated_repository.project_identity = update.target.project_identity.clone();
                repository_changed = true;
            }
            if updated_repository.project_identity_kind != ProjectIdentityKind::GitCommonDir {
                updated_repository.project_identity_kind = ProjectIdentityKind::GitCommonDir;
                repository_changed = true;
            }
        }
    }

    if should_update_repository_root && updated_repository.root_uri != target_repository_root {
        updated_repository.root_uri = target_repository_root.to_string();
        repository_changed = true;
    }

    let mut workspace_update = None;
    if let Some(workspace_id) = matching_workspace_ids.first() {
        let workspace = snapshot
            .workspaces
            .get(workspace_id)
            .cloned()
            .ok_or_else(|| {
                TokenizorError::Storage(format!(
                    "workspace update references missing workspace `{workspace_id}`"
                ))
            })?;
        let new_workspace_id = workspace_id_for_root_uri(target_workspace_root);

        if snapshot
            .workspaces
            .iter()
            .any(|(other_id, other_workspace)| {
                other_id != &workspace.workspace_id
                    && other_workspace.root_uri == target_workspace_root
            })
        {
            tracker.add_workspace_issue(
                &workspace,
                format!(
                    "requested replacement path `{target_workspace_root}` is already registered to another workspace"
                ),
                "Inspect the existing workspace state and choose the correct replacement path before retrying migration.".to_string(),
                vec![target_workspace_root.to_string()],
            );
            return Ok(());
        }

        if new_workspace_id != workspace.workspace_id
            && snapshot.workspaces.contains_key(&new_workspace_id)
        {
            tracker.add_workspace_issue(
                &workspace,
                format!(
                    "requested replacement path `{target_workspace_root}` would reuse an existing workspace identity"
                ),
                "Choose a different replacement path or clear the conflicting workspace entry before retrying migration.".to_string(),
                vec![target_workspace_root.to_string()],
            );
            return Ok(());
        }

        let mut updated_workspace = workspace.clone();
        let workspace_previous_path = updated_workspace.root_uri.clone();
        updated_workspace.root_uri = target_workspace_root.to_string();
        updated_workspace.workspace_id = new_workspace_id.clone();
        updated_workspace.repo_id = updated_repository.repo_id.clone();

        workspace_update = Some((
            workspace,
            updated_workspace,
            workspace_previous_path,
            new_workspace_id,
        ));
    }

    if repository_changed {
        snapshot.repositories.insert(
            updated_repository.repo_id.clone(),
            updated_repository.clone(),
        );
        let detail = if repository_requires_identity_migration {
            format!(
                "migrated repository `{}` to the canonical {} identity model via explicit operator mapping",
                updated_repository.repo_id,
                match updated_repository.kind {
                    RepositoryKind::Git => "git_common_dir",
                    RepositoryKind::Local => "local_root_path",
                }
            )
        } else {
            format!(
                "updated repository `{}` from `{repository_previous_path}` to `{}` via explicit operator mapping",
                updated_repository.repo_id, updated_repository.root_uri
            )
        };
        tracker.add_repository_record(
            if repository_requires_identity_migration {
                MigrationBucket::Migrated
            } else {
                MigrationBucket::Updated
            },
            &updated_repository,
            Some(repository_previous_path),
            Some(updated_repository.root_uri.clone()),
            detail,
        );
    } else {
        tracker.add_repository_record(
            MigrationBucket::Unchanged,
            &updated_repository,
            Some(repository_previous_path.clone()),
            Some(updated_repository.root_uri.clone()),
            "repository already matches the requested migration target",
        );
    }

    if let Some((
        original_workspace,
        updated_workspace,
        workspace_previous_path,
        original_workspace_id,
    )) = workspace_update
    {
        snapshot.workspaces.remove(&original_workspace.workspace_id);
        snapshot.workspaces.insert(
            updated_workspace.workspace_id.clone(),
            updated_workspace.clone(),
        );

        let bucket = if original_workspace.root_uri == updated_workspace.root_uri
            && original_workspace.workspace_id == updated_workspace.workspace_id
        {
            MigrationBucket::Unchanged
        } else {
            MigrationBucket::Updated
        };
        let detail = match bucket {
            MigrationBucket::Unchanged => {
                "workspace already matches the requested migration target".to_string()
            }
            _ => format!(
                "updated workspace `{}` from `{workspace_previous_path}` to `{}` via explicit operator mapping",
                original_workspace_id, updated_workspace.root_uri
            ),
        };
        tracker.add_workspace_record(
            bucket,
            &updated_workspace.workspace_id,
            Some(workspace_previous_path),
            Some(updated_workspace.root_uri.clone()),
            detail,
        );
    }

    Ok(())
}

fn apply_repository_migrations(
    snapshot: &mut RegistrySnapshot,
    tracker: &mut MigrationAccumulator,
) {
    let repo_ids = snapshot.repositories.keys().cloned().collect::<Vec<_>>();

    for repo_id in repo_ids {
        if tracker.repository_touched(&repo_id) {
            continue;
        }

        let repository = match snapshot.repositories.get(&repo_id).cloned() {
            Some(repository) => repository,
            None => continue,
        };

        match repository.kind {
            RepositoryKind::Local => migrate_local_repository(snapshot, repository, tracker),
            RepositoryKind::Git => migrate_git_repository(snapshot, repository, tracker),
        }
    }
}

fn migrate_local_repository(
    snapshot: &mut RegistrySnapshot,
    repository: Repository,
    tracker: &mut MigrationAccumulator,
) {
    let mut updated_repository = repository.clone();
    let mut changed = false;

    if updated_repository.project_identity != updated_repository.root_uri {
        updated_repository.project_identity = updated_repository.root_uri.clone();
        changed = true;
    }
    if updated_repository.project_identity_kind != ProjectIdentityKind::LocalRootPath {
        updated_repository.project_identity_kind = ProjectIdentityKind::LocalRootPath;
        changed = true;
    }

    if changed {
        snapshot.repositories.insert(
            updated_repository.repo_id.clone(),
            updated_repository.clone(),
        );
        tracker.add_repository_record(
            MigrationBucket::Migrated,
            &updated_repository,
            Some(repository.root_uri.clone()),
            Some(updated_repository.root_uri.clone()),
            format!(
                "migrated local repository `{}` to the canonical local_root_path identity model",
                updated_repository.repo_id
            ),
        );
    } else {
        tracker.add_repository_record(
            MigrationBucket::Unchanged,
            &updated_repository,
            Some(updated_repository.root_uri.clone()),
            Some(updated_repository.root_uri.clone()),
            "local repository already uses the canonical local_root_path identity model",
        );
    }
}

fn migrate_git_repository(
    snapshot: &mut RegistrySnapshot,
    repository: Repository,
    tracker: &mut MigrationAccumulator,
) {
    let evidence = collect_git_identity_evidence(snapshot, &repository);
    let candidate_paths = evidence
        .iter()
        .map(|candidate| candidate.path.display().to_string())
        .chain(std::iter::once(repository.root_uri.clone()))
        .chain(workspace_paths_for_repo(snapshot, &repository.repo_id))
        .collect::<Vec<_>>();
    let root_exists = is_directory_path(&repository.root_uri);
    let identity_migration_required = repository.project_identity.is_empty()
        || repository.project_identity_kind == ProjectIdentityKind::LegacyRootUri;

    if identity_migration_required {
        let Some(project_identity) = unique_git_project_identity(&evidence) else {
            let detail = if evidence.is_empty() {
                format!(
                    "legacy Git repository `{}` has no surviving local evidence that can prove a canonical project identity",
                    repository.repo_id
                )
            } else {
                format!(
                    "legacy Git repository `{}` has multiple plausible canonical project identities",
                    repository.repo_id
                )
            };
            tracker.add_repository_issue(
                &repository,
                detail,
                format!(
                    "Restore one surviving checkout/worktree or rerun `cargo run -- migrate {} <current-path>` with an explicit replacement path.",
                    repository.root_uri
                ),
                candidate_paths,
            );
            return;
        };

        let duplicate_identity_paths = snapshot
            .repositories
            .values()
            .filter(|other| {
                other.repo_id != repository.repo_id
                    && !other.project_identity.is_empty()
                    && other.project_identity == project_identity
            })
            .flat_map(|other| {
                std::iter::once(other.root_uri.clone())
                    .chain(workspace_paths_for_repo(snapshot, &other.repo_id))
            })
            .collect::<Vec<_>>();
        if !duplicate_identity_paths.is_empty() {
            tracker.add_repository_issue(
                &repository,
                format!(
                    "migrating repository `{}` would collide with another registered project that already owns the canonical identity",
                    repository.repo_id
                ),
                "Resolve the duplicate project registrations explicitly before retrying migration; Tokenizor will not merge projects silently.".to_string(),
                duplicate_identity_paths,
            );
            return;
        }

        let mut updated_repository = repository.clone();
        updated_repository.project_identity = project_identity;
        updated_repository.project_identity_kind = ProjectIdentityKind::GitCommonDir;

        if !root_exists {
            if let Some(preferred_root) = preferred_repository_root_from_evidence(&evidence) {
                updated_repository.root_uri = preferred_root;
            }
        }

        snapshot.repositories.insert(
            updated_repository.repo_id.clone(),
            updated_repository.clone(),
        );
        tracker.add_repository_record(
            MigrationBucket::Migrated,
            &updated_repository,
            Some(repository.root_uri.clone()),
            Some(updated_repository.root_uri.clone()),
            format!(
                "migrated legacy Git repository `{}` to the canonical git_common_dir identity model",
                updated_repository.repo_id
            ),
        );
        return;
    }

    if !root_exists {
        if let Some(preferred_root) = preferred_repository_root_from_evidence(&evidence) {
            if preferred_root != repository.root_uri {
                let mut updated_repository = repository.clone();
                updated_repository.root_uri = preferred_root.clone();
                snapshot.repositories.insert(
                    updated_repository.repo_id.clone(),
                    updated_repository.clone(),
                );
                tracker.add_repository_record(
                    MigrationBucket::Updated,
                    &updated_repository,
                    Some(repository.root_uri.clone()),
                    Some(preferred_root),
                    format!(
                        "updated repository `{}` to a surviving local root path after the original checkout disappeared",
                        updated_repository.repo_id
                    ),
                );
                return;
            }
        }

        tracker.add_repository_issue(
            &repository,
            format!(
                "registered repository root `{}` is missing and no unique surviving replacement path could be proven safely",
                repository.root_uri
            ),
            format!(
                "Rerun `cargo run -- migrate {} <current-path>` with the explicit replacement path or restore one surviving checkout/worktree.",
                repository.root_uri
            ),
            candidate_paths,
        );
        return;
    }

    tracker.add_repository_record(
        MigrationBucket::Unchanged,
        &repository,
        Some(repository.root_uri.clone()),
        Some(repository.root_uri.clone()),
        "Git repository already uses the canonical git_common_dir identity model",
    );
}

fn scan_workspace_state(snapshot: &RegistrySnapshot, tracker: &mut MigrationAccumulator) {
    let workspace_ids = snapshot.workspaces.keys().cloned().collect::<Vec<_>>();

    for workspace_id in workspace_ids {
        if tracker.workspace_touched(&workspace_id) {
            continue;
        }

        let workspace = match snapshot.workspaces.get(&workspace_id).cloned() {
            Some(workspace) => workspace,
            None => continue,
        };

        if is_directory_path(&workspace.root_uri) {
            tracker.add_workspace_record(
                MigrationBucket::Unchanged,
                &workspace.workspace_id,
                Some(workspace.root_uri.clone()),
                Some(workspace.root_uri.clone()),
                "workspace path remains valid",
            );
            continue;
        }

        let candidate_paths = snapshot
            .repositories
            .get(&workspace.repo_id)
            .map(|repository| {
                std::iter::once(repository.root_uri.clone())
                    .chain(workspace_paths_for_repo(snapshot, &workspace.repo_id))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| vec![workspace.root_uri.clone()]);
        tracker.add_workspace_issue(
            &workspace,
            format!(
                "registered workspace path `{}` is missing and no safe replacement path has been recorded",
                workspace.root_uri
            ),
            format!(
                "Run `cargo run -- migrate {} <current-path>` with the old workspace path and the surviving replacement path.",
                workspace.root_uri
            ),
            candidate_paths,
        );
    }
}

fn find_legacy_repository_candidates(
    snapshot: &RegistrySnapshot,
    target: &ResolvedRepositoryTarget,
) -> Vec<Repository> {
    snapshot
        .repositories
        .values()
        .filter(|repository| repository_matches_legacy_target(snapshot, repository, target))
        .cloned()
        .collect()
}

fn repository_matches_legacy_target(
    snapshot: &RegistrySnapshot,
    repository: &Repository,
    target: &ResolvedRepositoryTarget,
) -> bool {
    if repository.kind != target.repository_kind {
        return false;
    }

    let requires_migration = repository.project_identity.is_empty()
        || match repository.kind {
            RepositoryKind::Local => {
                repository.project_identity_kind != ProjectIdentityKind::LocalRootPath
            }
            RepositoryKind::Git => {
                repository.project_identity_kind == ProjectIdentityKind::LegacyRootUri
            }
        };
    if !requires_migration {
        return false;
    }

    let target_repository_root = target.repository_root.display().to_string();
    let target_workspace_root = target.workspace_root.display().to_string();

    if repository.root_uri == target_repository_root || repository.root_uri == target_workspace_root
    {
        return true;
    }

    match repository.kind {
        RepositoryKind::Local => workspace_paths_for_repo(snapshot, &repository.repo_id)
            .into_iter()
            .any(|workspace_path| workspace_path == target_workspace_root),
        RepositoryKind::Git => {
            if Path::new(&repository.root_uri).is_dir()
                && resolve_git_project_identity(Path::new(&repository.root_uri))
                    .map(|identity| identity.project_identity == target.project_identity)
                    .unwrap_or(false)
            {
                return true;
            }

            workspace_paths_for_repo(snapshot, &repository.repo_id)
                .into_iter()
                .filter(|workspace_path| Path::new(&workspace_path).is_dir())
                .any(|workspace_path| {
                    resolve_git_project_identity(Path::new(&workspace_path))
                        .map(|identity| identity.project_identity == target.project_identity)
                        .unwrap_or(false)
                })
        }
    }
}

fn legacy_migration_required_error(
    snapshot: &RegistrySnapshot,
    target: &ResolvedRepositoryTarget,
    candidates: &[Repository],
) -> TokenizorError {
    let candidate_paths = candidates
        .iter()
        .flat_map(|repository| {
            std::iter::once(repository.root_uri.clone())
                .chain(workspace_paths_for_repo(snapshot, &repository.repo_id))
        })
        .collect::<Vec<_>>();

    TokenizorError::InvalidArgument(format!(
        "workspace `{}` matches legacy bootstrap state that must be reconciled via `cargo run -- migrate` before registration can continue: {}",
        target.workspace_root.display(),
        normalize_candidate_paths(candidate_paths).join(", ")
    ))
}

fn resolve_repository_registration(
    snapshot: &mut RegistrySnapshot,
    target: &ResolvedRepositoryTarget,
    mode: RegistrationMode,
) -> Result<(Repository, RegistrationAction)> {
    let candidates = find_repository_candidates(snapshot, target);

    if candidates.len() > 1 {
        return Err(ambiguous_project_match_error(snapshot, target, &candidates));
    }

    if let Some(repository) = candidates.into_iter().next() {
        return Ok((repository, RegistrationAction::Reused));
    }

    let legacy_candidates = find_legacy_repository_candidates(snapshot, target);
    if !legacy_candidates.is_empty() {
        return Err(legacy_migration_required_error(
            snapshot,
            target,
            &legacy_candidates,
        ));
    }

    if mode == RegistrationMode::AttachOnly {
        return Err(TokenizorError::InvalidArgument(format!(
            "workspace `{}` does not match an existing registered project; separate initialization is required via `cargo run -- init {}`",
            target.workspace_root.display(),
            target.workspace_root.display()
        )));
    }

    let repository = build_repository(target);
    let action = upsert_repository(snapshot, repository.clone());
    let record = snapshot
        .repositories
        .get(&repository.repo_id)
        .cloned()
        .unwrap_or(repository);
    Ok((record, action))
}

fn find_repository_candidates(
    snapshot: &RegistrySnapshot,
    target: &ResolvedRepositoryTarget,
) -> Vec<Repository> {
    snapshot
        .repositories
        .values()
        .filter(|repository| {
            repository.kind == target.repository_kind
                && repository.project_identity == target.project_identity
        })
        .cloned()
        .collect()
}

fn ambiguous_project_match_error(
    snapshot: &RegistrySnapshot,
    target: &ResolvedRepositoryTarget,
    candidates: &[Repository],
) -> TokenizorError {
    let candidate_paths = candidates
        .iter()
        .map(|repository| {
            let workspace_paths = snapshot
                .workspaces
                .values()
                .filter(|workspace| workspace.repo_id == repository.repo_id)
                .map(|workspace| workspace.root_uri.as_str())
                .collect::<Vec<_>>();
            if workspace_paths.is_empty() {
                repository.root_uri.clone()
            } else {
                format!("{} [{}]", repository.root_uri, workspace_paths.join(", "))
            }
        })
        .collect::<Vec<_>>()
        .join("; ");

    TokenizorError::InvalidArgument(format!(
        "workspace `{}` matches multiple registered projects for canonical identity `{}`: {candidate_paths}",
        target.workspace_root.display(),
        target.project_identity
    ))
}

fn upsert_repository(
    snapshot: &mut RegistrySnapshot,
    repository: Repository,
) -> RegistrationAction {
    if snapshot.repositories.contains_key(&repository.repo_id) {
        return RegistrationAction::Reused;
    }

    snapshot
        .repositories
        .insert(repository.repo_id.clone(), repository);
    RegistrationAction::Created
}

fn upsert_workspace(
    snapshot: &mut RegistrySnapshot,
    workspace: Workspace,
) -> Result<RegistrationAction> {
    match snapshot.workspaces.get(&workspace.workspace_id) {
        Some(existing) if existing == &workspace => Ok(RegistrationAction::Reused),
        Some(existing) => Err(TokenizorError::InvalidArgument(format!(
            "workspace `{}` is already attached to project `{}` and cannot be reassigned implicitly",
            existing.root_uri, existing.repo_id
        ))),
        _ => {
            snapshot
                .workspaces
                .insert(workspace.workspace_id.clone(), workspace);
            Ok(RegistrationAction::Created)
        }
    }
}

fn load_snapshot(path: &Path) -> Result<RegistrySnapshot> {
    match fs::read(path) {
        Ok(bytes) => {
            let snapshot: RegistrySnapshot = serde_json::from_slice(&bytes).map_err(|error| {
                TokenizorError::Serialization(format!(
                    "failed to deserialize registry `{}`: {error}",
                    path.display()
                ))
            })?;

            if matches!(
                snapshot.schema_version,
                LEGACY_REGISTRY_SCHEMA_VERSION | CURRENT_REGISTRY_SCHEMA_VERSION
            ) {
                Ok(snapshot)
            } else {
                Err(TokenizorError::Storage(format!(
                    "unsupported project/workspace registry schema version {} at {}",
                    snapshot.schema_version,
                    path.display()
                )))
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(RegistrySnapshot {
            schema_version: CURRENT_REGISTRY_SCHEMA_VERSION,
            registry_kind: RegistryKind::LocalBootstrapProjectWorkspace,
            authority_mode: AuthorityMode::LocalBootstrapOnly,
            control_plane_backend: default_control_plane_backend(),
            ..RegistrySnapshot::default()
        }),
        Err(error) => Err(TokenizorError::io(path, error)),
    }
}

fn save_snapshot(path: &Path, snapshot: &RegistrySnapshot) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        TokenizorError::Storage(format!(
            "registry path `{}` is missing a parent directory",
            path.display()
        ))
    })?;

    fs::create_dir_all(parent).map_err(|error| TokenizorError::io(parent, error))?;
    let bytes = serde_json::to_vec_pretty(snapshot).map_err(|error| {
        TokenizorError::Serialization(format!(
            "failed to serialize registry `{}`: {error}",
            path.display()
        ))
    })?;
    let temp_path = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("project-workspace-registry.json"),
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

fn lock_path(path: &Path) -> PathBuf {
    path.with_extension("lock")
}

struct RegistryLock {
    path: PathBuf,
    file: File,
}

impl Drop for RegistryLock {
    fn drop(&mut self) {
        let _ = self.file.sync_all();
        let _ = fs::remove_file(&self.path);
    }
}

fn acquire_registry_lock(path: &Path) -> Result<RegistryLock> {
    let lock_path = lock_path(path);
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent).map_err(|error| TokenizorError::io(parent, error))?;
    }
    let started_at = SystemTime::now();

    loop {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(mut file) => {
                writeln!(file, "pid={}", std::process::id())
                    .map_err(|error| TokenizorError::io(&lock_path, error))?;
                file.sync_all()
                    .map_err(|error| TokenizorError::io(&lock_path, error))?;
                return Ok(RegistryLock {
                    path: lock_path,
                    file,
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if lock_is_stale(&lock_path)? {
                    match fs::remove_file(&lock_path) {
                        Ok(()) => continue,
                        Err(remove_error)
                            if remove_error.kind() == std::io::ErrorKind::NotFound =>
                        {
                            continue;
                        }
                        Err(remove_error) => {
                            return Err(TokenizorError::io(&lock_path, remove_error));
                        }
                    }
                }

                let waited_ms = started_at
                    .elapsed()
                    .unwrap_or_else(|_| Duration::from_millis(0))
                    .as_millis() as u64;
                if waited_ms >= REGISTRY_LOCK_TIMEOUT_MS {
                    return Err(TokenizorError::Storage(format!(
                        "timed out waiting for registry lock `{}` after {} ms",
                        lock_path.display(),
                        REGISTRY_LOCK_TIMEOUT_MS
                    )));
                }

                thread::sleep(Duration::from_millis(REGISTRY_LOCK_RETRY_DELAY_MS));
            }
            Err(error) => return Err(TokenizorError::io(&lock_path, error)),
        }
    }
}

fn lock_is_stale(path: &Path) -> Result<bool> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(TokenizorError::io(path, error)),
    };
    let modified = match metadata.modified() {
        Ok(modified) => modified,
        Err(_) => return Ok(false),
    };
    let age_ms = modified
        .elapsed()
        .unwrap_or_else(|_| Duration::from_millis(0))
        .as_millis() as u64;
    Ok(age_ms >= REGISTRY_LOCK_STALE_AFTER_MS)
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

#[cfg(test)]
mod tests {
    use crate::application::ApplicationContext;
    use crate::config::{ControlPlaneBackend, ServerConfig};
    use crate::domain::{
        ContextResolutionMode, HealthIssueCategory, ProjectIdentityKind, RegistrationAction,
    };
    use serde_json::json;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Barrier, Mutex, OnceLock};
    use std::thread;

    use super::{
        AuthorityMode, RegistryKind, RegistrySnapshot, acquire_registry_lock, load_snapshot,
        lock_path, registry_path, resolve_context_request, resolve_migration_request,
        resolve_repository_target, save_snapshot, workspace_id_for_root_uri,
    };

    struct TestDir {
        path: PathBuf,
    }

    struct CurrentDirGuard {
        original: PathBuf,
    }

    impl CurrentDirGuard {
        fn set(path: &Path) -> Self {
            let original = std::env::current_dir().expect("current directory should be readable");
            std::env::set_current_dir(path).expect("current directory should be updated");
            Self { original }
        }
    }

    impl Drop for CurrentDirGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.original);
        }
    }

    impl TestDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "tokenizor-{name}-{}-{}",
                std::process::id(),
                crate::domain::unix_timestamp_ms()
            ));
            fs::create_dir_all(&path).expect("test directory should be created");
            Self { path }
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn create_git_repo(path: &Path) {
        fs::create_dir_all(path.join(".git")).expect("git directory should be created");
        fs::write(path.join(".git").join("HEAD"), "ref: refs/heads/main\n")
            .expect("git HEAD should be created");
        fs::create_dir_all(path.join("src")).expect("nested directory should be created");
    }

    fn create_git_worktree(workspace_root: &Path, common_git_dir: &Path, worktree_name: &str) {
        let worktree_admin_dir = common_git_dir.join("worktrees").join(worktree_name);
        fs::create_dir_all(&worktree_admin_dir).expect("worktree admin directory should exist");
        fs::write(worktree_admin_dir.join("HEAD"), "ref: refs/heads/feature\n")
            .expect("worktree HEAD should be created");
        fs::write(worktree_admin_dir.join("commondir"), "..\\..\n")
            .expect("worktree commondir should be created");
        fs::create_dir_all(workspace_root).expect("worktree root should be created");
        fs::write(
            workspace_root.join(".git"),
            format!("gitdir: {}\n", worktree_admin_dir.display()),
        )
        .expect("worktree .git file should be created");
        fs::create_dir_all(workspace_root.join("src"))
            .expect("worktree nested directory should be created");
    }

    fn repository_record(
        repo_id: &str,
        kind: crate::domain::RepositoryKind,
        root_uri: &str,
        project_identity: &str,
        project_identity_kind: ProjectIdentityKind,
    ) -> crate::domain::Repository {
        crate::domain::Repository {
            repo_id: repo_id.to_string(),
            kind,
            root_uri: root_uri.to_string(),
            project_identity: project_identity.to_string(),
            project_identity_kind,
            default_branch: None,
            last_known_revision: None,
            status: crate::domain::RepositoryStatus::Ready,
        }
    }

    fn workspace_record(
        workspace_id: &str,
        repo_id: &str,
        root_uri: &str,
    ) -> crate::domain::Workspace {
        crate::domain::Workspace {
            workspace_id: workspace_id.to_string(),
            repo_id: repo_id.to_string(),
            root_uri: root_uri.to_string(),
            status: crate::domain::WorkspaceStatus::Active,
        }
    }

    fn application_context(blob_root: PathBuf) -> ApplicationContext {
        let mut config = ServerConfig::default();
        config.control_plane.backend = ControlPlaneBackend::InMemory;
        config.blob_store.root_dir = blob_root;

        ApplicationContext::from_config(config).expect("application context")
    }

    fn current_dir_test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn registry_snapshot(
        repositories: impl IntoIterator<Item = crate::domain::Repository>,
        workspaces: impl IntoIterator<Item = crate::domain::Workspace>,
    ) -> RegistrySnapshot {
        RegistrySnapshot {
            schema_version: 2,
            registry_kind: RegistryKind::LocalBootstrapProjectWorkspace,
            authority_mode: AuthorityMode::LocalBootstrapOnly,
            control_plane_backend: "in_memory".to_string(),
            repositories: repositories
                .into_iter()
                .map(|repository| (repository.repo_id.clone(), repository))
                .collect(),
            workspaces: workspaces
                .into_iter()
                .map(|workspace| (workspace.workspace_id.clone(), workspace))
                .collect(),
        }
    }

    fn write_snapshot_json(path: &Path, value: serde_json::Value) {
        let parent = path.parent().expect("snapshot parent should exist");
        fs::create_dir_all(parent).expect("snapshot parent should be created");
        fs::write(
            path,
            serde_json::to_vec_pretty(&value).expect("snapshot json should serialize"),
        )
        .expect("snapshot json should be written");
    }

    #[test]
    fn resolves_git_repository_root_from_nested_path() {
        let test_dir = TestDir::new("init-resolve-git-root");
        let repo_root = test_dir.path.join("repo");
        create_git_repo(&repo_root);
        let nested = repo_root.join("src");

        let resolved = resolve_repository_target(Some(&nested), &test_dir.path)
            .expect("repository target should resolve");

        assert_eq!(resolved.repository_root, repo_root);
        assert_eq!(resolved.workspace_root, repo_root);
        assert_eq!(resolved.repository_kind, crate::domain::RepositoryKind::Git);
        assert_eq!(
            resolved.project_identity,
            repo_root.join(".git").display().to_string()
        );
        assert_eq!(
            resolved.project_identity_kind,
            ProjectIdentityKind::GitCommonDir
        );
    }

    #[test]
    fn resolves_current_directory_when_no_explicit_path_is_provided() {
        let test_dir = TestDir::new("init-resolve-current-dir");
        let repo_root = test_dir.path.join("repo");
        create_git_repo(&repo_root);

        let resolved =
            resolve_repository_target(None, &repo_root).expect("current dir should resolve");

        assert_eq!(resolved.input_path, repo_root);
        assert_eq!(resolved.repository_root, resolved.workspace_root);
    }

    #[test]
    fn ignores_non_repository_dot_git_directories_when_resolving_root() {
        let test_dir = TestDir::new("init-ignore-fake-git-root");
        let fake_root = test_dir.path.join("outer");
        let local_project = fake_root.join("project");
        fs::create_dir_all(fake_root.join(".git")).expect("fake git marker should be created");
        fs::create_dir_all(&local_project).expect("local project should be created");

        let resolved = resolve_repository_target(Some(&local_project), &test_dir.path)
            .expect("local project should resolve");

        assert_eq!(resolved.repository_root, local_project);
        assert_eq!(resolved.workspace_root, resolved.repository_root);
        assert_eq!(
            resolved.repository_kind,
            crate::domain::RepositoryKind::Local
        );
        assert_eq!(
            resolved.project_identity_kind,
            ProjectIdentityKind::LocalRootPath
        );
    }

    #[test]
    fn initialize_repository_persists_and_reuses_registration_across_fresh_services() {
        let test_dir = TestDir::new("init-persist-reuse");
        let repo_root = test_dir.path.join("repo");
        create_git_repo(&repo_root);
        let blob_root = test_dir.path.join(".tokenizor");

        let first_report = application_context(blob_root.clone())
            .initialize_repository(Some(repo_root.clone()))
            .expect("first initialization should succeed");
        let second_report = application_context(blob_root.clone())
            .initialize_repository(Some(repo_root.clone()))
            .expect("second initialization should succeed");

        assert_eq!(first_report.repository.action, RegistrationAction::Created);
        assert_eq!(first_report.workspace.action, RegistrationAction::Created);
        assert_eq!(second_report.repository.action, RegistrationAction::Reused);
        assert_eq!(second_report.workspace.action, RegistrationAction::Reused);
        assert_eq!(
            first_report.repository.record.repo_id,
            second_report.repository.record.repo_id
        );
        assert_eq!(
            first_report.workspace.record.workspace_id,
            second_report.workspace.record.workspace_id
        );

        let snapshot = load_snapshot(&registry_path(&blob_root)).expect("snapshot should load");
        assert_eq!(snapshot.schema_version, 2);
        assert_eq!(
            snapshot.registry_kind,
            RegistryKind::LocalBootstrapProjectWorkspace
        );
        assert_eq!(snapshot.authority_mode, AuthorityMode::LocalBootstrapOnly);
        assert_eq!(snapshot.control_plane_backend, "in_memory");
        assert_eq!(snapshot.repositories.len(), 1);
        assert_eq!(snapshot.workspaces.len(), 1);
    }

    #[test]
    fn resolves_git_worktree_to_shared_project_identity() {
        let test_dir = TestDir::new("init-resolve-worktree");
        let repo_root = test_dir.path.join("repo");
        let worktree_root = test_dir.path.join("repo-feature");
        create_git_repo(&repo_root);
        create_git_worktree(&worktree_root, &repo_root.join(".git"), "feature");

        let resolved = resolve_repository_target(Some(&worktree_root), &test_dir.path)
            .expect("worktree target should resolve");

        assert_eq!(resolved.workspace_root, worktree_root);
        assert_eq!(resolved.repository_root, repo_root);
        assert_eq!(resolved.repository_kind, crate::domain::RepositoryKind::Git);
        assert_eq!(
            resolved.project_identity,
            repo_root.join(".git").display().to_string()
        );
        assert_eq!(
            resolved.project_identity_kind,
            ProjectIdentityKind::GitCommonDir
        );
    }

    #[test]
    fn initialize_repository_attaches_matching_worktree_without_duplicate_project() {
        let test_dir = TestDir::new("init-attach-worktree");
        let repo_root = test_dir.path.join("repo");
        let worktree_root = test_dir.path.join("repo-feature");
        let blob_root = test_dir.path.join(".tokenizor");
        create_git_repo(&repo_root);
        create_git_worktree(&worktree_root, &repo_root.join(".git"), "feature");

        let first_report = application_context(blob_root.clone())
            .initialize_repository(Some(repo_root.clone()))
            .expect("primary repo initialization should succeed");
        let second_report = application_context(blob_root.clone())
            .initialize_repository(Some(worktree_root.clone()))
            .expect("matching worktree initialization should attach");

        assert_eq!(first_report.repository.action, RegistrationAction::Created);
        assert_eq!(second_report.repository.action, RegistrationAction::Reused);
        assert_eq!(second_report.workspace.action, RegistrationAction::Created);
        assert_eq!(
            first_report.repository.record.repo_id,
            second_report.repository.record.repo_id
        );
        assert_ne!(
            first_report.workspace.record.workspace_id,
            second_report.workspace.record.workspace_id
        );
        assert_eq!(
            second_report.repository.record.root_uri,
            repo_root.display().to_string()
        );
        assert_eq!(
            second_report.workspace.record.root_uri,
            worktree_root.display().to_string()
        );

        let snapshot = load_snapshot(&registry_path(&blob_root)).expect("snapshot should load");
        assert_eq!(snapshot.repositories.len(), 1);
        assert_eq!(snapshot.workspaces.len(), 2);
    }

    #[test]
    fn initialize_repository_keeps_unrelated_git_projects_separate() {
        let test_dir = TestDir::new("init-separate-projects");
        let repo_a = test_dir.path.join("repo-a");
        let repo_b = test_dir.path.join("repo-b");
        let blob_root = test_dir.path.join(".tokenizor");
        create_git_repo(&repo_a);
        create_git_repo(&repo_b);

        let first_report = application_context(blob_root.clone())
            .initialize_repository(Some(repo_a.clone()))
            .expect("first project initialization should succeed");
        let second_report = application_context(blob_root.clone())
            .initialize_repository(Some(repo_b.clone()))
            .expect("second project initialization should succeed");

        assert_eq!(first_report.repository.action, RegistrationAction::Created);
        assert_eq!(second_report.repository.action, RegistrationAction::Created);
        assert_ne!(
            first_report.repository.record.repo_id,
            second_report.repository.record.repo_id
        );

        let snapshot = load_snapshot(&registry_path(&blob_root)).expect("snapshot should load");
        assert_eq!(snapshot.repositories.len(), 2);
        assert_eq!(snapshot.workspaces.len(), 2);
    }

    #[test]
    fn attach_workspace_is_idempotent_for_equivalent_inputs() {
        let test_dir = TestDir::new("attach-idempotent");
        let repo_root = test_dir.path.join("repo");
        let worktree_root = test_dir.path.join("repo-feature");
        let blob_root = test_dir.path.join(".tokenizor");
        create_git_repo(&repo_root);
        create_git_worktree(&worktree_root, &repo_root.join(".git"), "feature");

        application_context(blob_root.clone())
            .initialize_repository(Some(repo_root.clone()))
            .expect("primary repo initialization should succeed");

        let first_attach = application_context(blob_root.clone())
            .attach_workspace(Some(worktree_root.clone()))
            .expect("first attach should succeed");
        let second_attach = application_context(blob_root.clone())
            .attach_workspace(Some(worktree_root.clone()))
            .expect("second attach should reuse");

        assert_eq!(first_attach.repository.action, RegistrationAction::Reused);
        assert_eq!(first_attach.workspace.action, RegistrationAction::Created);
        assert_eq!(second_attach.repository.action, RegistrationAction::Reused);
        assert_eq!(second_attach.workspace.action, RegistrationAction::Reused);
        assert_eq!(
            first_attach.workspace.record.workspace_id,
            second_attach.workspace.record.workspace_id
        );

        let snapshot = load_snapshot(&registry_path(&blob_root)).expect("snapshot should load");
        assert_eq!(snapshot.repositories.len(), 1);
        assert_eq!(snapshot.workspaces.len(), 2);
    }

    #[test]
    fn inspect_registry_surfaces_attached_worktree_under_shared_project() {
        let test_dir = TestDir::new("inspect-attached-worktree");
        let repo_root = test_dir.path.join("repo");
        let worktree_root = test_dir.path.join("repo-feature");
        let blob_root = test_dir.path.join(".tokenizor");
        create_git_repo(&repo_root);
        create_git_worktree(&worktree_root, &repo_root.join(".git"), "feature");

        application_context(blob_root.clone())
            .initialize_repository(Some(repo_root.clone()))
            .expect("primary repo initialization should succeed");
        application_context(blob_root.clone())
            .attach_workspace(Some(worktree_root.clone()))
            .expect("worktree attachment should succeed");

        let report = application_context(blob_root)
            .inspect_registry()
            .expect("registry inspection should succeed");

        assert_eq!(report.project_count, 1);
        assert_eq!(report.workspace_count, 2);
        assert_eq!(report.projects.len(), 1);
        assert_eq!(
            report.projects[0].repository.root_uri,
            repo_root.display().to_string()
        );
        assert_eq!(
            report.projects[0].repository.project_identity,
            repo_root.join(".git").display().to_string()
        );
        assert_eq!(
            report.projects[0]
                .workspaces
                .iter()
                .map(|workspace| workspace.root_uri.clone())
                .collect::<Vec<_>>(),
            vec![
                repo_root.display().to_string(),
                worktree_root.display().to_string(),
            ]
        );
    }

    #[test]
    fn attach_workspace_requires_separate_initialization_for_unrelated_project() {
        let test_dir = TestDir::new("attach-separate-init");
        let repo_root = test_dir.path.join("repo");
        let other_repo = test_dir.path.join("other-repo");
        let blob_root = test_dir.path.join(".tokenizor");
        create_git_repo(&repo_root);
        create_git_repo(&other_repo);

        application_context(blob_root.clone())
            .initialize_repository(Some(repo_root))
            .expect("primary repo initialization should succeed");

        let error = application_context(blob_root)
            .attach_workspace(Some(other_repo.clone()))
            .expect_err("unrelated attach should fail");

        assert!(
            error
                .to_string()
                .contains("separate initialization is required")
        );
        assert!(error.to_string().contains("cargo run -- init"));
        assert!(
            error
                .to_string()
                .contains(&other_repo.display().to_string())
        );
    }

    #[test]
    fn initialize_repository_bootstraps_local_storage_before_writing_registry() {
        let test_dir = TestDir::new("init-bootstrap-storage");
        let local_root = test_dir.path.join("folder");
        fs::create_dir_all(&local_root).expect("local folder should be created");
        let blob_root = test_dir.path.join(".tokenizor");

        let report = application_context(blob_root.clone())
            .initialize_repository(Some(local_root.clone()))
            .expect("initialization should succeed");

        assert_eq!(
            report.repository.record.kind,
            crate::domain::RepositoryKind::Local
        );
        assert_eq!(
            report.repository.record.status,
            crate::domain::RepositoryStatus::Ready
        );
        assert!(blob_root.join("blobs").join("sha256").exists());
        assert!(blob_root.join("temp").exists());
        assert!(report.deployment.checks.iter().any(|check| {
            check.name == "blob_store" && check.category == HealthIssueCategory::Storage
        }));
    }

    #[test]
    fn loads_default_empty_registry_when_missing() {
        let test_dir = TestDir::new("init-empty-registry");
        let snapshot = load_snapshot(&registry_path(&test_dir.path)).expect("snapshot should load");

        assert_eq!(
            snapshot,
            RegistrySnapshot {
                schema_version: 2,
                registry_kind: RegistryKind::LocalBootstrapProjectWorkspace,
                authority_mode: AuthorityMode::LocalBootstrapOnly,
                control_plane_backend: "unknown".to_string(),
                repositories: Default::default(),
                workspaces: Default::default(),
            }
        );
    }

    #[test]
    fn inspect_registry_returns_explicit_empty_state_when_registry_is_missing() {
        let test_dir = TestDir::new("inspect-empty-state");
        let report = application_context(test_dir.path.clone())
            .inspect_registry()
            .expect("empty registry inspection should succeed");

        assert!(report.empty);
        assert_eq!(report.project_count, 0);
        assert_eq!(report.workspace_count, 0);
        assert_eq!(report.orphan_workspace_count, 0);
        assert!(report.projects.is_empty());
        assert!(report.orphan_workspaces.is_empty());
        assert_eq!(
            report.registry_kind,
            RegistryKind::LocalBootstrapProjectWorkspace
        );
        assert_eq!(report.authority_mode, AuthorityMode::LocalBootstrapOnly);
    }

    #[test]
    fn inspect_registry_groups_projects_and_workspaces_in_stable_order() {
        let test_dir = TestDir::new("inspect-grouped-registry");
        let registry_path = registry_path(&test_dir.path);
        let repo_b = repository_record(
            "repo_b",
            crate::domain::RepositoryKind::Git,
            "C:\\repos\\b",
            "C:\\repos\\b\\.git",
            ProjectIdentityKind::GitCommonDir,
        );
        let repo_a = repository_record(
            "repo_a",
            crate::domain::RepositoryKind::Git,
            "C:\\repos\\a",
            "C:\\repos\\a\\.git",
            ProjectIdentityKind::GitCommonDir,
        );
        let workspace_b2 =
            workspace_record("workspace_b2", &repo_b.repo_id, "C:\\repos\\b\\worktree-z");
        let workspace_b1 =
            workspace_record("workspace_b1", &repo_b.repo_id, "C:\\repos\\b\\worktree-a");
        let workspace_a = workspace_record("workspace_a", &repo_a.repo_id, "C:\\repos\\a");

        save_snapshot(
            &registry_path,
            &registry_snapshot(
                [repo_b.clone(), repo_a.clone()],
                [workspace_b2, workspace_a.clone(), workspace_b1],
            ),
        )
        .expect("snapshot should be saved");

        let report = application_context(test_dir.path.clone())
            .inspect_registry()
            .expect("populated registry inspection should succeed");

        assert!(!report.empty);
        assert_eq!(report.project_count, 2);
        assert_eq!(report.workspace_count, 3);
        assert_eq!(report.orphan_workspace_count, 0);
        assert_eq!(report.projects.len(), 2);
        assert_eq!(report.projects[0].repository.repo_id, repo_a.repo_id);
        assert_eq!(report.projects[0].workspaces, vec![workspace_a]);
        assert_eq!(report.projects[1].repository.repo_id, repo_b.repo_id);
        assert_eq!(
            report.projects[1]
                .workspaces
                .iter()
                .map(|workspace| workspace.workspace_id.as_str())
                .collect::<Vec<_>>(),
            vec!["workspace_b1", "workspace_b2"]
        );
    }

    #[test]
    fn inspect_registry_surfaces_orphan_workspaces_explicitly() {
        let test_dir = TestDir::new("inspect-orphan-workspaces");
        let registry_path = registry_path(&test_dir.path);
        let orphan_workspace =
            workspace_record("workspace_orphan", "repo_missing", "C:\\repos\\missing");

        save_snapshot(
            &registry_path,
            &registry_snapshot([], [orphan_workspace.clone()]),
        )
        .expect("snapshot should be saved");

        let report = application_context(test_dir.path.clone())
            .inspect_registry()
            .expect("registry inspection should succeed");

        assert!(!report.empty);
        assert_eq!(report.project_count, 0);
        assert_eq!(report.workspace_count, 1);
        assert_eq!(report.orphan_workspace_count, 1);
        assert!(report.projects.is_empty());
        assert_eq!(report.orphan_workspaces, vec![orphan_workspace]);
    }

    #[test]
    fn resolve_context_request_uses_current_directory_when_no_override_is_provided() {
        let test_dir = TestDir::new("resolve-current-directory-request");
        let request = resolve_context_request(None, &test_dir.path)
            .expect("current directory request should resolve");

        assert_eq!(request.requested_path, test_dir.path);
        assert_eq!(
            request.resolution_mode,
            ContextResolutionMode::CurrentDirectory
        );
    }

    #[test]
    fn resolve_active_context_uses_explicit_override_deterministically() {
        let test_dir = TestDir::new("resolve-explicit-override");
        let workspace_root = test_dir.path.join("repo");
        create_git_repo(&workspace_root);
        let nested_dir = workspace_root.join("src");
        let blob_root = test_dir.path.join(".tokenizor");

        application_context(blob_root.clone())
            .initialize_repository(Some(workspace_root.clone()))
            .expect("workspace registration should succeed");

        let report = application_context(blob_root)
            .resolve_active_context(Some(nested_dir.clone()))
            .expect("explicit override should resolve");

        assert_eq!(report.requested_path, nested_dir.display().to_string());
        assert_eq!(
            report.resolution_mode,
            ContextResolutionMode::ExplicitOverride
        );
        assert_eq!(
            report.workspace.root_uri,
            workspace_root.display().to_string()
        );
        assert_eq!(
            report.repository.root_uri,
            workspace_root.display().to_string()
        );
    }

    #[test]
    fn resolve_active_context_returns_attached_worktree_under_shared_project() {
        let test_dir = TestDir::new("resolve-attached-worktree");
        let repo_root = test_dir.path.join("repo");
        let worktree_root = test_dir.path.join("repo-feature");
        let nested_dir = worktree_root.join("src");
        let blob_root = test_dir.path.join(".tokenizor");
        create_git_repo(&repo_root);
        create_git_worktree(&worktree_root, &repo_root.join(".git"), "feature");

        application_context(blob_root.clone())
            .initialize_repository(Some(repo_root.clone()))
            .expect("primary repo initialization should succeed");
        application_context(blob_root.clone())
            .attach_workspace(Some(worktree_root.clone()))
            .expect("worktree attachment should succeed");

        let report = application_context(blob_root)
            .resolve_active_context(Some(nested_dir.clone()))
            .expect("attached worktree context should resolve");

        assert_eq!(report.requested_path, nested_dir.display().to_string());
        assert_eq!(
            report.resolution_mode,
            ContextResolutionMode::ExplicitOverride
        );
        assert_eq!(report.repository.root_uri, repo_root.display().to_string());
        assert_eq!(
            report.workspace.root_uri,
            worktree_root.display().to_string()
        );
        assert_eq!(
            report.repository.project_identity,
            repo_root.join(".git").display().to_string()
        );
    }

    #[test]
    fn resolve_active_context_uses_current_directory_end_to_end() {
        let _lock = current_dir_test_lock()
            .lock()
            .expect("current directory test lock should be available");
        let test_dir = TestDir::new("resolve-current-directory-end-to-end");
        let workspace_root = test_dir.path.join("repo");
        create_git_repo(&workspace_root);
        let nested_dir = workspace_root.join("src");
        let blob_root = test_dir.path.join(".tokenizor");

        application_context(blob_root.clone())
            .initialize_repository(Some(workspace_root.clone()))
            .expect("workspace registration should succeed");

        let _cwd_guard = CurrentDirGuard::set(&nested_dir);
        let report = application_context(blob_root)
            .resolve_active_context(None)
            .expect("current directory should resolve");

        assert_eq!(report.requested_path, nested_dir.display().to_string());
        assert_eq!(
            report.resolution_mode,
            ContextResolutionMode::CurrentDirectory
        );
        assert_eq!(
            report.workspace.root_uri,
            workspace_root.display().to_string()
        );
        assert_eq!(
            report.repository.root_uri,
            workspace_root.display().to_string()
        );
    }

    #[test]
    fn resolve_active_context_reports_unknown_override() {
        let test_dir = TestDir::new("resolve-unknown-override");
        let workspace_root = test_dir.path.join("repo");
        create_git_repo(&workspace_root);
        let unknown_dir = test_dir.path.join("other");
        fs::create_dir_all(&unknown_dir).expect("unknown directory should exist");
        let blob_root = test_dir.path.join(".tokenizor");

        application_context(blob_root.clone())
            .initialize_repository(Some(workspace_root))
            .expect("workspace registration should succeed");

        let error = application_context(blob_root)
            .resolve_active_context(Some(unknown_dir.clone()))
            .expect_err("unknown override should fail");

        assert!(
            error
                .to_string()
                .contains("is not within a registered workspace")
        );
        assert!(
            error
                .to_string()
                .contains(&unknown_dir.display().to_string())
        );
    }

    #[test]
    fn resolve_active_context_reports_nonexistent_explicit_override_deterministically() {
        let test_dir = TestDir::new("resolve-nonexistent-override");
        let workspace_root = test_dir.path.join("repo");
        create_git_repo(&workspace_root);
        let missing_dir = test_dir.path.join("missing");
        let blob_root = test_dir.path.join(".tokenizor");

        application_context(blob_root.clone())
            .initialize_repository(Some(workspace_root))
            .expect("workspace registration should succeed");

        let error = application_context(blob_root)
            .resolve_active_context(Some(missing_dir.clone()))
            .expect_err("nonexistent override should fail deterministically");

        assert!(error.to_string().contains("explicit override"));
        assert!(
            error
                .to_string()
                .contains("is not within a registered workspace")
        );
        assert!(
            error
                .to_string()
                .contains(&missing_dir.display().to_string())
        );
    }

    #[test]
    fn resolve_active_context_reports_conflicting_workspace_matches() {
        let test_dir = TestDir::new("resolve-conflicting-workspaces");
        let blob_root = test_dir.path.join(".tokenizor");
        let registry_path = registry_path(&blob_root);
        let root_workspace_path = test_dir.path.join("repo");
        let nested_workspace_path = root_workspace_path.join("nested");
        fs::create_dir_all(&nested_workspace_path).expect("workspace directories should exist");
        let conflicting_path = nested_workspace_path.join("child");
        fs::create_dir_all(&conflicting_path).expect("conflicting path should exist");

        save_snapshot(
            &registry_path,
            &registry_snapshot(
                [repository_record(
                    "repo_a",
                    crate::domain::RepositoryKind::Local,
                    &root_workspace_path.display().to_string(),
                    &root_workspace_path.display().to_string(),
                    ProjectIdentityKind::LocalRootPath,
                )],
                [
                    workspace_record(
                        "workspace_root",
                        "repo_a",
                        &root_workspace_path.display().to_string(),
                    ),
                    workspace_record(
                        "workspace_nested",
                        "repo_a",
                        &nested_workspace_path.display().to_string(),
                    ),
                ],
            ),
        )
        .expect("snapshot should be saved");

        let error = application_context(blob_root)
            .resolve_active_context(Some(conflicting_path.clone()))
            .expect_err("conflicting workspaces should fail");

        assert!(
            error
                .to_string()
                .contains("conflicts with multiple registered workspaces")
        );
        assert!(error.to_string().contains("workspace_root"));
        assert!(error.to_string().contains("workspace_nested"));
        assert!(
            error
                .to_string()
                .contains(&root_workspace_path.display().to_string())
        );
        assert!(
            error
                .to_string()
                .contains(&nested_workspace_path.display().to_string())
        );
    }

    #[test]
    fn attach_workspace_reports_ambiguous_candidate_project_and_workspace_paths() {
        let test_dir = TestDir::new("attach-ambiguous-project");
        let blob_root = test_dir.path.join(".tokenizor");
        let registry_path = registry_path(&blob_root);
        let repo_root = test_dir.path.join("repo");
        let existing_worktree = test_dir.path.join("repo-existing");
        let requested_worktree = test_dir.path.join("repo-requested");
        create_git_repo(&repo_root);
        create_git_worktree(&existing_worktree, &repo_root.join(".git"), "existing");
        create_git_worktree(&requested_worktree, &repo_root.join(".git"), "requested");

        let project_identity = repo_root.join(".git").display().to_string();
        save_snapshot(
            &registry_path,
            &registry_snapshot(
                [
                    repository_record(
                        "repo_primary",
                        crate::domain::RepositoryKind::Git,
                        &repo_root.display().to_string(),
                        &project_identity,
                        ProjectIdentityKind::GitCommonDir,
                    ),
                    repository_record(
                        "repo_duplicate",
                        crate::domain::RepositoryKind::Git,
                        &existing_worktree.display().to_string(),
                        &project_identity,
                        ProjectIdentityKind::GitCommonDir,
                    ),
                ],
                [
                    workspace_record(
                        "workspace_primary",
                        "repo_primary",
                        &repo_root.display().to_string(),
                    ),
                    workspace_record(
                        "workspace_existing",
                        "repo_duplicate",
                        &existing_worktree.display().to_string(),
                    ),
                ],
            ),
        )
        .expect("snapshot should be saved");

        let error = application_context(blob_root)
            .attach_workspace(Some(requested_worktree.clone()))
            .expect_err("ambiguous attach should fail");

        assert!(
            error
                .to_string()
                .contains("matches multiple registered projects")
        );
        assert!(error.to_string().contains(&repo_root.display().to_string()));
        assert!(
            error
                .to_string()
                .contains(&existing_worktree.display().to_string())
        );
        assert!(
            error
                .to_string()
                .contains(&requested_worktree.display().to_string())
        );
    }

    #[test]
    fn loads_legacy_registry_snapshots_without_hidden_identity_migration() {
        let test_dir = TestDir::new("init-legacy-snapshot");
        let registry_path = registry_path(&test_dir.path);
        write_snapshot_json(
            &registry_path,
            json!({
                "schema_version": 1,
                "repositories": {
                    "repo_legacy": {
                        "repo_id": "repo_legacy",
                        "kind": "git",
                        "root_uri": "C:\\legacy\\repo",
                        "default_branch": null,
                        "last_known_revision": null,
                        "status": "ready"
                    }
                },
                "workspaces": {}
            }),
        );

        let snapshot = load_snapshot(&registry_path).expect("legacy snapshot should load");
        let repository = snapshot
            .repositories
            .get("repo_legacy")
            .expect("legacy repository should exist");

        assert_eq!(snapshot.schema_version, 1);
        assert_eq!(
            snapshot.registry_kind,
            RegistryKind::LocalBootstrapProjectWorkspace
        );
        assert_eq!(snapshot.authority_mode, AuthorityMode::LocalBootstrapOnly);
        assert_eq!(snapshot.control_plane_backend, "unknown");
        assert!(repository.project_identity.is_empty());
        assert_eq!(
            repository.project_identity_kind,
            ProjectIdentityKind::LegacyRootUri
        );
    }

    #[test]
    fn migrate_upgrades_legacy_git_registration_when_current_path_exists() {
        let test_dir = TestDir::new("migrate-legacy-existing-root");
        let blob_root = test_dir.path.join(".tokenizor");
        let registry_path = registry_path(&blob_root);
        let repo_root = test_dir.path.join("repo");
        create_git_repo(&repo_root);

        write_snapshot_json(
            &registry_path,
            json!({
                "schema_version": 1,
                "repositories": {
                    "repo_legacy": {
                        "repo_id": "repo_legacy",
                        "kind": "git",
                        "root_uri": repo_root.display().to_string(),
                        "default_branch": null,
                        "last_known_revision": null,
                        "status": "ready"
                    }
                },
                "workspaces": {
                    "workspace_legacy": {
                        "workspace_id": "workspace_legacy",
                        "repo_id": "repo_legacy",
                        "root_uri": repo_root.display().to_string(),
                        "status": "active"
                    }
                }
            }),
        );

        let report = application_context(blob_root.clone())
            .migrate_registry(None, None)
            .expect("migration should succeed");

        assert!(report.is_successful());
        assert!(
            report
                .migrated
                .iter()
                .any(|record| record.entity_id == "repo_legacy")
        );

        let snapshot = load_snapshot(&registry_path).expect("migrated snapshot should load");
        let repository = snapshot
            .repositories
            .get("repo_legacy")
            .expect("migrated repository should exist");
        assert_eq!(snapshot.schema_version, 2);
        assert_eq!(
            repository.project_identity,
            repo_root.join(".git").display().to_string()
        );
        assert_eq!(
            repository.project_identity_kind,
            ProjectIdentityKind::GitCommonDir
        );
    }

    #[test]
    fn migrate_uses_surviving_worktree_to_upgrade_missing_legacy_repository_identity() {
        let test_dir = TestDir::new("migrate-legacy-worktree-evidence");
        let blob_root = test_dir.path.join(".tokenizor");
        let registry_path = registry_path(&blob_root);
        let repo_root = test_dir.path.join("repo");
        let worktree_root = test_dir.path.join("repo-feature");
        let missing_root = test_dir.path.join("missing-primary");
        create_git_repo(&repo_root);
        create_git_worktree(&worktree_root, &repo_root.join(".git"), "feature");

        write_snapshot_json(
            &registry_path,
            json!({
                "schema_version": 1,
                "repositories": {
                    "repo_legacy": {
                        "repo_id": "repo_legacy",
                        "kind": "git",
                        "root_uri": missing_root.display().to_string(),
                        "default_branch": null,
                        "last_known_revision": null,
                        "status": "ready"
                    }
                },
                "workspaces": {
                    "workspace_feature": {
                        "workspace_id": "workspace_feature",
                        "repo_id": "repo_legacy",
                        "root_uri": worktree_root.display().to_string(),
                        "status": "active"
                    }
                }
            }),
        );

        let report = application_context(blob_root.clone())
            .migrate_registry(None, None)
            .expect("migration should succeed");

        assert!(report.is_successful());
        assert!(
            report
                .migrated
                .iter()
                .any(|record| record.entity_id == "repo_legacy")
        );

        let snapshot = load_snapshot(&registry_path).expect("migrated snapshot should load");
        let repository = snapshot
            .repositories
            .get("repo_legacy")
            .expect("migrated repository should exist");
        assert_eq!(
            repository.project_identity,
            repo_root.join(".git").display().to_string()
        );
        assert_eq!(repository.root_uri, repo_root.display().to_string());
    }

    #[test]
    fn migrate_reports_unresolved_when_legacy_identity_is_ambiguous() {
        let test_dir = TestDir::new("migrate-legacy-ambiguous");
        let blob_root = test_dir.path.join(".tokenizor");
        let registry_path = registry_path(&blob_root);
        let repo_a = test_dir.path.join("repo-a");
        let repo_b = test_dir.path.join("repo-b");
        let missing_root = test_dir.path.join("missing-primary");
        create_git_repo(&repo_a);
        create_git_repo(&repo_b);

        write_snapshot_json(
            &registry_path,
            json!({
                "schema_version": 1,
                "repositories": {
                    "repo_legacy": {
                        "repo_id": "repo_legacy",
                        "kind": "git",
                        "root_uri": missing_root.display().to_string(),
                        "default_branch": null,
                        "last_known_revision": null,
                        "status": "ready"
                    }
                },
                "workspaces": {
                    "workspace_a": {
                        "workspace_id": "workspace_a",
                        "repo_id": "repo_legacy",
                        "root_uri": repo_a.display().to_string(),
                        "status": "active"
                    },
                    "workspace_b": {
                        "workspace_id": "workspace_b",
                        "repo_id": "repo_legacy",
                        "root_uri": repo_b.display().to_string(),
                        "status": "active"
                    }
                }
            }),
        );

        let report = application_context(blob_root)
            .migrate_registry(None, None)
            .expect("migration report should be produced");

        assert!(!report.is_successful());
        assert!(
            report
                .unresolved
                .iter()
                .any(|issue| issue.entity_id.as_deref() == Some("repo_legacy"))
        );

        let snapshot = load_snapshot(&registry_path).expect("snapshot should still load");
        let repository = snapshot
            .repositories
            .get("repo_legacy")
            .expect("legacy repository should remain");
        assert!(repository.project_identity.is_empty());
    }

    #[test]
    fn migrate_updates_workspace_path_from_explicit_operator_mapping() {
        let test_dir = TestDir::new("migrate-explicit-workspace-update");
        let blob_root = test_dir.path.join(".tokenizor");
        let registry_path = registry_path(&blob_root);
        let old_root = test_dir.path.join("old-local");
        let new_root = test_dir.path.join("new-local");
        fs::create_dir_all(&new_root).expect("new local root should exist");

        write_snapshot_json(
            &registry_path,
            json!({
                "schema_version": 2,
                "registry_kind": "local_bootstrap_project_workspace",
                "authority_mode": "local_bootstrap_only",
                "control_plane_backend": "in_memory",
                "repositories": {
                    "repo_local": {
                        "repo_id": "repo_local",
                        "kind": "local",
                        "root_uri": old_root.display().to_string(),
                        "project_identity": old_root.display().to_string(),
                        "project_identity_kind": "local_root_path",
                        "default_branch": null,
                        "last_known_revision": null,
                        "status": "ready"
                    }
                },
                "workspaces": {
                    "workspace_old": {
                        "workspace_id": "workspace_old",
                        "repo_id": "repo_local",
                        "root_uri": old_root.display().to_string(),
                        "status": "active"
                    }
                }
            }),
        );

        let report = application_context(blob_root.clone())
            .migrate_registry(Some(old_root.clone()), Some(new_root.clone()))
            .expect("explicit migration should succeed");

        assert!(report.is_successful());
        assert_eq!(report.summary.unresolved, 0);

        let snapshot = load_snapshot(&registry_path).expect("updated snapshot should load");
        let repository = snapshot
            .repositories
            .get("repo_local")
            .expect("local repository should remain");
        let expected_root = new_root.display().to_string();
        let expected_workspace_id = workspace_id_for_root_uri(&expected_root);
        let workspace = snapshot
            .workspaces
            .get(&expected_workspace_id)
            .expect("workspace should be re-keyed to the new path");

        assert_eq!(repository.root_uri, expected_root);
        assert_eq!(repository.project_identity, new_root.display().to_string());
        assert_eq!(workspace.root_uri, new_root.display().to_string());
    }

    #[test]
    fn migrate_is_idempotent_for_equivalent_update_request() {
        let test_dir = TestDir::new("migrate-explicit-idempotent");
        let blob_root = test_dir.path.join(".tokenizor");
        let registry_path = registry_path(&blob_root);
        let old_root = test_dir.path.join("old-local");
        let new_root = test_dir.path.join("new-local");
        fs::create_dir_all(&new_root).expect("new local root should exist");

        write_snapshot_json(
            &registry_path,
            json!({
                "schema_version": 2,
                "registry_kind": "local_bootstrap_project_workspace",
                "authority_mode": "local_bootstrap_only",
                "control_plane_backend": "in_memory",
                "repositories": {
                    "repo_local": {
                        "repo_id": "repo_local",
                        "kind": "local",
                        "root_uri": old_root.display().to_string(),
                        "project_identity": old_root.display().to_string(),
                        "project_identity_kind": "local_root_path",
                        "default_branch": null,
                        "last_known_revision": null,
                        "status": "ready"
                    }
                },
                "workspaces": {
                    "workspace_old": {
                        "workspace_id": "workspace_old",
                        "repo_id": "repo_local",
                        "root_uri": old_root.display().to_string(),
                        "status": "active"
                    }
                }
            }),
        );

        let first_report = application_context(blob_root.clone())
            .migrate_registry(Some(old_root.clone()), Some(new_root.clone()))
            .expect("first migration should succeed");
        let second_report = application_context(blob_root.clone())
            .migrate_registry(Some(old_root), Some(new_root.clone()))
            .expect("second migration should report unchanged state");

        assert!(first_report.is_successful());
        assert!(second_report.is_successful());
        assert_eq!(second_report.summary.updated, 0);
        assert!(
            second_report
                .unchanged
                .iter()
                .any(|record| record.detail.contains("already applied"))
        );

        let snapshot = load_snapshot(&registry_path).expect("updated snapshot should load");
        assert!(
            snapshot
                .workspaces
                .contains_key(&workspace_id_for_root_uri(&new_root.display().to_string()))
        );
    }

    #[test]
    fn initialize_repository_requires_explicit_migration_for_matching_legacy_git_state() {
        let test_dir = TestDir::new("init-requires-migrate");
        let blob_root = test_dir.path.join(".tokenizor");
        let registry_path = registry_path(&blob_root);
        let repo_root = test_dir.path.join("repo");
        create_git_repo(&repo_root);

        write_snapshot_json(
            &registry_path,
            json!({
                "schema_version": 1,
                "repositories": {
                    "repo_legacy": {
                        "repo_id": "repo_legacy",
                        "kind": "git",
                        "root_uri": repo_root.display().to_string(),
                        "default_branch": null,
                        "last_known_revision": null,
                        "status": "ready"
                    }
                },
                "workspaces": {
                    "workspace_legacy": {
                        "workspace_id": "workspace_legacy",
                        "repo_id": "repo_legacy",
                        "root_uri": repo_root.display().to_string(),
                        "status": "active"
                    }
                }
            }),
        );

        let error = application_context(blob_root)
            .initialize_repository(Some(repo_root.clone()))
            .expect_err("legacy registry state should require explicit migration");

        assert!(error.to_string().contains("cargo run -- migrate"));
        assert!(error.to_string().contains(&repo_root.display().to_string()));
    }

    #[test]
    fn concurrent_initialization_preserves_all_registrations() {
        let test_dir = TestDir::new("init-concurrent-registrations");
        let repo_a = test_dir.path.join("repo-a");
        let repo_b = test_dir.path.join("repo-b");
        create_git_repo(&repo_a);
        create_git_repo(&repo_b);
        let blob_root = test_dir.path.join(".tokenizor");
        let barrier = Arc::new(Barrier::new(3));

        let thread_a = {
            let barrier = Arc::clone(&barrier);
            let blob_root = blob_root.clone();
            let repo_a = repo_a.clone();
            thread::spawn(move || {
                barrier.wait();
                application_context(blob_root)
                    .initialize_repository(Some(repo_a))
                    .expect("first concurrent initialization should succeed");
            })
        };
        let thread_b = {
            let barrier = Arc::clone(&barrier);
            let blob_root = blob_root.clone();
            let repo_b = repo_b.clone();
            thread::spawn(move || {
                barrier.wait();
                application_context(blob_root)
                    .initialize_repository(Some(repo_b))
                    .expect("second concurrent initialization should succeed");
            })
        };

        barrier.wait();
        thread_a
            .join()
            .expect("first initialization thread should join");
        thread_b
            .join()
            .expect("second initialization thread should join");

        let snapshot = load_snapshot(&registry_path(&blob_root)).expect("snapshot should load");
        assert_eq!(snapshot.repositories.len(), 2);
        assert_eq!(snapshot.workspaces.len(), 2);
    }

    #[test]
    fn registry_lock_is_released_for_waiting_writer() {
        let test_dir = TestDir::new("init-lock-release");
        let registry_path = registry_path(&test_dir.path);
        let _first_lock = acquire_registry_lock(&registry_path).expect("first lock should acquire");
        let barrier = Arc::new(Barrier::new(2));
        let waiter_path = registry_path.clone();
        let waiter_barrier = Arc::clone(&barrier);
        let waiter = thread::spawn(move || {
            waiter_barrier.wait();
            acquire_registry_lock(&waiter_path)
                .expect("waiting writer should acquire after release");
        });

        barrier.wait();
        thread::sleep(std::time::Duration::from_millis(100));
        assert!(!waiter.is_finished());
        drop(_first_lock);
        waiter.join().expect("waiting writer should join");
        assert!(!lock_path(&registry_path).exists());
    }

    #[test]
    fn migrate_rejects_single_path_argument() {
        let test_dir = TestDir::new("migrate-single-arg");
        let current_dir = test_dir.path.clone();
        let source_only = PathBuf::from("/some/path");

        let error = resolve_migration_request(Some(&source_only), None, &current_dir)
            .expect_err("single source path without target should fail");
        assert!(error
            .to_string()
            .contains("migrate expects either no path arguments"));

        let target_only = PathBuf::from("/other/path");
        let error = resolve_migration_request(None, Some(&target_only), &current_dir)
            .expect_err("single target path without source should fail");
        assert!(error
            .to_string()
            .contains("migrate expects either no path arguments"));
    }
}
