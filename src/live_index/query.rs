use std::time::{Duration, SystemTime};

use crate::domain::SymbolRecord;

use super::store::{IndexedFile, IndexState, LiveIndex, ParseStatus};

/// Summary health statistics for the LiveIndex.
#[derive(Debug, Clone)]
pub struct HealthStats {
    pub file_count: usize,
    pub symbol_count: usize,
    pub parsed_count: usize,
    pub partial_parse_count: usize,
    pub failed_count: usize,
    pub load_duration: Duration,
}

impl LiveIndex {
    /// O(1) lookup of a file by its relative path.
    pub fn get_file(&self, relative_path: &str) -> Option<&IndexedFile> {
        self.files.get(relative_path)
    }

    /// Returns the symbol slice for a file, or an empty slice if not found.
    pub fn symbols_for_file(&self, relative_path: &str) -> &[SymbolRecord] {
        self.files
            .get(relative_path)
            .map(|f| f.symbols.as_slice())
            .unwrap_or(&[])
    }

    /// Iterate all (path, file) pairs in the index.
    pub fn all_files(&self) -> impl Iterator<Item = (&String, &IndexedFile)> {
        self.files.iter()
    }

    /// Number of indexed files.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Total symbols across all indexed files.
    pub fn symbol_count(&self) -> usize {
        self.files.values().map(|f| f.symbols.len()).sum()
    }

    /// `true` when the index has been loaded and the circuit breaker has NOT tripped.
    pub fn is_ready(&self) -> bool {
        if self.is_empty {
            return false;
        }
        !self.cb_state.is_tripped()
    }

    /// Returns the current index state.
    pub fn index_state(&self) -> IndexState {
        if self.is_empty {
            return IndexState::Empty;
        }
        if self.cb_state.is_tripped() {
            IndexState::CircuitBreakerTripped {
                summary: self.cb_state.summary(),
            }
        } else {
            IndexState::Ready
        }
    }

    /// Returns the wall-clock time when the index was last loaded.
    pub fn loaded_at_system(&self) -> SystemTime {
        self.loaded_at_system
    }

    /// Compute health statistics for the index.
    ///
    /// Per CONTEXT.md: includes file counts, symbol counts, parse status breakdown,
    /// and total load duration.
    pub fn health_stats(&self) -> HealthStats {
        let mut parsed_count = 0usize;
        let mut partial_parse_count = 0usize;
        let mut failed_count = 0usize;
        let mut symbol_count = 0usize;

        for file in self.files.values() {
            symbol_count += file.symbols.len();
            match &file.parse_status {
                ParseStatus::Parsed => parsed_count += 1,
                ParseStatus::PartialParse { .. } => partial_parse_count += 1,
                ParseStatus::Failed { .. } => failed_count += 1,
            }
        }

        HealthStats {
            file_count: self.files.len(),
            symbol_count,
            parsed_count,
            partial_parse_count,
            failed_count,
            load_duration: self.load_duration,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::{LanguageId, SymbolRecord, SymbolKind};
    use crate::live_index::store::{CircuitBreakerState, IndexedFile, IndexState, LiveIndex, ParseStatus};
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    fn make_symbol(name: &str) -> SymbolRecord {
        SymbolRecord {
            name: name.to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 10),
            line_range: (0, 1),
        }
    }

    fn make_indexed_file(path: &str, symbols: Vec<SymbolRecord>, status: ParseStatus) -> IndexedFile {
        IndexedFile {
            relative_path: path.to_string(),
            language: LanguageId::Rust,
            content: b"fn test() {}".to_vec(),
            symbols,
            parse_status: status,
            byte_len: 12,
            content_hash: "abc".to_string(),
        }
    }

    fn make_index(files: Vec<(&str, IndexedFile)>, tripped: bool) -> LiveIndex {
        let cb = CircuitBreakerState::new(0.20);
        if tripped {
            // Force-trip by recording enough failures
            for i in 0..10 {
                cb.record_success();
                if i < 7 { cb.record_success(); }
            }
            for i in 0..5 {
                cb.record_failure(&format!("f{i}.rs"), "error");
            }
            cb.should_abort();
        }

        LiveIndex {
            files: files
                .into_iter()
                .map(|(p, f)| (p.to_string(), f))
                .collect(),
            loaded_at: Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: Duration::from_millis(50),
            cb_state: cb,
            is_empty: false,
        }
    }

    #[test]
    fn test_get_file_returns_some_for_existing() {
        let f = make_indexed_file("src/main.rs", vec![make_symbol("main")], ParseStatus::Parsed);
        let index = make_index(vec![("src/main.rs", f)], false);
        assert!(index.get_file("src/main.rs").is_some());
    }

    #[test]
    fn test_get_file_returns_none_for_missing() {
        let index = make_index(vec![], false);
        assert!(index.get_file("nonexistent.rs").is_none());
    }

    #[test]
    fn test_symbols_for_file_returns_slice() {
        let sym = make_symbol("foo");
        let f = make_indexed_file("src/main.rs", vec![sym.clone()], ParseStatus::Parsed);
        let index = make_index(vec![("src/main.rs", f)], false);
        let syms = index.symbols_for_file("src/main.rs");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "foo");
    }

