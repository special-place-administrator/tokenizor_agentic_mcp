use serde::{Deserialize, Serialize};

use super::{Repository, Workspace, unix_timestamp_ms};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RegistryKind {
    #[default]
    LocalBootstrapProjectWorkspace,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityMode {
    #[default]
    LocalBootstrapOnly,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegisteredProject {
    pub repository: Repository,
    pub workspaces: Vec<Workspace>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegistryView {
    pub inspected_at_unix_ms: u64,
    pub registry_path: String,
    pub registry_kind: RegistryKind,
    pub authority_mode: AuthorityMode,
    pub control_plane_backend: String,
    pub empty: bool,
    pub project_count: usize,
    pub workspace_count: usize,
    pub orphan_workspace_count: usize,
    pub projects: Vec<RegisteredProject>,
    pub orphan_workspaces: Vec<Workspace>,
}

impl RegistryView {
    pub fn new(
        registry_path: impl Into<String>,
        registry_kind: RegistryKind,
        authority_mode: AuthorityMode,
        control_plane_backend: impl Into<String>,
        projects: Vec<RegisteredProject>,
        orphan_workspaces: Vec<Workspace>,
    ) -> Self {
        let project_count = projects.len();
        let workspace_count = projects
            .iter()
            .map(|project| project.workspaces.len())
            .sum::<usize>()
            + orphan_workspaces.len();
        let orphan_workspace_count = orphan_workspaces.len();
        let empty = project_count == 0 && orphan_workspace_count == 0;

        Self {
            inspected_at_unix_ms: unix_timestamp_ms(),
            registry_path: registry_path.into(),
            registry_kind,
            authority_mode,
            control_plane_backend: control_plane_backend.into(),
            empty,
            project_count,
            workspace_count,
            orphan_workspace_count,
            projects,
            orphan_workspaces,
        }
    }
}
