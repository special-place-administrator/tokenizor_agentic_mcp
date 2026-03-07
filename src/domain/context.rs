use serde::{Deserialize, Serialize};

use super::{AuthorityMode, RegistryKind, Repository, Workspace, unix_timestamp_ms};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextResolutionMode {
    CurrentDirectory,
    ExplicitOverride,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActiveWorkspaceContext {
    pub resolved_at_unix_ms: u64,
    pub requested_path: String,
    pub resolution_mode: ContextResolutionMode,
    pub registry_path: String,
    pub registry_kind: RegistryKind,
    pub authority_mode: AuthorityMode,
    pub control_plane_backend: String,
    pub repository: Repository,
    pub workspace: Workspace,
}

impl ActiveWorkspaceContext {
    pub fn new(
        requested_path: impl Into<String>,
        resolution_mode: ContextResolutionMode,
        registry_path: impl Into<String>,
        registry_kind: RegistryKind,
        authority_mode: AuthorityMode,
        control_plane_backend: impl Into<String>,
        repository: Repository,
        workspace: Workspace,
    ) -> Self {
        Self {
            resolved_at_unix_ms: unix_timestamp_ms(),
            requested_path: requested_path.into(),
            resolution_mode,
            registry_path: registry_path.into(),
            registry_kind,
            authority_mode,
            control_plane_backend: control_plane_backend.into(),
            repository,
            workspace,
        }
    }
}
