use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::domain::{
    DiscoveryManifest, FileOutcome, FileProcessingResult, FileRecord, IndexRunStatus, LanguageId,
    PersistedFileOutcome, RunPhase, SupportTier, unix_timestamp_ms,
};
use crate::error::{Result, TokenizorError};
use crate::indexing::{commit, discovery};
use crate::parsing;
use crate::storage::BlobStore;

pub struct PipelineProgress {
    pub total_files: AtomicU64,
    pub files_processed: AtomicU64,
    pub files_failed: AtomicU64,
    pub symbols_extracted: AtomicU64,
    phase: AtomicU8,
}

impl PipelineProgress {
    pub fn new() -> Self {
        Self {
            total_files: AtomicU64::new(0),
            files_processed: AtomicU64::new(0),
            files_failed: AtomicU64::new(0),
            symbols_extracted: AtomicU64::new(0),
            phase: AtomicU8::new(RunPhase::Discovering.to_u8()),
        }
    }

    pub fn set_phase(&self, phase: RunPhase) {
        self.phase.store(phase.to_u8(), Ordering::Release);
    }

    pub fn phase(&self) -> RunPhase {
        RunPhase::from_u8(self.phase.load(Ordering::Acquire))
    }
}

pub struct PipelineResult {
    pub status: IndexRunStatus,
    pub results: Vec<FileProcessingResult>,
    pub file_records: Vec<FileRecord>,
    pub not_yet_supported: BTreeMap<LanguageId, u64>,
    pub error_summary: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PipelineResumeState {
    pub cursor: String,
    pub total_files: u64,
    pub files_processed: u64,
    pub symbols_extracted: u64,
    pub files_failed: u64,
    pub manifest_paths: Vec<String>,
}

struct CheckpointState {
    completed_indices: Vec<bool>,
    sorted_paths: Vec<String>,
}

pub struct CheckpointTracker {
    state: Mutex<CheckpointState>,
}

impl CheckpointTracker {
    fn new() -> Self {
        Self {
            state: Mutex::new(CheckpointState {
                completed_indices: Vec::new(),
                sorted_paths: Vec::new(),
            }),
        }
    }

    #[cfg(test)]
    fn initialize(&self, paths: Vec<String>) {
        self.initialize_with_completed_prefix(paths, None);
    }

    fn initialize_with_completed_prefix(
        &self,
        paths: Vec<String>,
        completed_through: Option<usize>,
    ) {
        let mut state = self.state.lock().unwrap();
        let len = paths.len();
        state.sorted_paths = paths;
        state.completed_indices = vec![false; len];
        if let Some(last_completed) = completed_through {
            let capped = last_completed.min(len.saturating_sub(1));
            for index in 0..=capped {
                state.completed_indices[index] = true;
            }
        }
    }

    fn mark_complete(&self, index: usize) {
        let mut state = self.state.lock().unwrap();
        if index < state.completed_indices.len() {
            state.completed_indices[index] = true;
        }
    }

