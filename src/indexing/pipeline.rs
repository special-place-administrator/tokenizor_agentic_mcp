use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};

use crate::domain::{FileOutcome, FileProcessingResult, FileRecord, IndexRunStatus, PersistedFileOutcome};
use crate::indexing::{commit, discovery};
use crate::parsing;
use crate::storage::BlobStore;

pub struct PipelineProgress {
    pub total_files: AtomicU64,
    pub files_processed: AtomicU64,
    pub files_failed: AtomicU64,
}

impl PipelineProgress {
    pub fn new() -> Self {
        Self {
            total_files: AtomicU64::new(0),
            files_processed: AtomicU64::new(0),
            files_failed: AtomicU64::new(0),
        }
    }
}

pub struct PipelineResult {
    pub status: IndexRunStatus,
    pub results: Vec<FileProcessingResult>,
    pub file_records: Vec<FileRecord>,
    pub error_summary: Option<String>,
}

pub struct IndexingPipeline {
    run_id: String,
    repo_id: String,
    repo_root: PathBuf,
    concurrency_cap: usize,
    circuit_breaker_threshold: usize,
    progress: Arc<PipelineProgress>,
    cas: Option<Arc<dyn BlobStore>>,
}

impl IndexingPipeline {
    pub fn new(run_id: String, repo_root: PathBuf) -> Self {
        let concurrency_cap = num_cpus::get().max(1).min(16);
        Self {
            run_id,
            repo_id: String::new(),
            repo_root,
            concurrency_cap,
            circuit_breaker_threshold: 5,
            progress: Arc::new(PipelineProgress::new()),
            cas: None,
        }
    }

    pub fn with_cas(mut self, cas: Arc<dyn BlobStore>, repo_id: String) -> Self {
        self.cas = Some(cas);
        self.repo_id = repo_id;
        self
    }

    pub fn with_concurrency(mut self, cap: usize) -> Self {
        self.concurrency_cap = cap.max(1);
        self
    }

    pub fn with_circuit_breaker(mut self, threshold: usize) -> Self {
        self.circuit_breaker_threshold = threshold;
        self
    }

    pub fn progress(&self) -> Arc<PipelineProgress> {
        self.progress.clone()
    }

    pub async fn execute(self) -> PipelineResult {
        info!(run_id = %self.run_id, root = %self.repo_root.display(), "pipeline starting");

        let files = match discovery::discover_files(&self.repo_root) {
            Ok(files) => files,
            Err(e) => {
                error!(run_id = %self.run_id, error = %e, "file discovery failed");
                return PipelineResult {
                    status: IndexRunStatus::Failed,
                    results: vec![],
                    file_records: vec![],
                    error_summary: Some(format!("discovery failed: {e}")),
                };
            }
        };

        self.process_discovered(files).await
    }

