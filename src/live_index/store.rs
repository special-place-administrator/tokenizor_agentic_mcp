use std::collections::{HashMap, HashSet};
use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, LockResult, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::{Duration, Instant, SystemTime};

use rayon::prelude::*;
use tracing::{error, info, warn};

use super::query::RepoOutlineView;
use crate::domain::index::{AdmissionTier, SkippedFile};
use crate::domain::{
    FileClassification, FileOutcome, FileProcessingResult, LanguageId, ReferenceRecord,
    SymbolRecord, find_enclosing_symbol,
};
use crate::error::Result;
use crate::{discovery, parsing};

/// Per-file parse status stored in the index.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ParseStatus {
    /// File parsed successfully with no syntax errors.
    Parsed,
    /// File parsed but tree-sitter reported syntax errors; symbols were still extracted.
    PartialParse { warning: String },
    /// File could not be parsed at all; symbols list is empty but content bytes are stored.
    Failed { error: String },
}

/// A single indexed file — all data needed for query and display.
#[derive(Clone, Debug)]
pub struct IndexedFile {
    pub relative_path: String,
    pub language: LanguageId,
    pub classification: FileClassification,
    /// Raw file bytes stored in memory (LIDX-03 — zero disk I/O on read path).
    pub content: Vec<u8>,
    /// Symbols extracted by the parser.
    pub symbols: Vec<SymbolRecord>,
    pub parse_status: ParseStatus,
    pub byte_len: u64,
    pub content_hash: String,
    /// Cross-references extracted by xref::extract_references (Phase 4).
    pub references: Vec<ReferenceRecord>,
    /// Import alias map for this file: alias -> original name.
    pub alias_map: HashMap<String, String>,
}

/// Identifies a single reference within a specific file.
/// Used as a value in `LiveIndex::reverse_index`.
#[derive(Clone, Debug)]
pub struct ReferenceLocation {
    /// Relative path of the file containing the reference.
    pub file_path: String,
    /// Index into `IndexedFile::references` for the specific `ReferenceRecord`.
    pub reference_idx: u32,
}

impl IndexedFile {
    /// Build an `IndexedFile` from a `FileProcessingResult` plus the raw bytes.
    ///
    /// Maps `FileOutcome` → `ParseStatus`:
    /// - `Processed`     → `ParseStatus::Parsed`
    /// - `PartialParse`  → `ParseStatus::PartialParse` (symbols kept)
    /// - `Failed`        → `ParseStatus::Failed` (empty symbols, content still stored)
    pub fn from_parse_result(result: FileProcessingResult, content: Vec<u8>) -> Self {
        let parse_status = match &result.outcome {
            FileOutcome::Processed => ParseStatus::Parsed,
            FileOutcome::PartialParse { warning } => ParseStatus::PartialParse {
                warning: warning.clone(),
            },
            FileOutcome::Failed { error } => ParseStatus::Failed {
                error: error.clone(),
            },
        };

        // Destructure the result so we can consume references while borrowing symbols.
        let FileProcessingResult {
            relative_path,
            language,
            classification,
            outcome: _,
            symbols,
            byte_len,
            content_hash,
            references: raw_references,
            alias_map,
        } = result;

        // Build a set of symbol byte ranges so we can filter definition-site hits
        // (Pitfall 1: a reference whose byte_range exactly matches a symbol's byte_range
        // is the definition itself — not a usage site).
        let symbol_byte_ranges: std::collections::HashSet<(u32, u32)> =
            symbols.iter().map(|s| s.byte_range).collect();

        // Assign enclosing_symbol_index for each reference and skip definition sites.
        let references: Vec<ReferenceRecord> = raw_references
            .into_iter()
            .filter(|r| !symbol_byte_ranges.contains(&r.byte_range))
            .map(|mut r| {
                if r.enclosing_symbol_index.is_none() {
                    r.enclosing_symbol_index = find_enclosing_symbol(&symbols, r.line_range.0);
                }
                r
            })
            .collect();

        IndexedFile {
            relative_path,
            language,
            classification,
            content,
            symbols,
            parse_status,
            byte_len,
            content_hash,
            references,
            alias_map,
        }
    }
}

impl AsRef<IndexedFile> for IndexedFile {
    fn as_ref(&self) -> &IndexedFile {
        self
    }
}

/// Tracks parse failures during index loading for the circuit breaker.
pub struct CircuitBreakerState {
    total: AtomicUsize,
    failed: AtomicUsize,
    tripped: AtomicBool,
    /// Failure threshold as a fraction (e.g., 0.20 = 20%).
    threshold: f64,
    /// First few failure details (path, reason) for summary reporting.
    failure_details: Mutex<Vec<(String, String)>>,
}

impl CircuitBreakerState {
    /// Create with an explicit threshold (for testability).
    pub fn new(threshold: f64) -> Self {
        Self {
            total: AtomicUsize::new(0),
            failed: AtomicUsize::new(0),
            tripped: AtomicBool::new(false),
            threshold,
            failure_details: Mutex::new(Vec::new()),
        }
    }

    /// Create using the `TOKENIZOR_CB_THRESHOLD` env var, defaulting to 0.20.
    pub fn from_env() -> Self {
        let threshold = std::env::var("TOKENIZOR_CB_THRESHOLD")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.20);
        Self::new(threshold)
    }

    pub fn record_success(&self) {
        self.total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_failure(&self, path: &str, reason: &str) {
        self.total.fetch_add(1, Ordering::Relaxed);
        self.failed.fetch_add(1, Ordering::Relaxed);

        let mut details = self.failure_details.lock().unwrap();
        if details.len() < 5 {
            details.push((path.to_string(), reason.to_string()));
        }
    }

    /// Returns `true` when the failure rate exceeds the threshold.
    ///
    /// IMPORTANT: returns `false` when fewer than 5 files have been processed
    /// (minimum-file guard prevents spurious trips on tiny repos).
    pub fn should_abort(&self) -> bool {
        let total = self.total.load(Ordering::Relaxed);
        if total < 5 {
            return false;
        }
        let failed = self.failed.load(Ordering::Relaxed);
        let rate = failed as f64 / total as f64;
        if rate > self.threshold {
            self.tripped.store(true, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    pub fn is_tripped(&self) -> bool {
        self.tripped.load(Ordering::Relaxed)
    }

    /// One-line summary plus top failure details.
    pub fn summary(&self) -> String {
        let total = self.total.load(Ordering::Relaxed);
        let failed = self.failed.load(Ordering::Relaxed);
        let rate = if total > 0 {
            (failed as f64 / total as f64 * 100.0) as u32
        } else {
            0
        };

        let details = self.failure_details.lock().unwrap();
        let top_failures: Vec<String> = details
            .iter()
            .take(3)
            .map(|(p, r)| format!("  - {p}: {r}"))
            .collect();

        let mut msg = format!(
            "circuit breaker tripped: {failed}/{total} files failed ({rate}% > {}%)",
            (self.threshold * 100.0) as u32
        );
        if !top_failures.is_empty() {
            msg.push_str("\nTop failures:\n");
            msg.push_str(&top_failures.join("\n"));
        }
        msg
    }
}

/// Overall state of the index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IndexState {
    /// Index was constructed with empty() — no files loaded yet.
    Empty,
    Loading,
    Ready,
    CircuitBreakerTripped {
        summary: String,
    },
}

/// Where the current in-memory index contents were sourced from.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum IndexLoadSource {
    EmptyBootstrap,
    FreshLoad,
    SnapshotRestore,
}

/// Reconciliation status after restoring from a persisted snapshot.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SnapshotVerifyState {
    NotNeeded,
    Pending,
    Running,
    Completed,
}

/// Compact published status label for handle-level state consumers.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PublishedIndexStatus {
    Empty,
    Loading,
    Ready,
    Degraded,
}

/// Lightweight published state captured from the live index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublishedIndexState {
    pub generation: u64,
    pub status: PublishedIndexStatus,
    pub degraded_summary: Option<String>,
    pub file_count: usize,
    pub parsed_count: usize,
    pub partial_parse_count: usize,
    pub failed_count: usize,
    pub symbol_count: usize,
    pub loaded_at_system: SystemTime,
    pub load_duration: Duration,
    pub load_source: IndexLoadSource,
    pub snapshot_verify_state: SnapshotVerifyState,
    pub is_empty: bool,
    /// Admission tier counts: (Tier1 indexed, Tier2 metadata-only, Tier3 hard-skipped).
    pub tier_counts: (usize, usize, usize),
}

