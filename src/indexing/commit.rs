use crate::domain::{
    FileOutcome, FileProcessingResult, FileRecord, PersistedFileOutcome, SupportTier,
    unix_timestamp_ms,
};
use crate::error::{Result, TokenizorError};
use crate::storage::BlobStore;

pub fn validate_for_commit(result: &FileProcessingResult, blob_id: &str) -> PersistedFileOutcome {
    if blob_id != result.content_hash {
        return PersistedFileOutcome::Quarantined {
            reason: "blob_id/content_hash mismatch".to_string(),
        };
    }

    match &result.outcome {
        FileOutcome::Processed => {
            if result.symbols.is_empty() {
                PersistedFileOutcome::EmptySymbols
            } else {
                PersistedFileOutcome::Committed
            }
        }
        FileOutcome::Failed { error } => PersistedFileOutcome::Failed {
            error: error.clone(),
        },
        FileOutcome::PartialParse { warning } => {
            if result.symbols.is_empty() {
                PersistedFileOutcome::Quarantined {
                    reason: warning.clone(),
                }
            } else {
                PersistedFileOutcome::Committed
            }
        }
    }
}

pub fn commit_file_result(
    result: FileProcessingResult,
    bytes: &[u8],
    cas: &dyn BlobStore,
    run_id: &str,
    repo_id: &str,
) -> Result<FileRecord> {
    if result.language.support_tier() != SupportTier::QualityFocus {
        return Err(TokenizorError::InvalidArgument(format!(
            "language {:?} is not in the quality-focus set",
            result.language
        )));
    }

    let (blob_id, byte_len, outcome) = match cas.store_bytes(bytes) {
        Ok(stored) => {
            let outcome = validate_for_commit(&result, &stored.blob_id);
            (stored.blob_id, stored.byte_len, outcome)
        }
        Err(err) => {
            if !cas.root_dir().exists() {
                return Err(TokenizorError::Storage(format!(
                    "CAS root inaccessible: {}",
                    err
                )));
            }
            let outcome = PersistedFileOutcome::Failed {
                error: format!("CAS write failed: {}", err),
            };
            (String::new(), bytes.len() as u64, outcome)
        }
    };

    Ok(FileRecord {
        relative_path: result.relative_path,
        language: result.language,
        blob_id,
        byte_len,
        content_hash: result.content_hash,
        outcome,
        symbols: result.symbols,
        run_id: run_id.to_string(),
        repo_id: repo_id.to_string(),
        committed_at_unix_ms: unix_timestamp_ms(),
    })
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::domain::{ComponentHealth, LanguageId, SymbolKind, SymbolRecord};
    use crate::storage::StoredBlob;

    struct FakeCas {
        root: PathBuf,
        store_count: AtomicUsize,
        fail_store: bool,
    }

    impl FakeCas {
        fn new(root: PathBuf) -> Self {
            Self {
                root,
                store_count: AtomicUsize::new(0),
                fail_store: false,
            }
        }

        fn failing(root: PathBuf) -> Self {
            Self {
                root,
                store_count: AtomicUsize::new(0),
                fail_store: true,
            }
        }
    }

    impl BlobStore for FakeCas {
        fn backend_name(&self) -> &'static str {
            "fake"
        }

        fn root_dir(&self) -> &Path {
            &self.root
        }

        fn initialize(&self) -> Result<ComponentHealth> {
            unreachable!("initialize not needed in commit tests")
        }

        fn health_check(&self) -> Result<ComponentHealth> {
            unreachable!("health_check not needed in commit tests")
        }

        fn store_bytes(&self, bytes: &[u8]) -> Result<StoredBlob> {
            self.store_count.fetch_add(1, Ordering::SeqCst);
            if self.fail_store {
                return Err(TokenizorError::Storage("fake CAS write error".into()));
            }
            let blob_id = crate::storage::digest_hex(bytes);
            Ok(StoredBlob {
                blob_id,
                byte_len: bytes.len() as u64,
                was_created: true,
            })
        }

        fn read_bytes(&self, _blob_id: &str) -> Result<Vec<u8>> {
            unreachable!("read_bytes not needed in commit tests")
        }
    }

    fn sample_symbol() -> SymbolRecord {
        SymbolRecord {
            name: "main".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 50),
            line_range: (1, 3),
        }
    }

    fn sample_result(outcome: FileOutcome, symbols: Vec<SymbolRecord>) -> FileProcessingResult {
        let content_hash = crate::storage::digest_hex(b"fn main() {}");
        FileProcessingResult {
            relative_path: "src/main.rs".to_string(),
            language: LanguageId::Rust,
            outcome,
            symbols,
            byte_len: 12,
            content_hash,
        }
    }

    // === validate_for_commit tests ===

    #[test]
    fn test_validate_processed_with_symbols_returns_committed() {
        let result = sample_result(FileOutcome::Processed, vec![sample_symbol()]);
        let outcome = validate_for_commit(&result, &result.content_hash);
        assert_eq!(outcome, PersistedFileOutcome::Committed);
    }

    #[test]
    fn test_validate_processed_with_empty_symbols_returns_empty_symbols() {
        let result = sample_result(FileOutcome::Processed, vec![]);
        let outcome = validate_for_commit(&result, &result.content_hash);
        assert_eq!(outcome, PersistedFileOutcome::EmptySymbols);
    }

    #[test]
    fn test_validate_failed_maps_to_failed() {
        let result = sample_result(
            FileOutcome::Failed {
                error: "parse error".to_string(),
            },
            vec![],
        );
        let outcome = validate_for_commit(&result, &result.content_hash);
        assert_eq!(
            outcome,
            PersistedFileOutcome::Failed {
                error: "parse error".to_string()
            }
        );
    }

    #[test]
    fn test_validate_partial_parse_with_symbols_returns_committed() {
        let result = sample_result(
            FileOutcome::PartialParse {
                warning: "minor issue".to_string(),
            },
            vec![sample_symbol()],
        );
        let outcome = validate_for_commit(&result, &result.content_hash);
        assert_eq!(outcome, PersistedFileOutcome::Committed);
    }

    #[test]
    fn test_validate_partial_parse_no_symbols_returns_quarantined() {
        let result = sample_result(
            FileOutcome::PartialParse {
                warning: "syntax error at line 5".to_string(),
            },
            vec![],
        );
        let outcome = validate_for_commit(&result, &result.content_hash);
        assert_eq!(
            outcome,
            PersistedFileOutcome::Quarantined {
                reason: "syntax error at line 5".to_string()
            }
        );
    }

    #[test]
    fn test_validate_hash_mismatch_returns_quarantined() {
        let result = sample_result(FileOutcome::Processed, vec![sample_symbol()]);
        let outcome = validate_for_commit(&result, "wrong_hash");
        assert_eq!(
            outcome,
            PersistedFileOutcome::Quarantined {
                reason: "blob_id/content_hash mismatch".to_string()
            }
        );
    }

    #[test]
    fn test_validate_hash_mismatch_takes_precedence_over_outcome() {
        let result = sample_result(
            FileOutcome::Failed {
                error: "parse error".to_string(),
            },
            vec![],
        );
        let outcome = validate_for_commit(&result, "wrong_hash");
        assert_eq!(
            outcome,
            PersistedFileOutcome::Quarantined {
                reason: "blob_id/content_hash mismatch".to_string()
            }
        );
    }

    // === commit_file_result tests ===

    #[test]
    fn test_commit_file_result_stores_blob_and_creates_record() {
        let dir = tempfile::tempdir().unwrap();
        let cas = FakeCas::new(dir.path().to_path_buf());
        let result = sample_result(FileOutcome::Processed, vec![sample_symbol()]);
        let bytes = b"fn main() {}";

        let record = commit_file_result(result, bytes, &cas, "run-1", "repo-1").unwrap();

        assert_eq!(record.outcome, PersistedFileOutcome::Committed);
        assert!(!record.blob_id.is_empty());
        assert_eq!(record.byte_len, 12);
        assert_eq!(record.run_id, "run-1");
        assert_eq!(record.repo_id, "repo-1");
        assert!(record.committed_at_unix_ms > 0);
        assert_eq!(record.symbols.len(), 1);
        assert_eq!(cas.store_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_commit_file_result_cas_failure_returns_failed_record() {
        let dir = tempfile::tempdir().unwrap();
        let cas = FakeCas::failing(dir.path().to_path_buf());
        let result = sample_result(FileOutcome::Processed, vec![sample_symbol()]);
        let bytes = b"fn main() {}";

        let record = commit_file_result(result, bytes, &cas, "run-1", "repo-1").unwrap();

        match &record.outcome {
            PersistedFileOutcome::Failed { error } => {
                assert!(error.contains("CAS write failed"));
            }
            other => panic!("expected Failed, got {:?}", other),
        }
        assert_eq!(cas.store_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_commit_file_result_cas_root_missing_returns_systemic_error() {
        let cas = FakeCas::failing(PathBuf::from("/nonexistent/cas/root"));
        let result = sample_result(FileOutcome::Processed, vec![sample_symbol()]);
        let bytes = b"fn main() {}";

        let err = commit_file_result(result, bytes, &cas, "run-1", "repo-1").unwrap_err();

        match err {
            TokenizorError::Storage(msg) => {
                assert!(msg.contains("CAS root inaccessible"));
            }
            other => panic!("expected Storage error, got {:?}", other),
        }
    }

    #[test]
    fn test_commit_file_result_empty_symbols_produces_empty_symbols_outcome() {
        let dir = tempfile::tempdir().unwrap();
        let cas = FakeCas::new(dir.path().to_path_buf());
        let result = sample_result(FileOutcome::Processed, vec![]);
        let bytes = b"fn main() {}";

        let record = commit_file_result(result, bytes, &cas, "run-1", "repo-1").unwrap();

        assert_eq!(record.outcome, PersistedFileOutcome::EmptySymbols);
    }

    #[test]
    fn test_commit_file_result_preserves_file_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let cas = FakeCas::new(dir.path().to_path_buf());
        let result = sample_result(FileOutcome::Processed, vec![sample_symbol()]);
        let bytes = b"fn main() {}";

        let record = commit_file_result(result, bytes, &cas, "run-1", "repo-1").unwrap();

        assert_eq!(record.relative_path, "src/main.rs");
        assert_eq!(record.language, LanguageId::Rust);
        assert_eq!(record.content_hash, record.blob_id);
    }
}
