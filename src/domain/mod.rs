mod context;
mod health;
mod idempotency;
mod index;
mod init;
mod migration;
mod registry;
mod repository;
mod workspace;

pub use context::{ActiveWorkspaceContext, ContextResolutionMode};
pub use health::{
    ComponentHealth, DeploymentReport, HealthIssueCategory, HealthReport, HealthSeverity,
    HealthStatus, ServiceIdentity, aggregate_status, unix_timestamp_ms,
};
pub use idempotency::{IdempotencyRecord, IdempotencyStatus};
pub use index::{Checkpoint, IndexRun, IndexRunMode, IndexRunStatus};
pub use init::{InitializationReport, RegistrationAction, RegistrationResult};
pub use migration::{
    MigrationEntityKind, MigrationIssue, MigrationMode, MigrationRecord, MigrationReport,
    MigrationRequest, MigrationSummary,
};
pub use registry::{AuthorityMode, RegisteredProject, RegistryKind, RegistryView};
pub use repository::{ProjectIdentityKind, Repository, RepositoryKind, RepositoryStatus};
pub use workspace::{Workspace, WorkspaceStatus};