/// The in-memory index: file contents and parsed symbols for all discovered files.
pub struct LiveIndex {
    /// Keyed by `relative_path` (forward-slash normalized).
    pub(crate) files: HashMap<String, Arc<IndexedFile>>,
    pub(crate) loaded_at: Instant,
    /// Wall-clock time when index was last loaded. Used by what_changed tool.
    pub(crate) loaded_at_system: SystemTime,
    pub(crate) load_duration: Duration,
    pub(crate) cb_state: CircuitBreakerState,
    /// True when constructed with empty() and reload() has not been called.
    pub(crate) is_empty: bool,
    /// Provenance for the current live contents.
    pub(crate) load_source: IndexLoadSource,
    /// Snapshot reconciliation status for snapshot-restored indices.
    pub(crate) snapshot_verify_state: SnapshotVerifyState,
    /// Repo-level reverse index: reference name -> all locations in the index.
    /// Updated incrementally on single-file mutations (update_file, remove_file);
    /// rebuilt from scratch on bulk operations (load, reload, snapshot restore).
    pub(crate) reverse_index: HashMap<String, Vec<ReferenceLocation>>,
    /// Secondary path index: lowercase basename -> sorted matching relative paths.
    pub(crate) files_by_basename: HashMap<String, Vec<String>>,
    /// Secondary path index: lowercase directory component -> sorted matching relative paths.
    pub(crate) files_by_dir_component: HashMap<String, Vec<String>>,
    /// Trigram search index for file-level text search acceleration.
    pub(crate) trigram_index: super::trigram::TrigramIndex,
    /// Compiled gitignore patterns loaded at index time. Used by NoisePolicy
    /// to classify files as vendor/generated/ignored noise.
    pub(crate) gitignore: Option<ignore::gitignore::Gitignore>,
    /// Files that were not fully indexed (Tier 2 metadata-only or Tier 3 hard-skipped).
    pub(crate) skipped_files: Vec<SkippedFile>,
}

/// Central shared handle for the live in-memory index.
///
/// This is intentionally a thin compatibility shell over the current `RwLock<LiveIndex>` so the
/// project can later attach published read snapshots or other state-machine metadata here without
/// another repo-wide alias migration.
pub struct SharedIndexHandle {
    live: RwLock<LiveIndex>,
    published_state: RwLock<Arc<PublishedIndexState>>,
    published_repo_outline: RwLock<Arc<RepoOutlineView>>,
    next_generation: AtomicU64,
    /// Git temporal intelligence — independently locked side-table with
    /// per-file churn, ownership, and co-change data. Populated asynchronously
    /// after index load/reload completes.
    git_temporal: RwLock<Arc<super::git_temporal::GitTemporalIndex>>,
}

/// Write guard that republishes lightweight handle state when mutated data is released.
pub struct SharedIndexWriteGuard<'a> {
    handle: &'a SharedIndexHandle,
    guard: RwLockWriteGuard<'a, LiveIndex>,
    dirty: bool,
}

impl SharedIndexHandle {
    pub fn new(index: LiveIndex) -> Self {
        let published_state = Arc::new(PublishedIndexState::capture(0, &index));
        let published_repo_outline = Arc::new(index.capture_repo_outline_view());
        Self {
            live: RwLock::new(index),
            published_state: RwLock::new(published_state),
            published_repo_outline: RwLock::new(published_repo_outline),
            next_generation: AtomicU64::new(1),
            git_temporal: RwLock::new(Arc::new(super::git_temporal::GitTemporalIndex::pending())),
        }
    }

    pub fn shared(index: LiveIndex) -> Arc<Self> {
        Arc::new(Self::new(index))
    }

    pub fn read(&self) -> LockResult<RwLockReadGuard<'_, LiveIndex>> {
        self.live.read()
    }

    pub fn write(
        &self,
    ) -> std::result::Result<
        SharedIndexWriteGuard<'_>,
        std::sync::PoisonError<RwLockWriteGuard<'_, LiveIndex>>,
    > {
        self.live.write().map(|guard| SharedIndexWriteGuard {
            handle: self,
            guard,
            dirty: false,
        })
    }

    pub fn published_state(&self) -> Arc<PublishedIndexState> {
        self.published_state.read().expect("lock poisoned").clone()
    }

    pub fn published_repo_outline(&self) -> Arc<RepoOutlineView> {
        self.published_repo_outline
            .read()
            .expect("lock poisoned")
            .clone()
    }

    pub fn reload(&self, root: &Path) -> crate::error::Result<()> {
        // Build new index data OUTSIDE the write lock (file I/O + parsing).
        // Only the final swap acquires the lock, reducing block time from
        // seconds (full I/O) to milliseconds (in-memory index rebuild).
        let data = LiveIndex::build_reload_data(root)?;
        let mut live = self.live.write().expect("lock poisoned");
        live.apply_reload_data(data);
        self.publish_locked(&live);
        Ok(())
    }

    pub fn update_file(&self, path: String, file: IndexedFile) {
        let mut live = self.live.write().expect("lock poisoned");
        live.update_file(path, file);
        self.publish_locked(&live);
    }

    pub fn add_file(&self, path: String, file: IndexedFile) {
        let mut live = self.live.write().expect("lock poisoned");
        live.add_file(path, file);
        self.publish_locked(&live);
    }

    pub fn remove_file(&self, path: &str) {
        let mut live = self.live.write().expect("lock poisoned");
        live.remove_file(path);
        self.publish_locked(&live);
    }

    pub fn mark_snapshot_verify_running(&self) {
        let mut live = self.live.write().expect("lock poisoned");
        live.mark_snapshot_verify_running();
        self.publish_locked(&live);
    }

    pub fn mark_snapshot_verify_completed(&self) {
        let mut live = self.live.write().expect("lock poisoned");
        live.mark_snapshot_verify_completed();
        self.publish_locked(&live);
    }

    fn publish_locked(&self, live: &LiveIndex) {
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        let published_state = Arc::new(PublishedIndexState::capture(generation, live));
        let published_repo_outline = Arc::new(live.capture_repo_outline_view());
        *self.published_state.write().expect("lock poisoned") = published_state;
        *self.published_repo_outline.write().expect("lock poisoned") = published_repo_outline;
    }

    /// Read the current git temporal index (lock-free Arc clone).
    pub fn git_temporal(&self) -> Arc<super::git_temporal::GitTemporalIndex> {
        self.git_temporal.read().expect("lock poisoned").clone()
    }

    /// Atomically replace the git temporal index with a new version.
    pub fn update_git_temporal(&self, index: super::git_temporal::GitTemporalIndex) {
        *self.git_temporal.write().expect("lock poisoned") = Arc::new(index);
    }
}

impl<'a> Deref for SharedIndexWriteGuard<'a> {
    type Target = LiveIndex;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl DerefMut for SharedIndexWriteGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.dirty = true;
        &mut self.guard
    }
}

impl Drop for SharedIndexWriteGuard<'_> {
    fn drop(&mut self) {
        if self.dirty {
            self.handle.publish_locked(&self.guard);
        }
    }
}

/// Thread-safe shared handle to the index.
pub type SharedIndex = Arc<SharedIndexHandle>;

impl PublishedIndexState {
    fn capture(generation: u64, index: &LiveIndex) -> Self {
        let (status, degraded_summary) = match index.index_state() {
            IndexState::Empty => (PublishedIndexStatus::Empty, None),
            IndexState::Loading => (PublishedIndexStatus::Loading, None),
            IndexState::Ready => (PublishedIndexStatus::Ready, None),
            IndexState::CircuitBreakerTripped { summary } => {
                (PublishedIndexStatus::Degraded, Some(summary))
            }
        };
        let stats = index.health_stats();
        Self {
            generation,
            status,
            degraded_summary,
            file_count: stats.file_count,
            parsed_count: stats.parsed_count,
            partial_parse_count: stats.partial_parse_count,
            failed_count: stats.failed_count,
            symbol_count: stats.symbol_count,
            loaded_at_system: index.loaded_at_system,
            load_duration: stats.load_duration,
            load_source: index.load_source,
            snapshot_verify_state: index.snapshot_verify_state,
            is_empty: index.is_empty,
            tier_counts: stats.tier_counts,
        }
    }

    pub fn status_label(&self) -> &'static str {
        match self.status {
            PublishedIndexStatus::Empty => "Empty",
            PublishedIndexStatus::Loading => "Loading",
            PublishedIndexStatus::Ready => "Ready",
            PublishedIndexStatus::Degraded => "Degraded",
        }
    }
}

/// Secondary indices derived from a single `files` map snapshot.
/// Invariant: these indices are one coherent snapshot derived from exactly
/// the `files` map they are paired with. Grouping them enforces this.
pub(crate) struct DerivedIndices {
    pub trigram_index: super::trigram::TrigramIndex,
    pub reverse_index: HashMap<String, Vec<ReferenceLocation>>,
    pub files_by_basename: HashMap<String, Vec<String>>,
    pub files_by_dir_component: HashMap<String, Vec<String>>,
}

impl DerivedIndices {
    /// Build all derived indices from a file map. Pure function — no side effects,
    /// no locks, safe to call from any thread.
    pub(crate) fn build_from_files(files: &HashMap<String, Arc<IndexedFile>>) -> Self {
        let (files_by_basename, files_by_dir_component) = build_path_indices_from_files(files);
        Self {
            trigram_index: super::trigram::TrigramIndex::build_from_files(files),
            reverse_index: build_reverse_index_from_files(files),
            files_by_basename,
            files_by_dir_component,
        }
    }
}

/// Pre-computed reload data built outside any lock.
///
/// Contains everything needed to swap into a `LiveIndex` under the write lock.
/// All derived indices are pre-built so that `apply_reload_data` is pure field
/// assignment (microseconds, not milliseconds).
///
/// # Failure boundaries
///
/// `build_reload_data()` is all-or-nothing and side-effect-free with respect to
/// the live index state. Only `apply_reload_data()` mutates the live state, and
/// it cannot fail — it's pure assignment.
pub(crate) struct ReloadData {
    pub files: HashMap<String, Arc<IndexedFile>>,
    pub cb_state: CircuitBreakerState,
    pub load_duration: Duration,
    pub gitignore: Option<ignore::gitignore::Gitignore>,
    pub derived: DerivedIndices,
}