    #[test]
    fn test_symbols_for_file_returns_empty_for_missing() {
        let index = make_index(vec![], false);
        let syms = index.symbols_for_file("nonexistent.rs");
        assert!(syms.is_empty());
    }

    #[test]
    fn test_all_files_returns_all_entries() {
        let f1 = make_indexed_file("a.rs", vec![], ParseStatus::Parsed);
        let f2 = make_indexed_file("b.rs", vec![], ParseStatus::Parsed);
        let index = make_index(vec![("a.rs", f1), ("b.rs", f2)], false);
        let pairs: Vec<_> = index.all_files().collect();
        assert_eq!(pairs.len(), 2);
    }

    #[test]
    fn test_file_count_correct() {
        let f1 = make_indexed_file("a.rs", vec![], ParseStatus::Parsed);
        let f2 = make_indexed_file("b.rs", vec![], ParseStatus::Parsed);
        let f3 = make_indexed_file("c.rs", vec![], ParseStatus::Parsed);
        let index = make_index(vec![("a.rs", f1), ("b.rs", f2), ("c.rs", f3)], false);
        assert_eq!(index.file_count(), 3);
    }

    #[test]
    fn test_symbol_count_across_all_files() {
        let f1 = make_indexed_file("a.rs", vec![make_symbol("x"), make_symbol("y")], ParseStatus::Parsed);
        let f2 = make_indexed_file("b.rs", vec![make_symbol("z")], ParseStatus::Parsed);
        let index = make_index(vec![("a.rs", f1), ("b.rs", f2)], false);
        assert_eq!(index.symbol_count(), 3);
    }

    #[test]
    fn test_health_stats_correct_breakdown() {
        let f1 = make_indexed_file("a.rs", vec![make_symbol("x")], ParseStatus::Parsed);
        let f2 = make_indexed_file("b.rs", vec![make_symbol("y")], ParseStatus::PartialParse {
            warning: "syntax err".to_string(),
        });
        let f3 = make_indexed_file("c.rs", vec![], ParseStatus::Failed {
            error: "failed".to_string(),
        });
        let index = make_index(vec![("a.rs", f1), ("b.rs", f2), ("c.rs", f3)], false);

        let stats = index.health_stats();
        assert_eq!(stats.file_count, 3);
        assert_eq!(stats.symbol_count, 2);
        assert_eq!(stats.parsed_count, 1);
        assert_eq!(stats.partial_parse_count, 1);
        assert_eq!(stats.failed_count, 1);
    }

    #[test]
    fn test_is_ready_true_when_not_tripped() {
        let index = make_index(vec![], false);
        assert!(index.is_ready());
    }

    #[test]
    fn test_is_ready_false_when_tripped() {
        // Build a tripped circuit breaker by direct manipulation
        let cb = CircuitBreakerState::new(0.20);
        for _ in 0..7 { cb.record_success(); }
        for i in 0..3 { cb.record_failure(&format!("f{i}.rs"), "err"); }
        cb.should_abort(); // This will trip it

        let index = LiveIndex {
            files: HashMap::new(),
            loaded_at: Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: Duration::from_millis(10),
            cb_state: cb,
            is_empty: false,
        };
        assert!(!index.is_ready());
    }

    #[test]
    fn test_index_state_ready() {
        let index = make_index(vec![], false);
        assert_eq!(index.index_state(), IndexState::Ready);
    }

    #[test]
    fn test_index_state_circuit_breaker_tripped_with_summary() {
        let cb = CircuitBreakerState::new(0.20);
        for _ in 0..7 { cb.record_success(); }
        for i in 0..3 { cb.record_failure(&format!("f{i}.rs"), "err"); }
        cb.should_abort();

        let index = LiveIndex {
            files: HashMap::new(),
            loaded_at: Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: Duration::from_millis(10),
            cb_state: cb,
            is_empty: false,
        };

        match index.index_state() {
            IndexState::CircuitBreakerTripped { summary } => {
                assert!(!summary.is_empty(), "summary should not be empty");
            }
            other => panic!("expected CircuitBreakerTripped, got {:?}", other),
        }
    }
}
