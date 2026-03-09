use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::retrieval::NextAction;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum LanguageId {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    Java,
    C,
    Cpp,
    CSharp,
    Ruby,
    Php,
    Swift,
    Dart,
    Perl,
    Elixir,
}

impl LanguageId {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "rs" => Some(Self::Rust),
            "py" => Some(Self::Python),
            "js" | "jsx" => Some(Self::JavaScript),
            "ts" | "tsx" => Some(Self::TypeScript),
            "go" => Some(Self::Go),
            "java" => Some(Self::Java),
            "c" | "h" => Some(Self::C),
            "cpp" | "cxx" | "cc" | "hpp" | "hxx" | "hh" => Some(Self::Cpp),
            "cs" => Some(Self::CSharp),
            "rb" => Some(Self::Ruby),
            "php" => Some(Self::Php),
            "swift" => Some(Self::Swift),
            "dart" => Some(Self::Dart),
            "pl" | "pm" => Some(Self::Perl),
            "ex" | "exs" => Some(Self::Elixir),
            _ => None,
        }
    }

    pub fn extensions(&self) -> &[&str] {
        match self {
            Self::Rust => &["rs"],
            Self::Python => &["py"],
            Self::JavaScript => &["js", "jsx"],
            Self::TypeScript => &["ts", "tsx"],
            Self::Go => &["go"],
            Self::Java => &["java"],
            Self::C => &["c", "h"],
            Self::Cpp => &["cpp", "cxx", "cc", "hpp", "hxx", "hh"],
            Self::CSharp => &["cs"],
            Self::Ruby => &["rb"],
            Self::Php => &["php"],
            Self::Swift => &["swift"],
            Self::Dart => &["dart"],
            Self::Perl => &["pl", "pm"],
            Self::Elixir => &["ex", "exs"],
        }
    }

    pub fn support_tier(&self) -> SupportTier {
        match self {
            Self::Rust | Self::Python | Self::JavaScript | Self::TypeScript | Self::Go => {
                SupportTier::QualityFocus
            }
            Self::Java => SupportTier::Broader,
            Self::C
            | Self::Cpp
            | Self::CSharp
            | Self::Ruby
            | Self::Php
            | Self::Swift
            | Self::Dart
            | Self::Perl
            | Self::Elixir => SupportTier::Unsupported,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SupportTier {
    QualityFocus,
    Broader,
    Unsupported,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileProcessingResult {
    pub relative_path: String,
    pub language: LanguageId,
    pub outcome: FileOutcome,
    pub symbols: Vec<SymbolRecord>,
    pub byte_len: u64,
    pub content_hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileOutcome {
    Processed,
    PartialParse { warning: String },
    Failed { error: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SymbolRecord {
    pub name: String,
    pub kind: SymbolKind,
    pub depth: u32,
    pub sort_order: u32,
    pub byte_range: (u32, u32),
    pub line_range: (u32, u32),
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Struct,
    Enum,
    Interface,
    Module,
    Constant,
    Variable,
    Type,
    Trait,
    Impl,
    Other,
}

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
    #[serde(default)]
    pub not_yet_supported: Option<BTreeMap<LanguageId, u64>>,
    #[serde(default)]
    pub prior_run_id: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_state: Option<RunRecoveryState>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IndexRunMode {
    Full,
    Incremental,
    Repair,
    Verify,
    Reindex,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IndexRunStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
    Aborted,
}

impl IndexRunStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            IndexRunStatus::Succeeded
                | IndexRunStatus::Failed
                | IndexRunStatus::Cancelled
                | IndexRunStatus::Interrupted
                | IndexRunStatus::Aborted
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Checkpoint {
    pub run_id: String,
    pub cursor: String,
    pub files_processed: u64,
    pub symbols_written: u64,
    pub created_at_unix_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscoveryManifest {
    pub run_id: String,
    pub discovered_at_unix_ms: u64,
    pub relative_paths: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryStateKind {
    Resumed,
    ResumeRejected,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResumeRejectReason {
    RunNotInterrupted,
    MissingCheckpoint,
    EmptyCheckpointCursor,
    ActiveRunConflict,
    RepositoryInvalidated,
    RepositoryFailed,
    RepositoryDegraded,
    RepositoryQuarantined,
    MissingDiscoveryManifest,
    CorruptDiscoveryManifest,
    CheckpointCursorMissing,
    MissingDurableOutputs,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunRecoveryState {
    pub state: RecoveryStateKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rejection_reason: Option<ResumeRejectReason>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_action: Option<NextAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub updated_at_unix_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ResumeRunOutcome {
    Resumed {
        run: IndexRun,
        checkpoint: Checkpoint,
        durable_files_skipped: u64,
    },
    Rejected {
        run: IndexRun,
        reason: ResumeRejectReason,
        next_action: NextAction,
        detail: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PersistedFileOutcome {
    Committed,
    EmptySymbols,
    Failed { error: String },
    Quarantined { reason: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileRecord {
    pub relative_path: String,
    pub language: LanguageId,
    pub blob_id: String,
    pub byte_len: u64,
    pub content_hash: String,
    pub outcome: PersistedFileOutcome,
    pub symbols: Vec<SymbolRecord>,
    pub run_id: String,
    pub repo_id: String,
    pub committed_at_unix_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunPhase {
    Discovering,
    Processing,
    Finalizing,
    Complete,
}

impl RunPhase {
    pub fn to_u8(&self) -> u8 {
        match self {
            RunPhase::Discovering => 0,
            RunPhase::Processing => 1,
            RunPhase::Finalizing => 2,
            RunPhase::Complete => 3,
        }
    }

    pub fn from_u8(value: u8) -> RunPhase {
        match value {
            0 => RunPhase::Discovering,
            1 => RunPhase::Processing,
            2 => RunPhase::Finalizing,
            3 => RunPhase::Complete,
            other => {
                tracing::debug!(
                    value = other,
                    "unexpected RunPhase u8 value, defaulting to Complete"
                );
                RunPhase::Complete
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunHealth {
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunProgressSnapshot {
    pub phase: RunPhase,
    pub total_files: u64,
    pub files_processed: u64,
    pub files_failed: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileOutcomeSummary {
    pub total_committed: u64,
    pub processed_ok: u64,
    pub partial_parse: u64,
    pub failed: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunStatusReport {
    pub run: IndexRun,
    pub health: RunHealth,
    pub is_active: bool,
    pub progress: Option<RunProgressSnapshot>,
    pub file_outcome_summary: Option<FileOutcomeSummary>,
    pub action_required: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_extension_maps_rust() {
        assert_eq!(LanguageId::from_extension("rs"), Some(LanguageId::Rust));
    }

    #[test]
    fn test_from_extension_maps_python() {
        assert_eq!(LanguageId::from_extension("py"), Some(LanguageId::Python));
    }

    #[test]
    fn test_from_extension_maps_javascript() {
        assert_eq!(
            LanguageId::from_extension("js"),
            Some(LanguageId::JavaScript)
        );
        assert_eq!(
            LanguageId::from_extension("jsx"),
            Some(LanguageId::JavaScript)
        );
    }

    #[test]
    fn test_from_extension_maps_typescript() {
        assert_eq!(
            LanguageId::from_extension("ts"),
            Some(LanguageId::TypeScript)
        );
        assert_eq!(
            LanguageId::from_extension("tsx"),
            Some(LanguageId::TypeScript)
        );
    }

    #[test]
    fn test_from_extension_maps_go() {
        assert_eq!(LanguageId::from_extension("go"), Some(LanguageId::Go));
    }

    #[test]
    fn test_from_extension_returns_none_for_unknown() {
        assert_eq!(LanguageId::from_extension("zig"), None);
        assert_eq!(LanguageId::from_extension("lua"), None);
        assert_eq!(LanguageId::from_extension(""), None);
    }

    #[test]
    fn test_extensions_returns_correct_set() {
        assert_eq!(LanguageId::Rust.extensions(), &["rs"]);
        assert_eq!(LanguageId::Python.extensions(), &["py"]);
        assert_eq!(LanguageId::JavaScript.extensions(), &["js", "jsx"]);
        assert_eq!(LanguageId::TypeScript.extensions(), &["ts", "tsx"]);
        assert_eq!(LanguageId::Go.extensions(), &["go"]);
    }

    #[test]
    fn test_support_tier_all_quality_focus() {
        assert_eq!(LanguageId::Rust.support_tier(), SupportTier::QualityFocus);
        assert_eq!(LanguageId::Python.support_tier(), SupportTier::QualityFocus);
        assert_eq!(
            LanguageId::JavaScript.support_tier(),
            SupportTier::QualityFocus
        );
        assert_eq!(
            LanguageId::TypeScript.support_tier(),
            SupportTier::QualityFocus
        );
        assert_eq!(LanguageId::Go.support_tier(), SupportTier::QualityFocus);
    }

    #[test]
    fn test_from_extension_maps_java() {
        assert_eq!(LanguageId::from_extension("java"), Some(LanguageId::Java));
    }

    #[test]
    fn test_from_extension_maps_c() {
        assert_eq!(LanguageId::from_extension("c"), Some(LanguageId::C));
        assert_eq!(LanguageId::from_extension("h"), Some(LanguageId::C));
    }

    #[test]
    fn test_from_extension_maps_cpp() {
        assert_eq!(LanguageId::from_extension("cpp"), Some(LanguageId::Cpp));
        assert_eq!(LanguageId::from_extension("cxx"), Some(LanguageId::Cpp));
        assert_eq!(LanguageId::from_extension("cc"), Some(LanguageId::Cpp));
        assert_eq!(LanguageId::from_extension("hpp"), Some(LanguageId::Cpp));
        assert_eq!(LanguageId::from_extension("hxx"), Some(LanguageId::Cpp));
        assert_eq!(LanguageId::from_extension("hh"), Some(LanguageId::Cpp));
    }

    #[test]
    fn test_from_extension_maps_csharp() {
        assert_eq!(LanguageId::from_extension("cs"), Some(LanguageId::CSharp));
    }

    #[test]
    fn test_from_extension_maps_ruby() {
        assert_eq!(LanguageId::from_extension("rb"), Some(LanguageId::Ruby));
    }

    #[test]
    fn test_from_extension_maps_php() {
        assert_eq!(LanguageId::from_extension("php"), Some(LanguageId::Php));
    }

    #[test]
    fn test_from_extension_maps_swift() {
        assert_eq!(LanguageId::from_extension("swift"), Some(LanguageId::Swift));
    }

    #[test]
    fn test_from_extension_maps_dart() {
        assert_eq!(LanguageId::from_extension("dart"), Some(LanguageId::Dart));
    }

    #[test]
    fn test_from_extension_maps_perl() {
        assert_eq!(LanguageId::from_extension("pl"), Some(LanguageId::Perl));
        assert_eq!(LanguageId::from_extension("pm"), Some(LanguageId::Perl));
    }

    #[test]
    fn test_from_extension_maps_elixir() {
        assert_eq!(LanguageId::from_extension("ex"), Some(LanguageId::Elixir));
        assert_eq!(LanguageId::from_extension("exs"), Some(LanguageId::Elixir));
    }

    #[test]
    fn test_support_tier_java_is_broader() {
        assert_eq!(LanguageId::Java.support_tier(), SupportTier::Broader);
    }

    #[test]
    fn test_support_tier_unsupported_languages() {
        let unsupported = vec![
            LanguageId::C,
            LanguageId::Cpp,
            LanguageId::CSharp,
            LanguageId::Ruby,
            LanguageId::Php,
            LanguageId::Swift,
            LanguageId::Dart,
            LanguageId::Perl,
            LanguageId::Elixir,
        ];
        for lang in unsupported {
            assert_eq!(
                lang.support_tier(),
                SupportTier::Unsupported,
                "{lang:?} should be Unsupported"
            );
        }
    }

    #[test]
    fn test_extensions_broader_languages() {
        assert_eq!(LanguageId::Java.extensions(), &["java"]);
        assert_eq!(LanguageId::C.extensions(), &["c", "h"]);
        assert_eq!(
            LanguageId::Cpp.extensions(),
            &["cpp", "cxx", "cc", "hpp", "hxx", "hh"]
        );
        assert_eq!(LanguageId::CSharp.extensions(), &["cs"]);
        assert_eq!(LanguageId::Ruby.extensions(), &["rb"]);
        assert_eq!(LanguageId::Php.extensions(), &["php"]);
        assert_eq!(LanguageId::Swift.extensions(), &["swift"]);
        assert_eq!(LanguageId::Dart.extensions(), &["dart"]);
        assert_eq!(LanguageId::Perl.extensions(), &["pl", "pm"]);
        assert_eq!(LanguageId::Elixir.extensions(), &["ex", "exs"]);
    }

    #[test]
    fn test_broader_language_serde_roundtrip() {
        let languages = vec![
            LanguageId::Java,
            LanguageId::C,
            LanguageId::Cpp,
            LanguageId::CSharp,
            LanguageId::Ruby,
            LanguageId::Php,
            LanguageId::Swift,
            LanguageId::Dart,
            LanguageId::Perl,
            LanguageId::Elixir,
        ];
        for lang in languages {
            let json = serde_json::to_string(&lang).unwrap();
            let deserialized: LanguageId = serde_json::from_str(&json).unwrap();
            assert_eq!(lang, deserialized, "serde roundtrip failed for {lang:?}");
        }
    }

    #[test]
    fn test_language_id_serde_roundtrip() {
        let languages = vec![
            LanguageId::Rust,
            LanguageId::Python,
            LanguageId::JavaScript,
            LanguageId::TypeScript,
            LanguageId::Go,
        ];
        for lang in languages {
            let json = serde_json::to_string(&lang).unwrap();
            let deserialized: LanguageId = serde_json::from_str(&json).unwrap();
            assert_eq!(lang, deserialized);
        }
    }

    #[test]
    fn test_support_tier_serde_roundtrip() {
        let tiers = vec![
            SupportTier::QualityFocus,
            SupportTier::Broader,
            SupportTier::Unsupported,
        ];
        for tier in tiers {
            let json = serde_json::to_string(&tier).unwrap();
            let deserialized: SupportTier = serde_json::from_str(&json).unwrap();
            assert_eq!(tier, deserialized);
        }
    }

    #[test]
    fn test_file_processing_result_serde_roundtrip() {
        let result = FileProcessingResult {
            relative_path: "src/main.rs".to_string(),
            language: LanguageId::Rust,
            outcome: FileOutcome::Processed,
            symbols: vec![SymbolRecord {
                name: "main".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 50),
                line_range: (1, 3),
            }],
            byte_len: 50,
            content_hash: "abc123".to_string(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: FileProcessingResult = serde_json::from_str(&json).unwrap();
        assert_eq!(result, deserialized);
    }

    #[test]
    fn test_file_outcome_partial_parse_serde_roundtrip() {
        let outcome = FileOutcome::PartialParse {
            warning: "syntax error at line 5".to_string(),
        };
        let json = serde_json::to_string(&outcome).unwrap();
        let deserialized: FileOutcome = serde_json::from_str(&json).unwrap();
        assert_eq!(outcome, deserialized);
    }

    #[test]
    fn test_file_outcome_failed_serde_roundtrip() {
        let outcome = FileOutcome::Failed {
            error: "could not parse file".to_string(),
        };
        let json = serde_json::to_string(&outcome).unwrap();
        let deserialized: FileOutcome = serde_json::from_str(&json).unwrap();
        assert_eq!(outcome, deserialized);
    }

    #[test]
    fn test_symbol_record_serde_roundtrip() {
        let record = SymbolRecord {
            name: "MyStruct".to_string(),
            kind: SymbolKind::Struct,
            depth: 0,
            sort_order: 1,
            byte_range: (10, 100),
            line_range: (2, 10),
        };
        let json = serde_json::to_string(&record).unwrap();
        let deserialized: SymbolRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, deserialized);
    }

    #[test]
    fn test_symbol_kind_all_variants_serde() {
        let kinds = vec![
            SymbolKind::Function,
            SymbolKind::Method,
            SymbolKind::Class,
            SymbolKind::Struct,
            SymbolKind::Enum,
            SymbolKind::Interface,
            SymbolKind::Module,
            SymbolKind::Constant,
            SymbolKind::Variable,
            SymbolKind::Type,
            SymbolKind::Trait,
            SymbolKind::Impl,
            SymbolKind::Other,
        ];
        for kind in kinds {
            let json = serde_json::to_string(&kind).unwrap();
            let deserialized: SymbolKind = serde_json::from_str(&json).unwrap();
            assert_eq!(kind, deserialized);
        }
    }

    #[test]
    fn test_file_processing_result_failed_has_empty_symbols() {
        let result = FileProcessingResult {
            relative_path: "bad.rs".to_string(),
            language: LanguageId::Rust,
            outcome: FileOutcome::Failed {
                error: "parse failed".to_string(),
            },
            symbols: vec![],
            byte_len: 100,
            content_hash: "def456".to_string(),
        };
        assert!(result.symbols.is_empty());
    }

    #[test]
    fn test_deserialize_interrupted_status() {
        let json = r#""interrupted""#;
        let status: IndexRunStatus = serde_json::from_str(json).unwrap();
        assert_eq!(status, IndexRunStatus::Interrupted);
    }

    #[test]
    fn test_serialize_interrupted_status() {
        let json = serde_json::to_string(&IndexRunStatus::Interrupted).unwrap();
        assert_eq!(json, r#""interrupted""#);
    }

    #[test]
    fn test_deserialize_existing_statuses_backward_compatible() {
        let cases = vec![
            (r#""queued""#, IndexRunStatus::Queued),
            (r#""running""#, IndexRunStatus::Running),
            (r#""succeeded""#, IndexRunStatus::Succeeded),
            (r#""failed""#, IndexRunStatus::Failed),
            (r#""cancelled""#, IndexRunStatus::Cancelled),
        ];
        for (json, expected) in cases {
            let status: IndexRunStatus = serde_json::from_str(json).unwrap();
            assert_eq!(status, expected, "failed for {json}");
        }
    }

    #[test]
    fn test_persisted_file_outcome_committed_serde_roundtrip() {
        let outcome = PersistedFileOutcome::Committed;
        let json = serde_json::to_string(&outcome).unwrap();
        let deserialized: PersistedFileOutcome = serde_json::from_str(&json).unwrap();
        assert_eq!(outcome, deserialized);
    }

    #[test]
    fn test_persisted_file_outcome_empty_symbols_serde_roundtrip() {
        let outcome = PersistedFileOutcome::EmptySymbols;
        let json = serde_json::to_string(&outcome).unwrap();
        let deserialized: PersistedFileOutcome = serde_json::from_str(&json).unwrap();
        assert_eq!(outcome, deserialized);
    }

    #[test]
    fn test_persisted_file_outcome_failed_serde_roundtrip() {
        let outcome = PersistedFileOutcome::Failed {
            error: "disk full".to_string(),
        };
        let json = serde_json::to_string(&outcome).unwrap();
        let deserialized: PersistedFileOutcome = serde_json::from_str(&json).unwrap();
        assert_eq!(outcome, deserialized);
    }

    #[test]
    fn test_persisted_file_outcome_quarantined_serde_roundtrip() {
        let outcome = PersistedFileOutcome::Quarantined {
            reason: "blob_id/content_hash mismatch".to_string(),
        };
        let json = serde_json::to_string(&outcome).unwrap();
        let deserialized: PersistedFileOutcome = serde_json::from_str(&json).unwrap();
        assert_eq!(outcome, deserialized);
    }

    #[test]
    fn test_file_record_construction_committed() {
        let record = FileRecord {
            relative_path: "src/main.rs".to_string(),
            language: LanguageId::Rust,
            blob_id: "abcdef1234567890".to_string(),
            byte_len: 256,
            content_hash: "abcdef1234567890".to_string(),
            outcome: PersistedFileOutcome::Committed,
            symbols: vec![SymbolRecord {
                name: "main".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 50),
                line_range: (1, 3),
            }],
            run_id: "run-001".to_string(),
            repo_id: "repo-001".to_string(),
            committed_at_unix_ms: 1700000000000,
        };
        assert_eq!(record.outcome, PersistedFileOutcome::Committed);
        assert_eq!(record.symbols.len(), 1);
    }

    #[test]
    fn test_file_record_construction_empty_symbols() {
        let record = FileRecord {
            relative_path: "src/empty.py".to_string(),
            language: LanguageId::Python,
            blob_id: "hash123".to_string(),
            byte_len: 10,
            content_hash: "hash123".to_string(),
            outcome: PersistedFileOutcome::EmptySymbols,
            symbols: vec![],
            run_id: "run-001".to_string(),
            repo_id: "repo-001".to_string(),
            committed_at_unix_ms: 1700000000000,
        };
        assert_eq!(record.outcome, PersistedFileOutcome::EmptySymbols);
        assert!(record.symbols.is_empty());
    }

    #[test]
    fn test_file_record_serde_roundtrip() {
        let record = FileRecord {
            relative_path: "src/lib.rs".to_string(),
            language: LanguageId::Rust,
            blob_id: "deadbeef".to_string(),
            byte_len: 1024,
            content_hash: "deadbeef".to_string(),
            outcome: PersistedFileOutcome::Committed,
            symbols: vec![
                SymbolRecord {
                    name: "MyStruct".to_string(),
                    kind: SymbolKind::Struct,
                    depth: 0,
                    sort_order: 0,
                    byte_range: (0, 200),
                    line_range: (1, 10),
                },
                SymbolRecord {
                    name: "new".to_string(),
                    kind: SymbolKind::Method,
                    depth: 1,
                    sort_order: 1,
                    byte_range: (50, 150),
                    line_range: (3, 8),
                },
            ],
            run_id: "run-002".to_string(),
            repo_id: "repo-002".to_string(),
            committed_at_unix_ms: 1700000000000,
        };
        let json = serde_json::to_string(&record).unwrap();
        let deserialized: FileRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, deserialized);
    }

    #[test]
    fn test_file_record_failed_serde_roundtrip() {
        let record = FileRecord {
            relative_path: "bad.go".to_string(),
            language: LanguageId::Go,
            blob_id: "abc".to_string(),
            byte_len: 50,
            content_hash: "abc".to_string(),
            outcome: PersistedFileOutcome::Failed {
                error: "CAS write failed".to_string(),
            },
            symbols: vec![],
            run_id: "run-003".to_string(),
            repo_id: "repo-003".to_string(),
            committed_at_unix_ms: 1700000000000,
        };
        let json = serde_json::to_string(&record).unwrap();
        let deserialized: FileRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, deserialized);
    }

    #[test]
    fn test_file_record_quarantined_serde_roundtrip() {
        let record = FileRecord {
            relative_path: "suspect.ts".to_string(),
            language: LanguageId::TypeScript,
            blob_id: "xyz".to_string(),
            byte_len: 100,
            content_hash: "different_hash".to_string(),
            outcome: PersistedFileOutcome::Quarantined {
                reason: "blob_id/content_hash mismatch".to_string(),
            },
            symbols: vec![],
            run_id: "run-004".to_string(),
            repo_id: "repo-004".to_string(),
            committed_at_unix_ms: 1700000000000,
        };
        let json = serde_json::to_string(&record).unwrap();
        let deserialized: FileRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, deserialized);
    }

    #[test]
    fn test_roundtrip_index_run_with_interrupted() {
        let run = IndexRun {
            run_id: "test-run".to_string(),
            repo_id: "repo-1".to_string(),
            mode: IndexRunMode::Full,
            status: IndexRunStatus::Interrupted,
            requested_at_unix_ms: 1000,
            started_at_unix_ms: Some(1001),
            finished_at_unix_ms: None,
            idempotency_key: None,
            request_hash: None,
            checkpoint_cursor: None,
            error_summary: Some("process exited unexpectedly".to_string()),
            not_yet_supported: None,
            prior_run_id: None,
            description: None,
            recovery_state: Some(RunRecoveryState {
                state: RecoveryStateKind::ResumeRejected,
                rejection_reason: Some(ResumeRejectReason::MissingCheckpoint),
                next_action: Some(NextAction::Reindex),
                detail: Some("missing checkpoint".to_string()),
                updated_at_unix_ms: 1010,
            }),
        };
        let json = serde_json::to_string(&run).unwrap();
        let deserialized: IndexRun = serde_json::from_str(&json).unwrap();
        assert_eq!(run, deserialized);
    }

    #[test]
    fn test_run_health_serde_roundtrip() {
        for variant in [
            RunHealth::Healthy,
            RunHealth::Degraded,
            RunHealth::Unhealthy,
        ] {
            let json = serde_json::to_string(&variant).unwrap();
            let deserialized: RunHealth = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, deserialized);
        }
    }

    #[test]
    fn test_run_progress_snapshot_serde_roundtrip() {
        let snapshot = RunProgressSnapshot {
            phase: RunPhase::Processing,
            total_files: 100,
            files_processed: 80,
            files_failed: 5,
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        let deserialized: RunProgressSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(snapshot, deserialized);
    }

    #[test]
    fn test_file_outcome_summary_serde_roundtrip() {
        let summary = FileOutcomeSummary {
            total_committed: 50,
            processed_ok: 40,
            partial_parse: 7,
            failed: 3,
        };
        let json = serde_json::to_string(&summary).unwrap();
        let deserialized: FileOutcomeSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(summary, deserialized);
    }

    #[test]
    fn test_run_status_report_serde_roundtrip() {
        let report = RunStatusReport {
            run: IndexRun {
                run_id: "run-1".into(),
                repo_id: "repo-1".into(),
                mode: IndexRunMode::Full,
                status: IndexRunStatus::Succeeded,
                requested_at_unix_ms: 1000,
                started_at_unix_ms: Some(1001),
                finished_at_unix_ms: Some(2000),
                idempotency_key: None,
                request_hash: None,
                checkpoint_cursor: None,
                error_summary: None,
                not_yet_supported: None,
                prior_run_id: None,
                description: None,
                recovery_state: Some(RunRecoveryState {
                    state: RecoveryStateKind::Resumed,
                    rejection_reason: None,
                    next_action: None,
                    detail: None,
                    updated_at_unix_ms: 1500,
                }),
            },
            health: RunHealth::Healthy,
            is_active: false,
            progress: Some(RunProgressSnapshot {
                phase: RunPhase::Complete,
                total_files: 10,
                files_processed: 10,
                files_failed: 0,
            }),
            file_outcome_summary: Some(FileOutcomeSummary {
                total_committed: 10,
                processed_ok: 10,
                partial_parse: 0,
                failed: 0,
            }),
            action_required: None,
        };
        let json = serde_json::to_string(&report).unwrap();
        let deserialized: RunStatusReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report, deserialized);
    }

    #[test]
    fn test_run_phase_serde_roundtrip() {
        let variants = [
            RunPhase::Discovering,
            RunPhase::Processing,
            RunPhase::Finalizing,
            RunPhase::Complete,
        ];
        for phase in &variants {
            let json = serde_json::to_string(phase).unwrap();
            let deserialized: RunPhase = serde_json::from_str(&json).unwrap();
            assert_eq!(*phase, deserialized);
        }
    }

    #[test]
    fn test_run_phase_serde_snake_case() {
        assert_eq!(
            serde_json::to_string(&RunPhase::Discovering).unwrap(),
            "\"discovering\""
        );
        assert_eq!(
            serde_json::to_string(&RunPhase::Processing).unwrap(),
            "\"processing\""
        );
        assert_eq!(
            serde_json::to_string(&RunPhase::Finalizing).unwrap(),
            "\"finalizing\""
        );
        assert_eq!(
            serde_json::to_string(&RunPhase::Complete).unwrap(),
            "\"complete\""
        );
    }

    #[test]
    fn test_run_phase_u8_conversion_round_trips() {
        let variants = [
            RunPhase::Discovering,
            RunPhase::Processing,
            RunPhase::Finalizing,
            RunPhase::Complete,
        ];
        for (i, phase) in variants.iter().enumerate() {
            assert_eq!(phase.to_u8(), i as u8);
            assert_eq!(RunPhase::from_u8(i as u8), *phase);
        }
    }

    #[test]
    fn test_run_phase_from_u8_unknown_defaults_to_complete() {
        assert_eq!(RunPhase::from_u8(4), RunPhase::Complete);
        assert_eq!(RunPhase::from_u8(255), RunPhase::Complete);
    }

    #[test]
    fn test_run_progress_snapshot_with_phase_serde_roundtrip() {
        let snapshot = RunProgressSnapshot {
            phase: RunPhase::Finalizing,
            total_files: 50,
            files_processed: 48,
            files_failed: 2,
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        let deserialized: RunProgressSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(snapshot, deserialized);
    }

    #[test]
    fn test_roundtrip_index_run_with_prior_run_id() {
        let run = IndexRun {
            run_id: "reindex-run".to_string(),
            repo_id: "repo-1".to_string(),
            mode: IndexRunMode::Reindex,
            status: IndexRunStatus::Queued,
            requested_at_unix_ms: 2000,
            started_at_unix_ms: None,
            finished_at_unix_ms: None,
            idempotency_key: None,
            request_hash: None,
            checkpoint_cursor: None,
            error_summary: None,
            not_yet_supported: None,
            prior_run_id: Some("previous-run-id".to_string()),
            description: None,
            recovery_state: None,
        };
        let json = serde_json::to_string(&run).unwrap();
        let deserialized: IndexRun = serde_json::from_str(&json).unwrap();
        assert_eq!(run, deserialized);
        assert_eq!(
            deserialized.prior_run_id,
            Some("previous-run-id".to_string())
        );
    }

    #[test]
    fn test_deserialize_index_run_without_prior_run_id_backward_compat() {
        let json = r#"{
            "run_id": "old-run",
            "repo_id": "repo-1",
            "mode": "full",
            "status": "succeeded",
            "requested_at_unix_ms": 1000,
            "started_at_unix_ms": 1001,
            "finished_at_unix_ms": 2000,
            "idempotency_key": null,
            "request_hash": null,
            "checkpoint_cursor": null,
            "error_summary": null
        }"#;
        let run: IndexRun = serde_json::from_str(json).unwrap();
        assert_eq!(run.run_id, "old-run");
        assert_eq!(run.prior_run_id, None);
        assert_eq!(run.not_yet_supported, None);
        assert_eq!(run.recovery_state, None);
    }

    #[test]
    fn test_run_recovery_state_serde_roundtrip() {
        let recovery = RunRecoveryState {
            state: RecoveryStateKind::ResumeRejected,
            rejection_reason: Some(ResumeRejectReason::MissingDurableOutputs),
            next_action: Some(NextAction::Reindex),
            detail: Some("missing durable file record for src/lib.rs".to_string()),
            updated_at_unix_ms: 1234,
        };
        let json = serde_json::to_string(&recovery).unwrap();
        let deserialized: RunRecoveryState = serde_json::from_str(&json).unwrap();
        assert_eq!(recovery, deserialized);
    }

    #[test]
    fn test_resume_run_outcome_serde_roundtrip() {
        let run = IndexRun {
            run_id: "run-1".to_string(),
            repo_id: "repo-1".to_string(),
            mode: IndexRunMode::Full,
            status: IndexRunStatus::Running,
            requested_at_unix_ms: 1000,
            started_at_unix_ms: Some(1001),
            finished_at_unix_ms: None,
            idempotency_key: None,
            request_hash: None,
            checkpoint_cursor: Some("src/main.rs".to_string()),
            error_summary: None,
            not_yet_supported: None,
            prior_run_id: None,
            description: None,
            recovery_state: Some(RunRecoveryState {
                state: RecoveryStateKind::Resumed,
                rejection_reason: None,
                next_action: None,
                detail: None,
                updated_at_unix_ms: 1200,
            }),
        };
        let outcome = ResumeRunOutcome::Resumed {
            run,
            checkpoint: Checkpoint {
                run_id: "run-1".to_string(),
                cursor: "src/main.rs".to_string(),
                files_processed: 1,
                symbols_written: 2,
                created_at_unix_ms: 1100,
            },
            durable_files_skipped: 1,
        };
        let json = serde_json::to_string(&outcome).unwrap();
        let deserialized: ResumeRunOutcome = serde_json::from_str(&json).unwrap();
        assert_eq!(outcome, deserialized);
    }

    #[test]
    fn test_reindex_mode_serde_roundtrip() {
        let mode = IndexRunMode::Reindex;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, "\"reindex\"");
        let deserialized: IndexRunMode = serde_json::from_str(&json).unwrap();
        assert_eq!(mode, deserialized);
    }
}
