mod context;
mod health;
mod idempotency;
mod index;
mod init;
mod migration;
mod registry;
mod repository;
mod retrieval;
mod workspace;

pub use context::{ActiveWorkspaceContext, ContextResolutionMode};
pub use health::{
    ComponentHealth, DeploymentReport, HealthIssueCategory, HealthReport, HealthSeverity,
    HealthStatus, ServiceIdentity, aggregate_status, unix_timestamp_ms,
};
pub use idempotency::{IdempotencyRecord, IdempotencyStatus};
pub use index::{
    Checkpoint, DiscoveryManifest, FileOutcome, FileOutcomeSummary, FileProcessingResult,
    FileRecord, IndexRun, IndexRunMode, IndexRunStatus, LanguageId, PersistedFileOutcome,
    RecoveryStateKind, ResumeRejectReason, ResumeRunOutcome, RunHealth, RunPhase,
    RunProgressSnapshot, RunRecoveryState, RunStatusReport, SupportTier, SymbolKind, SymbolRecord,
};
pub use init::{InitializationReport, RegistrationAction, RegistrationResult};
pub use migration::{
    MigrationEntityKind, MigrationIssue, MigrationMode, MigrationRecord, MigrationReport,
    MigrationRequest, MigrationSummary,
};
pub use registry::{AuthorityMode, RegisteredProject, RegistryKind, RegistryView};
pub use repository::{
    InvalidationResult, ProjectIdentityKind, Repository, RepositoryKind, RepositoryStatus,
};
pub use retrieval::{
    BatchRetrievalRequest, BatchRetrievalResponseData, BatchRetrievalResultItem, CodeSliceRequest,
    FileOutcomeStatus, FileOutlineResponse, GetSymbolsResponse, NextAction, OutlineSymbol,
    Provenance, RepoOutlineCoverage, RepoOutlineEntry, RepoOutlineResponse, RequestGateError,
    ResultEnvelope, RetrievalOutcome, SearchResultItem, SymbolCoverage, SymbolRequest,
    SymbolResultItem, SymbolSearchResponse, TrustLevel, VerifiedCodeSliceResponse,
    VerifiedSourceResponse,
};
pub use workspace::{Workspace, WorkspaceStatus};
