use serde::{Deserialize, Serialize};

use crate::domain::{DeploymentReport, Repository, Workspace, unix_timestamp_ms};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RegistrationAction {
    Created,
    Reused,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegistrationResult<T> {
    pub action: RegistrationAction,
    pub record: T,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InitializationReport {
    pub initialized_at_unix_ms: u64,
    pub target_path: String,
    pub registry_path: String,
    pub repository: RegistrationResult<Repository>,
    pub workspace: RegistrationResult<Workspace>,
    pub deployment: DeploymentReport,
}

impl InitializationReport {
    pub fn new(
        target_path: impl Into<String>,
        registry_path: impl Into<String>,
        repository: RegistrationResult<Repository>,
        workspace: RegistrationResult<Workspace>,
        deployment: DeploymentReport,
    ) -> Self {
        Self {
            initialized_at_unix_ms: unix_timestamp_ms(),
            target_path: target_path.into(),
            registry_path: registry_path.into(),
            repository,
            workspace,
            deployment,
        }
    }

    pub fn is_ready(&self) -> bool {
        self.deployment.is_ready()
    }
}
