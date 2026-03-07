use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexRun {
    pub run_id: String,
    pub repo_id: String,
    pub mode: IndexRunMode,
    pub status: IndexRunStatus,
    pub requested_at_unix_ms: u64,
    pub started_at_unix_ms: Option<u64>,
    pub finished_at_unix_ms: Option<u64>,
    pub idempotency_key: Option<String>,
    pub request_hash: Option<String>,
    pub checkpoint_cursor: Option<String>,
    pub error_summary: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IndexRunMode {
    Full,
    Incremental,
    Repair,
    Verify,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IndexRunStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Checkpoint {
    pub run_id: String,
    pub cursor: String,
    pub files_processed: u64,
    pub symbols_written: u64,
    pub created_at_unix_ms: u64,
}