/// Build a reverse index from a file map (standalone, no `&self` needed).
pub(crate) fn build_reverse_index_from_files(
    files: &HashMap<String, Arc<IndexedFile>>,
) -> HashMap<String, Vec<ReferenceLocation>> {
    let mut idx: HashMap<String, Vec<ReferenceLocation>> = HashMap::new();
    for (file_path, indexed_file) in files {
        for (reference_idx, reference) in indexed_file.references.iter().enumerate() {
            idx.entry(reference.name.clone())
                .or_default()
                .push(ReferenceLocation {
                    file_path: file_path.clone(),
                    reference_idx: reference_idx as u32,
                });
        }
    }
    idx
}

/// Build path indices (basename + dir component) from a file map.
pub(crate) fn build_path_indices_from_files(
    files: &HashMap<String, Arc<IndexedFile>>,
) -> (HashMap<String, Vec<String>>, HashMap<String, Vec<String>>) {
    let mut by_basename: HashMap<String, Vec<String>> = HashMap::new();
    let mut by_dir_component: HashMap<String, Vec<String>> = HashMap::new();
    for path in files.keys() {
        if let Some(basename) = basename_key(path) {
            insert_sorted_unique(by_basename.entry(basename).or_default(), path);
        }
        for component in dir_component_keys(path) {
            insert_sorted_unique(by_dir_component.entry(component).or_default(), path);
        }
    }
    (by_basename, by_dir_component)
}

impl LiveIndex {
    /// Load all source files under `root` into memory in parallel (Rayon), parse them,
    /// and return a `SharedIndex`.
    ///
    /// This function is **synchronous** — it must complete before the async tokio runtime
    /// needs the index. Rayon handles internal parallelism.
    pub fn load(root: &Path) -> Result<SharedIndex> {
        let start = Instant::now();

        info!("LiveIndex::load starting at {:?}", root);

        // 1. Discover ALL files (not just known-language ones) so the admission gate
        //    can classify every file, including those with denylisted or unknown extensions.
        let all_entries = discovery::discover_all_files(root)?;
        info!(
            "discovered {} total files (pre-admission)",
            all_entries.len()
        );

        // 2. Run admission gate in parallel.
        //    For files that pass Tier-1 initially (size/extension checks), we read content
        //    and re-run the binary sniff before committing to parse.
        //    Files that are non-Normal skip reading entirely.
        use crate::discovery::classify_admission;
        use crate::domain::index::{AdmissionTier, SkippedFile};

        enum AdmissionOutcome {
            Parse {
                relative_path: String,
                language: crate::domain::LanguageId,
                classification: crate::domain::FileClassification,
                bytes: Vec<u8>,
            },
            Skip(SkippedFile),
        }

        let outcomes: Vec<AdmissionOutcome> = all_entries
            .par_iter()
            .filter_map(|entry| {
                // Phase 1: size + extension check (no I/O beyond what the walk gave us).
                let decision_pre = classify_admission(
                    &entry.absolute_path,
                    entry.file_size,
                    None, // no content yet
                );

                match decision_pre.tier {
                    AdmissionTier::HardSkip | AdmissionTier::MetadataOnly => {
                        // No need to read content — already decided.
                        let sf = SkippedFile {
                            path: entry.relative_path.clone(),
                            size: entry.file_size,
                            extension: entry
                                .absolute_path
                                .extension()
                                .and_then(|e| e.to_str())
                                .map(|s| s.to_string()),
                            decision: decision_pre,
                        };
                        return Some(AdmissionOutcome::Skip(sf));
                    }
                    AdmissionTier::Normal => {}
                }

                // Phase 2: we tentatively have Tier-1. If the file has no recognized
                // language, we cannot parse it — skip it as metadata-only.
                let language = match &entry.language {
                    Some(lang) => lang.clone(),
                    None => {
                        // Unknown extension, not on denylist, under size limit.
                        // Read content to do binary sniff, then store as skipped.
                        let bytes = match std::fs::read(&entry.absolute_path) {
                            Ok(b) => b,
                            Err(e) => {
                                warn!("failed to read {:?}: {}", entry.absolute_path, e);
                                return None;
                            }
                        };
                        let decision_post =
                            classify_admission(&entry.absolute_path, entry.file_size, Some(&bytes));
                        let sf = SkippedFile {
                            path: entry.relative_path.clone(),
                            size: entry.file_size,
                            extension: entry
                                .absolute_path
                                .extension()
                                .and_then(|e| e.to_str())
                                .map(|s| s.to_string()),
                            decision: decision_post,
                        };
                        return Some(AdmissionOutcome::Skip(sf));
                    }
                };

                // Phase 3: read content and do binary sniff before passing to parser.
                let bytes = match std::fs::read(&entry.absolute_path) {
                    Ok(b) => b,
                    Err(e) => {
                        warn!("failed to read {:?}: {}", entry.absolute_path, e);
                        return None;
                    }
                };

                let decision_post =
                    classify_admission(&entry.absolute_path, entry.file_size, Some(&bytes));

                match decision_post.tier {
                    AdmissionTier::HardSkip | AdmissionTier::MetadataOnly => {
                        // Binary sniff reclassified this file — do NOT parse.
                        let sf = SkippedFile {
                            path: entry.relative_path.clone(),
                            size: entry.file_size,
                            extension: entry
                                .absolute_path
                                .extension()
                                .and_then(|e| e.to_str())
                                .map(|s| s.to_string()),
                            decision: decision_post,
                        };
                        Some(AdmissionOutcome::Skip(sf))
                    }
                    AdmissionTier::Normal => Some(AdmissionOutcome::Parse {
                        relative_path: entry.relative_path.clone(),
                        language,
                        classification: entry.classification,
                        bytes,
                    }),
                }
            })
            .collect();

        // 3. Split outcomes into parse candidates and skipped files.
        let mut skipped_files: Vec<SkippedFile> = Vec::new();
        let mut to_parse: Vec<(
            String,
            crate::domain::LanguageId,
            crate::domain::FileClassification,
            Vec<u8>,
        )> = Vec::new();

        for outcome in outcomes {
            match outcome {
                AdmissionOutcome::Skip(sf) => skipped_files.push(sf),
                AdmissionOutcome::Parse {
                    relative_path,
                    language,
                    classification,
                    bytes,
                } => {
                    to_parse.push((relative_path, language, classification, bytes));
                }
            }
        }

        info!(
            "admission gate: {} to parse, {} skipped",
            to_parse.len(),
            skipped_files.len()
        );

        // 4. Parse all admitted files in parallel via Rayon.
        let parse_results: Vec<(String, IndexedFile)> = to_parse
            .par_iter()
            .map(|(relative_path, language, classification, bytes)| {
                let result = parsing::process_file_with_classification(
                    relative_path,
                    bytes,
                    language.clone(),
                    *classification,
                );
                let indexed = IndexedFile::from_parse_result(result, bytes.clone());
                (relative_path.clone(), indexed)
            })
            .collect();

        // 5. Build HashMap sequentially, running circuit breaker checks.
        let cb_state = CircuitBreakerState::from_env();
        let mut files: HashMap<String, Arc<IndexedFile>> =
            HashMap::with_capacity(parse_results.len());

        let mut cb_tripped = false;
        for (path, indexed_file) in parse_results {
            match &indexed_file.parse_status {
                ParseStatus::Failed { error } => {
                    cb_state.record_failure(&path, error);
                }
                _ => {
                    cb_state.record_success();
                }
            }

            if cb_state.should_abort() {
                let summary = cb_state.summary();
                error!("{}", summary);
                cb_tripped = true;
                // Still insert the file before breaking
                files.insert(path, Arc::new(indexed_file));
                break;
            }

            files.insert(path, Arc::new(indexed_file));
        }

        if cb_tripped {
            cb_state.tripped.store(true, Ordering::Relaxed);
        }

        let load_duration = start.elapsed();
        info!(
            "LiveIndex loaded: {} files, {} symbols, {} skipped, {:?}",
            files.len(),
            files.values().map(|f| f.symbols.len()).sum::<usize>(),
            skipped_files.len(),
            load_duration
        );

        let trigram_index = super::trigram::TrigramIndex::build_from_files(&files);
        let gitignore = discovery::load_gitignore(root);

        let mut index = LiveIndex {
            files,
            loaded_at: Instant::now(),
            loaded_at_system: SystemTime::now(),
            load_duration,
            cb_state,
            is_empty: false,
            load_source: IndexLoadSource::FreshLoad,
            snapshot_verify_state: SnapshotVerifyState::NotNeeded,
            reverse_index: HashMap::new(),
            files_by_basename: HashMap::new(),
            files_by_dir_component: HashMap::new(),
            trigram_index,
            gitignore,
            skipped_files,
        };
        index.rebuild_reverse_index();
        index.rebuild_path_indices();

        Ok(SharedIndexHandle::shared(index))
    }

