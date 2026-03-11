use std::collections::HashSet;
use std::time::{Duration, SystemTime};

use crate::domain::{LanguageId, ReferenceKind, ReferenceRecord, SymbolRecord};
use crate::watcher::{WatcherInfo, WatcherState};

use super::store::{IndexState, IndexedFile, LiveIndex, ParseStatus};

// ---------------------------------------------------------------------------
// Module path resolution for find_dependents
// ---------------------------------------------------------------------------

/// Resolve the logical module path for a file based on language conventions.
///
/// Returns `None` if the file doesn't follow a recognized module convention.
///
/// Examples (Rust):
///   "src/lib.rs"              → Some("crate")
///   "src/main.rs"             → Some("crate")
///   "src/error.rs"            → Some("crate::error")
///   "src/live_index/mod.rs"   → Some("crate::live_index")
///   "src/live_index/store.rs" → Some("crate::live_index::store")
///
/// Examples (Python):
///   "src/__init__.py"         → Some("src")
///   "src/foo.py"              → Some("src.foo")
///   "src/foo/__init__.py"     → Some("src.foo")
///
/// Examples (JS/TS):
///   "src/index.js"            → Some("src")
///   "src/utils/index.ts"      → Some("src/utils")
fn resolve_module_path(file_path: &str, language: &LanguageId) -> Option<String> {
    let path = std::path::Path::new(file_path);

    match language {
        LanguageId::Rust => {
            // Strip "src/" prefix; files outside src/ don't have crate module paths.
            let stripped = path.strip_prefix("src").ok()?;
            let mut components: Vec<String> = stripped
                .components()
                .filter_map(|c| c.as_os_str().to_str().map(String::from))
                .collect();

            // Remove extension from last component
            if let Some(last) = components.last_mut()
                && let Some(stem) = std::path::Path::new(last.as_str())
                    .file_stem()
                    .and_then(|s| s.to_str())
            {
                *last = stem.to_string();
            }

            // Drop "lib", "main", "mod" — these map to their parent module
            if matches!(
                components.last().map(|s| s.as_str()),
                Some("lib" | "main" | "mod")
            ) {
                components.pop();
            }

            if components.is_empty() {
                Some("crate".to_string())
            } else {
                Some(format!("crate::{}", components.join("::")))
            }
        }
        LanguageId::Python => {
            let mut components: Vec<String> = path
                .components()
                .filter_map(|c| c.as_os_str().to_str().map(String::from))
                .collect();

            // Remove extension from last component
            if let Some(last) = components.last_mut()
                && let Some(stem) = std::path::Path::new(last.as_str())
                    .file_stem()
                    .and_then(|s| s.to_str())
            {
                *last = stem.to_string();
            }

            // Drop __init__ — maps to the package (parent directory)
            if matches!(components.last().map(|s| s.as_str()), Some("__init__")) {
                components.pop();
            }

            if components.is_empty() {
                None
            } else {
                Some(components.join("."))
            }
        }
        LanguageId::JavaScript | LanguageId::TypeScript => {
            let mut components: Vec<String> = path
                .components()
                .filter_map(|c| c.as_os_str().to_str().map(String::from))
                .collect();

            // Remove extension from last component
            if let Some(last) = components.last_mut()
                && let Some(stem) = std::path::Path::new(last.as_str())
                    .file_stem()
                    .and_then(|s| s.to_str())
            {
                *last = stem.to_string();
            }

            // Drop "index" — maps to the directory
            if matches!(components.last().map(|s| s.as_str()), Some("index")) {
                components.pop();
            }

            if components.is_empty() {
                None
            } else {
                Some(components.join("/"))
            }
        }
        _ => None,
    }
}

fn declared_scope(file: &IndexedFile) -> Option<String> {
    let content = String::from_utf8_lossy(&file.content);
    match file.language {
        LanguageId::CSharp => parse_declared_scope(&content, "namespace"),
        LanguageId::Java => parse_declared_scope(&content, "package"),
        _ => None,
    }
}

fn parse_declared_scope(content: &str, keyword: &str) -> Option<String> {
    content.lines().find_map(|line| {
        let trimmed = line.split("//").next().unwrap_or("").trim();
        let rest = trimmed.strip_prefix(keyword)?.trim_start();
        let scope: String = rest
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '.')
            .collect();
        if scope.is_empty() { None } else { Some(scope) }
    })
}

fn imported_scope(language: &LanguageId, reference: &ReferenceRecord) -> Option<String> {
    if reference.kind != ReferenceKind::Import {
        return None;
    }

    let qualified_name = reference.qualified_name.as_deref()?;
    match language {
        LanguageId::CSharp => Some(qualified_name.to_string()),
        LanguageId::Java => {
            let trimmed = qualified_name.trim_end_matches(".*");
            match trimmed.rsplit_once('.') {
                Some((scope, _)) if !scope.is_empty() => Some(scope.to_string()),
                _ if !trimmed.is_empty() => Some(trimmed.to_string()),
                _ => None,
            }
        }
        _ => None,
    }
}

fn can_match_type_dependents(
    dependent_file: &IndexedFile,
    target_language: &LanguageId,
    target_scope: Option<&str>,
) -> bool {
    if &dependent_file.language != target_language {
        return false;
    }

    match target_language {
        LanguageId::CSharp | LanguageId::Java => {
            let Some(target_scope) = target_scope else {
                return true;
            };

            if declared_scope(dependent_file).as_deref() == Some(target_scope) {
                return true;
            }

            dependent_file
                .references
                .iter()
                .filter_map(|reference| imported_scope(&dependent_file.language, reference))
                .any(|scope| scope == target_scope)
        }
        _ => false,
    }
}

fn matches_target_stem(text: &str, stem: &str) -> bool {
    text == stem
        || text.ends_with(&format!("/{stem}"))
        || text.ends_with(&format!("::{stem}"))
        || text.ends_with(&format!(".{stem}"))
        || text.contains(&format!("/{stem}/"))
        || text.contains(&format!("::{stem}::"))
}

fn matches_target_module(language: &LanguageId, text: &str, module_path: Option<&str>) -> bool {
    let Some(module_path) = module_path else {
        return false;
    };

    let module_sep = match language {
        LanguageId::Python => ".",
        LanguageId::JavaScript | LanguageId::TypeScript => "/",
        _ => "::",
    };

    if text == module_path || text.starts_with(&format!("{module_path}{module_sep}")) {
        return true;
    }

    if *language == LanguageId::Rust
        && let Some(tail) = module_path.strip_prefix("crate::")
    {
        return text == tail
            || text.ends_with(&format!("::{tail}"))
            || text.contains(&format!("::{tail}::"));
    }

    false
}

