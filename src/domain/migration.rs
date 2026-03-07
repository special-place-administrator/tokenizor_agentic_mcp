use serde::{Deserialize, Serialize};

use super::unix_timestamp_ms;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MigrationMode {
    Scan,
    Update,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MigrationEntityKind {
    Repository,
    Workspace,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrationRequest {
    pub mode: MigrationMode,
    pub source_path: Option<String>,
    pub target_path: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrationRecord {
    pub entity_kind: MigrationEntityKind,
    pub entity_id: String,
    pub previous_path: Option<String>,
    pub current_path: Option<String>,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrationIssue {
    pub entity_kind: MigrationEntityKind,
    pub entity_id: Option<String>,
    pub path: Option<String>,
    pub detail: String,
    pub guidance: String,
    pub candidate_paths: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrationSummary {
    pub migrated: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub unresolved: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrationReport {
    pub migrated_at_unix_ms: u64,
    pub registry_path: String,
    pub request: MigrationRequest,
    pub changed: bool,
    pub summary: MigrationSummary,
    pub migrated: Vec<MigrationRecord>,
    pub updated: Vec<MigrationRecord>,
    pub unchanged: Vec<MigrationRecord>,
    pub unresolved: Vec<MigrationIssue>,
}

impl MigrationReport {
    pub fn new(
        registry_path: impl Into<String>,
        request: MigrationRequest,
        changed: bool,
        migrated: Vec<MigrationRecord>,
        updated: Vec<MigrationRecord>,
        unchanged: Vec<MigrationRecord>,
        unresolved: Vec<MigrationIssue>,
    ) -> Self {
        let summary = MigrationSummary {
            migrated: migrated.len(),
            updated: updated.len(),
            unchanged: unchanged.len(),
            unresolved: unresolved.len(),
        };

        Self {
            migrated_at_unix_ms: unix_timestamp_ms(),
            registry_path: registry_path.into(),
            request,
            changed,
            summary,
            migrated,
            updated,
            unchanged,
            unresolved,
        }
    }

    /// Returns `true` when no unresolved items remain — meaning every record was
    /// either migrated, updated, or confirmed unchanged. A report where `changed`
    /// is `false` and all counts are zero still counts as successful because there
    /// is nothing for the operator to act on. Use the `changed` field and `summary`
    /// counts to distinguish "nothing to do" from "work was performed."
    pub fn is_successful(&self) -> bool {
        self.unresolved.is_empty()
    }
}