    /// Create an empty `SharedIndex` with no files loaded.
    ///
    /// Used when `TOKENIZOR_AUTO_INDEX=false`. The caller must call `reload()` to populate it.
    /// Returns `IndexState::Empty` and `is_ready() == false` until reloaded.
    pub fn empty() -> SharedIndex {
        let index = LiveIndex {
            files: HashMap::new(),
            loaded_at: Instant::now(),
            loaded_at_system: SystemTime::now(),
            load_duration: Duration::ZERO,
            cb_state: CircuitBreakerState::new(0.20),
            is_empty: true,
            load_source: IndexLoadSource::EmptyBootstrap,
            snapshot_verify_state: SnapshotVerifyState::NotNeeded,
            reverse_index: HashMap::new(),
            files_by_basename: HashMap::new(),
            files_by_dir_component: HashMap::new(),
            trigram_index: super::trigram::TrigramIndex::new(),
            gitignore: None,
            skipped_files: Vec::new(),
        };
        SharedIndexHandle::shared(index)
    }

    pub fn add_skipped_file(&mut self, sf: SkippedFile) {
        self.skipped_files.push(sf);
    }

    pub fn skipped_files(&self) -> &[SkippedFile] {
        &self.skipped_files
    }

    /// Returns (tier1_count, tier2_count, tier3_count).
    /// Tier 1 = number of indexed files (self.files.len()).
    /// Tier 2/3 = from skipped_files.
    pub fn tier_counts(&self) -> (usize, usize, usize) {
        let tier1 = self.files.len();
        let mut tier2 = 0;
        let mut tier3 = 0;
        for sf in &self.skipped_files {
            match sf.tier() {
                AdmissionTier::MetadataOnly => tier2 += 1,
                AdmissionTier::HardSkip => tier3 += 1,
                AdmissionTier::Normal => {} // shouldn't happen
            }
        }
        (tier1, tier2, tier3)
    }

    /// Build reload data without holding any lock. Performs all file I/O and
    /// parsing via Rayon. The returned `ReloadData` is applied under the write
    /// lock via `apply_reload_data` — reducing lock hold time from seconds to
    /// milliseconds.
    pub(crate) fn build_reload_data(root: &Path) -> crate::error::Result<ReloadData> {
        use crate::error::TokenizorError;

        let start = Instant::now();

        info!("LiveIndex::build_reload_data starting at {:?}", root);

        if !root.exists() {
            return Err(TokenizorError::Discovery(format!(
                "root path does not exist: {}",
                root.display()
            )));
        }

        // 1. Discover all source files
        let discovered = discovery::discover_files(root)?;
        info!("discovered {} source files", discovered.len());

        // 2. Parse all files in parallel via Rayon
        let parse_results: Vec<(String, IndexedFile)> = discovered
            .par_iter()
            .filter_map(|df| {
                let bytes = match std::fs::read(&df.absolute_path) {
                    Ok(b) => b,
                    Err(e) => {
                        warn!("failed to read {:?}: {}", df.absolute_path, e);
                        return None;
                    }
                };

                let result = parsing::process_file_with_classification(
                    &df.relative_path,
                    &bytes,
                    df.language.clone(),
                    df.classification,
                );
                let indexed = IndexedFile::from_parse_result(result, bytes);
                Some((df.relative_path.clone(), indexed))
            })
            .collect();

        // 3. Build new file map with fresh circuit breaker
        let new_cb = CircuitBreakerState::from_env();
        let mut new_files: HashMap<String, Arc<IndexedFile>> =
            HashMap::with_capacity(parse_results.len());

        let mut cb_tripped = false;
        for (path, indexed_file) in parse_results {
            match &indexed_file.parse_status {
                ParseStatus::Failed { error } => {
                    new_cb.record_failure(&path, error);
                }
                _ => {
                    new_cb.record_success();
                }
            }

            if new_cb.should_abort() {
                let summary = new_cb.summary();
                error!("{}", summary);
                cb_tripped = true;
                new_files.insert(path, Arc::new(indexed_file));
                break;
            }

            new_files.insert(path, Arc::new(indexed_file));
        }

        if cb_tripped {
            new_cb.tripped.store(true, Ordering::Relaxed);
        }

        let load_duration = start.elapsed();
        info!(
            "LiveIndex::build_reload_data done: {} files, {} symbols, {:?}",
            new_files.len(),
            new_files.values().map(|f| f.symbols.len()).sum::<usize>(),
            load_duration
        );

        // Pre-build all derived indices outside any lock.
        let derived = DerivedIndices::build_from_files(&new_files);

        Ok(ReloadData {
            files: new_files,
            cb_state: new_cb,
            load_duration,
            gitignore: discovery::load_gitignore(root),
            derived,
        })
    }

    /// Apply pre-built reload data under the write lock. Pure field assignment —
    /// all derived indices are pre-built in `ReloadData`, so this takes
    /// microseconds instead of milliseconds. Cannot fail.
    pub(crate) fn apply_reload_data(&mut self, data: ReloadData) {
        self.files = data.files;
        self.loaded_at = Instant::now();
        self.loaded_at_system = SystemTime::now();
        self.load_duration = data.load_duration;
        self.cb_state = data.cb_state;
        self.is_empty = false;
        self.load_source = IndexLoadSource::FreshLoad;
        self.snapshot_verify_state = SnapshotVerifyState::NotNeeded;
        self.trigram_index = data.derived.trigram_index;
        self.reverse_index = data.derived.reverse_index;
        self.files_by_basename = data.derived.files_by_basename;
        self.files_by_dir_component = data.derived.files_by_dir_component;
        self.gitignore = data.gitignore;
    }

    /// Replaces all files, resets circuit breaker, and updates timestamps.
    /// On success sets `is_empty = false`. On error the index remains in its previous state
    /// (but partial results may have been loaded).
    ///
    /// NOTE: This method does all I/O under `&mut self`. Prefer calling
    /// `build_reload_data` outside the lock and then `apply_reload_data` under
    /// the lock when called via `SharedIndexHandle::reload`.
    pub fn reload(&mut self, root: &Path) -> crate::error::Result<()> {
        let data = Self::build_reload_data(root)?;
        self.apply_reload_data(data);
        Ok(())
    }

    /// Insert or replace a single file in the index without a full reload.
    ///
    /// Updates `loaded_at_system` to reflect the mutation time.
    /// If the file already exists, its entry is replaced atomically.
    pub fn update_file(&mut self, path: String, file: IndexedFile) {
        if self.files.contains_key(&path) {
            self.remove_path_indices_for_path(&path);
        }
        self.trigram_index.update_file(&path, &file.content);
        self.remove_reverse_index_for_path(&path);
        self.files.insert(path.clone(), Arc::new(file));
        self.insert_reverse_index_for_path(&path);
        self.insert_path_indices_for_path(&path);
        self.is_empty = false;
        self.loaded_at_system = SystemTime::now();
    }

    /// Insert a new file into the index (alias for `update_file`).
    ///
    /// Semantically identical to `update_file` — if the file already exists
    /// it is replaced. The name `add_file` is provided for clarity at call sites
    /// where the caller knows the file is new.
    pub fn add_file(&mut self, path: String, file: IndexedFile) {
        self.update_file(path, file);
    }

    /// Remove a single file from the index by its relative path.
    ///
    /// If the path is not present, this is a no-op (no timestamp update).
    /// If the path is found and removed, `loaded_at_system` is updated.
    pub fn remove_file(&mut self, path: &str) {
        self.remove_reverse_index_for_path(path);
        if self.files.remove(path).is_some() {
            self.trigram_index.remove_file(path);
            self.remove_path_indices_for_path(path);
            self.loaded_at_system = SystemTime::now();
        }
    }

    /// Remove reverse index entries for a single file path.
    /// Must be called BEFORE removing the file from `self.files`.
    fn remove_reverse_index_for_path(&mut self, path: &str) {
        if let Some(file) = self.files.get(path) {
            let names: Vec<String> = file.references.iter().map(|r| r.name.clone()).collect();
            for name in names {
                if let Some(locs) = self.reverse_index.get_mut(&name) {
                    locs.retain(|loc| loc.file_path != path);
                    if locs.is_empty() {
                        self.reverse_index.remove(&name);
                    }
                }
            }
        }
    }

    /// Insert reverse index entries for a single file path.
    /// Must be called AFTER inserting the file into `self.files`.
    fn insert_reverse_index_for_path(&mut self, path: &str) {
        if let Some(file) = self.files.get(path) {
            for (reference_idx, reference) in file.references.iter().enumerate() {
                self.reverse_index
                    .entry(reference.name.clone())
                    .or_default()
                    .push(ReferenceLocation {
                        file_path: path.to_string(),
                        reference_idx: reference_idx as u32,
                    });
            }
        }
    }

    /// Rebuild `reverse_index` from scratch using current `self.files`.
    ///
    /// Used by incremental callers (load, snapshot restore, tests).
    /// For bulk reload, prefer `DerivedIndices::build_from_files` outside the lock.
    pub(crate) fn rebuild_reverse_index(&mut self) {
        self.reverse_index = build_reverse_index_from_files(&self.files);
    }

    /// Rebuild path indices (basename + dir component) from current `self.files`.
    ///
    /// Used by incremental callers (load, snapshot restore, tests).
    /// For bulk reload, prefer `DerivedIndices::build_from_files` outside the lock.
    pub(crate) fn rebuild_path_indices(&mut self) {
        let (by_basename, by_dir_component) = build_path_indices_from_files(&self.files);
        self.files_by_basename = by_basename;
        self.files_by_dir_component = by_dir_component;
    }

