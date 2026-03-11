use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime};

use rayon::prelude::*;
use tracing::{error, info, warn};

use crate::domain::{
    FileOutcome, FileProcessingResult, LanguageId, ReferenceRecord, SymbolRecord,
    find_enclosing_symbol,
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

/// The in-memory index: file contents and parsed symbols for all discovered files.
pub struct LiveIndex {
    /// Keyed by `relative_path` (forward-slash normalized).
    pub(crate) files: HashMap<String, IndexedFile>,
    pub(crate) loaded_at: Instant,
    /// Wall-clock time when index was last loaded. Used by what_changed tool.
    pub(crate) loaded_at_system: SystemTime,
    pub(crate) load_duration: Duration,
    pub(crate) cb_state: CircuitBreakerState,
    /// True when constructed with empty() and reload() has not been called.
    pub(crate) is_empty: bool,
    /// Repo-level reverse index: reference name -> all locations in the index.
    /// Rebuilt synchronously after every mutation (update_file, add_file, remove_file, reload).
    pub(crate) reverse_index: HashMap<String, Vec<ReferenceLocation>>,
    /// Trigram search index for file-level text search acceleration.
    pub(crate) trigram_index: super::trigram::TrigramIndex,
}

/// Thread-safe shared handle to the index.
pub type SharedIndex = Arc<RwLock<LiveIndex>>;

impl LiveIndex {
    /// Load all source files under `root` into memory in parallel (Rayon), parse them,
    /// and return a `SharedIndex`.
    ///
    /// This function is **synchronous** — it must complete before the async tokio runtime
    /// needs the index. Rayon handles internal parallelism.
    pub fn load(root: &Path) -> Result<SharedIndex> {
        let start = Instant::now();

        info!("LiveIndex::load starting at {:?}", root);

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

                let result = parsing::process_file(&df.relative_path, &bytes, df.language.clone());
                let indexed = IndexedFile::from_parse_result(result, bytes);
                Some((df.relative_path.clone(), indexed))
            })
            .collect();

        // 3. Build HashMap sequentially, running circuit breaker checks
        let cb_state = CircuitBreakerState::from_env();
        let mut files: HashMap<String, IndexedFile> = HashMap::with_capacity(parse_results.len());

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
                files.insert(path, indexed_file);
                break;
            }

            files.insert(path, indexed_file);
        }

        if cb_tripped {
            cb_state.tripped.store(true, Ordering::Relaxed);
        }

        let load_duration = start.elapsed();
        info!(
            "LiveIndex loaded: {} files, {} symbols, {:?}",
            files.len(),
            files.values().map(|f| f.symbols.len()).sum::<usize>(),
            load_duration
        );

        let trigram_index = super::trigram::TrigramIndex::build_from_files(&files);

        let mut index = LiveIndex {
            files,
            loaded_at: Instant::now(),
            loaded_at_system: SystemTime::now(),
            load_duration,
            cb_state,
            is_empty: false,
            reverse_index: HashMap::new(),
            trigram_index,
        };
        index.rebuild_reverse_index();

        Ok(Arc::new(RwLock::new(index)))
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
            reverse_index: HashMap::new(),
            trigram_index: super::trigram::TrigramIndex::new(),
        };
        Arc::new(RwLock::new(index))
    }

    /// Reload all source files under `root` into this index in-place.
    ///
    /// Replaces all files, resets circuit breaker, and updates timestamps.
    /// On success sets `is_empty = false`. On error the index remains in its previous state
    /// (but partial results may have been loaded).
    pub fn reload(&mut self, root: &Path) -> crate::error::Result<()> {
        use crate::error::TokenizorError;

        let start = Instant::now();

        info!("LiveIndex::reload starting at {:?}", root);

        // Validate root exists before attempting discovery
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

                let result = parsing::process_file(&df.relative_path, &bytes, df.language.clone());
                let indexed = IndexedFile::from_parse_result(result, bytes);
                Some((df.relative_path.clone(), indexed))
            })
            .collect();

        // 3. Build new file map with fresh circuit breaker
        let new_cb = CircuitBreakerState::from_env();
        let mut new_files: HashMap<String, IndexedFile> =
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
                new_files.insert(path, indexed_file);
                break;
            }

            new_files.insert(path, indexed_file);
        }

        if cb_tripped {
            new_cb.tripped.store(true, Ordering::Relaxed);
        }

        let load_duration = start.elapsed();
        info!(
            "LiveIndex::reload done: {} files, {} symbols, {:?}",
            new_files.len(),
            new_files.values().map(|f| f.symbols.len()).sum::<usize>(),
            load_duration
        );

        // 4. Swap in-place
        self.files = new_files;
        self.loaded_at = Instant::now();
        self.loaded_at_system = SystemTime::now();
        self.load_duration = load_duration;
        self.cb_state = new_cb;
        self.is_empty = false;
        self.trigram_index = super::trigram::TrigramIndex::build_from_files(&self.files);
        self.rebuild_reverse_index();

        Ok(())
    }

    /// Insert or replace a single file in the index without a full reload.
    ///
    /// Updates `loaded_at_system` to reflect the mutation time.
    /// If the file already exists, its entry is replaced atomically.
    pub fn update_file(&mut self, path: String, file: IndexedFile) {
        self.trigram_index.update_file(&path, &file.content);
        self.files.insert(path, file);
        self.loaded_at_system = SystemTime::now();
        self.rebuild_reverse_index();
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
        if self.files.remove(path).is_some() {
            self.trigram_index.remove_file(path);
            self.loaded_at_system = SystemTime::now();
            self.rebuild_reverse_index();
        }
    }

    /// Rebuild `reverse_index` from scratch by iterating all files' references.
    ///
    /// Called synchronously after every mutation to keep the index consistent.
    /// Maps reference name -> Vec of ReferenceLocation (file + index into references vec).
    pub(crate) fn rebuild_reverse_index(&mut self) {
        let mut idx: HashMap<String, Vec<ReferenceLocation>> = HashMap::new();
        for (file_path, indexed_file) in &self.files {
            for (reference_idx, reference) in indexed_file.references.iter().enumerate() {
                idx.entry(reference.name.clone())
                    .or_default()
                    .push(ReferenceLocation {
                        file_path: file_path.clone(),
                        reference_idx: reference_idx as u32,
                    });
            }
        }
        self.reverse_index = idx;
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
        }
    }

    fn make_result(outcome: FileOutcome, symbols: Vec<SymbolRecord>) -> FileProcessingResult {
        FileProcessingResult {
            relative_path: "test.rs".to_string(),
            language: LanguageId::Rust,
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
            reverse_index: HashMap::new(),
            trigram_index: crate::live_index::trigram::TrigramIndex::new(),
        }
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
}