fn matches_target_import(
    language: &LanguageId,
    reference: &ReferenceRecord,
    stem: &str,
    module_path: Option<&str>,
) -> bool {
    if reference.kind != ReferenceKind::Import {
        return false;
    }

    matches_target_stem(&reference.name, stem)
        || reference
            .qualified_name
            .as_deref()
            .map(|text| {
                matches_target_stem(text, stem)
                    || matches_target_module(language, text, module_path)
            })
            .unwrap_or(false)
        || matches_target_module(language, &reference.name, module_path)
}

// ---------------------------------------------------------------------------
// Built-in type filter lists (per-language)
// ---------------------------------------------------------------------------

const RUST_BUILTINS: &[&str] = &[
    "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize", "f32",
    "f64", "bool", "char", "str", "String", "Self", "self",
];

const PYTHON_BUILTINS: &[&str] = &[
    "int", "float", "str", "bool", "list", "dict", "tuple", "set", "None", "bytes", "object",
    "type",
];

const JS_BUILTINS: &[&str] = &[
    "string",
    "number",
    "boolean",
    "undefined",
    "null",
    "Object",
    "Array",
    "Function",
    "Symbol",
    "Promise",
    "Error",
];

const TS_BUILTINS: &[&str] = &[
    "string",
    "number",
    "boolean",
    "undefined",
    "null",
    "void",
    "never",
    "any",
    "unknown",
    "Object",
    "Array",
    "Function",
    "Symbol",
    "Promise",
    "Error",
    "Record",
    "Partial",
    "Required",
    "Readonly",
    "Pick",
    "Omit",
];

const GO_BUILTINS: &[&str] = &[
    "int",
    "int8",
    "int16",
    "int32",
    "int64",
    "uint",
    "uint8",
    "uint16",
    "uint32",
    "uint64",
    "float32",
    "float64",
    "complex64",
    "complex128",
    "bool",
    "string",
    "byte",
    "rune",
    "error",
    "any",
];

const JAVA_BUILTINS: &[&str] = &[
    "int",
    "long",
    "short",
    "byte",
    "float",
    "double",
    "boolean",
    "char",
    "void",
    "String",
    "Object",
    "Integer",
    "Long",
    "Short",
    "Byte",
    "Float",
    "Double",
    "Boolean",
    "Character",
];

/// Single-letter generic type parameter names that are almost always noise.
const SINGLE_LETTER_GENERICS: &[&str] = &[
    "T", "K", "V", "E", "R", "S", "A", "B", "C", "D", "N", "M", "P", "U", "W", "X", "Y", "Z",
];

/// Returns `true` when `name` is a known built-in primitive/stdlib type or a
/// single-letter generic parameter that would generate false-positive matches.
///
/// This is a coarse, language-agnostic filter applied at query time. It checks
/// all language lists so that cross-language repos are handled uniformly.
fn is_filtered_name(name: &str) -> bool {
    SINGLE_LETTER_GENERICS.contains(&name)
        || RUST_BUILTINS.contains(&name)
        || PYTHON_BUILTINS.contains(&name)
        || JS_BUILTINS.contains(&name)
        || TS_BUILTINS.contains(&name)
        || GO_BUILTINS.contains(&name)
        || JAVA_BUILTINS.contains(&name)
}