    fn insert_path_indices_for_path(&mut self, path: &str) {
        if let Some(basename) = basename_key(path) {
            insert_sorted_unique(self.files_by_basename.entry(basename).or_default(), path);
        }

        for component in dir_component_keys(path) {
            insert_sorted_unique(
                self.files_by_dir_component.entry(component).or_default(),
                path,
            );
        }
    }

    fn remove_path_indices_for_path(&mut self, path: &str) {
        if let Some(basename) = basename_key(path)
            && let Some(paths) = self.files_by_basename.get_mut(&basename)
        {
            remove_sorted_path(paths, path);
            if paths.is_empty() {
                self.files_by_basename.remove(&basename);
            }
        }

        for component in dir_component_keys(path) {
            if let Some(paths) = self.files_by_dir_component.get_mut(&component) {
                remove_sorted_path(paths, path);
                if paths.is_empty() {
                    self.files_by_dir_component.remove(&component);
                }
            }
        }
    }

    /// Returns where the current in-memory contents came from.
    pub fn load_source(&self) -> IndexLoadSource {
        self.load_source
    }

    /// Returns the current snapshot reconciliation state.
    pub fn snapshot_verify_state(&self) -> SnapshotVerifyState {
        self.snapshot_verify_state
    }

    pub(crate) fn mark_snapshot_verify_running(&mut self) {
        if self.load_source == IndexLoadSource::SnapshotRestore {
            self.snapshot_verify_state = SnapshotVerifyState::Running;
        }
    }

    pub(crate) fn mark_snapshot_verify_completed(&mut self) {
        if self.load_source == IndexLoadSource::SnapshotRestore {
            self.snapshot_verify_state = SnapshotVerifyState::Completed;
        }
    }
}

fn basename_key(path: &str) -> Option<String> {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_ascii_lowercase())
}

fn dir_component_keys(path: &str) -> Vec<String> {
    let components: Vec<&str> = path
        .split(['/', '\\'])
        .filter(|component| !component.is_empty())
        .collect();
    if components.len() <= 1 {
        return Vec::new();
    }

    let mut seen = HashSet::new();
    let mut keys = Vec::new();
    for component in &components[..components.len() - 1] {
        let key = component.to_ascii_lowercase();
        if seen.insert(key.clone()) {
            keys.push(key);
        }
    }
    keys.sort();
    keys
}

fn insert_sorted_unique(paths: &mut Vec<String>, path: &str) {
    match paths.binary_search_by(|existing| existing.as_str().cmp(path)) {
        Ok(_) => {}
        Err(pos) => paths.insert(pos, path.to_string()),
    }
}

