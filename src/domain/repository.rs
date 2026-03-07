use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Repository {
    pub repo_id: String,
    pub kind: RepositoryKind,
    pub root_uri: String,
    #[serde(default)]
    pub project_identity: String,
    #[serde(default)]
    pub project_identity_kind: ProjectIdentityKind,
    pub default_branch: Option<String>,
    pub last_known_revision: Option<String>,
    pub status: RepositoryStatus,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectIdentityKind {
    #[default]
    LegacyRootUri,
    LocalRootPath,
    GitCommonDir,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryKind {
    Local,
    Git,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryStatus {
    Pending,
    Ready,
    Degraded,
    Failed,
}
