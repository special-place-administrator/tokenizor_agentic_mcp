use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{IndexRunStatus, LanguageId, PersistedFileOutcome, SymbolKind};

/// Actionable guidance for blocked, quarantined, or gated responses.
/// Shared vocabulary with Epic 4 repair flows.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NextAction {
    Resume,
    Reindex,
    Repair,
    Wait,
    ResolveContext,
}

impl fmt::Display for NextAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Resume => "resume",
            Self::Reindex => "reindex",
            Self::Repair => "repair",
            Self::Wait => "wait",
            Self::ResolveContext => "resolve_context",
        };
        write!(f, "{value}")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalOutcome {
    Success,
    Empty,
    Missing,
    NotIndexed,
    Stale,
    Quarantined,
    Blocked { reason: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevel {
    Verified,
    Unverified,
    Suspect,
    Quarantined,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Provenance {
    pub run_id: String,
    pub committed_at_unix_ms: u64,
    pub repo_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResultEnvelope<T> {
    pub outcome: RetrievalOutcome,
    pub trust: TrustLevel,
    pub provenance: Option<Provenance>,
    pub data: Option<T>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_action: Option<NextAction>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RequestGateError {
    NoActiveContext,
    RepositoryInvalidated { reason: Option<String> },
    RepositoryFailed,
    RepositoryDegraded,
    RepositoryQuarantined { reason: Option<String> },
    ActiveMutation { run_id: String },
    NeverIndexed,
    NoSuccessfulRuns { latest_status: IndexRunStatus },
}

impl RequestGateError {
    pub fn next_action(&self) -> NextAction {
        match self {
            Self::NoActiveContext => NextAction::ResolveContext,
            Self::RepositoryInvalidated { .. } => NextAction::Reindex,
            Self::RepositoryFailed => NextAction::Repair,
            Self::RepositoryDegraded => NextAction::Repair,
            Self::RepositoryQuarantined { .. } => NextAction::Repair,
            Self::ActiveMutation { .. } => NextAction::Wait,
            Self::NeverIndexed => NextAction::Reindex,
            Self::NoSuccessfulRuns { .. } => NextAction::Wait,
        }
    }
}

impl fmt::Display for RequestGateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoActiveContext => write!(f, "repository not found in registry"),
            Self::RepositoryInvalidated { reason } => match reason {
                Some(r) => write!(f, "repository invalidated: {r}"),
                None => write!(f, "repository invalidated"),
            },
            Self::RepositoryFailed => write!(f, "repository in failed state"),
            Self::RepositoryDegraded => write!(f, "repository in degraded state"),
            Self::RepositoryQuarantined { reason } => match reason {
                Some(r) => write!(f, "repository quarantined: {r}"),
                None => write!(f, "repository quarantined"),
            },
            Self::ActiveMutation { run_id } => {
                write!(f, "active mutation in progress (run: {run_id})")
            }
            Self::NeverIndexed => write!(f, "repository has not been indexed"),
            Self::NoSuccessfulRuns { latest_status } => {
                write!(
                    f,
                    "no successful index exists (latest run status: {latest_status:?})"
                )
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchResultItem {
    pub relative_path: String,
    pub language: LanguageId,
    pub line_number: u32,
    pub line_content: String,
    pub match_offset: u32,
    pub match_length: u32,
    pub provenance: Provenance,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SymbolResultItem {
    pub symbol_name: String,
    pub symbol_kind: SymbolKind,
    pub relative_path: String,
    pub language: LanguageId,
    pub line_range: (u32, u32),
    pub byte_range: (u32, u32),
    pub depth: u32,
    pub provenance: Provenance,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SymbolCoverage {
    pub files_searched: u32,
    pub files_without_symbols: u32,
    pub files_skipped_quarantined: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SymbolSearchResponse {
    pub matches: Vec<SymbolResultItem>,
    pub coverage: SymbolCoverage,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutlineSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub line_range: (u32, u32),
    pub byte_range: (u32, u32),
    pub depth: u32,
    pub sort_order: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileOutlineResponse {
    pub relative_path: String,
    pub language: LanguageId,
    pub byte_len: u64,
    pub symbols: Vec<OutlineSymbol>,
    pub has_symbol_support: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileOutcomeStatus {
    Committed,
    EmptySymbols,
    Failed,
    Quarantined,
}

impl From<&PersistedFileOutcome> for FileOutcomeStatus {
    fn from(value: &PersistedFileOutcome) -> Self {
        match value {
            PersistedFileOutcome::Committed => Self::Committed,
            PersistedFileOutcome::EmptySymbols => Self::EmptySymbols,
            PersistedFileOutcome::Failed { .. } => Self::Failed,
            PersistedFileOutcome::Quarantined { .. } => Self::Quarantined,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepoOutlineEntry {
    pub relative_path: String,
    pub language: LanguageId,
    pub byte_len: u64,
    pub symbol_count: u32,
    pub status: FileOutcomeStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepoOutlineCoverage {
    pub total_files: u32,
    pub files_with_symbols: u32,
    pub files_without_symbols: u32,
    pub files_quarantined: u32,
    pub files_failed: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepoOutlineResponse {
    pub files: Vec<RepoOutlineEntry>,
    pub coverage: RepoOutlineCoverage,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_gate_error_next_action_mapping() {
        assert_eq!(
            RequestGateError::NoActiveContext.next_action(),
            NextAction::ResolveContext,
        );
        assert_eq!(
            RequestGateError::RepositoryInvalidated { reason: None }.next_action(),
            NextAction::Reindex,
        );
        assert_eq!(
            RequestGateError::RepositoryFailed.next_action(),
            NextAction::Repair,
        );
        assert_eq!(
            RequestGateError::RepositoryDegraded.next_action(),
            NextAction::Repair,
        );
        assert_eq!(
            RequestGateError::RepositoryQuarantined { reason: None }.next_action(),
            NextAction::Repair,
        );
        assert_eq!(
            RequestGateError::ActiveMutation { run_id: "r".into() }.next_action(),
            NextAction::Wait,
        );
        assert_eq!(
            RequestGateError::NeverIndexed.next_action(),
            NextAction::Reindex,
        );
        assert_eq!(
            RequestGateError::NoSuccessfulRuns {
                latest_status: IndexRunStatus::Failed
            }
            .next_action(),
            NextAction::Wait,
        );
    }
}

/// A single symbol retrieval target within a batched get_symbols request.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct SymbolRequest {
    pub relative_path: String,
    pub symbol_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind_filter: Option<SymbolKind>,
}

/// A single raw code-slice retrieval target within a batched get_symbols request.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct CodeSliceRequest {
    pub relative_path: String,
    pub byte_range: (u32, u32),
}

/// A single retrieval target within a batched get_symbols request.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(tag = "request_type", rename_all = "snake_case")]
pub enum BatchRetrievalRequest {
    Symbol {
        relative_path: String,
        symbol_name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind_filter: Option<SymbolKind>,
    },
    CodeSlice {
        relative_path: String,
        byte_range: (u32, u32),
    },
}

impl From<SymbolRequest> for BatchRetrievalRequest {
    fn from(value: SymbolRequest) -> Self {
        Self::Symbol {
            relative_path: value.relative_path,
            symbol_name: value.symbol_name,
            kind_filter: value.kind_filter,
        }
    }
}

impl From<CodeSliceRequest> for BatchRetrievalRequest {
    fn from(value: CodeSliceRequest) -> Self {
        Self::CodeSlice {
            relative_path: value.relative_path,
            byte_range: value.byte_range,
        }
    }
}

/// A single item result within a batched get_symbols response.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "request_type", rename_all = "snake_case")]
pub enum BatchRetrievalResultItem {
    Symbol {
        relative_path: String,
        symbol_name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind_filter: Option<SymbolKind>,
        result: ResultEnvelope<BatchRetrievalResponseData>,
    },
    CodeSlice {
        relative_path: String,
        byte_range: (u32, u32),
        result: ResultEnvelope<BatchRetrievalResponseData>,
    },
}

impl BatchRetrievalResultItem {
    pub fn from_request(
        request: &BatchRetrievalRequest,
        result: ResultEnvelope<BatchRetrievalResponseData>,
    ) -> Self {
        match request {
            BatchRetrievalRequest::Symbol {
                relative_path,
                symbol_name,
                kind_filter,
            } => Self::Symbol {
                relative_path: relative_path.clone(),
                symbol_name: symbol_name.clone(),
                kind_filter: *kind_filter,
                result,
            },
            BatchRetrievalRequest::CodeSlice {
                relative_path,
                byte_range,
            } => Self::CodeSlice {
                relative_path: relative_path.clone(),
                byte_range: *byte_range,
                result,
            },
        }
    }
}

/// Response for get_symbols — batch of per-item verified source results.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetSymbolsResponse {
    pub results: Vec<BatchRetrievalResultItem>,
}

/// Response for get_symbol — verified source text for a single symbol
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerifiedSourceResponse {
    pub relative_path: String,
    pub language: LanguageId,
    pub symbol_name: String,
    pub symbol_kind: SymbolKind,
    pub line_range: (u32, u32),
    pub byte_range: (u32, u32),
    pub source: String,
}

/// Response for a raw verified code slice requested by byte range.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerifiedCodeSliceResponse {
    pub relative_path: String,
    pub language: LanguageId,
    pub line_range: (u32, u32),
    pub byte_range: (u32, u32),
    pub source: String,
}

/// Batch retrieval can return either symbol-backed or raw code-slice source data.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum BatchRetrievalResponseData {
    Symbol(VerifiedSourceResponse),
    CodeSlice(VerifiedCodeSliceResponse),
}