fn remove_sorted_path(paths: &mut Vec<String>, path: &str) {
    if let Ok(pos) = paths.binary_search_by(|existing| existing.as_str().cmp(path)) {
        paths.remove(pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        FileOutcome, LanguageId, ReferenceKind, ReferenceRecord, SymbolKind, SymbolRecord,
    };
    use std::fs;
    use tempfile::TempDir;

    fn dummy_symbol() -> SymbolRecord {
        SymbolRecord {
            name: "foo".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 10),
            line_range: (0, 1),
            doc_byte_range: None,
        }
    }

    fn make_result(outcome: FileOutcome, symbols: Vec<SymbolRecord>) -> FileProcessingResult {
        FileProcessingResult {
            relative_path: "test.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("test.rs"),
            outcome,
            symbols,
            byte_len: 42,
            content_hash: "abc123".to_string(),
            references: vec![],
            alias_map: std::collections::HashMap::new(),
        }
    }

    // --- IndexedFile::from_parse_result ---

    #[test]
    fn test_indexed_file_maps_processed_status() {
        let result = make_result(FileOutcome::Processed, vec![dummy_symbol()]);
        let indexed = IndexedFile::from_parse_result(result, b"fn foo() {}".to_vec());
        assert_eq!(indexed.parse_status, ParseStatus::Parsed);
        assert_eq!(indexed.symbols.len(), 1);
    }

    #[test]
    fn test_indexed_file_maps_partial_parse_keeps_symbols() {
        let result = make_result(
            FileOutcome::PartialParse {
                warning: "syntax error".to_string(),
            },
            vec![dummy_symbol()],
        );
        let indexed = IndexedFile::from_parse_result(result, b"fn bad(".to_vec());
        assert!(matches!(
            indexed.parse_status,
            ParseStatus::PartialParse { .. }
        ));
        assert_eq!(
            indexed.symbols.len(),
            1,
            "symbols kept even on partial parse"
        );
    }

    #[test]
    fn test_indexed_file_maps_failed_status_empty_symbols_content_preserved() {
        let result = make_result(
            FileOutcome::Failed {
                error: "parse failed".to_string(),
            },
            vec![],
        );
        let content = b"some content bytes".to_vec();
        let indexed = IndexedFile::from_parse_result(result, content.clone());
        assert!(matches!(indexed.parse_status, ParseStatus::Failed { .. }));
        assert!(indexed.symbols.is_empty(), "failed parse has no symbols");
        assert_eq!(
            indexed.content, content,
            "content bytes stored even on failure"
        );
    }

    // --- CircuitBreakerState ---

    #[test]
    fn test_circuit_breaker_does_not_trip_at_20pct_of_10_files() {
        // 20% of 10 = exactly threshold — NOT exceeded
        let cb = CircuitBreakerState::new(0.20);
        for _ in 0..8 {
            cb.record_success();
        }
        for i in 0..2 {
            cb.record_failure(&format!("file{i}.rs"), "error");
        }
        assert!(
            !cb.should_abort(),
            "2/10 = 20% should NOT trip (threshold not exceeded)"
        );
    }

    #[test]
    fn test_circuit_breaker_trips_at_30pct_of_10_files() {
        // 30% > 20% threshold — SHOULD trip
        let cb = CircuitBreakerState::new(0.20);
        for _ in 0..7 {
            cb.record_success();
        }
        for i in 0..3 {
            cb.record_failure(&format!("file{i}.rs"), "error");
        }
        assert!(cb.should_abort(), "3/10 = 30% should trip");
    }

    #[test]
    fn test_circuit_breaker_does_not_trip_on_tiny_repos() {
        // Fewer than 5 files processed — minimum-file guard must prevent tripping
        let cb = CircuitBreakerState::new(0.20);
        cb.record_failure("a.rs", "err");
        cb.record_failure("b.rs", "err");
        cb.record_failure("c.rs", "err");
        // 3 total, all failed — but < 5 minimum threshold
        assert!(
            !cb.should_abort(),
            "< 5 files processed: circuit breaker must not trip"
        );
    }

    #[test]
    fn test_circuit_breaker_threshold_configurable() {
        // Use a strict threshold of 0.10 (10%)
        let cb = CircuitBreakerState::new(0.10);
        for _ in 0..9 {
            cb.record_success();
        }
        cb.record_failure("file.rs", "error");
        // 1/10 = 10% = threshold, NOT exceeded
        assert!(!cb.should_abort(), "10% == threshold, not exceeded");

        // Now one more failure puts it at 2/11 ~ 18.2% > 10% — but we add 1 more success first
        let cb2 = CircuitBreakerState::new(0.10);
        for _ in 0..8 {
            cb2.record_success();
        }
        for i in 0..2 {
            cb2.record_failure(&format!("file{i}.rs"), "error");
        }
        // 2/10 = 20% > 10% threshold
        assert!(cb2.should_abort(), "20% > 10% threshold should trip");
    }

    // --- LiveIndex::load ---

    fn write_file(dir: &Path, name: &str, content: &str) {
        let path = dir.join(name);
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn test_live_index_load_valid_files_produces_ready_state() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "a.rs", "fn alpha() {}");
        write_file(tmp.path(), "b.py", "def beta(): pass");
        write_file(tmp.path(), "c.js", "function gamma() {}");
        write_file(tmp.path(), "d.ts", "function delta(): void {}");
        write_file(tmp.path(), "e.go", "package main\nfunc epsilon() {}");

        let shared = LiveIndex::load(tmp.path()).unwrap();
        let index = shared.read().unwrap();
        assert!(
            !index.cb_state.is_tripped(),
            "valid files should not trip circuit breaker"
        );
        assert_eq!(index.file_count(), 5);
        assert_eq!(index.load_source(), IndexLoadSource::FreshLoad);
        assert_eq!(
            index.snapshot_verify_state(),
            SnapshotVerifyState::NotNeeded
        );
    }

    #[test]
    fn test_live_index_load_circuit_breaker_not_tripped_with_all_languages() {
        // All 16 languages now parse successfully (tree-sitter 0.26 + ABI-compatible grammars).
        // A mix of language files should not trip the circuit breaker.
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "a.rs", "fn alpha() {}");
        write_file(tmp.path(), "b.py", "def beta(): pass");
        write_file(tmp.path(), "c.js", "function gamma() {}");
        // Swift, PHP, Perl now parse successfully — CB should not trip
        write_file(tmp.path(), "x.swift", "class A {}");
        write_file(tmp.path(), "y.php", "<?php class B {}");
        write_file(tmp.path(), "z.pl", "sub greet { print \"hi\"; }");

        let shared = LiveIndex::load(tmp.path()).unwrap();
        let index = shared.read().unwrap();
        assert!(
            !index.cb_state.is_tripped(),
            "all-parseable files should not trip circuit breaker"
        );
    }

    #[test]
    fn test_live_index_file_count() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "a.rs", "fn a() {}");
        write_file(tmp.path(), "b.rs", "fn b() {}");
        write_file(tmp.path(), "c.rs", "fn c() {}");

        let shared = LiveIndex::load(tmp.path()).unwrap();
        let index = shared.read().unwrap();
        assert_eq!(index.file_count(), 3);
    }

    #[test]
    fn test_live_index_symbol_count() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "a.rs", "fn foo() {}\nfn bar() {}");
        write_file(tmp.path(), "b.rs", "fn baz() {}");

        let shared = LiveIndex::load(tmp.path()).unwrap();
        let index = shared.read().unwrap();
        // a.rs: 2 symbols, b.rs: 1 symbol → total 3
        assert_eq!(index.symbol_count(), 3);
    }

    // --- LiveIndex::empty() and reload() ---

    #[test]
    fn test_live_index_empty_has_zero_files() {
        let shared = LiveIndex::empty();
        let index = shared.read().unwrap();
        assert_eq!(index.file_count(), 0);
        assert_eq!(index.load_source(), IndexLoadSource::EmptyBootstrap);
        assert_eq!(
            index.snapshot_verify_state(),
            SnapshotVerifyState::NotNeeded
        );
    }

    #[test]
    fn test_shared_index_handle_preserves_read_write_access() {
        let shared = LiveIndex::empty();
        {
            let mut live = shared.write().expect("lock poisoned");
            live.add_file(
                "src/new.rs".to_string(),
                make_indexed_file_for_mutation("src/new.rs"),
            );
        }

        let index = shared.read().unwrap();
        assert!(index.get_file("src/new.rs").is_some());
    }

    #[test]
    fn test_shared_index_handle_published_state_tracks_generation_and_counts() {
        let shared = LiveIndex::empty();
        let initial = shared.published_state();
        assert_eq!(initial.generation, 0);
        assert_eq!(initial.status, PublishedIndexStatus::Empty);
        assert_eq!(initial.degraded_summary, None);
        assert_eq!(initial.file_count, 0);
        assert_eq!(initial.parsed_count, 0);
        assert_eq!(initial.partial_parse_count, 0);
        assert_eq!(initial.failed_count, 0);
        assert_eq!(initial.load_source, IndexLoadSource::EmptyBootstrap);

        shared.add_file(
            "src/new.rs".to_string(),
            make_indexed_file_for_mutation("src/new.rs"),
        );
        let after_add = shared.published_state();
        assert_eq!(after_add.generation, 1);
        assert_eq!(after_add.status, PublishedIndexStatus::Ready);
        assert_eq!(after_add.degraded_summary, None);
        assert_eq!(after_add.file_count, 1);
        assert_eq!(after_add.parsed_count, 1);
        assert_eq!(after_add.partial_parse_count, 0);
        assert_eq!(after_add.failed_count, 0);
        assert_eq!(after_add.symbol_count, 1);

        shared.remove_file("src/new.rs");
        let after_remove = shared.published_state();
        assert_eq!(after_remove.generation, 2);
        assert_eq!(after_remove.status, PublishedIndexStatus::Ready);
        assert_eq!(after_remove.degraded_summary, None);
        assert_eq!(after_remove.file_count, 0);
        assert_eq!(after_remove.symbol_count, 0);
    }

    #[test]
    fn test_shared_index_handle_write_guard_publishes_on_drop() {
        let shared = LiveIndex::empty();

        {
            let mut live = shared.write().expect("lock poisoned");
            live.add_file(
                "src/new.rs".to_string(),
                make_indexed_file_for_mutation("src/new.rs"),
            );
        }

        let after_add = shared.published_state();
        assert_eq!(after_add.generation, 1);
        assert_eq!(after_add.status, PublishedIndexStatus::Ready);
        assert_eq!(after_add.degraded_summary, None);
        assert_eq!(after_add.file_count, 1);

        {
            let mut live = shared.write().expect("lock poisoned");
            live.remove_file("src/new.rs");
        }

        let after_remove = shared.published_state();
        assert_eq!(after_remove.generation, 2);
        assert_eq!(after_remove.status, PublishedIndexStatus::Ready);
        assert_eq!(after_remove.degraded_summary, None);
        assert_eq!(after_remove.file_count, 0);
    }

    #[test]
    fn test_shared_index_handle_published_state_tracks_verify_transitions() {
        let mut live = make_empty_live_index();
        live.is_empty = false;
        live.load_source = IndexLoadSource::SnapshotRestore;
        live.snapshot_verify_state = SnapshotVerifyState::Pending;
        let shared = SharedIndexHandle::shared(live);

        shared.mark_snapshot_verify_running();
        let running = shared.published_state();
        assert_eq!(running.generation, 1);
        assert_eq!(running.status, PublishedIndexStatus::Ready);
        assert_eq!(running.degraded_summary, None);
        assert_eq!(running.snapshot_verify_state, SnapshotVerifyState::Running);

        shared.mark_snapshot_verify_completed();
        let completed = shared.published_state();
        assert_eq!(completed.generation, 2);
        assert_eq!(
            completed.snapshot_verify_state,
            SnapshotVerifyState::Completed
        );
    }

    #[test]
    fn test_shared_index_handle_published_state_captures_degraded_summary() {
        let mut live = make_empty_live_index();
        live.is_empty = false;
        for _ in 0..3 {
            live.cb_state.record_failure("src/bad.rs", "parse failure");
        }
        for _ in 0..7 {
            live.cb_state.record_success();
        }
        assert!(live.cb_state.should_abort(), "circuit breaker should trip");
        let shared = SharedIndexHandle::shared(live);

        let published = shared.published_state();
        assert_eq!(published.status, PublishedIndexStatus::Degraded);
        assert!(
            published
                .degraded_summary
                .as_deref()
                .is_some_and(|summary| summary.contains("circuit breaker tripped")),
            "expected degraded summary, got {:?}",
            published.degraded_summary
        );
    }

    #[test]
    fn test_shared_index_handle_published_repo_outline_tracks_mutations() {
        let shared = LiveIndex::empty();

        let initial = shared.published_repo_outline();
        assert_eq!(initial.total_files, 0);
        assert_eq!(initial.total_symbols, 0);
        assert!(initial.files.is_empty());

        shared.add_file(
            "src/main.rs".to_string(),
            make_indexed_file_for_mutation("src/main.rs"),
        );
        let after_add = shared.published_repo_outline();
        assert_eq!(after_add.total_files, 1);
        assert_eq!(after_add.total_symbols, 1);
        assert_eq!(after_add.files[0].relative_path, "src/main.rs");

        {
            let mut live = shared.write().expect("lock poisoned");
            live.remove_file("src/main.rs");
        }
        let after_remove = shared.published_repo_outline();
        assert_eq!(after_remove.total_files, 0);
        assert_eq!(after_remove.total_symbols, 0);
        assert!(after_remove.files.is_empty());
    }

    #[test]
    fn test_live_index_empty_returns_empty_state() {
        let shared = LiveIndex::empty();
        let index = shared.read().unwrap();
        assert_eq!(index.index_state(), IndexState::Empty);
    }

    #[test]
    fn test_live_index_empty_is_not_ready() {
        let shared = LiveIndex::empty();
        let index = shared.read().unwrap();
        assert!(!index.is_ready(), "empty index should not be ready");
    }

    #[test]
    fn test_live_index_reload_loads_files_and_becomes_ready() {
        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "a.rs", "fn alpha() {}");
        write_file(tmp.path(), "b.rs", "fn beta() {}");

        let shared = LiveIndex::empty();
        {
            let mut index = shared.write().unwrap();
            index.reload(tmp.path()).expect("reload should succeed");
        }
        let index = shared.read().unwrap();
        assert_eq!(index.file_count(), 2);
        assert!(index.is_ready(), "after reload should be ready");
        assert_eq!(index.index_state(), IndexState::Ready);
        assert_eq!(index.load_source(), IndexLoadSource::FreshLoad);
        assert_eq!(
            index.snapshot_verify_state(),
            SnapshotVerifyState::NotNeeded
        );
    }

    #[test]
    fn test_live_index_reload_invalid_root_returns_error() {
        let shared = LiveIndex::empty();
        let mut index = shared.write().unwrap();
        let result = index.reload(Path::new("/nonexistent/path/that/does/not/exist"));
        assert!(
            result.is_err(),
            "reload on invalid root should return error"
        );
    }

    #[test]
    fn test_live_index_loaded_at_system_is_recent() {
        use std::time::SystemTime;
        let before = SystemTime::now();
        let shared = LiveIndex::empty();
        let index = shared.read().unwrap();
        let after = SystemTime::now();
        let ts = index.loaded_at_system();
        assert!(
            ts >= before,
            "loaded_at_system should be >= before creation"
        );
        assert!(ts <= after, "loaded_at_system should be <= after creation");
    }

    #[test]
    fn test_concurrent_readers_no_deadlock() {
        use std::thread;

        let tmp = TempDir::new().unwrap();
        write_file(tmp.path(), "a.rs", "fn foo() {}");
        write_file(tmp.path(), "b.rs", "fn bar() {}");
        write_file(tmp.path(), "c.rs", "fn baz() {}");

        let shared = LiveIndex::load(tmp.path()).unwrap();

        let handles: Vec<_> = (0..8)
            .map(|_| {
                let shared_clone = Arc::clone(&shared);
                thread::spawn(move || {
                    let index = shared_clone.read().unwrap();
                    let _ = index.file_count();
                    let _ = index.symbol_count();
                })
            })
            .collect();

        for h in handles {
            h.join().expect("reader thread should not panic");
        }
    }

    // --- LiveIndex mutation methods ---

    fn make_indexed_file_for_mutation(path: &str) -> IndexedFile {
        IndexedFile {
            relative_path: path.to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path(path),
            content: b"fn test() {}".to_vec(),
            symbols: vec![dummy_symbol()],
            parse_status: ParseStatus::Parsed,
            byte_len: 12,
            content_hash: "abc123".to_string(),
            references: vec![],
            alias_map: std::collections::HashMap::new(),
        }
    }

    fn make_empty_live_index() -> LiveIndex {
        LiveIndex {
            files: HashMap::new(),
            loaded_at: Instant::now(),
            loaded_at_system: SystemTime::now(),
            load_duration: Duration::ZERO,
            cb_state: CircuitBreakerState::new(0.20),
            is_empty: false,
            load_source: IndexLoadSource::FreshLoad,
            snapshot_verify_state: SnapshotVerifyState::NotNeeded,
            reverse_index: HashMap::new(),
            files_by_basename: HashMap::new(),
            files_by_dir_component: HashMap::new(),
            trigram_index: crate::live_index::trigram::TrigramIndex::new(),
            gitignore: None,
            skipped_files: Vec::new(),
        }
    }

    #[test]
    fn test_live_index_load_builds_path_indices() {
        let dir = TempDir::new().expect("failed to create tempdir");
        fs::create_dir_all(dir.path().join("src")).expect("failed to create src dir");
        fs::create_dir_all(dir.path().join("tests")).expect("failed to create tests dir");
        write_file(dir.path(), "src/lib.rs", "pub fn lib_fn() {}");
        write_file(dir.path(), "tests/lib.rs", "fn test_lib() {}");

        let shared = LiveIndex::load(dir.path()).expect("LiveIndex::load failed");
        let index = shared.read().unwrap();

        assert_eq!(
            index.files_by_basename.get("lib.rs"),
            Some(&vec!["src/lib.rs".to_string(), "tests/lib.rs".to_string()])
        );
        assert_eq!(
            index.files_by_dir_component.get("src"),
            Some(&vec!["src/lib.rs".to_string()])
        );
        assert_eq!(
            index.files_by_dir_component.get("tests"),
            Some(&vec!["tests/lib.rs".to_string()])
        );
    }

    #[test]
    fn test_live_index_reload_rebuilds_path_indices() {
        let dir = TempDir::new().expect("failed to create tempdir");
        fs::create_dir_all(dir.path().join("src")).expect("failed to create src dir");
        write_file(dir.path(), "src/alpha.rs", "fn alpha() {}");

        let shared = LiveIndex::load(dir.path()).expect("LiveIndex::load failed");

        fs::remove_file(dir.path().join("src/alpha.rs")).expect("failed to remove alpha");
        fs::create_dir_all(dir.path().join("tests")).expect("failed to create tests dir");
        write_file(dir.path(), "tests/beta.rs", "fn beta() {}");

        {
            let mut index = shared.write().unwrap();
            index.reload(dir.path()).expect("reload should succeed");
        }

        let index = shared.read().unwrap();
        assert!(!index.files_by_basename.contains_key("alpha.rs"));
        assert_eq!(
            index.files_by_basename.get("beta.rs"),
            Some(&vec!["tests/beta.rs".to_string()])
        );
        assert!(!index.files_by_dir_component.contains_key("src"));
        assert_eq!(
            index.files_by_dir_component.get("tests"),
            Some(&vec!["tests/beta.rs".to_string()])
        );
    }

    #[test]
    fn test_dir_component_keys_deduplicate_and_accept_backslashes() {
        assert_eq!(
            dir_component_keys("src\\live_index\\src\\store.rs"),
            vec!["live_index".to_string(), "src".to_string()]
        );
    }

    #[test]
    fn test_update_file_inserts_and_updates_timestamp() {
        let mut index = make_empty_live_index();
        let before = SystemTime::now();
        let file = make_indexed_file_for_mutation("src/new.rs");
        index.update_file("src/new.rs".to_string(), file);
        let after = SystemTime::now();

        assert!(
            index.get_file("src/new.rs").is_some(),
            "file should be inserted"
        );
        assert_eq!(
            index.files_by_basename.get("new.rs"),
            Some(&vec!["src/new.rs".to_string()])
        );
        assert_eq!(
            index.files_by_dir_component.get("src"),
            Some(&vec!["src/new.rs".to_string()])
        );
        let ts = index.loaded_at_system;
        assert!(ts >= before, "loaded_at_system should be >= before update");
        assert!(ts <= after, "loaded_at_system should be <= after update");
    }

    #[test]
    fn test_update_file_replaces_existing() {
        let mut index = make_empty_live_index();
        let file1 = IndexedFile {
            relative_path: "src/foo.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/foo.rs"),
            content: b"fn old() {}".to_vec(),
            symbols: vec![],
            parse_status: ParseStatus::Parsed,
            byte_len: 11,
            content_hash: "old_hash".to_string(),
            references: vec![],
            alias_map: std::collections::HashMap::new(),
        };
        index.update_file("src/foo.rs".to_string(), file1);

        let file2 = IndexedFile {
            relative_path: "src/foo.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/foo.rs"),
            content: b"fn new() {}".to_vec(),
            symbols: vec![dummy_symbol()],
            parse_status: ParseStatus::Parsed,
            byte_len: 11,
            content_hash: "new_hash".to_string(),
            references: vec![],
            alias_map: std::collections::HashMap::new(),
        };
        index.update_file("src/foo.rs".to_string(), file2);

        let retrieved = index.get_file("src/foo.rs").unwrap();
        assert_eq!(
            retrieved.content_hash, "new_hash",
            "should have replaced the file"
        );
        assert_eq!(index.file_count(), 1, "should still have exactly 1 file");
        assert_eq!(
            index.files_by_basename.get("foo.rs"),
            Some(&vec!["src/foo.rs".to_string()])
        );
        assert_eq!(
            index.files_by_dir_component.get("src"),
            Some(&vec!["src/foo.rs".to_string()])
        );
    }

    #[test]
    fn test_add_file_inserts_new() {
        let mut index = make_empty_live_index();
        assert_eq!(index.file_count(), 0);

        let file = make_indexed_file_for_mutation("src/new.rs");
        index.add_file("src/new.rs".to_string(), file);

        assert_eq!(
            index.file_count(),
            1,
            "file count should increase by 1 after add_file"
        );
        assert!(index.get_file("src/new.rs").is_some());
    }

    #[test]
    fn test_remove_file_removes_existing() {
        let mut index = make_empty_live_index();
        let file = make_indexed_file_for_mutation("src/to_delete.rs");
        index.update_file("src/to_delete.rs".to_string(), file);
        assert_eq!(index.file_count(), 1);

        index.remove_file("src/to_delete.rs");
        assert!(
            index.get_file("src/to_delete.rs").is_none(),
            "file should be removed"
        );
        assert_eq!(index.file_count(), 0);
        assert!(!index.files_by_basename.contains_key("to_delete.rs"));
        assert!(!index.files_by_dir_component.contains_key("src"));
    }

    #[test]
    fn test_remove_file_nonexistent_is_noop() {
        let mut index = make_empty_live_index();
        // Set a known timestamp
        let known_ts = index.loaded_at_system;
        // Small sleep to ensure any timestamp update would be different
        std::thread::sleep(Duration::from_millis(5));

        index.remove_file("nonexistent.rs");

        assert_eq!(
            index.loaded_at_system, known_ts,
            "loaded_at_system must NOT change when removing non-existent file"
        );
    }

    #[test]
    fn test_file_count_after_mutations() {
        let mut index = make_empty_live_index();
        assert_eq!(index.file_count(), 0);

        index.add_file("a.rs".to_string(), make_indexed_file_for_mutation("a.rs"));
        assert_eq!(index.file_count(), 1);

        index.add_file("b.rs".to_string(), make_indexed_file_for_mutation("b.rs"));
        assert_eq!(index.file_count(), 2);

        index.update_file("a.rs".to_string(), make_indexed_file_for_mutation("a.rs"));
        assert_eq!(index.file_count(), 2, "update does not add a new entry");

        index.remove_file("a.rs");
        assert_eq!(index.file_count(), 1);

        index.remove_file("nonexistent.rs");
        assert_eq!(
            index.file_count(),
            1,
            "removing nonexistent does not change count"
        );
    }

    // --- Cross-reference fields and reverse index ---

    fn make_ref(name: &str, kind: ReferenceKind, line: u32) -> ReferenceRecord {
        ReferenceRecord {
            name: name.to_string(),
            qualified_name: None,
            kind,
            byte_range: (0, 1),
            line_range: (line, line),
            enclosing_symbol_index: None,
        }
    }

    fn make_indexed_file_with_refs(path: &str, refs: Vec<ReferenceRecord>) -> IndexedFile {
        IndexedFile {
            relative_path: path.to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path(path),
            content: b"fn test() {}".to_vec(),
            symbols: vec![],
            parse_status: ParseStatus::Parsed,
            byte_len: 12,
            content_hash: "abc".to_string(),
            references: refs,
            alias_map: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_indexed_file_from_parse_result_transfers_refs_and_alias_map() {
        use std::collections::HashMap;
        let mut alias_map = HashMap::new();
        alias_map.insert("Map".to_string(), "HashMap".to_string());
        let refs = vec![make_ref("foo", ReferenceKind::Call, 1)];

        let result = FileProcessingResult {
            relative_path: "test.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("test.rs"),
            outcome: FileOutcome::Processed,
            symbols: vec![],
            byte_len: 0,
            content_hash: "abc".to_string(),
            references: refs.clone(),
            alias_map: alias_map.clone(),
        };

        let indexed = IndexedFile::from_parse_result(result, vec![]);
        assert_eq!(indexed.references.len(), 1);
        assert_eq!(indexed.references[0].name, "foo");
        assert_eq!(
            indexed.alias_map.get("Map").map(|s| s.as_str()),
            Some("HashMap")
        );
    }

    #[test]
    fn test_rebuild_reverse_index_builds_name_to_locations() {
        let mut index = make_empty_live_index();

        let refs_a = vec![
            make_ref("process", ReferenceKind::Call, 5),
            make_ref("load", ReferenceKind::Call, 10),
        ];
        let refs_b = vec![make_ref("process", ReferenceKind::Call, 3)];

        index.add_file(
            "a.rs".to_string(),
            make_indexed_file_with_refs("a.rs", refs_a),
        );
        index.add_file(
            "b.rs".to_string(),
            make_indexed_file_with_refs("b.rs", refs_b),
        );

        // process appears in both files
        let locs = index
            .reverse_index
            .get("process")
            .expect("process should be in reverse index");
        assert_eq!(locs.len(), 2, "process referenced in 2 files");

        // load appears only in a.rs
        let locs_load = index
            .reverse_index
            .get("load")
            .expect("load should be in reverse index");
        assert_eq!(locs_load.len(), 1);
        assert_eq!(locs_load[0].file_path, "a.rs");
        assert_eq!(locs_load[0].reference_idx, 1);
    }

    #[test]
    fn test_rebuild_reverse_index_consistent_after_update_file() {
        let mut index = make_empty_live_index();

        let refs_old = vec![make_ref("old_func", ReferenceKind::Call, 1)];
        index.add_file(
            "src.rs".to_string(),
            make_indexed_file_with_refs("src.rs", refs_old),
        );
        assert!(index.reverse_index.contains_key("old_func"));

        let refs_new = vec![make_ref("new_func", ReferenceKind::Call, 1)];
        index.update_file(
            "src.rs".to_string(),
            make_indexed_file_with_refs("src.rs", refs_new),
        );

        assert!(
            !index.reverse_index.contains_key("old_func"),
            "stale entry should be gone"
        );
        assert!(
            index.reverse_index.contains_key("new_func"),
            "new entry should be present"
        );
    }

    #[test]
    fn test_rebuild_reverse_index_excludes_removed_file() {
        let mut index = make_empty_live_index();

        let refs = vec![make_ref("target_fn", ReferenceKind::Call, 2)];
        index.add_file(
            "will_delete.rs".to_string(),
            make_indexed_file_with_refs("will_delete.rs", refs),
        );
        assert!(index.reverse_index.contains_key("target_fn"));

        index.remove_file("will_delete.rs");
        assert!(
            !index.reverse_index.contains_key("target_fn"),
            "removed file's refs should be gone"
        );
    }

    #[test]
    fn test_reference_location_fields() {
        let loc = ReferenceLocation {
            file_path: "src/main.rs".to_string(),
            reference_idx: 3,
        };
        assert_eq!(loc.file_path, "src/main.rs");
        assert_eq!(loc.reference_idx, 3);
    }

    #[test]
    fn test_empty_live_index_has_empty_reverse_index() {
        let index = make_empty_live_index();
        assert!(
            index.reverse_index.is_empty(),
            "fresh index should have empty reverse index"
        );
    }

    #[test]
    fn test_incremental_reverse_index_matches_full_rebuild() {
        let mut index = make_empty_live_index();

        // Add two files with overlapping references
        let refs_a = vec![
            make_ref("shared_fn", ReferenceKind::Call, 1),
            make_ref("only_a", ReferenceKind::Call, 5),
        ];
        let refs_b = vec![
            make_ref("shared_fn", ReferenceKind::Call, 2),
            make_ref("only_b", ReferenceKind::Call, 8),
        ];
        index.add_file(
            "a.rs".to_string(),
            make_indexed_file_with_refs("a.rs", refs_a),
        );
        index.add_file(
            "b.rs".to_string(),
            make_indexed_file_with_refs("b.rs", refs_b),
        );

        // Update a.rs with new references (triggers incremental update)
        let refs_a_new = vec![
            make_ref("shared_fn", ReferenceKind::Call, 1),
            make_ref("replaced_a", ReferenceKind::Call, 10),
        ];
        index.update_file(
            "a.rs".to_string(),
            make_indexed_file_with_refs("a.rs", refs_a_new),
        );

        // Snapshot the incremental result
        let incremental: HashMap<String, Vec<(String, u32)>> = index
            .reverse_index
            .iter()
            .map(|(k, v)| {
                let mut locs: Vec<(String, u32)> = v
                    .iter()
                    .map(|l| (l.file_path.clone(), l.reference_idx))
                    .collect();
                locs.sort();
                (k.clone(), locs)
            })
            .collect();

        // Now do a full rebuild and compare
        index.rebuild_reverse_index();
        let full_rebuild: HashMap<String, Vec<(String, u32)>> = index
            .reverse_index
            .iter()
            .map(|(k, v)| {
                let mut locs: Vec<(String, u32)> = v
                    .iter()
                    .map(|l| (l.file_path.clone(), l.reference_idx))
                    .collect();
                locs.sort();
                (k.clone(), locs)
            })
            .collect();

        assert_eq!(
            incremental, full_rebuild,
            "incremental update should produce same result as full rebuild"
        );

        // Verify specific expectations
        assert!(
            !index.reverse_index.contains_key("only_a"),
            "only_a should be gone after update"
        );
        assert!(
            index.reverse_index.contains_key("replaced_a"),
            "replaced_a should be present"
        );
        assert!(
            index.reverse_index.contains_key("only_b"),
            "only_b should still be present from b.rs"
        );
        let shared = index.reverse_index.get("shared_fn").unwrap();
        assert_eq!(shared.len(), 2, "shared_fn still referenced in both files");
    }

    #[test]
    fn test_incremental_reverse_index_remove() {
        let mut index = make_empty_live_index();

        let refs_a = vec![
            make_ref("common", ReferenceKind::Call, 1),
            make_ref("unique_a", ReferenceKind::Call, 3),
        ];
        let refs_b = vec![
            make_ref("common", ReferenceKind::Call, 2),
            make_ref("unique_b", ReferenceKind::Call, 4),
        ];
        index.add_file(
            "a.rs".to_string(),
            make_indexed_file_with_refs("a.rs", refs_a),
        );
        index.add_file(
            "b.rs".to_string(),
            make_indexed_file_with_refs("b.rs", refs_b),
        );

        // Remove a.rs
        index.remove_file("a.rs");

        // unique_a should be gone entirely
        assert!(
            !index.reverse_index.contains_key("unique_a"),
            "unique_a should be removed with a.rs"
        );
        // unique_b should remain
        assert!(
            index.reverse_index.contains_key("unique_b"),
            "unique_b should survive"
        );
        // common should only have b.rs
        let common_locs = index
            .reverse_index
            .get("common")
            .expect("common should still exist from b.rs");
        assert_eq!(common_locs.len(), 1);
        assert_eq!(common_locs[0].file_path, "b.rs");

        // Verify incremental matches full rebuild
        let incremental: HashMap<String, Vec<(String, u32)>> = index
            .reverse_index
            .iter()
            .map(|(k, v)| {
                let mut locs: Vec<(String, u32)> = v
                    .iter()
                    .map(|l| (l.file_path.clone(), l.reference_idx))
                    .collect();
                locs.sort();
                (k.clone(), locs)
            })
            .collect();

        index.rebuild_reverse_index();
        let full_rebuild: HashMap<String, Vec<(String, u32)>> = index
            .reverse_index
            .iter()
            .map(|(k, v)| {
                let mut locs: Vec<(String, u32)> = v
                    .iter()
                    .map(|l| (l.file_path.clone(), l.reference_idx))
                    .collect();
                locs.sort();
                (k.clone(), locs)
            })
            .collect();

        assert_eq!(
            incremental, full_rebuild,
            "incremental remove should match full rebuild"
        );
    }

    #[test]
    fn test_tier_counts() {
        use crate::domain::index::{AdmissionDecision, AdmissionTier, SkipReason, SkippedFile};

        let mut index = make_empty_live_index();
        assert_eq!(index.tier_counts(), (0, 0, 0));

        index.add_skipped_file(SkippedFile {
            path: "model.bin".into(),
            size: 1000,
            extension: Some("bin".into()),
            decision: AdmissionDecision::skip(
                AdmissionTier::MetadataOnly,
                SkipReason::DenylistedExtension,
            ),
        });
        index.add_skipped_file(SkippedFile {
            path: "huge.dat".into(),
            size: 200_000_000,
            extension: Some("dat".into()),
            decision: AdmissionDecision::skip(AdmissionTier::HardSkip, SkipReason::SizeCeiling),
        });

        assert_eq!(index.tier_counts(), (0, 1, 1));
    }
}
