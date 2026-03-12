use std::collections::HashSet;

use crate::domain::{FileClass, FileClassification};
use crate::live_index::LiveIndex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SymbolMatchTier {
    Exact = 0,
    Prefix = 1,
    Substring = 2,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolSearchHit {
    pub tier: SymbolMatchTier,
    pub name: String,
    pub path: String,
    pub kind: String,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolSearchResult {
    pub file_count: usize,
    pub hits: Vec<SymbolSearchHit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathScope {
    Any,
    Exact(String),
    Prefix(String),
}

impl PathScope {
    pub const fn any() -> Self {
        Self::Any
    }

    pub fn exact(path: impl Into<String>) -> Self {
        Self::Exact(path.into())
    }

    pub fn prefix(path_prefix: impl Into<String>) -> Self {
        Self::Prefix(path_prefix.into())
    }

    pub fn matches(&self, path: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Exact(exact_path) => path == exact_path,
            Self::Prefix(path_prefix) => path.starts_with(path_prefix),
        }
    }
}

impl Default for PathScope {
    fn default() -> Self {
        Self::Any
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchScope {
    All,
    Code,
    Text,
    Binary,
}

impl SearchScope {
    pub const fn allows(self, classification: &FileClassification) -> bool {
        match self {
            Self::All => true,
            Self::Code => matches!(classification.class, FileClass::Code),
            Self::Text => matches!(classification.class, FileClass::Text),
            Self::Binary => matches!(classification.class, FileClass::Binary),
        }
    }
}

impl Default for SearchScope {
    fn default() -> Self {
        Self::Code
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResultLimit(usize);

impl ResultLimit {
    pub const fn new(limit: usize) -> Self {
        Self(limit)
    }

    pub const fn symbol_search_default() -> Self {
        Self(50)
    }

    pub const fn get(self) -> usize {
        self.0
    }
}

impl Default for ResultLimit {
    fn default() -> Self {
        Self::symbol_search_default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentContext {
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
}

impl ContentContext {
    pub const fn line_range(start_line: Option<u32>, end_line: Option<u32>) -> Self {
        Self {
            start_line,
            end_line,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoisePolicy {
    pub include_generated: bool,
    pub include_tests: bool,
    pub include_vendor: bool,
}

impl NoisePolicy {
    pub const fn permissive() -> Self {
        Self {
            include_generated: true,
            include_tests: true,
            include_vendor: true,
        }
    }

    pub const fn hide_classified_noise() -> Self {
        Self {
            include_generated: false,
            include_tests: false,
            include_vendor: false,
        }
    }

    pub const fn allows(self, classification: &FileClassification) -> bool {
        (self.include_generated || !classification.is_generated)
            && (self.include_tests || !classification.is_test)
            && (self.include_vendor || !classification.is_vendor)
    }
}

impl Default for NoisePolicy {
    fn default() -> Self {
        Self::permissive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolSearchOptions {
    pub path_scope: PathScope,
    pub search_scope: SearchScope,
    pub result_limit: ResultLimit,
    pub noise_policy: NoisePolicy,
}

impl Default for SymbolSearchOptions {
    fn default() -> Self {
        Self {
            path_scope: PathScope::default(),
            search_scope: SearchScope::default(),
            result_limit: ResultLimit::default(),
            noise_policy: NoisePolicy::default(),
        }
    }
}

impl SymbolSearchOptions {
    pub fn for_current_code_search(result_limit: usize) -> Self {
        Self {
            path_scope: PathScope::any(),
            search_scope: SearchScope::Code,
            result_limit: ResultLimit::new(result_limit),
            noise_policy: NoisePolicy::permissive(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSearchOptions {
    pub path_scope: PathScope,
    pub search_scope: SearchScope,
    pub noise_policy: NoisePolicy,
}

impl Default for TextSearchOptions {
    fn default() -> Self {
        Self {
            path_scope: PathScope::default(),
            search_scope: SearchScope::default(),
            noise_policy: NoisePolicy::default(),
        }
    }
}

impl TextSearchOptions {
    pub fn for_current_code_search() -> Self {
        Self {
            path_scope: PathScope::any(),
            search_scope: SearchScope::Code,
            noise_policy: NoisePolicy::permissive(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileContentOptions {
    pub path_scope: PathScope,
    pub content_context: ContentContext,
}

impl FileContentOptions {
    pub fn for_explicit_path_read(
        path: impl Into<String>,
        start_line: Option<u32>,
        end_line: Option<u32>,
    ) -> Self {
        Self {
            path_scope: PathScope::exact(path),
            content_context: ContentContext::line_range(start_line, end_line),
        }
    }

    pub fn exact_lines(
        path: impl Into<String>,
        start_line: Option<u32>,
        end_line: Option<u32>,
    ) -> Self {
        Self::for_explicit_path_read(path, start_line, end_line)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextLineMatch {
    pub line_number: usize,
    pub line: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextFileMatches {
    pub path: String,
    pub matches: Vec<TextLineMatch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSearchResult {
    pub label: String,
    pub total_matches: usize,
    pub files: Vec<TextFileMatches>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextSearchError {
    EmptyRegexQuery,
    EmptyQueryOrTerms,
    InvalidRegex { pattern: String, error: String },
}

struct ScoredSymbolMatch {
    tier: SymbolMatchTier,
    tiebreak: u32,
    name: String,
    path: String,
    kind: String,
    line: u32,
}

pub fn search_symbols(
    index: &LiveIndex,
    query: &str,
    kind_filter: Option<&str>,
    result_limit: usize,
) -> SymbolSearchResult {
    let options = SymbolSearchOptions::for_current_code_search(result_limit);
    search_symbols_with_options(index, query, kind_filter, &options)
}

pub fn search_symbols_with_options(
    index: &LiveIndex,
    query: &str,
    kind_filter: Option<&str>,
    options: &SymbolSearchOptions,
) -> SymbolSearchResult {
    let query_lower = query.to_lowercase();
    let mut matches: Vec<ScoredSymbolMatch> = Vec::new();
    let mut files_with_hits: HashSet<String> = HashSet::new();

    let mut paths: Vec<&String> = index.all_files().map(|(path, _)| path).collect();
    paths.sort();

    for path in paths {
        let file = index
            .get_file(path)
            .expect("path from all_files must exist");
        if !options.path_scope.matches(path)
            || !options.search_scope.allows(&file.classification)
            || !options.noise_policy.allows(&file.classification)
        {
            continue;
        }
        for sym in &file.symbols {
            if let Some(filter) = kind_filter
                && !filter.eq_ignore_ascii_case("all")
                && !sym.kind.to_string().eq_ignore_ascii_case(filter)
            {
                continue;
            }

            let name_lower = sym.name.to_lowercase();
            if !name_lower.contains(&query_lower) {
                continue;
            }

            let (tier, tiebreak) = if name_lower == query_lower {
                (SymbolMatchTier::Exact, 0u32)
            } else if name_lower.starts_with(&query_lower) {
                (SymbolMatchTier::Prefix, sym.name.len() as u32)
            } else {
                let pos = name_lower.find(&query_lower).unwrap_or(0) as u32;
                (SymbolMatchTier::Substring, pos)
            };

            files_with_hits.insert(path.clone());
            matches.push(ScoredSymbolMatch {
                tier,
                tiebreak,
                name: sym.name.clone(),
                path: path.clone(),
                kind: sym.kind.to_string(),
                line: sym.line_range.0,
            });
        }
    }

    matches.sort_by(|a, b| {
        a.tier
            .cmp(&b.tier)
            .then(a.tiebreak.cmp(&b.tiebreak))
            .then(a.name.cmp(&b.name))
    });

    let hits = matches
        .into_iter()
        .take(options.result_limit.get())
        .map(|m| SymbolSearchHit {
            tier: m.tier,
            name: m.name,
            path: m.path,
            kind: m.kind,
            line: m.line,
        })
        .collect();

    SymbolSearchResult {
        file_count: files_with_hits.len(),
        hits,
    }
}

pub fn search_text(
    index: &LiveIndex,
    query: Option<&str>,
    terms: Option<&[String]>,
    regex: bool,
) -> Result<TextSearchResult, TextSearchError> {
    search_text_with_options(index, query, terms, regex, &TextSearchOptions::for_current_code_search())
}

pub fn search_text_with_options(
    index: &LiveIndex,
    query: Option<&str>,
    terms: Option<&[String]>,
    regex: bool,
    options: &TextSearchOptions,
) -> Result<TextSearchResult, TextSearchError> {
    let normalized_terms: Vec<String> = match terms {
        Some(raw_terms) if !raw_terms.is_empty() => raw_terms
            .iter()
            .map(|term| term.trim())
            .filter(|term| !term.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
        _ => query
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(|text| vec![text.to_string()])
            .unwrap_or_default(),
    };

    if regex {
        let pattern = query
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .unwrap_or("");
        if pattern.is_empty() {
            return Err(TextSearchError::EmptyRegexQuery);
        }

        let regex = match regex::Regex::new(pattern) {
            Ok(regex) => regex,
            Err(error) => {
                return Err(TextSearchError::InvalidRegex {
                    pattern: pattern.to_string(),
                    error: error.to_string(),
                });
            }
        };

        let candidate_paths = index
            .all_files()
            .filter(|(path, file)| file_matches_text_options(path, file, options))
            .map(|(path, _)| path.clone())
            .collect();
        return Ok(collect_text_matches(
            index,
            candidate_paths,
            |line| regex.is_match(line),
            format!("regex '{pattern}'"),
        ));
    }

    if normalized_terms.is_empty() {
        return Err(TextSearchError::EmptyQueryOrTerms);
    }

    let mut candidate_paths = HashSet::new();
    for term in &normalized_terms {
        for path in index.trigram_index.search(term.as_bytes(), &index.files) {
            let Some(file) = index.get_file(&path) else {
                continue;
            };
            if file_matches_text_options(&path, file, options) {
                candidate_paths.insert(path);
            }
        }
    }

    let lowered_terms: Vec<String> = normalized_terms
        .iter()
        .map(|term| term.to_lowercase())
        .collect();

    let label = if normalized_terms.len() == 1 {
        format!("'{}'", normalized_terms[0])
    } else {
        format!("terms [{}]", normalized_terms.join(", "))
    };

    Ok(collect_text_matches(
        index,
        candidate_paths.into_iter().collect(),
        |line| {
            let lowered = line.to_lowercase();
            lowered_terms.iter().any(|term| lowered.contains(term))
        },
        label,
    ))
}

fn file_matches_text_options(
    path: &str,
    file: &crate::live_index::IndexedFile,
    options: &TextSearchOptions,
) -> bool {
    options.path_scope.matches(path)
        && options.search_scope.allows(&file.classification)
        && options.noise_policy.allows(&file.classification)
}

fn collect_text_matches<F>(
    index: &LiveIndex,
    mut candidate_paths: Vec<String>,
    mut is_match: F,
    label: String,
) -> TextSearchResult
where
    F: FnMut(&str) -> bool,
{
    candidate_paths.sort();

    let mut files: Vec<TextFileMatches> = Vec::new();
    let mut total_matches = 0usize;

    for path in &candidate_paths {
        let file = match index.get_file(path) {
            Some(file) => file,
            None => continue,
        };
        let content_str = String::from_utf8_lossy(&file.content);

        let matches: Vec<TextLineMatch> = content_str
            .lines()
            .enumerate()
            .filter_map(|(line_idx, line)| {
                let line = line.trim_end_matches('\r');
                if is_match(line) {
                    Some(TextLineMatch {
                        line_number: line_idx + 1,
                        line: line.to_string(),
                    })
                } else {
                    None
                }
            })
            .collect();

        if !matches.is_empty() {
            total_matches += matches.len();
            files.push(TextFileMatches {
                path: path.clone(),
                matches,
            });
        }
    }

    TextSearchResult {
        label,
        total_matches,
        files,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::{Duration, Instant, SystemTime};

    use super::*;
    use crate::domain::{LanguageId, SymbolKind, SymbolRecord};
    use crate::live_index::store::{CircuitBreakerState, IndexedFile, ParseStatus};
    use crate::live_index::trigram::TrigramIndex;

    fn make_symbol(name: &str, kind: SymbolKind, line: u32) -> SymbolRecord {
        SymbolRecord {
            name: name.to_string(),
            kind,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 0),
            line_range: (line, line),
        }
    }

    fn make_file_with_classification(
        path: &str,
        content: &str,
        symbols: Vec<SymbolRecord>,
        classification: crate::domain::FileClassification,
    ) -> (String, IndexedFile) {
        (
            path.to_string(),
            IndexedFile {
                relative_path: path.to_string(),
                language: LanguageId::Rust,
                classification,
                content: content.as_bytes().to_vec(),
                symbols,
                parse_status: ParseStatus::Parsed,
                byte_len: content.len() as u64,
                content_hash: "hash".to_string(),
                references: Vec::new(),
                alias_map: HashMap::new(),
            },
        )
    }

    fn make_file(path: &str, content: &str, symbols: Vec<SymbolRecord>) -> (String, IndexedFile) {
        make_file_with_classification(
            path,
            content,
            symbols,
            crate::domain::FileClassification::for_code_path(path),
        )
    }

    fn make_index(files: Vec<(String, IndexedFile)>) -> LiveIndex {
        let file_map: HashMap<String, std::sync::Arc<IndexedFile>> = files
            .into_iter()
            .map(|(path, file)| (path, std::sync::Arc::new(file)))
            .collect();
        let trigram_index = TrigramIndex::build_from_files(&file_map);
        let mut index = LiveIndex {
            files: file_map,
            loaded_at: Instant::now(),
            loaded_at_system: SystemTime::now(),
            load_duration: Duration::ZERO,
            cb_state: CircuitBreakerState::new(0.20),
            is_empty: false,
            load_source: crate::live_index::store::IndexLoadSource::FreshLoad,
            snapshot_verify_state: crate::live_index::store::SnapshotVerifyState::NotNeeded,
            reverse_index: HashMap::new(),
            files_by_basename: HashMap::new(),
            files_by_dir_component: HashMap::new(),
            trigram_index,
        };
        index.rebuild_path_indices();
        index
    }

    #[test]
    fn test_search_module_symbol_search_respects_tiers_and_limit() {
        let index = make_index(vec![
            make_file(
                "src/a.rs",
                "",
                vec![
                    make_symbol("job", SymbolKind::Function, 1),
                    make_symbol("jobQueue", SymbolKind::Function, 2),
                    make_symbol("enqueueJob", SymbolKind::Function, 3),
                ],
            ),
            make_file(
                "src/b.rs",
                "",
                vec![make_symbol("jobber", SymbolKind::Method, 4)],
            ),
        ]);

        let result = search_symbols(&index, "job", None, 3);

        assert_eq!(result.file_count, 2);
        assert_eq!(result.hits.len(), 3);
        assert_eq!(result.hits[0].tier, SymbolMatchTier::Exact);
        assert_eq!(result.hits[0].name, "job");
        assert_eq!(result.hits[1].tier, SymbolMatchTier::Prefix);
        assert_eq!(result.hits[1].name, "jobber");
        assert_eq!(result.hits[2].tier, SymbolMatchTier::Prefix);
        assert_eq!(result.hits[2].name, "jobQueue");
    }

    #[test]
    fn test_search_module_symbol_search_kind_filter_allows_all_keyword() {
        let index = make_index(vec![make_file(
            "src/a.rs",
            "",
            vec![
                make_symbol("job", SymbolKind::Function, 1),
                make_symbol("job", SymbolKind::Method, 2),
            ],
        )]);

        let result = search_symbols(&index, "job", Some("all"), 50);

        assert_eq!(result.hits.len(), 2);
    }

    #[test]
    fn test_search_module_text_search_terms_are_trimmed_and_grouped() {
        let index = make_index(vec![
            make_file("src/a.rs", "TODO one\nother\nFIXME two\n", Vec::new()),
            make_file("src/b.rs", "todo lower\n", Vec::new()),
        ]);
        let terms = vec![" TODO ".to_string(), "".to_string(), "FIXME".to_string()];

        let result = search_text(&index, None, Some(&terms), false).expect("search should work");

        assert_eq!(result.label, "terms [TODO, FIXME]");
        assert_eq!(result.total_matches, 3);
        assert_eq!(result.files.len(), 2);
        assert_eq!(result.files[0].path, "src/a.rs");
        assert_eq!(result.files[0].matches[0].line_number, 1);
        assert_eq!(result.files[1].path, "src/b.rs");
    }

    #[test]
    fn test_search_module_text_search_empty_regex_query_errors() {
        let index = make_index(vec![make_file("src/a.rs", "content", Vec::new())]);

        let result = search_text(&index, Some(" "), None, true);

        assert_eq!(result, Err(TextSearchError::EmptyRegexQuery));
    }

    #[test]
    fn test_search_module_symbol_search_with_options_respects_path_scope_and_noise_policy() {
        let index = make_index(vec![
            make_file(
                "src/job.rs",
                "",
                vec![make_symbol("job", SymbolKind::Function, 1)],
            ),
            make_file(
                "tests/generated/job_test.rs",
                "",
                vec![make_symbol("jobNoise", SymbolKind::Function, 2)],
            ),
        ]);
        let options = SymbolSearchOptions {
            path_scope: PathScope::prefix("src/"),
            noise_policy: NoisePolicy::hide_classified_noise(),
            ..Default::default()
        };

        let result = search_symbols_with_options(&index, "job", None, &options);

        assert_eq!(result.file_count, 1);
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].path, "src/job.rs");
        assert_eq!(result.hits[0].name, "job");
    }

    #[test]
    fn test_search_module_text_search_with_options_respects_scope_and_path() {
        let mut text_classification = crate::domain::FileClassification::for_code_path("docs/readme.md");
        text_classification.class = FileClass::Text;
        let index = make_index(vec![
            make_file_with_classification(
                "docs/readme.md",
                "needle in docs\n",
                Vec::new(),
                text_classification,
            ),
            make_file("src/lib.rs", "needle in code\n", Vec::new()),
        ]);
        let options = TextSearchOptions {
            path_scope: PathScope::prefix("docs/"),
            search_scope: SearchScope::Text,
            ..Default::default()
        };

        let result = search_text_with_options(&index, Some("needle"), None, false, &options)
            .expect("search should work");

        assert_eq!(result.total_matches, 1);
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].path, "docs/readme.md");
        assert_eq!(result.files[0].matches[0].line, "needle in docs");
    }

    #[test]
    fn test_current_code_symbol_search_options_are_explicit() {
        let options = SymbolSearchOptions::for_current_code_search(17);

        assert_eq!(options.path_scope, PathScope::Any);
        assert_eq!(options.search_scope, SearchScope::Code);
        assert_eq!(options.result_limit, ResultLimit::new(17));
        assert_eq!(options.noise_policy, NoisePolicy::permissive());
    }

    #[test]
    fn test_current_code_text_search_options_are_explicit() {
        let options = TextSearchOptions::for_current_code_search();

        assert_eq!(options.path_scope, PathScope::Any);
        assert_eq!(options.search_scope, SearchScope::Code);
        assert_eq!(options.noise_policy, NoisePolicy::permissive());
    }

    #[test]
    fn test_explicit_path_read_options_are_exact() {
        let options = FileContentOptions::for_explicit_path_read("src/lib.rs", Some(2), Some(4));

        assert_eq!(options.path_scope, PathScope::Exact("src/lib.rs".to_string()));
        assert_eq!(
            options.content_context,
            ContentContext::line_range(Some(2), Some(4))
        );
    }
}