    pub fn checkpoint_cursor(&self) -> Option<String> {
        let state = self.state.lock().unwrap();
        if state.completed_indices.is_empty() {
            return None;
        }
        let mut high_water: Option<usize> = None;
        for (i, &done) in state.completed_indices.iter().enumerate() {
            if done {
                high_water = Some(i);
            } else {
                break;
            }
        }
        high_water.map(|i| state.sorted_paths[i].clone())
    }
}

pub struct IndexingPipeline {
    run_id: String,
    repo_id: String,
    repo_root: PathBuf,
    concurrency_cap: usize,
    circuit_breaker_threshold: usize,
    progress: Arc<PipelineProgress>,
    cas: Option<Arc<dyn BlobStore>>,
    cancellation_token: CancellationToken,
    checkpoint_tracker: Arc<CheckpointTracker>,
    discovery_manifest_callback:
        Option<Box<dyn Fn(&DiscoveryManifest) -> Result<()> + Send + Sync>>,
    durable_record_callback: Option<Box<dyn Fn(&FileRecord) -> Result<()> + Send + Sync>>,
    checkpoint_callback: Option<Box<dyn Fn() + Send + Sync>>,
    checkpoint_interval: u64,
    resume_from: Option<PipelineResumeState>,
}

impl IndexingPipeline {
    pub fn new(run_id: String, repo_root: PathBuf, cancellation_token: CancellationToken) -> Self {
        let concurrency_cap = num_cpus::get().max(1).min(16);
        Self {
            run_id,
            repo_id: String::new(),
            repo_root,
            concurrency_cap,
            circuit_breaker_threshold: 5,
            progress: Arc::new(PipelineProgress::new()),
            cas: None,
            cancellation_token,
            checkpoint_tracker: Arc::new(CheckpointTracker::new()),
            discovery_manifest_callback: None,
            durable_record_callback: None,
            checkpoint_callback: None,
            checkpoint_interval: 100,
            resume_from: None,
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

    pub fn with_checkpoint_callback(
        mut self,
        callback: Box<dyn Fn() + Send + Sync>,
        interval: u64,
    ) -> Self {
        self.checkpoint_callback = Some(callback);
        self.checkpoint_interval = interval.max(1);
        self
    }

    pub fn with_durable_record_callback(
        mut self,
        callback: Box<dyn Fn(&FileRecord) -> Result<()> + Send + Sync>,
    ) -> Self {
        self.durable_record_callback = Some(callback);
        self
    }

    pub fn with_discovery_manifest_callback(
        mut self,
        callback: Box<dyn Fn(&DiscoveryManifest) -> Result<()> + Send + Sync>,
    ) -> Self {
        self.discovery_manifest_callback = Some(callback);
        self
    }

    pub fn with_resume_state(mut self, resume_from: PipelineResumeState) -> Self {
        self.progress
            .total_files
            .store(resume_from.total_files, Ordering::Relaxed);
        self.progress
            .files_processed
            .store(resume_from.files_processed, Ordering::Relaxed);
        self.progress
            .symbols_extracted
            .store(resume_from.symbols_extracted, Ordering::Relaxed);
        self.progress
            .files_failed
            .store(resume_from.files_failed, Ordering::Relaxed);
        self.resume_from = Some(resume_from);
        self
    }

    pub fn progress(&self) -> Arc<PipelineProgress> {
        self.progress.clone()
    }

    pub fn checkpoint_tracker(&self) -> Arc<CheckpointTracker> {
        self.checkpoint_tracker.clone()
    }

    pub fn prepare_indexable_files(
        files: Vec<discovery::DiscoveredFile>,
    ) -> (Vec<discovery::DiscoveredFile>, BTreeMap<LanguageId, u64>) {
        let (mut indexable, not_yet_supported_files): (Vec<_>, Vec<_>) = files
            .into_iter()
            .partition(|f| f.language.support_tier() != SupportTier::Unsupported);

        let mut not_yet_supported = BTreeMap::new();
        for file in &not_yet_supported_files {
            *not_yet_supported
                .entry(file.language.clone())
                .or_insert(0u64) += 1;
        }

        indexable.sort_by(|a, b| {
            a.relative_path
                .to_lowercase()
                .cmp(&b.relative_path.to_lowercase())
                .then_with(|| a.relative_path.cmp(&b.relative_path))
        });

        (indexable, not_yet_supported)
    }

    pub async fn execute(self) -> PipelineResult {
        info!(run_id = %self.run_id, root = %self.repo_root.display(), "pipeline starting");

        if self.cancellation_token.is_cancelled() {
            info!(run_id = %self.run_id, "pipeline cancelled before discovery");
            self.progress.set_phase(RunPhase::Complete);
            return PipelineResult {
                status: IndexRunStatus::Cancelled,
                results: vec![],
                file_records: vec![],
                not_yet_supported: BTreeMap::new(),
                error_summary: None,
            };
        }

        let files = if let Some(resume) = &self.resume_from {
            match manifest_files_for_resume(&self.repo_root, &resume.manifest_paths) {
                Ok(files) => files,
                Err(error) => {
                    error!(
                        run_id = %self.run_id,
                        error = %error,
                        "failed to materialize persisted discovery manifest"
                    );
                    return PipelineResult {
                        status: IndexRunStatus::Failed,
                        results: vec![],
                        file_records: vec![],
                        not_yet_supported: BTreeMap::new(),
                        error_summary: Some(format!(
                            "persisted discovery manifest could not be materialized: {error}"
                        )),
                    };
                }
            }
        } else {
            match discovery::discover_files(&self.repo_root) {
                Ok(files) => files,
                Err(e) => {
                    error!(run_id = %self.run_id, error = %e, "file discovery failed");
                    return PipelineResult {
                        status: IndexRunStatus::Failed,
                        results: vec![],
                        file_records: vec![],
                        not_yet_supported: BTreeMap::new(),
                        error_summary: Some(format!("discovery failed: {e}")),
                    };
                }
            }
        };

        self.process_discovered(files).await
    }

    async fn process_discovered(mut self, files: Vec<discovery::DiscoveredFile>) -> PipelineResult {
        let (indexable, not_yet_supported) = Self::prepare_indexable_files(files);
        if !not_yet_supported.is_empty() {
            let total_unsupported: u64 = not_yet_supported.values().sum();
            info!(
                run_id = %self.run_id,
                count = total_unsupported,
                languages = not_yet_supported.len(),
                "discovered {total_unsupported} not-yet-supported files across {} languages",
                not_yet_supported.len()
            );
        }

        let total = indexable.len() as u64;
        if self.resume_from.is_none() {
            let manifest = DiscoveryManifest {
                run_id: self.run_id.clone(),
                discovered_at_unix_ms: unix_timestamp_ms(),
                relative_paths: indexable
                    .iter()
                    .map(|file| file.relative_path.clone())
                    .collect(),
            };
            if let Some(callback) = &self.discovery_manifest_callback {
                if let Err(error) = callback(&manifest) {
                    return PipelineResult {
                        status: IndexRunStatus::Failed,
                        results: vec![],
                        file_records: vec![],
                        not_yet_supported,
                        error_summary: Some(format!(
                            "failed to persist discovery manifest: {error}"
                        )),
                    };
                }
            }
        }
        self.progress.total_files.store(total, Ordering::Relaxed);
        self.progress.set_phase(RunPhase::Processing);
        info!(run_id = %self.run_id, total_files = total, "discovery complete");

        if indexable.is_empty() {
            return PipelineResult {
                status: IndexRunStatus::Succeeded,
                results: vec![],
                file_records: vec![],
                not_yet_supported,
                error_summary: None,
            };
        }

        let files = indexable;
        let sorted_paths: Vec<String> = files.iter().map(|f| f.relative_path.clone()).collect();
        let resume_boundary = self
            .resume_from
            .as_ref()
            .and_then(|resume| sorted_paths.iter().position(|path| path == &resume.cursor));
        if let Some(resume) = &self.resume_from {
            if resume_boundary.is_none() {
                return PipelineResult {
                    status: IndexRunStatus::Failed,
                    results: vec![],
                    file_records: vec![],
                    not_yet_supported,
                    error_summary: Some(format!(
                        "resume cursor `{}` not found in deterministic discovery order",
                        resume.cursor
                    )),
                };
            }
        }
        self.checkpoint_tracker
            .initialize_with_completed_prefix(sorted_paths, resume_boundary);

        if let Some(resume) = &self.resume_from {
            self.progress
                .files_processed
                .store(resume.files_processed, Ordering::Relaxed);
            self.progress
                .symbols_extracted
                .store(resume.symbols_extracted, Ordering::Relaxed);
            self.progress
                .files_failed
                .store(resume.files_failed, Ordering::Relaxed);
        }

        let semaphore = Arc::new(Semaphore::new(self.concurrency_cap));
        let progress = self.progress.clone();
        let consecutive_failures = Arc::new(AtomicU64::new(0));
        let circuit_broken = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let threshold = self.circuit_breaker_threshold as u64;
        let cas = self.cas.clone();
        let checkpoint_tracker = self.checkpoint_tracker.clone();
        let durable_record_callback: Option<Arc<dyn Fn(&FileRecord) -> Result<()> + Send + Sync>> =
            self.durable_record_callback.take().map(Arc::from);
        let checkpoint_callback: Option<Arc<dyn Fn() + Send + Sync>> =
            self.checkpoint_callback.take().map(|cb| Arc::from(cb));
        let checkpoint_interval = self.checkpoint_interval;
        let spawn_from_index = resume_boundary.map_or(0usize, |index| index.saturating_add(1));

        let mut handles = Vec::with_capacity(files.len().saturating_sub(spawn_from_index));

        for (sorted_index, file) in files.into_iter().enumerate().skip(spawn_from_index) {
            // M3: Stop spawning tasks once the circuit breaker has tripped
            if circuit_broken.load(Ordering::Relaxed) {
                break;
            }

            // Cooperative cancellation: stop spawning new file tasks
            if self.cancellation_token.is_cancelled() {
                debug!(run_id = %self.run_id, "cancellation detected — stopping file spawn loop");
                break;
            }

            let permit = semaphore.clone().acquire_owned().await.unwrap();
            let progress = progress.clone();
            let consecutive_failures = consecutive_failures.clone();
            let circuit_broken = circuit_broken.clone();
            let run_id = self.run_id.clone();
            let repo_id = self.repo_id.clone();
            let cas = cas.clone();
            let tracker = checkpoint_tracker.clone();
            let durable_record_callback = durable_record_callback.clone();
            let cb = checkpoint_callback.clone();
            let cb_interval = checkpoint_interval;

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

                if let (Some(record), Some(persist_record)) =
                    (file_record.as_ref(), durable_record_callback.as_ref())
                {
                    if let Err(err) = persist_record(record) {
                        error!(
                            run_id = %run_id,
                            path = %record.relative_path,
                            error = %err,
                            "registry persistence failed for durable file record"
                        );
                        circuit_broken.store(true, Ordering::Relaxed);
                        drop(permit);
                        return Some((result, file_record));
                    }
                }

                // Record successful processing only after registry durability succeeds.
                let record_success =
                    |result: &FileProcessingResult, file_record: &Option<FileRecord>| {
                        let symbol_count = result.symbols.len() as u64;
                        progress
                            .symbols_extracted
                            .fetch_add(symbol_count, Ordering::Relaxed);
                        let processed =
                            progress.files_processed.fetch_add(1, Ordering::Relaxed) + 1;
                        // Mark complete in tracker only after durable CAS commit
                        if file_record.is_some() {
                            tracker.mark_complete(sorted_index);
                        }
                        // Periodic checkpoint callback
                        if cb_interval > 0 && processed % cb_interval == 0 {
                            if let Some(ref callback) = cb {
                                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    callback()
                                })) {
                                    Ok(()) => {}
                                    Err(_) => {
                                        warn!(run_id = %run_id, "checkpoint callback panicked")
                                    }
                                }
                            }
                        }
                    };

                match &result.outcome {
                    FileOutcome::Processed => {
                        consecutive_failures.store(0, Ordering::Relaxed);
                        record_success(&result, &file_record);
                        debug!(run_id = %run_id, path = %result.relative_path, "processed");
                    }
                    FileOutcome::PartialParse { warning } => {
                        consecutive_failures.store(0, Ordering::Relaxed);
                        record_success(&result, &file_record);
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

        // If cancelled, set phase to Complete and return Cancelled immediately
        if self.cancellation_token.is_cancelled() {
            info!(run_id = %self.run_id, "pipeline cancelled — returning Cancelled status");
            self.progress.set_phase(RunPhase::Complete);
            return PipelineResult {
                status: IndexRunStatus::Cancelled,
                results,
                file_records,
                not_yet_supported,
                error_summary: None,
            };
        }

        self.progress.set_phase(RunPhase::Finalizing);

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

        let not_yet_supported_summary = if !not_yet_supported.is_empty() {
            let total_unsupported: u64 = not_yet_supported.values().sum();
            format!(
                "; not-yet-supported: {total_unsupported} files across {} languages",
                not_yet_supported.len()
            )
        } else {
            String::new()
        };

        let (status, error_summary) = if was_broken {
            info!(run_id = %self.run_id, "pipeline aborted by circuit breaker");
            (
                IndexRunStatus::Aborted,
                Some(format!(
                    "circuit breaker triggered after {failed_count} failures{not_yet_supported_summary}"
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
                     {quarantined_count} quarantined{not_yet_supported_summary}"
                )),
            )
        } else {
            info!(
                run_id = %self.run_id,
                persisted = persisted_count,
                committed = committed_count,
                "pipeline succeeded{not_yet_supported_summary}"
            );
            let error_summary = if not_yet_supported_summary.is_empty() {
                None
            } else {
                Some(
                    not_yet_supported_summary
                        .trim_start_matches("; ")
                        .to_string(),
                )
            };
            (IndexRunStatus::Succeeded, error_summary)
        };

        self.progress.set_phase(RunPhase::Complete);

        PipelineResult {
            status,
            results,
            file_records,
            not_yet_supported,
            error_summary,
        }
    }
}

fn manifest_files_for_resume(
    repo_root: &PathBuf,
    manifest_paths: &[String],
) -> Result<Vec<discovery::DiscoveredFile>> {
    let mut files = Vec::with_capacity(manifest_paths.len());
    for relative_path in manifest_paths {
        let manifest_path = PathBuf::from(relative_path);
        let extension = manifest_path
            .extension()
            .and_then(|ext| ext.to_str())
            .ok_or_else(|| {
                TokenizorError::InvalidOperation(format!(
                    "persisted discovery manifest path `{relative_path}` is missing a supported file extension"
                ))
            })?;
        let language = LanguageId::from_extension(extension).ok_or_else(|| {
            TokenizorError::InvalidOperation(format!(
                "persisted discovery manifest path `{relative_path}` uses unsupported extension `{extension}`"
            ))
        })?;
        files.push(discovery::DiscoveredFile {
            relative_path: relative_path.clone(),
            absolute_path: repo_root.join(relative_path),
            language,
        });
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tokio_util::sync::CancellationToken;

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
        let dir =
            temp_repo_with_files(&[("main.rs", "fn main() {}"), ("lib.py", "def foo(): pass")]);

        let pipeline = IndexingPipeline::new(
            "test-run".into(),
            dir.path().to_path_buf(),
            CancellationToken::new(),
        )
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

        let pipeline = IndexingPipeline::new(
            "test-cb".into(),
            PathBuf::from("/tmp"),
            CancellationToken::new(),
        )
        .with_concurrency(1)
        .with_circuit_breaker(3);
        let result = pipeline.process_discovered(fake_files).await;

        assert_eq!(result.status, IndexRunStatus::Aborted);
        assert!(result.error_summary.is_some());
        assert!(
            result
                .error_summary
                .as_ref()
                .unwrap()
                .contains("circuit breaker")
        );
        // Threshold 3, concurrency 1: exactly 3 files fail before breaker trips,
        // then the early-exit check stops spawning remaining files.
        assert!(
            result.results.len() <= 4,
            "expected at most 4 results (3 failures + possible 1 in-flight), got {}",
            result.results.len()
        );
        assert!(
            result
                .results
                .iter()
                .all(|r| matches!(r.outcome, FileOutcome::Failed { .. }))
        );
    }

    #[tokio::test]
    async fn test_pipeline_progress_tracking() {
        let dir = temp_repo_with_files(&[
            ("a.rs", "fn a() {}"),
            ("b.py", "def b(): pass"),
            ("c.go", "package main\nfunc c() {}"),
        ]);

        let pipeline = IndexingPipeline::new(
            "test-prog".into(),
            dir.path().to_path_buf(),
            CancellationToken::new(),
        )
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
        let pipeline = IndexingPipeline::new(
            "test-empty".into(),
            dir.path().to_path_buf(),
            CancellationToken::new(),
        );
        let result = pipeline.execute().await;

        assert_eq!(result.status, IndexRunStatus::Succeeded);
        assert!(result.results.is_empty());
    }

    #[tokio::test]
    async fn test_pipeline_discovery_failure() {
        let pipeline = IndexingPipeline::new(
            "test-bad".into(),
            PathBuf::from("/nonexistent/path/repo"),
            CancellationToken::new(),
        );
        let result = pipeline.execute().await;

        assert_eq!(result.status, IndexRunStatus::Failed);
        assert!(result.error_summary.is_some());
    }

    #[tokio::test]
    async fn test_pipeline_with_cas_persists_file_records() {
        use crate::config::BlobStoreConfig;
        use crate::storage::LocalCasBlobStore;

        let repo_dir =
            temp_repo_with_files(&[("main.rs", "fn main() {}"), ("lib.py", "def foo(): pass")]);
        let cas_dir = tempfile::tempdir().unwrap();
        let cas = Arc::new(LocalCasBlobStore::new(BlobStoreConfig {
            root_dir: cas_dir.path().to_path_buf(),
        }));

        let pipeline = IndexingPipeline::new(
            "test-cas".into(),
            repo_dir.path().to_path_buf(),
            CancellationToken::new(),
        )
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

        let pipeline = IndexingPipeline::new(
            "test-no-cas".into(),
            dir.path().to_path_buf(),
            CancellationToken::new(),
        )
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

        let pipeline = IndexingPipeline::new(
            "test-systemic".into(),
            repo_dir.path().to_path_buf(),
            CancellationToken::new(),
        )
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

    #[tokio::test]
    async fn test_pipeline_partitions_unsupported_files() {
        let dir = temp_repo_with_files(&[
            ("main.rs", "fn main() {}"),
            ("App.java", "class App {}"),
            ("app.rb", "def hello; end"),
            ("main.cs", "class Main {}"),
        ]);

        let pipeline = IndexingPipeline::new(
            "test-partition".into(),
            dir.path().to_path_buf(),
            CancellationToken::new(),
        )
        .with_concurrency(1);
        let result = pipeline.execute().await;

        assert_eq!(result.status, IndexRunStatus::Succeeded);
        // Only Rust (QualityFocus) and Java (Broader) should be processed
        assert_eq!(result.results.len(), 2);
        assert!(
            result
                .results
                .iter()
                .any(|r| r.language == LanguageId::Rust)
        );
        assert!(
            result
                .results
                .iter()
                .any(|r| r.language == LanguageId::Java)
        );
        // Ruby and C# are Unsupported — counted but not processed
        assert_eq!(result.not_yet_supported.len(), 2);
        assert_eq!(result.not_yet_supported[&LanguageId::Ruby], 1);
        assert_eq!(result.not_yet_supported[&LanguageId::CSharp], 1);
    }

    #[test]
    fn test_pipeline_progress_phase_defaults_to_discovering() {
        let progress = PipelineProgress::new();
        assert_eq!(progress.phase(), RunPhase::Discovering);
    }

    #[test]
    fn test_pipeline_progress_phase_round_trips_all_variants() {
        let progress = PipelineProgress::new();
        let variants = [
            RunPhase::Discovering,
            RunPhase::Processing,
            RunPhase::Finalizing,
            RunPhase::Complete,
        ];
        for phase in &variants {
            progress.set_phase(phase.clone());
            assert_eq!(progress.phase(), *phase);
        }
    }

    #[tokio::test]
    async fn test_pipeline_returns_cancelled_when_token_pre_cancelled() {
        let dir = temp_repo_with_files(&[("main.rs", "fn main() {}")]);
        let token = CancellationToken::new();
        token.cancel();

        let pipeline =
            IndexingPipeline::new("test-precancel".into(), dir.path().to_path_buf(), token);
        let result = pipeline.execute().await;

        assert_eq!(result.status, IndexRunStatus::Cancelled);
        assert!(result.results.is_empty());
        assert!(result.file_records.is_empty());
        assert!(result.error_summary.is_none());
    }

    #[tokio::test]
    async fn test_pipeline_checks_cancellation_between_files() {
        let dir = temp_repo_with_files(&[
            ("a.rs", "fn a() {}"),
            ("b.py", "def b(): pass"),
            ("c.go", "package main\nfunc c() {}"),
        ]);

        let token = CancellationToken::new();
        let token_clone = token.clone();

        // Cancel after a short delay so discovery completes but not all files process
        let pipeline =
            IndexingPipeline::new("test-midcancel".into(), dir.path().to_path_buf(), token)
                .with_concurrency(1);
        let progress = pipeline.progress();

        // Cancel immediately after spawning — with concurrency 1, the loop checks
        // cancellation before each spawn, so at most 1 file may already be in-flight
        token_clone.cancel();

        let result = pipeline.execute().await;

        assert_eq!(result.status, IndexRunStatus::Cancelled);
        assert!(result.error_summary.is_none());
        // With concurrency 1 and immediate cancellation, files_processed should be
        // less than total discovered (3 files for Rust, Python, Go)
        let processed = progress.files_processed.load(Ordering::Relaxed);
        let total = progress.total_files.load(Ordering::Relaxed);
        assert!(
            processed <= total,
            "expected processed ({processed}) <= total ({total})"
        );
    }

    #[test]
    fn test_checkpoint_cursor_returns_none_when_no_files_complete() {
        let tracker = CheckpointTracker::new();
        tracker.initialize(vec!["a.rs".into(), "b.rs".into(), "c.rs".into()]);
        assert!(tracker.checkpoint_cursor().is_none());
    }

    #[test]
    fn test_checkpoint_cursor_tracks_contiguous_completion() {
        let tracker = CheckpointTracker::new();
        tracker.initialize(vec![
            "a.rs".into(),
            "b.rs".into(),
            "c.rs".into(),
            "d.rs".into(),
            "e.rs".into(),
        ]);

        // Complete files 0, 1, 2 contiguously
        tracker.mark_complete(0);
        tracker.mark_complete(1);
        tracker.mark_complete(2);
        assert_eq!(tracker.checkpoint_cursor(), Some("c.rs".to_string()));

        // Complete file 4 (gap at 3) — cursor should NOT advance
        tracker.mark_complete(4);
        assert_eq!(tracker.checkpoint_cursor(), Some("c.rs".to_string()));
    }

    #[test]
    fn test_checkpoint_cursor_advances_when_gap_fills() {
        let tracker = CheckpointTracker::new();
        tracker.initialize(vec![
            "a.rs".into(),
            "b.rs".into(),
            "c.rs".into(),
            "d.rs".into(),
            "e.rs".into(),
        ]);

        tracker.mark_complete(0);
        tracker.mark_complete(1);
        tracker.mark_complete(2);
        tracker.mark_complete(4);
        assert_eq!(tracker.checkpoint_cursor(), Some("c.rs".to_string()));

        // Fill the gap at 3
        tracker.mark_complete(3);
        assert_eq!(tracker.checkpoint_cursor(), Some("e.rs".to_string()));
    }

    #[test]
    fn test_checkpoint_cursor_returns_none_with_empty_tracker() {
        let tracker = CheckpointTracker::new();
        assert!(tracker.checkpoint_cursor().is_none());
    }

    #[tokio::test]
    async fn test_pipeline_invokes_checkpoint_callback_at_interval() {
        use crate::config::BlobStoreConfig;
        use crate::storage::LocalCasBlobStore;
        use std::sync::atomic::AtomicUsize;

        let repo_dir = temp_repo_with_files(&[
            ("a.go", "package main\nfunc a() {}"),
            ("b.py", "def b(): pass"),
            ("c.rs", "fn c() {}"),
            ("d.go", "package main\nfunc d() {}"),
            ("e.py", "def e(): pass"),
        ]);
        let cas_dir = tempfile::tempdir().unwrap();
        let cas = Arc::new(LocalCasBlobStore::new(BlobStoreConfig {
            root_dir: cas_dir.path().to_path_buf(),
        }));

        let call_count = Arc::new(AtomicUsize::new(0));
        let counter = call_count.clone();
        let callback = Box::new(move || {
            counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        });

        let pipeline = IndexingPipeline::new(
            "test-cb-interval".into(),
            repo_dir.path().to_path_buf(),
            CancellationToken::new(),
        )
        .with_cas(cas, "repo-1".to_string())
        .with_concurrency(1)
        .with_checkpoint_callback(callback, 2);
        let result = pipeline.execute().await;

        assert_eq!(result.status, IndexRunStatus::Succeeded);
        // With 5 files and interval 2, callback fires at files_processed=2 and 4
        let count = call_count.load(std::sync::atomic::Ordering::Relaxed);
        assert_eq!(
            count, 2,
            "expected callback at processed=2 and processed=4, got {count} calls"
        );
    }

    #[tokio::test]
    async fn test_pipeline_skips_checkpoint_when_no_callback() {
        let dir = temp_repo_with_files(&[("a.rs", "fn a() {}"), ("b.py", "def b(): pass")]);

        // No callback set — should not panic
        let pipeline = IndexingPipeline::new(
            "test-no-cb".into(),
            dir.path().to_path_buf(),
            CancellationToken::new(),
        )
        .with_concurrency(1);
        let result = pipeline.execute().await;

        assert_eq!(result.status, IndexRunStatus::Succeeded);
        assert_eq!(result.results.len(), 2);
    }
}