/// Summary health statistics for the LiveIndex.
#[derive(Debug, Clone)]
pub struct HealthStats {
    pub file_count: usize,
    pub symbol_count: usize,
    pub parsed_count: usize,
    pub partial_parse_count: usize,
    pub failed_count: usize,
    pub load_duration: Duration,
    /// Current state of the file watcher.
    pub watcher_state: WatcherState,
    /// Total number of file-system events processed by the watcher.
    pub events_processed: u64,
    /// Wall-clock time of the most recent event processed, if any.
    pub last_event_at: Option<SystemTime>,
    /// Effective debounce window in milliseconds.
    pub debounce_window_ms: u64,
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
    /// Watcher fields are populated with safe defaults (Off state, zero counts).
    /// Use `health_stats_with_watcher` when a watcher is active.
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
            watcher_state: WatcherState::Off,
            events_processed: 0,
            last_event_at: None,
            debounce_window_ms: 200,
        }
    }

    /// Compute health statistics, populating watcher fields from the provided `WatcherInfo`.
    ///
    /// Use this variant when the file watcher is active and its state should be reflected
    /// in health reports.
    pub fn health_stats_with_watcher(&self, watcher: &WatcherInfo) -> HealthStats {
        let mut stats = self.health_stats();
        stats.watcher_state = watcher.state.clone();
        stats.events_processed = watcher.events_processed;
        stats.last_event_at = watcher.last_event_at;
        stats.debounce_window_ms = watcher.debounce_window_ms;
        stats
    }

    // -----------------------------------------------------------------------
    // Cross-reference query methods (Phase 4, Plan 02)
    // -----------------------------------------------------------------------

    /// Find all `ReferenceRecord`s across the repo that match `name`.
    ///
    /// # Arguments
    /// * `name` — the reference name to look up. If it contains `::` or `.`,
    ///   it is treated as a qualified name and matched against `qualified_name`.
    ///   Otherwise matched against `name`.
    /// * `kind_filter` — when `Some(k)`, only references of kind `k` are returned.
    /// * `include_filtered` — when `false` (default), built-in type names and
    ///   single-letter generic parameters are silently filtered out (returns empty).
    ///   Set to `true` to bypass that filter.
    ///
    /// # Alias resolution (XREF-05)
    /// In addition to the direct reverse-index lookup, the method also checks
    /// every file's `alias_map`. If a file declares `alias_map["Map"] = "HashMap"`,
    /// then searching for `"HashMap"` will also yield references stored under `"Map"`.
    ///
    /// Returns a `Vec` of `(file_path, &ReferenceRecord)` tuples.
    pub fn find_references_for_name(
        &self,
        name: &str,
        kind_filter: Option<ReferenceKind>,
        include_filtered: bool,
    ) -> Vec<(&str, &ReferenceRecord)> {
        // Apply built-in / generic filter first.
        if !include_filtered && is_filtered_name(name) {
            return vec![];
        }

        let is_qualified = name.contains("::") || name.contains('.');

        let mut results: Vec<(&str, &ReferenceRecord)> = Vec::new();

        if is_qualified {
            // Qualified lookup: the reverse index is keyed by simple name, not qualified name.
            // We must scan all files and match against the qualified_name field.
            for (file_path, file) in &self.files {
                for reference in &file.references {
                    if let Some(qn) = reference.qualified_name.as_deref() {
                        if qn != name {
                            continue;
                        }
                    } else {
                        continue;
                    }
                    if let Some(kf) = kind_filter
                        && reference.kind != kf
                    {
                        continue;
                    }
                    results.push((file_path.as_str(), reference));
                }
            }
        } else {
            // Simple lookup: use the reverse index for O(1) name lookup.
            self.collect_refs_for_key(name, kind_filter, &mut results);

            // Alias resolution: find any alias that resolves to `name`.
            // e.g. alias_map["Map"] = "HashMap" means we also look up "Map".
            // Collect aliases first to avoid re-borrowing self during mutation of results.
            let aliases: Vec<String> = self
                .files
                .values()
                .flat_map(|file| {
                    file.alias_map
                        .iter()
                        .filter(|(_alias, original)| original.as_str() == name)
                        .map(|(alias, _)| alias.clone())
                })
                .collect();

            for alias in &aliases {
                self.collect_refs_for_key(alias, kind_filter, &mut results);
            }
        }

        results
    }

    /// Internal helper: look up `lookup_key` in `reverse_index`, resolve each location,
    /// apply kind filter (no qualified-name check), and append matching results.
    ///
    /// Only used for simple (non-qualified) name lookups.
    fn collect_refs_for_key<'a>(
        &'a self,
        lookup_key: &str,
        kind_filter: Option<ReferenceKind>,
        results: &mut Vec<(&'a str, &'a ReferenceRecord)>,
    ) {
        if let Some(locations) = self.reverse_index.get(lookup_key) {
            for loc in locations {
                let file = match self.files.get(&loc.file_path) {
                    Some(f) => f,
                    None => continue,
                };
                let reference = match file.references.get(loc.reference_idx as usize) {
                    Some(r) => r,
                    None => continue,
                };
                if let Some(kf) = kind_filter
                    && reference.kind != kf
                {
                    continue;
                }
                results.push((loc.file_path.as_str(), reference));
            }
        }
    }

    /// Find all files that import (depend on) `target_path`.
    ///
    /// Uses two strategies:
    /// 1. **Stem matching** — the import's `name`/`qualified_name` contains the file stem
    ///    as a path segment (e.g. `import db` matches `src/db.rs`).
    /// 2. **Module path matching** — resolves the file to its logical module path
    ///    (e.g. `src/live_index/mod.rs` → `crate::live_index`) and checks if any import
    ///    starts with that module path. This handles `lib.rs`, `mod.rs`, `__init__.py`,
    ///    and `index.js`/`index.ts` which stem matching misses.
    ///
    /// Returns a `Vec` of `(importing_file_path, &import_reference)` tuples.
    pub fn find_dependents_for_file(&self, target_path: &str) -> Vec<(&str, &ReferenceRecord)> {
        let Some(target_file) = self.files.get(target_path) else {
            return vec![];
        };

        // Extract the file stem: "src/db.rs" → "db"
        let stem = std::path::Path::new(target_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(target_path);

        // Resolve the logical module path for the target file.
        let module_path = resolve_module_path(target_path, &target_file.language);

        let target_language = target_file.language.clone();
        let target_scope = declared_scope(target_file);
        let target_symbol_names: HashSet<&str> = target_file
            .symbols
            .iter()
            .map(|symbol| symbol.name.as_str())
            .filter(|name| !name.is_empty())
            .collect();

        let mut results = Vec::new();

        for (file_path, file) in &self.files {
            // Don't report a file as depending on itself.
            if file_path.as_str() == target_path {
                continue;
            }

            let matching_imports: Vec<&ReferenceRecord> = file
                .references
                .iter()
                .filter(|reference| {
                    matches_target_import(&target_language, reference, stem, module_path.as_deref())
                })
                .collect();

            if !target_symbol_names.is_empty()
                && (can_match_type_dependents(file, &target_language, target_scope.as_deref())
                    || !matching_imports.is_empty())
            {
                let symbol_refs: Vec<&ReferenceRecord> = file
                    .references
                    .iter()
                    .filter(|reference| {
                        reference.kind != ReferenceKind::Import
                            && target_symbol_names.contains(reference.name.as_str())
                    })
                    .collect();

                if !symbol_refs.is_empty() {
                    results.extend(
                        symbol_refs
                            .into_iter()
                            .map(|reference| (file_path.as_str(), reference)),
                    );
                    continue;
                }
            }

            results.extend(
                matching_imports
                    .into_iter()
                    .map(|reference| (file_path.as_str(), reference)),
            );
        }

        results.sort_by(|a, b| {
            a.0.cmp(b.0)
                .then(a.1.line_range.0.cmp(&b.1.line_range.0))
                .then(a.1.byte_range.0.cmp(&b.1.byte_range.0))
        });

        results
    }

    /// Returns all `Call` references inside the given file whose
    /// `enclosing_symbol_index` equals `symbol_index`.
    ///
    /// These are the "callees" — functions called from within the target symbol.
    /// Consumed by `get_context_bundle` (Plan 03).
    pub fn callees_for_symbol(
        &self,
        file_path: &str,
        symbol_index: usize,
    ) -> Vec<&ReferenceRecord> {
        match self.files.get(file_path) {
            None => vec![],
            Some(file) => {
                let symbol_range = file
                    .symbols
                    .get(symbol_index)
                    .map(|symbol| symbol.line_range);
                file.references
                    .iter()
                    .filter(|reference| {
                        if reference.kind != ReferenceKind::Call {
                            return false;
                        }

                        if let Some((start_line, end_line)) = symbol_range {
                            reference.line_range.0 >= start_line
                                && reference.line_range.1 <= end_line
                        } else {
                            reference.enclosing_symbol_index == Some(symbol_index as u32)
                        }
                    })
                    .collect()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::{LanguageId, ReferenceKind, ReferenceRecord, SymbolKind, SymbolRecord};
    use crate::live_index::store::{
        CircuitBreakerState, IndexState, IndexedFile, LiveIndex, ParseStatus,
    };
    use crate::watcher::{WatcherInfo, WatcherState};
    use std::collections::HashMap;
    use std::time::{Duration, Instant, SystemTime};

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

    fn make_indexed_file(
        path: &str,
        symbols: Vec<SymbolRecord>,
        status: ParseStatus,
    ) -> IndexedFile {
        IndexedFile {
            relative_path: path.to_string(),
            language: LanguageId::Rust,
            content: b"fn test() {}".to_vec(),
            symbols,
            parse_status: status,
            byte_len: 12,
            content_hash: "abc".to_string(),
            references: vec![],
            alias_map: std::collections::HashMap::new(),
        }
    }

    fn make_index(files: Vec<(&str, IndexedFile)>, tripped: bool) -> LiveIndex {
        let cb = CircuitBreakerState::new(0.20);
        if tripped {
            // Force-trip by recording enough failures
            for i in 0..10 {
                cb.record_success();
                if i < 7 {
                    cb.record_success();
                }
            }
            for i in 0..5 {
                cb.record_failure(&format!("f{i}.rs"), "error");
            }
            cb.should_abort();
        }

        let files_map: std::collections::HashMap<String, IndexedFile> =
            files.into_iter().map(|(p, f)| (p.to_string(), f)).collect();
        let trigram_index = crate::live_index::trigram::TrigramIndex::build_from_files(&files_map);
        let mut index = LiveIndex {
            files: files_map,
            loaded_at: Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: Duration::from_millis(50),
            cb_state: cb,
            is_empty: false,
            reverse_index: std::collections::HashMap::new(),
            trigram_index,
        };
        // Rebuild the reverse index so xref query tests work.
        index.rebuild_reverse_index();
        index
    }

    // --- xref test helpers ---

    fn make_ref(
        name: &str,
        qualified_name: Option<&str>,
        kind: ReferenceKind,
        enclosing: Option<u32>,
        byte_start: u32,
    ) -> ReferenceRecord {
        ReferenceRecord {
            name: name.to_string(),
            qualified_name: qualified_name.map(|s| s.to_string()),
            kind,
            byte_range: (byte_start, byte_start + 10),
            line_range: (byte_start / 100, byte_start / 100),
            enclosing_symbol_index: enclosing,
        }
    }

    fn make_file_with_refs(
        path: &str,
        refs: Vec<ReferenceRecord>,
        alias_map: HashMap<String, String>,
    ) -> IndexedFile {
        IndexedFile {
            relative_path: path.to_string(),
            language: LanguageId::Rust,
            content: b"fn test() {}".to_vec(),
            symbols: vec![],
            parse_status: ParseStatus::Parsed,
            byte_len: 12,
            content_hash: "abc".to_string(),
            references: refs,
            alias_map,
        }
    }

    fn make_file_with_refs_and_content(
        path: &str,
        language: LanguageId,
        content: &str,
        refs: Vec<ReferenceRecord>,
        symbols: Vec<SymbolRecord>,
    ) -> IndexedFile {
        IndexedFile {
            relative_path: path.to_string(),
            language,
            content: content.as_bytes().to_vec(),
            symbols,
            parse_status: ParseStatus::Parsed,
            byte_len: content.len() as u64,
            content_hash: "abc".to_string(),
            references: refs,
            alias_map: HashMap::new(),
        }
    }

    #[test]
    fn test_get_file_returns_some_for_existing() {
        let f = make_indexed_file(
            "src/main.rs",
            vec![make_symbol("main")],
            ParseStatus::Parsed,
        );
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
        let f1 = make_indexed_file(
            "a.rs",
            vec![make_symbol("x"), make_symbol("y")],
            ParseStatus::Parsed,
        );
        let f2 = make_indexed_file("b.rs", vec![make_symbol("z")], ParseStatus::Parsed);
        let index = make_index(vec![("a.rs", f1), ("b.rs", f2)], false);
        assert_eq!(index.symbol_count(), 3);
    }

    #[test]
    fn test_health_stats_correct_breakdown() {
        let f1 = make_indexed_file("a.rs", vec![make_symbol("x")], ParseStatus::Parsed);
        let f2 = make_indexed_file(
            "b.rs",
            vec![make_symbol("y")],
            ParseStatus::PartialParse {
                warning: "syntax err".to_string(),
            },
        );
        let f3 = make_indexed_file(
            "c.rs",
            vec![],
            ParseStatus::Failed {
                error: "failed".to_string(),
            },
        );
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
        for _ in 0..7 {
            cb.record_success();
        }
        for i in 0..3 {
            cb.record_failure(&format!("f{i}.rs"), "err");
        }
        cb.should_abort(); // This will trip it

        let index = LiveIndex {
            files: HashMap::new(),
            loaded_at: Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: Duration::from_millis(10),
            cb_state: cb,
            is_empty: false,
            reverse_index: std::collections::HashMap::new(),
            trigram_index: crate::live_index::trigram::TrigramIndex::new(),
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
        for _ in 0..7 {
            cb.record_success();
        }
        for i in 0..3 {
            cb.record_failure(&format!("f{i}.rs"), "err");
        }
        cb.should_abort();

        let index = LiveIndex {
            files: HashMap::new(),
            loaded_at: Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: Duration::from_millis(10),
            cb_state: cb,
            is_empty: false,
            reverse_index: std::collections::HashMap::new(),
            trigram_index: crate::live_index::trigram::TrigramIndex::new(),
        };

        match index.index_state() {
            IndexState::CircuitBreakerTripped { summary } => {
                assert!(!summary.is_empty(), "summary should not be empty");
            }
            other => panic!("expected CircuitBreakerTripped, got {:?}", other),
        }
    }

    // --- Extended HealthStats with watcher fields ---

    #[test]
    fn test_health_stats_default_watcher_fields() {
        let index = make_index(vec![], false);
        let stats = index.health_stats();
        assert_eq!(
            stats.watcher_state,
            WatcherState::Off,
            "default watcher state should be Off"
        );
        assert_eq!(
            stats.events_processed, 0,
            "default events_processed should be 0"
        );
        assert!(
            stats.last_event_at.is_none(),
            "default last_event_at should be None"
        );
        assert_eq!(
            stats.debounce_window_ms, 200,
            "default debounce_window_ms should be 200"
        );
    }

    #[test]
    fn test_health_stats_with_watcher_active() {
        let index = make_index(vec![], false);
        let now = SystemTime::now();
        let watcher = WatcherInfo {
            state: WatcherState::Active,
            events_processed: 42,
            last_event_at: Some(now),
            debounce_window_ms: 500,
        };
        let stats = index.health_stats_with_watcher(&watcher);
        assert_eq!(stats.watcher_state, WatcherState::Active);
        assert_eq!(stats.events_processed, 42);
        assert_eq!(stats.last_event_at, Some(now));
        assert_eq!(stats.debounce_window_ms, 500);
    }

    // -----------------------------------------------------------------------
    // Cross-reference query tests (Task 1, Plan 04-02)
    // -----------------------------------------------------------------------

    // --- find_references_for_name: basic ---

    #[test]
    fn test_find_references_for_name_returns_all_matching() {
        // "foo" referenced in two files — both should be returned.
        let refs_a = vec![make_ref("foo", None, ReferenceKind::Call, None, 0)];
        let refs_b = vec![make_ref("foo", None, ReferenceKind::Call, None, 0)];
        let f_a = make_file_with_refs("a.rs", refs_a, HashMap::new());
        let f_b = make_file_with_refs("b.rs", refs_b, HashMap::new());
        let index = make_index(vec![("a.rs", f_a), ("b.rs", f_b)], false);

        let results = index.find_references_for_name("foo", None, false);
        assert_eq!(results.len(), 2, "both files should match");
    }

    #[test]
    fn test_find_references_for_name_kind_filter_call_only() {
        // Two references to "foo" in same file: one Call, one Import. Kind filter returns only Call.
        let refs = vec![
            make_ref("foo", None, ReferenceKind::Call, None, 0),
            make_ref("foo", None, ReferenceKind::Import, None, 100),
        ];
        let f = make_file_with_refs("a.rs", refs, HashMap::new());
        let index = make_index(vec![("a.rs", f)], false);

        let results = index.find_references_for_name("foo", Some(ReferenceKind::Call), false);
        assert_eq!(results.len(), 1, "only Call reference should be returned");
        assert_eq!(results[0].1.kind, ReferenceKind::Call);
    }

    #[test]
    fn test_find_references_for_name_kind_filter_excludes_import() {
        let refs = vec![make_ref("foo", None, ReferenceKind::Import, None, 0)];
        let f = make_file_with_refs("a.rs", refs, HashMap::new());
        let index = make_index(vec![("a.rs", f)], false);

        let results = index.find_references_for_name("foo", Some(ReferenceKind::Call), false);
        assert!(
            results.is_empty(),
            "Import reference should be excluded when filtering for Call"
        );
    }

    // --- Built-in filter (XREF-04 / XREF-06) ---

    #[test]
    fn test_find_references_builtin_string_filtered() {
        // "string" is a JS/TS built-in — should be filtered.
        let refs = vec![make_ref("string", None, ReferenceKind::TypeUsage, None, 0)];
        let f = make_file_with_refs("a.ts", refs, HashMap::new());
        let index = make_index(vec![("a.ts", f)], false);

        let results = index.find_references_for_name("string", None, false);
        assert!(
            results.is_empty(),
            "built-in 'string' should be filtered by default"
        );
    }

    #[test]
    fn test_find_references_builtin_i32_filtered() {
        let refs = vec![make_ref("i32", None, ReferenceKind::TypeUsage, None, 0)];
        let f = make_file_with_refs("a.rs", refs, HashMap::new());
        let index = make_index(vec![("a.rs", f)], false);

        let results = index.find_references_for_name("i32", None, false);
        assert!(results.is_empty(), "Rust built-in 'i32' should be filtered");
    }

    #[test]
    fn test_find_references_mystruct_not_filtered() {
        let refs = vec![make_ref(
            "MyStruct",
            None,
            ReferenceKind::TypeUsage,
            None,
            0,
        )];
        let f = make_file_with_refs("a.rs", refs, HashMap::new());
        let index = make_index(vec![("a.rs", f)], false);

        let results = index.find_references_for_name("MyStruct", None, false);
        assert_eq!(
            results.len(),
            1,
            "user-defined type 'MyStruct' should NOT be filtered"
        );
    }

    #[test]
    fn test_find_references_builtin_include_filtered_bypasses() {
        // include_filtered=true should return even built-in matches.
        let refs = vec![make_ref("i32", None, ReferenceKind::TypeUsage, None, 0)];
        let f = make_file_with_refs("a.rs", refs, HashMap::new());
        let index = make_index(vec![("a.rs", f)], false);

        let results = index.find_references_for_name("i32", None, true);
        assert_eq!(
            results.len(),
            1,
            "include_filtered=true should bypass the filter"
        );
    }

    // --- Generic filter ---

    #[test]
    fn test_find_references_single_letter_t_filtered() {
        let refs = vec![make_ref("T", None, ReferenceKind::TypeUsage, None, 0)];
        let f = make_file_with_refs("a.rs", refs, HashMap::new());
        let index = make_index(vec![("a.rs", f)], false);

        let results = index.find_references_for_name("T", None, false);
        assert!(
            results.is_empty(),
            "single-letter generic 'T' should be filtered"
        );
    }

    #[test]
    fn test_find_references_single_letter_k_filtered() {
        let refs = vec![make_ref("K", None, ReferenceKind::TypeUsage, None, 0)];
        let f = make_file_with_refs("a.rs", refs, HashMap::new());
        let index = make_index(vec![("a.rs", f)], false);

        let results = index.find_references_for_name("K", None, false);
        assert!(
            results.is_empty(),
            "single-letter generic 'K' should be filtered"
        );
    }

    #[test]
    fn test_find_references_multi_letter_key_not_filtered() {
        let refs = vec![make_ref("Key", None, ReferenceKind::TypeUsage, None, 0)];
        let f = make_file_with_refs("a.rs", refs, HashMap::new());
        let index = make_index(vec![("a.rs", f)], false);

        let results = index.find_references_for_name("Key", None, false);
        assert_eq!(
            results.len(),
            1,
            "multi-letter name 'Key' should NOT be filtered"
        );
    }

    // --- Alias resolution (XREF-05) ---

    #[test]
    fn test_find_references_alias_resolution_hashmap_via_map() {
        // File b.rs has a reference to "Map" with alias_map["Map"] = "HashMap".
        // Searching for "HashMap" should also return the "Map" reference.
        let mut alias_map = HashMap::new();
        alias_map.insert("Map".to_string(), "HashMap".to_string());

        let refs_b = vec![make_ref("Map", None, ReferenceKind::Call, None, 0)];
        let f_a = make_file_with_refs("a.rs", vec![], HashMap::new()); // no refs
        let f_b = make_file_with_refs("b.rs", refs_b, alias_map);
        let index = make_index(vec![("a.rs", f_a), ("b.rs", f_b)], false);

        let results = index.find_references_for_name("HashMap", None, false);
        // Should find the "Map" reference from b.rs via alias resolution
        assert!(
            !results.is_empty(),
            "alias resolution should find 'Map' when searching 'HashMap'"
        );
        assert_eq!(results[0].1.name, "Map");
    }

    // --- Qualified name matching ---

    #[test]
    fn test_find_references_qualified_name_vec_new() {
        let refs = vec![make_ref(
            "new",
            Some("Vec::new"),
            ReferenceKind::Call,
            None,
            0,
        )];
        let f = make_file_with_refs("a.rs", refs, HashMap::new());
        let index = make_index(vec![("a.rs", f)], false);

        // Qualified search: "Vec::new" matches against qualified_name field.
        let results = index.find_references_for_name("Vec::new", None, false);
        assert_eq!(
            results.len(),
            1,
            "qualified 'Vec::new' should match via qualified_name field"
        );
    }

    #[test]
    fn test_find_references_qualified_does_not_match_unqualified() {
        // "new" (simple) should not match when searching for qualified "Vec::new".
        let refs = vec![make_ref("new", None, ReferenceKind::Call, None, 0)]; // no qualified_name
        let f = make_file_with_refs("a.rs", refs, HashMap::new());
        let index = make_index(vec![("a.rs", f)], false);

        let results = index.find_references_for_name("Vec::new", None, false);
        assert!(
            results.is_empty(),
            "qualified search should not match reference without qualified_name"
        );
    }

    // --- Result fields ---

    #[test]
    fn test_find_references_result_includes_correct_file_path_and_record() {
        let refs = vec![make_ref("load", None, ReferenceKind::Call, None, 0)];
        let f = make_file_with_refs("src/loader.rs", refs, HashMap::new());
        let index = make_index(vec![("src/loader.rs", f)], false);

        let results = index.find_references_for_name("load", None, false);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "src/loader.rs", "file_path should match");
        assert_eq!(results[0].1.name, "load");
    }

    // --- find_dependents_for_file ---

    #[test]
    fn test_find_dependents_for_file_returns_importer() {
        // b.rs imports "db" — should be a dependent of src/db.rs.
        let import_ref = make_ref("db", None, ReferenceKind::Import, None, 0);
        let f_b = make_file_with_refs("src/b.rs", vec![import_ref], HashMap::new());
        let f_db = make_file_with_refs("src/db.rs", vec![], HashMap::new());
        let index = make_index(vec![("src/b.rs", f_b), ("src/db.rs", f_db)], false);

        let deps = index.find_dependents_for_file("src/db.rs");
        assert_eq!(
            deps.len(),
            1,
            "b.rs imports 'db' so it is a dependent of db.rs"
        );
        assert_eq!(deps[0].0, "src/b.rs");
    }

    #[test]
    fn test_find_dependents_no_importers_returns_empty() {
        let f = make_file_with_refs("src/db.rs", vec![], HashMap::new());
        let index = make_index(vec![("src/db.rs", f)], false);

        let deps = index.find_dependents_for_file("src/db.rs");
        assert!(deps.is_empty(), "no importers means empty dependents list");
    }

    #[test]
    fn test_find_dependents_excludes_self() {
        // A file that imports its own stem should not appear as its own dependent.
        let self_import = make_ref("db", None, ReferenceKind::Import, None, 0);
        let f_db = make_file_with_refs("src/db.rs", vec![self_import], HashMap::new());
        let index = make_index(vec![("src/db.rs", f_db)], false);

        let deps = index.find_dependents_for_file("src/db.rs");
        assert!(deps.is_empty(), "a file should not be its own dependent");
    }

    #[test]
    fn test_find_dependents_qualified_import_crate_db() {
        // b.rs has import "crate::db" — should match src/db.rs.
        let import_ref = make_ref("crate::db", None, ReferenceKind::Import, None, 0);
        let f_b = make_file_with_refs("src/b.rs", vec![import_ref], HashMap::new());
        let f_db = make_file_with_refs("src/db.rs", vec![], HashMap::new());
        let index = make_index(vec![("src/b.rs", f_b), ("src/db.rs", f_db)], false);

        let deps = index.find_dependents_for_file("src/db.rs");
        assert_eq!(
            deps.len(),
            1,
            "qualified 'crate::db' should match src/db.rs"
        );
    }

    // --- find_dependents: module path resolution ---

    #[test]
    fn test_find_dependents_lib_rs_via_module_path() {
        // main.rs imports "crate::error" — lib.rs is the crate root, so it's a dependent.
        let import_ref = make_ref("crate::error", None, ReferenceKind::Import, None, 0);
        let f_main = make_file_with_refs("src/main.rs", vec![import_ref], HashMap::new());
        let f_lib = make_file_with_refs("src/lib.rs", vec![], HashMap::new());
        let index = make_index(vec![("src/main.rs", f_main), ("src/lib.rs", f_lib)], false);

        let deps = index.find_dependents_for_file("src/lib.rs");
        assert_eq!(
            deps.len(),
            1,
            "crate::error starts with 'crate' module path of lib.rs"
        );
        assert_eq!(deps[0].0, "src/main.rs");
    }

    #[test]
    fn test_find_dependents_mod_rs_via_module_path() {
        // store.rs imports "crate::live_index" — mod.rs defines that module.
        let import_ref = make_ref(
            "crate::live_index::store",
            None,
            ReferenceKind::Import,
            None,
            0,
        );
        let f_store = make_file_with_refs("src/store.rs", vec![import_ref], HashMap::new());
        let f_mod = make_file_with_refs("src/live_index/mod.rs", vec![], HashMap::new());
        let index = make_index(
            vec![("src/store.rs", f_store), ("src/live_index/mod.rs", f_mod)],
            false,
        );

        let deps = index.find_dependents_for_file("src/live_index/mod.rs");
        assert_eq!(
            deps.len(),
            1,
            "'crate::live_index::store' starts with module path 'crate::live_index'"
        );
        assert_eq!(deps[0].0, "src/store.rs");
    }

    #[test]
    fn test_find_dependents_rust_returns_symbol_usage_when_module_import_matches() {
        let target = make_file_with_refs_and_content(
            "src/daemon.rs",
            LanguageId::Rust,
            "pub fn connect_or_spawn_session() {}",
            vec![],
            vec![make_symbol("connect_or_spawn_session")],
        );
        let dependent = make_file_with_refs_and_content(
            "src/main.rs",
            LanguageId::Rust,
            "use crate::{daemon, other}; fn main() { daemon::connect_or_spawn_session(); }",
            vec![
                make_ref(
                    "daemon",
                    Some("crate::daemon"),
                    ReferenceKind::Import,
                    None,
                    0,
                ),
                make_ref(
                    "connect_or_spawn_session",
                    Some("daemon::connect_or_spawn_session"),
                    ReferenceKind::Call,
                    Some(0),
                    100,
                ),
            ],
            vec![make_symbol("main")],
        );
        let index = make_index(
            vec![("src/daemon.rs", target), ("src/main.rs", dependent)],
            false,
        );

        let deps = index.find_dependents_for_file("src/daemon.rs");
        assert_eq!(
            deps.len(),
            1,
            "matched Rust module imports should surface actual symbol usage, not just import stubs"
        );
        assert_eq!(deps[0].0, "src/main.rs");
        assert_eq!(deps[0].1.kind, ReferenceKind::Call);
        assert_eq!(deps[0].1.name, "connect_or_spawn_session");
    }

    #[test]
    fn test_find_dependents_python_init_via_module_path() {
        // app.py imports "utils.helpers" — __init__.py defines the utils package.
        let import_ref = make_ref("utils.helpers", None, ReferenceKind::Import, None, 0);
        let mut f_app = make_file_with_refs("src/app.py", vec![import_ref], HashMap::new());
        f_app.language = LanguageId::Python;
        let mut f_init = make_file_with_refs("utils/__init__.py", vec![], HashMap::new());
        f_init.language = LanguageId::Python;
        let index = make_index(
            vec![("src/app.py", f_app), ("utils/__init__.py", f_init)],
            false,
        );

        let deps = index.find_dependents_for_file("utils/__init__.py");
        assert_eq!(
            deps.len(),
            1,
            "'utils.helpers' starts with module path 'utils'"
        );
    }

    #[test]
    fn test_find_dependents_js_index_via_module_path() {
        // app.js imports "src/utils" — index.ts defines that directory module.
        let import_ref = make_ref("src/utils/foo", None, ReferenceKind::Import, None, 0);
        let mut f_app = make_file_with_refs("src/app.js", vec![import_ref], HashMap::new());
        f_app.language = LanguageId::JavaScript;
        let mut f_index = make_file_with_refs("src/utils/index.js", vec![], HashMap::new());
        f_index.language = LanguageId::JavaScript;
        let index = make_index(
            vec![("src/app.js", f_app), ("src/utils/index.js", f_index)],
            false,
        );

        let deps = index.find_dependents_for_file("src/utils/index.js");
        assert_eq!(
            deps.len(),
            1,
            "'src/utils/foo' starts with module path 'src/utils'"
        );
    }

    #[test]
    fn test_find_dependents_csharp_type_usage_with_imported_namespace() {
        let target = make_file_with_refs_and_content(
            "Core/Services/IMinioService.cs",
            LanguageId::CSharp,
            "namespace CeRegistry.Core.Services { public interface IMinioService {} }",
            vec![],
            vec![SymbolRecord {
                name: "IMinioService".to_string(),
                kind: SymbolKind::Interface,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 10),
                line_range: (0, 0),
            }],
        );
        let dependent = make_file_with_refs_and_content(
            "Api/Controllers/PacketsController.cs",
            LanguageId::CSharp,
            r#"using CeRegistry.Core.Services;
namespace CeRegistry.Api.Controllers {
    public class PacketsController {
        private readonly IMinioService _minio;
        public PacketsController(IMinioService minioService) {}
    }
}"#,
            vec![
                make_ref(
                    "Services",
                    Some("CeRegistry.Core.Services"),
                    ReferenceKind::Import,
                    None,
                    0,
                ),
                make_ref(
                    "IMinioService",
                    None,
                    ReferenceKind::TypeUsage,
                    Some(0),
                    100,
                ),
                make_ref(
                    "IMinioService",
                    None,
                    ReferenceKind::TypeUsage,
                    Some(0),
                    200,
                ),
            ],
            vec![make_symbol("PacketsController")],
        );
        let index = make_index(
            vec![
                ("Core/Services/IMinioService.cs", target),
                ("Api/Controllers/PacketsController.cs", dependent),
            ],
            false,
        );

        let deps = index.find_dependents_for_file("Core/Services/IMinioService.cs");
        assert_eq!(
            deps.len(),
            2,
            "constructor and field type usage should both be treated as dependencies"
        );
        assert!(
            deps.iter()
                .all(|(path, _)| *path == "Api/Controllers/PacketsController.cs")
        );
        assert!(deps.iter().all(|(_, r)| r.kind == ReferenceKind::TypeUsage));
    }

    #[test]
    fn test_find_dependents_csharp_type_usage_in_same_namespace_without_import() {
        let target = make_file_with_refs_and_content(
            "Core/Services/IMinioService.cs",
            LanguageId::CSharp,
            "namespace CeRegistry.Core.Services { public interface IMinioService {} }",
            vec![],
            vec![SymbolRecord {
                name: "IMinioService".to_string(),
                kind: SymbolKind::Interface,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 10),
                line_range: (0, 0),
            }],
        );
        let dependent = make_file_with_refs_and_content(
            "Core/Services/MinioServiceConsumer.cs",
            LanguageId::CSharp,
            r#"namespace CeRegistry.Core.Services {
    public class MinioServiceConsumer {
        private readonly IMinioService _minio;
    }
}"#,
            vec![make_ref(
                "IMinioService",
                None,
                ReferenceKind::TypeUsage,
                Some(0),
                100,
            )],
            vec![make_symbol("MinioServiceConsumer")],
        );
        let index = make_index(
            vec![
                ("Core/Services/IMinioService.cs", target),
                ("Core/Services/MinioServiceConsumer.cs", dependent),
            ],
            false,
        );

        let deps = index.find_dependents_for_file("Core/Services/IMinioService.cs");
        assert_eq!(
            deps.len(),
            1,
            "same-namespace C# type usage should count even without a using directive"
        );
        assert_eq!(deps[0].0, "Core/Services/MinioServiceConsumer.cs");
        assert_eq!(deps[0].1.kind, ReferenceKind::TypeUsage);
    }

    #[test]
    fn test_find_dependents_java_type_usage_with_imported_package() {
        let target = make_file_with_refs_and_content(
            "src/main/java/com/acme/storage/MinioService.java",
            LanguageId::Java,
            "package com.acme.storage; public class MinioService {}",
            vec![],
            vec![SymbolRecord {
                name: "MinioService".to_string(),
                kind: SymbolKind::Class,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 10),
                line_range: (0, 0),
            }],
        );
        let dependent = make_file_with_refs_and_content(
            "src/main/java/com/acme/api/PacketsController.java",
            LanguageId::Java,
            r#"package com.acme.api;
import com.acme.storage.MinioService;
public class PacketsController {
    private final MinioService minioService;
}"#,
            vec![
                make_ref(
                    "MinioService",
                    Some("com.acme.storage.MinioService"),
                    ReferenceKind::Import,
                    None,
                    0,
                ),
                make_ref("MinioService", None, ReferenceKind::TypeUsage, Some(0), 100),
            ],
            vec![make_symbol("PacketsController")],
        );
        let index = make_index(
            vec![
                ("src/main/java/com/acme/storage/MinioService.java", target),
                (
                    "src/main/java/com/acme/api/PacketsController.java",
                    dependent,
                ),
            ],
            false,
        );

        let deps =
            index.find_dependents_for_file("src/main/java/com/acme/storage/MinioService.java");
        assert_eq!(
            deps.len(),
            1,
            "Java field type usage should resolve through the imported package"
        );
        assert_eq!(
            deps[0].0,
            "src/main/java/com/acme/api/PacketsController.java"
        );
        assert_eq!(deps[0].1.kind, ReferenceKind::TypeUsage);
    }

    #[test]
    fn test_find_dependents_typescript_prefers_type_usage_when_module_import_matches() {
        let target = make_file_with_refs_and_content(
            "src/service.ts",
            LanguageId::TypeScript,
            "export class Service {}",
            vec![],
            vec![SymbolRecord {
                name: "Service".to_string(),
                kind: SymbolKind::Class,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 10),
                line_range: (0, 0),
            }],
        );
        let dependent = make_file_with_refs_and_content(
            "src/app.ts",
            LanguageId::TypeScript,
            "import { Service } from \"./service\";\nexport class App { constructor(private service: Service) {} }",
            vec![
                make_ref("service", Some("./service"), ReferenceKind::Import, None, 0),
                make_ref("Service", None, ReferenceKind::TypeUsage, Some(0), 100),
            ],
            vec![SymbolRecord {
                name: "App".to_string(),
                kind: SymbolKind::Class,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 10),
                line_range: (0, 0),
            }],
        );
        let index = make_index(
            vec![("src/service.ts", target), ("src/app.ts", dependent)],
            false,
        );

        let deps = index.find_dependents_for_file("src/service.ts");
        assert_eq!(
            deps.len(),
            1,
            "module-backed TypeScript dependents should report type usage, not just the import"
        );
        assert_eq!(deps[0].0, "src/app.ts");
        assert_eq!(deps[0].1.kind, ReferenceKind::TypeUsage);
        assert_eq!(deps[0].1.name, "Service");
    }

    #[test]
    fn test_resolve_module_path_rust_cases() {
        use super::resolve_module_path;
        assert_eq!(
            resolve_module_path("src/lib.rs", &LanguageId::Rust),
            Some("crate".to_string())
        );
        assert_eq!(
            resolve_module_path("src/main.rs", &LanguageId::Rust),
            Some("crate".to_string())
        );
        assert_eq!(
            resolve_module_path("src/error.rs", &LanguageId::Rust),
            Some("crate::error".to_string())
        );
        assert_eq!(
            resolve_module_path("src/live_index/mod.rs", &LanguageId::Rust),
            Some("crate::live_index".to_string())
        );
        assert_eq!(
            resolve_module_path("src/live_index/store.rs", &LanguageId::Rust),
            Some("crate::live_index::store".to_string())
        );
        // Files outside src/ have no module path
        assert_eq!(resolve_module_path("tests/foo.rs", &LanguageId::Rust), None);
    }

    #[test]
    fn test_resolve_module_path_python_cases() {
        use super::resolve_module_path;
        assert_eq!(
            resolve_module_path("utils/__init__.py", &LanguageId::Python),
            Some("utils".to_string())
        );
        assert_eq!(
            resolve_module_path("utils/helpers.py", &LanguageId::Python),
            Some("utils.helpers".to_string())
        );
    }

    #[test]
    fn test_resolve_module_path_js_cases() {
        use super::resolve_module_path;
        assert_eq!(
            resolve_module_path("src/utils/index.js", &LanguageId::JavaScript),
            Some("src/utils".to_string())
        );
        assert_eq!(
            resolve_module_path("src/utils/index.ts", &LanguageId::TypeScript),
            Some("src/utils".to_string())
        );
        assert_eq!(
            resolve_module_path("src/foo.js", &LanguageId::JavaScript),
            Some("src/foo".to_string())
        );
    }

    // --- callees_for_symbol ---

    #[test]
    fn test_callees_for_symbol_returns_enclosed_calls() {
        let refs = vec![
            make_ref("helper", None, ReferenceKind::Call, Some(0), 0),
            make_ref("other", None, ReferenceKind::Call, Some(1), 100), // different enclosing
            make_ref("imported", None, ReferenceKind::Import, Some(0), 200), // not a Call
        ];
        let f = make_file_with_refs("src/main.rs", refs, HashMap::new());
        let index = make_index(vec![("src/main.rs", f)], false);

        let callees = index.callees_for_symbol("src/main.rs", 0);
        assert_eq!(
            callees.len(),
            1,
            "only the Call reference with enclosing=0 should be returned"
        );
        assert_eq!(callees[0].name, "helper");
    }

    #[test]
    fn test_callees_for_symbol_includes_calls_inside_nested_methods() {
        let file = make_file_with_refs_and_content(
            "src/service.rs",
            LanguageId::CSharp,
            r#"public class MinioService {
    public async Task UploadAsync() {
        _minioClient.BucketExistsAsync();
        _logger.LogInformation(""upload"");
    }
}"#,
            vec![
                make_ref("BucketExistsAsync", None, ReferenceKind::Call, Some(1), 100),
                make_ref("LogInformation", None, ReferenceKind::Call, Some(1), 200),
            ],
            vec![
                SymbolRecord {
                    name: "MinioService".to_string(),
                    kind: SymbolKind::Class,
                    depth: 0,
                    sort_order: 0,
                    byte_range: (0, 500),
                    line_range: (0, 4),
                },
                SymbolRecord {
                    name: "UploadAsync".to_string(),
                    kind: SymbolKind::Method,
                    depth: 1,
                    sort_order: 1,
                    byte_range: (50, 400),
                    line_range: (1, 3),
                },
            ],
        );
        let index = make_index(vec![("src/service.rs", file)], false);

        let callees = index.callees_for_symbol("src/service.rs", 0);
        assert_eq!(
            callees.len(),
            2,
            "class bundles should surface calls made inside enclosed methods"
        );
        assert_eq!(callees[0].name, "BucketExistsAsync");
        assert_eq!(callees[1].name, "LogInformation");
    }

    #[test]
    fn test_callees_for_symbol_empty_for_nonexistent_file() {
        let index = make_index(vec![], false);
        let callees = index.callees_for_symbol("nonexistent.rs", 0);
        assert!(callees.is_empty(), "nonexistent file returns empty callees");
    }

    #[test]
    fn test_callees_for_symbol_excludes_non_call_kinds() {
        let refs = vec![
            make_ref("T", None, ReferenceKind::TypeUsage, Some(0), 0),
            make_ref("my_macro", None, ReferenceKind::MacroUse, Some(0), 50),
        ];
        let f = make_file_with_refs("src/lib.rs", refs, HashMap::new());
        let index = make_index(vec![("src/lib.rs", f)], false);

        let callees = index.callees_for_symbol("src/lib.rs", 0);
        assert!(
            callees.is_empty(),
            "TypeUsage and MacroUse should not appear in callees"
        );
    }

    // --- is_filtered_name (unit coverage) ---

    #[test]
    fn test_is_filtered_name_rust_builtins() {
        use super::is_filtered_name;
        assert!(is_filtered_name("i32"), "i32 is a Rust built-in");
        assert!(is_filtered_name("bool"), "bool is a Rust built-in");
        assert!(is_filtered_name("String"), "String is a Rust built-in");
        assert!(!is_filtered_name("MyString"), "MyString is not a built-in");
    }

    #[test]
    fn test_is_filtered_name_single_letter_generics() {
        use super::is_filtered_name;
        assert!(is_filtered_name("T"), "T is a single-letter generic");
        assert!(is_filtered_name("K"), "K is a single-letter generic");
        assert!(is_filtered_name("V"), "V is a single-letter generic");
        assert!(
            !is_filtered_name("Key"),
            "Key is not a single-letter generic"
        );
    }
}