    async fn process_discovered(
        self,
        files: Vec<discovery::DiscoveredFile>,
    ) -> PipelineResult {
        let total = files.len() as u64;
        self.progress.total_files.store(total, Ordering::Relaxed);
        info!(run_id = %self.run_id, total_files = total, "discovery complete");

        if files.is_empty() {
            return PipelineResult {
                status: IndexRunStatus::Succeeded,
                results: vec![],
                file_records: vec![],
                error_summary: None,
            };
        }

        let semaphore = Arc::new(Semaphore::new(self.concurrency_cap));
        let progress = self.progress.clone();
        let consecutive_failures = Arc::new(AtomicU64::new(0));
        let circuit_broken = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let threshold = self.circuit_breaker_threshold as u64;
        let cas = self.cas.clone();

        let mut handles = Vec::with_capacity(files.len());

        for file in files {
            // M3: Stop spawning tasks once the circuit breaker has tripped
            if circuit_broken.load(Ordering::Relaxed) {
                break;
            }

            let permit = semaphore.clone().acquire_owned().await.unwrap();
            let progress = progress.clone();
            let consecutive_failures = consecutive_failures.clone();
            let circuit_broken = circuit_broken.clone();
            let run_id = self.run_id.clone();
            let repo_id = self.repo_id.clone();
            let cas = cas.clone();

            let handle = tokio::spawn(async move {
                if circuit_broken.load(Ordering::Relaxed) {
                    drop(permit);
                    return None;
                }

                // H1: File-level I/O errors are NOT systemic — they go through
                // the consecutive-failure counter like any other file failure.
                // Only system-level errors (registry, CAS root) are systemic.
                let bytes = match std::fs::read(&file.absolute_path) {
                    Ok(b) => b,
                    Err(e) => {
                        warn!(
                            run_id = %run_id,
                            path = %file.relative_path,
                            error = %e,
                            "file read failed"
                        );
                        progress.files_failed.fetch_add(1, Ordering::Relaxed);
                        let prev = consecutive_failures.fetch_add(1, Ordering::Relaxed);
                        if prev + 1 >= threshold {
                            circuit_broken.store(true, Ordering::Relaxed);
                        }
                        drop(permit);
                        return Some((
                            FileProcessingResult {
                                relative_path: file.relative_path,
                                language: file.language,
                                outcome: FileOutcome::Failed {
                                    error: format!("file read error: {e}"),
                                },
                                symbols: vec![],
                                byte_len: 0,
                                content_hash: String::new(),
                            },
                            None,
                        ));
                    }
                };

                let result = parsing::process_file(&file.relative_path, &bytes, file.language);

                // Commit to CAS if available — persist within the bounded-concurrency slot
                let file_record = if let Some(ref cas) = cas {
                    match commit::commit_file_result(
                        result.clone(),
                        &bytes,
                        cas.as_ref(),
                        &run_id,
                        &repo_id,
                    ) {
                        Ok(record) => {
                            debug!(
                                run_id = %run_id,
                                path = %record.relative_path,
                                outcome = ?record.outcome,
                                "file committed"
                            );
                            Some(record)
                        }
                        Err(err) => {
                            // CAS root inaccessible — systemic error, immediate abort
                            error!(
                                run_id = %run_id,
                                error = %err,
                                "CAS systemic failure — aborting pipeline"
                            );
                            circuit_broken.store(true, Ordering::Relaxed);
                            None
                        }
                    }
                } else {
                    None
                };

                match &result.outcome {
                    FileOutcome::Processed => {
                        consecutive_failures.store(0, Ordering::Relaxed);
                        progress.files_processed.fetch_add(1, Ordering::Relaxed);
                        debug!(run_id = %run_id, path = %result.relative_path, "processed");
                    }
                    FileOutcome::PartialParse { warning } => {
                        consecutive_failures.store(0, Ordering::Relaxed);
                        progress.files_processed.fetch_add(1, Ordering::Relaxed);
                        warn!(run_id = %run_id, path = %result.relative_path, warning = %warning, "partial parse");
                    }
                    FileOutcome::Failed { error } => {
                        progress.files_failed.fetch_add(1, Ordering::Relaxed);
                        let prev = consecutive_failures.fetch_add(1, Ordering::Relaxed);
                        if prev + 1 >= threshold {
                            circuit_broken.store(true, Ordering::Relaxed);
                        }
                        warn!(run_id = %run_id, path = %result.relative_path, error = %error, "file failed");
                    }
                }

                drop(permit);
                Some((result, file_record))
            });

            handles.push(handle);
        }

        let mut results = Vec::with_capacity(handles.len());
        let mut file_records = Vec::with_capacity(handles.len());
        for handle in handles {
            if let Ok(Some((result, record))) = handle.await {
                results.push(result);
                if let Some(record) = record {
                    file_records.push(record);
                }
            }
        }

        let was_broken = circuit_broken.load(Ordering::Relaxed);
        let failed_count = progress.files_failed.load(Ordering::Relaxed);

        // Compute persistence outcome breakdown for finish summary
        let persisted_count = file_records.len();
        let committed_count = file_records
            .iter()
            .filter(|r| matches!(r.outcome, PersistedFileOutcome::Committed))
            .count();
        let empty_symbol_count = file_records
            .iter()
            .filter(|r| matches!(r.outcome, PersistedFileOutcome::EmptySymbols))
            .count();
        let persist_failed_count = file_records
            .iter()
            .filter(|r| matches!(r.outcome, PersistedFileOutcome::Failed { .. }))
            .count();
        let quarantined_count = file_records
            .iter()
            .filter(|r| matches!(r.outcome, PersistedFileOutcome::Quarantined { .. }))
            .count();

        let (status, error_summary) = if was_broken {
            info!(run_id = %self.run_id, "pipeline aborted by circuit breaker");
            (
                IndexRunStatus::Aborted,
                Some(format!(
                    "circuit breaker triggered after {failed_count} failures"
                )),
            )
        } else if failed_count > 0 {
            info!(
                run_id = %self.run_id,
                failed = failed_count,
                persisted = persisted_count,
                committed = committed_count,
                "pipeline completed with failures"
            );
            (
                IndexRunStatus::Succeeded,
                Some(format!(
                    "{failed_count} files failed processing; persisted: {committed_count} committed, \
                     {empty_symbol_count} empty-symbols, {persist_failed_count} failed, \
                     {quarantined_count} quarantined"
                )),
            )
        } else {
            info!(
                run_id = %self.run_id,
                persisted = persisted_count,
                committed = committed_count,
                "pipeline succeeded"
            );
            (IndexRunStatus::Succeeded, None)
        };

        PipelineResult {
            status,
            results,
            file_records,
            error_summary,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_repo_with_files(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (name, content) in files {
            let path = dir.path().join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, content).unwrap();
        }
        dir
    }

    #[tokio::test]
    async fn test_pipeline_processes_files() {
        let dir = temp_repo_with_files(&[
            ("main.rs", "fn main() {}"),
            ("lib.py", "def foo(): pass"),
        ]);

        let pipeline = IndexingPipeline::new("test-run".into(), dir.path().to_path_buf())
            .with_concurrency(2);
        let result = pipeline.execute().await;

        assert_eq!(result.status, IndexRunStatus::Succeeded);
        assert_eq!(result.results.len(), 2);
        assert!(result.error_summary.is_none());
    }

    #[tokio::test]
    async fn test_pipeline_circuit_breaker_triggers() {
        // Feed the pipeline pre-discovered files that point to nonexistent paths.
        // Every file read will fail, triggering the consecutive-failure circuit breaker.
        let fake_files: Vec<discovery::DiscoveredFile> = (0..6)
            .map(|i| discovery::DiscoveredFile {
                relative_path: format!("nonexistent_{i}.rs"),
                absolute_path: PathBuf::from(format!("/nonexistent/path_{i}.rs")),
                language: crate::domain::LanguageId::Rust,
            })
            .collect();

        let pipeline = IndexingPipeline::new("test-cb".into(), PathBuf::from("/tmp"))
            .with_concurrency(1)
            .with_circuit_breaker(3);
        let result = pipeline.process_discovered(fake_files).await;

        assert_eq!(result.status, IndexRunStatus::Aborted);
        assert!(result.error_summary.is_some());
        assert!(result.error_summary.as_ref().unwrap().contains("circuit breaker"));
        // Threshold 3, concurrency 1: exactly 3 files fail before breaker trips,
        // then the early-exit check stops spawning remaining files.
        assert!(
            result.results.len() <= 4,
            "expected at most 4 results (3 failures + possible 1 in-flight), got {}",
            result.results.len()
        );
        assert!(result.results.iter().all(|r| matches!(
            r.outcome,
            FileOutcome::Failed { .. }
        )));
    }

    #[tokio::test]
    async fn test_pipeline_progress_tracking() {
        let dir = temp_repo_with_files(&[
            ("a.rs", "fn a() {}"),
            ("b.py", "def b(): pass"),
            ("c.go", "package main\nfunc c() {}"),
        ]);

        let pipeline = IndexingPipeline::new("test-prog".into(), dir.path().to_path_buf())
            .with_concurrency(1);
        let progress = pipeline.progress();
        let result = pipeline.execute().await;

        assert_eq!(result.status, IndexRunStatus::Succeeded);
        assert_eq!(progress.total_files.load(Ordering::Relaxed), 3);
        assert_eq!(progress.files_processed.load(Ordering::Relaxed), 3);
        assert_eq!(progress.files_failed.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn test_pipeline_empty_repo() {
        let dir = tempfile::tempdir().unwrap();
        let pipeline = IndexingPipeline::new("test-empty".into(), dir.path().to_path_buf());
        let result = pipeline.execute().await;

        assert_eq!(result.status, IndexRunStatus::Succeeded);
        assert!(result.results.is_empty());
    }

    #[tokio::test]
    async fn test_pipeline_discovery_failure() {
        let pipeline =
            IndexingPipeline::new("test-bad".into(), PathBuf::from("/nonexistent/path/repo"));
        let result = pipeline.execute().await;

        assert_eq!(result.status, IndexRunStatus::Failed);
        assert!(result.error_summary.is_some());
    }

    #[tokio::test]
    async fn test_pipeline_with_cas_persists_file_records() {
        use crate::config::BlobStoreConfig;
        use crate::storage::LocalCasBlobStore;

        let repo_dir = temp_repo_with_files(&[
            ("main.rs", "fn main() {}"),
            ("lib.py", "def foo(): pass"),
        ]);
        let cas_dir = tempfile::tempdir().unwrap();
        let cas = Arc::new(LocalCasBlobStore::new(BlobStoreConfig {
            root_dir: cas_dir.path().to_path_buf(),
        }));

        let pipeline = IndexingPipeline::new("test-cas".into(), repo_dir.path().to_path_buf())
            .with_cas(cas, "repo-1".to_string())
            .with_concurrency(1);
        let result = pipeline.execute().await;

        assert_eq!(result.status, IndexRunStatus::Succeeded);
        assert_eq!(result.results.len(), 2);
        assert_eq!(result.file_records.len(), 2);
        for record in &result.file_records {
            assert_eq!(record.run_id, "test-cas");
            assert_eq!(record.repo_id, "repo-1");
            assert!(!record.blob_id.is_empty());
            assert!(record.committed_at_unix_ms > 0);
        }
    }

    #[tokio::test]
    async fn test_pipeline_without_cas_produces_no_file_records() {
        let dir = temp_repo_with_files(&[("main.rs", "fn main() {}")]);

        let pipeline = IndexingPipeline::new("test-no-cas".into(), dir.path().to_path_buf())
            .with_concurrency(1);
        let result = pipeline.execute().await;

        assert_eq!(result.status, IndexRunStatus::Succeeded);
        assert_eq!(result.results.len(), 1);
        assert!(result.file_records.is_empty());
    }

    #[tokio::test]
    async fn test_pipeline_cas_systemic_error_aborts_via_circuit_breaker() {
        use std::path::Path;

        use crate::domain::ComponentHealth;
        use crate::error::TokenizorError;
        use crate::storage::StoredBlob;

        struct SystemicFailCas {
            root: PathBuf,
        }

        impl BlobStore for SystemicFailCas {
            fn backend_name(&self) -> &'static str {
                "systemic_fail"
            }

            fn root_dir(&self) -> &Path {
                &self.root
            }

            fn initialize(&self) -> crate::error::Result<ComponentHealth> {
                unreachable!("initialize not needed in systemic fail test")
            }

            fn health_check(&self) -> crate::error::Result<ComponentHealth> {
                unreachable!("health_check not needed in systemic fail test")
            }

            fn store_bytes(&self, _bytes: &[u8]) -> crate::error::Result<StoredBlob> {
                Err(TokenizorError::Storage("CAS write error".into()))
            }

            fn read_bytes(&self, _blob_id: &str) -> crate::error::Result<Vec<u8>> {
                unreachable!("read_bytes not needed in systemic fail test")
            }
        }

        let repo_dir = temp_repo_with_files(&[
            ("main.rs", "fn main() {}"),
            ("lib.py", "def foo(): pass"),
            ("app.go", "package main\nfunc main() {}"),
        ]);

        // CAS root doesn't exist → commit_file_result returns systemic Storage error
        let cas: Arc<dyn BlobStore> = Arc::new(SystemicFailCas {
            root: PathBuf::from("/nonexistent/cas/root"),
        });

        let pipeline = IndexingPipeline::new("test-systemic".into(), repo_dir.path().to_path_buf())
            .with_cas(cas, "repo-1".to_string())
            .with_concurrency(1);
        let result = pipeline.execute().await;

        assert_eq!(result.status, IndexRunStatus::Aborted);
        assert!(result.error_summary.is_some());
        assert!(
            result
                .error_summary
                .as_ref()
                .unwrap()
                .contains("circuit breaker")
        );
        // No file records should exist — systemic CAS error prevents record creation
        assert!(result.file_records.is_empty());
    }
}
