/// Pure formatting functions for all 10 tool responses.
///
/// All functions take `&LiveIndex` (or data derived from it) and return `String`.
/// No I/O, no async. Output matches the locked formats defined in CONTEXT.md.

/// Budget limits for reference/dependent output to prevent unbounded token usage.
pub struct OutputLimits {
    /// Maximum number of files to include in the output.
    pub max_files: usize,
    /// Maximum number of reference/hit lines per file.
    pub max_per_file: usize,
    /// Maximum total hits across all files (max_files * max_per_file).
    pub total_hits: usize,
}

impl OutputLimits {
    pub fn new(max_files: u32, max_per_file: u32) -> Self {
        Self {
            max_files: max_files.min(100) as usize,
            max_per_file: max_per_file.min(50) as usize,
            total_hits: (max_files.min(100) * max_per_file.min(50)) as usize,
        }
    }
}

impl Default for OutputLimits {
    fn default() -> Self {
        Self {
            max_files: 20,
            max_per_file: 10,
            total_hits: 200,
        }
    }
}

use crate::live_index::{
    ContextBundleFoundView, ContextBundleSectionView, ContextBundleView, FileContentView,
    FileOutlineView, FindDependentsView, FindImplementationsView, FindReferencesView, HealthStats,
    IndexedFile, InspectMatchView, LiveIndex, PublishedIndexState, RepoOutlineFileView,
    RepoOutlineView, ResolvePathView, SearchFilesTier, SearchFilesView, SymbolDetailView,
    TypeDependencyView, WhatChangedTimestampView, search,
};

/// Format the file outline for a given path.
///
/// Header: `{path}  ({N} symbols)`
/// Body: each symbol indented by `depth * 2` spaces, then `{kind:<12} {name:<30} {start}-{end}`
/// Not-found: "File not found: {path}"
pub fn file_outline(index: &LiveIndex, path: &str) -> String {
    match index.capture_shared_file(path) {
        Some(file) => file_outline_from_indexed_file(file.as_ref()),
        None => not_found_file(path),
    }
}

pub fn file_outline_from_indexed_file(file: &IndexedFile) -> String {
    render_file_outline(&file.relative_path, &file.symbols)
}

fn render_file_outline(relative_path: &str, symbols: &[crate::domain::SymbolRecord]) -> String {
    let mut lines = Vec::new();
    lines.push(format!("{}  ({} symbols)", relative_path, symbols.len()));

    for sym in symbols {
        let indent = "  ".repeat(sym.depth as usize);
        let kind_str = sym.kind.to_string();
        lines.push(format!(
            "{}{:<12} {:<30} {}-{}",
            indent,
            kind_str,
            sym.name,
            sym.line_range.0 + 1,
            sym.line_range.1 + 1
        ));
    }

    lines.join("\n")
}

/// Compatibility renderer for `FileOutlineView`.
///
/// Main hot-path readers should prefer `file_outline_from_indexed_file()`.
pub fn file_outline_view(view: &FileOutlineView) -> String {
    render_file_outline(&view.relative_path, &view.symbols)
}

/// Return the full source body for a named symbol plus a footer.
///
/// Footer: `[{kind}, lines {start}-{end}, {byte_count} bytes]`
/// Not-found: see `not_found_symbol`
pub fn symbol_detail(
    index: &LiveIndex,
    path: &str,
    name: &str,
    kind_filter: Option<&str>,
) -> String {
    match index.capture_shared_file(path) {
        Some(file) => symbol_detail_from_indexed_file(file.as_ref(), name, kind_filter),
        None => not_found_file(path),
    }
}

pub fn symbol_detail_from_indexed_file(
    file: &IndexedFile,
    name: &str,
    kind_filter: Option<&str>,
) -> String {
    render_symbol_detail(
        &file.relative_path,
        &file.content,
        &file.symbols,
        name,
        kind_filter,
    )
}

/// Compatibility renderer for `SymbolDetailView`.
///
/// Main hot-path readers should prefer `symbol_detail_from_indexed_file()`.
pub fn symbol_detail_view(
    view: &SymbolDetailView,
    name: &str,
    kind_filter: Option<&str>,
) -> String {
    render_symbol_detail(
        &view.relative_path,
        &view.content,
        &view.symbols,
        name,
        kind_filter,
    )
}

fn render_symbol_detail(
    relative_path: &str,
    content: &[u8],
    symbols: &[crate::domain::SymbolRecord],
    name: &str,
    kind_filter: Option<&str>,
) -> String {
    let sym = symbols.iter().find(|s| {
        s.name == name
            && kind_filter
                .map(|k| s.kind.to_string().eq_ignore_ascii_case(k))
                .unwrap_or(true)
    });

    match sym {
        None => render_not_found_symbol(relative_path, symbols, name),
        Some(s) => {
            let start = s.effective_start() as usize;
            let end = s.byte_range.1 as usize;
            let body = if end <= content.len() {
                String::from_utf8_lossy(&content[start..end]).into_owned()
            } else {
                String::from_utf8_lossy(content).into_owned()
            };
            let byte_count = end.saturating_sub(start);
            format!(
                "{}\n[{}, lines {}-{}, {} bytes]",
                body,
                s.kind,
                s.line_range.0 + 1,
                s.line_range.1 + 1,
                byte_count
            )
        }
    }
}

pub fn code_slice_view(path: &str, slice: &[u8]) -> String {
    let text = String::from_utf8_lossy(slice).into_owned();
    format!("{path}\n{text}")
}

pub fn code_slice_from_indexed_file(
    file: &IndexedFile,
    start_byte: usize,
    end_byte: Option<usize>,
) -> String {
    let end = end_byte
        .unwrap_or(file.content.len())
        .min(file.content.len());
    let start = start_byte.min(end);
    code_slice_view(&file.relative_path, &file.content[start..end])
}

/// Search for symbols matching a query (case-insensitive), with 3-tier scored ranking.
///
/// Output sections (only non-empty tiers shown):
/// ```text
/// ── Exact matches ──
///   {line}: {kind} {name}  ({file})
///
/// ── Prefix matches ──
///   ...
///
/// ── Substring matches ──
///   ...
/// ```
/// Header: `{N} matches in {M} files`
/// Empty: "No symbols matching '{query}'"
pub fn search_symbols_result(index: &LiveIndex, query: &str) -> String {
    search_symbols_result_with_kind(index, query, None)
}

pub fn search_symbols_result_with_kind(
    index: &LiveIndex,
    query: &str,
    kind_filter: Option<&str>,
) -> String {
    let result = search::search_symbols(
        index,
        query,
        kind_filter,
        search::ResultLimit::symbol_search_default().get(),
    );
    search_symbols_result_view(&result, query)
}

pub fn search_symbols_result_view(result: &search::SymbolSearchResult, query: &str) -> String {
    if result.hits.is_empty() {
        return format!("No symbols matching '{query}'");
    }

    let mut lines = vec![format!(
        "{} matches in {} files",
        result.hits.len(),
        result.file_count
    )];

    let mut last_tier: Option<search::SymbolMatchTier> = None;
    for hit in &result.hits {
        if last_tier != Some(hit.tier) {
            last_tier = Some(hit.tier);
            let header = match hit.tier {
                search::SymbolMatchTier::Exact => "\u{2500}\u{2500} Exact matches \u{2500}\u{2500}",
                search::SymbolMatchTier::Prefix => {
                    "\u{2500}\u{2500} Prefix matches \u{2500}\u{2500}"
                }
                search::SymbolMatchTier::Substring => {
                    "\u{2500}\u{2500} Substring matches \u{2500}\u{2500}"
                }
            };
            if lines.len() > 1 {
                lines.push(String::new());
            }
            lines.push(header.to_string());
        }
        lines.push(format!(
            "  {}: {} {}  ({})",
            hit.line, hit.kind, hit.name, hit.path
        ));
    }

    lines.join("\n")
}

/// Search for text content matches (case-insensitive substring).
///
/// For queries >= 3 chars, uses the TrigramIndex to select candidate files before scanning.
/// For queries < 3 chars, falls back to scanning all files (trigram search handles this internally).
///
/// Header: `{N} matches in {M} files`
/// Body: grouped by file, each match: `  {line_number}: {line_content}`
/// Empty: "No matches for '{query}'"
pub fn search_text_result(index: &LiveIndex, query: &str) -> String {
    search_text_result_with_options(index, Some(query), None, false)
}

pub fn search_text_result_with_options(
    index: &LiveIndex,
    query: Option<&str>,
    terms: Option<&[String]>,
    regex: bool,
) -> String {
    let result = search::search_text(index, query, terms, regex);
    search_text_result_view(result, None)
}

/// Returns true if the line looks like an import statement or a non-doc comment.
pub fn is_noise_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.starts_with("///") || trimmed.starts_with("//!") || trimmed.starts_with("/**") {
        return false;
    }
    trimmed.starts_with("use ")
        || trimmed.starts_with("import ")
        || trimmed.starts_with("from ")
        || trimmed.starts_with("require(")
        || trimmed.starts_with("#include")
        || trimmed.starts_with("//")
        || trimmed.starts_with('#')
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
        || trimmed.starts_with("--")
        || line.contains("require(")
}

pub fn search_text_result_view(
    result: Result<search::TextSearchResult, search::TextSearchError>,
    group_by: Option<&str>,
) -> String {
    let result = match result {
        Ok(result) => result,
        Err(search::TextSearchError::EmptyRegexQuery) => {
            return "Regex search requires a non-empty query.".to_string();
        }
        Err(search::TextSearchError::EmptyQueryOrTerms) => {
            return "Search requires a non-empty query or terms.".to_string();
        }
        Err(search::TextSearchError::InvalidRegex { pattern, error }) => {
            return format!("Invalid regex '{pattern}': {error}");
        }
        Err(search::TextSearchError::InvalidGlob {
            field,
            pattern,
            error,
        }) => {
            return format!("Invalid glob for `{field}` ('{pattern}'): {error}");
        }
        Err(search::TextSearchError::UnsupportedWholeWordRegex) => {
            return "whole_word is not supported when `regex=true`.".to_string();
        }
    };

    if result.files.is_empty() {
        if result.suppressed_by_noise > 0 {
            return format!(
                "No matches for {} in source code. {} match(es) found in test modules — set include_tests=true to include them.",
                result.label, result.suppressed_by_noise
            );
        }
        return format!("No matches for {}", result.label);
    }

    let mut lines = vec![format!(
        "{} matches in {} files",
        result.total_matches,
        result.files.len()
    )];
    for file in &result.files {
        lines.push(file.path.clone());
        if let Some(rendered_lines) = &file.rendered_lines {
            // Context mode: don't apply grouping — context windows don't compose well with it
            for rendered_line in rendered_lines {
                match rendered_line {
                    search::TextDisplayLine::Separator => lines.push("  ...".to_string()),
                    search::TextDisplayLine::Line(rendered_line) => lines.push(format!(
                        "{} {}: {}",
                        if rendered_line.is_match { ">" } else { " " },
                        rendered_line.line_number,
                        rendered_line.line
                    )),
                }
            }
        } else {
            match group_by {
                Some("symbol") => {
                    // One entry per unique enclosing symbol, showing match count
                    // Preserve insertion order by tracking symbol names in order
                    let mut symbol_order: Vec<String> = Vec::new();
                    let mut symbol_counts: std::collections::HashMap<
                        String,
                        (usize, String, u32, u32),
                    > = std::collections::HashMap::new();
                    let mut no_symbol_count = 0usize;
                    for line_match in &file.matches {
                        if let Some(ref enc) = line_match.enclosing_symbol {
                            let key = enc.name.clone();
                            if !symbol_counts.contains_key(&key) {
                                symbol_order.push(key.clone());
                                symbol_counts.insert(
                                    key,
                                    (
                                        1,
                                        enc.kind.clone(),
                                        enc.line_range.0 + 1,
                                        enc.line_range.1 + 1,
                                    ),
                                );
                            } else {
                                symbol_counts.get_mut(&enc.name).unwrap().0 += 1;
                            }
                        } else {
                            no_symbol_count += 1;
                        }
                    }
                    for sym_name in &symbol_order {
                        if let Some((count, kind, start, end)) = symbol_counts.get(sym_name) {
                            let match_word = if *count == 1 { "match" } else { "matches" };
                            lines.push(format!(
                                "  {} {} (lines {}-{}): {} {}",
                                kind, sym_name, start, end, count, match_word
                            ));
                        }
                    }
                    if no_symbol_count > 0 {
                        let match_word = if no_symbol_count == 1 {
                            "match"
                        } else {
                            "matches"
                        };
                        lines.push(format!("  (top-level): {} {}", no_symbol_count, match_word));
                    }
                }
                Some("usage") | Some("purpose") => {
                    let mut last_symbol: Option<String> = None;
                    let mut filtered_count = 0usize;
                    for line_match in &file.matches {
                        if is_noise_line(&line_match.line) {
                            filtered_count += 1;
                            continue;
                        }
                        if let Some(ref enc) = line_match.enclosing_symbol {
                            if last_symbol.as_deref() != Some(enc.name.as_str()) {
                                lines.push(format!(
                                    "  in {} {} (lines {}-{}):",
                                    enc.kind,
                                    enc.name,
                                    enc.line_range.0 + 1,
                                    enc.line_range.1 + 1
                                ));
                                last_symbol = Some(enc.name.clone());
                            }
                            lines.push(format!(
                                "    > {}: {}",
                                line_match.line_number, line_match.line
                            ));
                        } else {
                            last_symbol = None;
                            lines
                                .push(format!("  {}: {}", line_match.line_number, line_match.line));
                        }
                    }
                    if filtered_count > 0 {
                        lines.push(format!(
                            "  ({filtered_count} import/comment match(es) excluded by usage filter)"
                        ));
                    }
                }
                // None or Some("file") — default behavior
                _ => {
                    let mut last_symbol: Option<String> = None;
                    for line_match in &file.matches {
                        if let Some(ref enc) = line_match.enclosing_symbol {
                            if last_symbol.as_deref() != Some(enc.name.as_str()) {
                                lines.push(format!(
                                    "  in {} {} (lines {}-{}):",
                                    enc.kind,
                                    enc.name,
                                    enc.line_range.0 + 1,
                                    enc.line_range.1 + 1
                                ));
                                last_symbol = Some(enc.name.clone());
                            }
                            lines.push(format!(
                                "    > {}: {}",
                                line_match.line_number, line_match.line
                            ));
                        } else {
                            last_symbol = None;
                            lines
                                .push(format!("  {}: {}", line_match.line_number, line_match.line));
                        }
                    }
                }
            }
        }
        if let Some(ref callers) = file.callers {
            if callers.is_empty() {
                lines.push("    (no cross-references found)".to_string());
            } else {
                let caller_strs: Vec<String> = callers
                    .iter()
                    .map(|c| format!("{} ({}:{})", c.symbol, c.file, c.line))
                    .collect();
                lines.push(format!("    Called by: {}", caller_strs.join(", ")));
            }
        }
    }
    lines.join("\n")
}

/// Generate a depth-limited source file tree with symbol counts per file and directory.
///
/// - `path`: subtree prefix filter (empty/blank = project root).
/// - `depth`: maximum depth levels to expand (default 2, max 5).
///
/// Output format:
/// ```text
/// {dir}/  ({N} files, {M} symbols)
///   {file} [{lang}]  ({K} symbols)
///   {subdir}/  ({N} files, {M} symbols)
/// ...
/// {D} directories, {F} files, {S} symbols
/// ```
pub fn file_tree(index: &LiveIndex, path: &str, depth: u32) -> String {
    let view = index.capture_repo_outline_view();
    file_tree_view(&view.files, path, depth)
}

pub fn file_tree_view(files: &[RepoOutlineFileView], path: &str, depth: u32) -> String {
    let depth = depth.min(5);
    let prefix = path.trim_matches('/');

    // Collect all files whose relative_path starts with the path prefix.
    let matching_files: Vec<&RepoOutlineFileView> = files
        .iter()
        .filter(|file| {
            let p = file.relative_path.as_str();
            if prefix.is_empty() {
                true
            } else {
                p.starts_with(prefix)
                    && (p.len() == prefix.len() || p.as_bytes().get(prefix.len()) == Some(&b'/'))
            }
        })
        .collect();

    if matching_files.is_empty() {
        return format!(
            "No source files found under '{}'",
            if prefix.is_empty() { "." } else { prefix }
        );
    }

    // Build a tree: BTreeMap from directory path -> Vec<(filename, lang, symbol_count)>
    // Node entries are keyed by their path component at each level.
    use std::collections::BTreeMap;

    // Strip the prefix from all paths before building the tree.
    let strip_len = if prefix.is_empty() {
        0
    } else {
        prefix.len() + 1
    };
    let stripped: Vec<(&str, &RepoOutlineFileView)> = matching_files
        .into_iter()
        .map(|file| {
            let p = file.relative_path.as_str();
            (
                if p.len() >= strip_len {
                    &p[strip_len..]
                } else {
                    p
                },
                file,
            )
        })
        .collect();

    // Recursively build tree lines.
    fn build_lines(
        entries: &[(&str, &RepoOutlineFileView)],
        current_depth: u32,
        max_depth: u32,
        indent: usize,
    ) -> Vec<String> {
        // Group by first path component.
        let mut dirs: BTreeMap<&str, Vec<(&str, &RepoOutlineFileView)>> = BTreeMap::new();
        let mut files_here: Vec<(&str, &RepoOutlineFileView)> = Vec::new();

        for (rel, file) in entries {
            if let Some(slash) = rel.find('/') {
                let dir_part = &rel[..slash];
                let rest = &rel[slash + 1..];
                dirs.entry(dir_part).or_default().push((rest, file));
            } else {
                files_here.push((rel, file));
            }
        }

        let pad = "  ".repeat(indent);
        let mut lines = Vec::new();

        // Files at this level
        files_here.sort_by_key(|(name, _)| *name);
        for (name, file) in &files_here {
            let sym_count = file.symbol_count;
            let sym_label = if sym_count == 1 { "symbol" } else { "symbols" };
            lines.push(format!(
                "{}{} [{}]  ({} {})",
                pad, name, file.language, sym_count, sym_label
            ));
        }

        // Directories at this level
        for (dir_name, children) in &dirs {
            let file_count = count_files(children);
            let sym_count: usize = children.iter().map(|(_, f)| f.symbol_count).sum();
            let sym_label = if sym_count == 1 { "symbol" } else { "symbols" };

            if current_depth >= max_depth {
                // Collapsed — just show summary line
                lines.push(format!(
                    "{}{}/  ({} files, {} {})",
                    pad, dir_name, file_count, sym_count, sym_label
                ));
            } else {
                lines.push(format!(
                    "{}{}/  ({} files, {} {})",
                    pad, dir_name, file_count, sym_count, sym_label
                ));
                let sub_lines = build_lines(children, current_depth + 1, max_depth, indent + 1);
                lines.extend(sub_lines);
            }
        }

        lines
    }

    fn count_files(entries: &[(&str, &RepoOutlineFileView)]) -> usize {
        let mut count = 0;
        for (rel, _) in entries {
            if rel.contains('/') {
                // nested
            } else {
                count += 1;
            }
        }
        // also count files in sub-directories
        let mut dirs: std::collections::HashMap<&str, Vec<(&str, &RepoOutlineFileView)>> =
            std::collections::HashMap::new();
        for (rel, file) in entries {
            if let Some(slash) = rel.find('/') {
                dirs.entry(&rel[..slash])
                    .or_default()
                    .push((&rel[slash + 1..], file));
            }
        }
        for children in dirs.values() {
            count += count_files(children);
        }
        count
    }

    fn count_dirs(entries: &[(&str, &RepoOutlineFileView)]) -> usize {
        let mut dirs: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let mut sub_entries: std::collections::HashMap<&str, Vec<(&str, &RepoOutlineFileView)>> =
            std::collections::HashMap::new();
        for (rel, file) in entries {
            if let Some(slash) = rel.find('/') {
                let dir_name = &rel[..slash];
                dirs.insert(dir_name);
                sub_entries
                    .entry(dir_name)
                    .or_default()
                    .push((&rel[slash + 1..], file));
            }
        }
        let mut total = dirs.len();
        for children in sub_entries.values() {
            total += count_dirs(children);
        }
        total
    }

    let body_lines = build_lines(&stripped, 1, depth, 0);

    let total_files = stripped.len();
    let total_dirs = count_dirs(&stripped);
    let total_symbols: usize = stripped.iter().map(|(_, f)| f.symbol_count).sum();
    let sym_label = if total_symbols == 1 {
        "symbol"
    } else {
        "symbols"
    };

    let mut output = body_lines;
    output.push(format!(
        "{} directories, {} files, {} {}",
        total_dirs, total_files, total_symbols, sym_label
    ));

    output.join("\n")
}

/// Generate a directory-tree overview of the repo.
///
/// Header: `{project_name}  ({N} files, {M} symbols)`
/// Body: sorted paths, each: `  {filename:<20} {language:<12} {symbol_count} symbols`
pub fn repo_outline(index: &LiveIndex, project_name: &str) -> String {
    let view = index.capture_repo_outline_view();
    repo_outline_view(&view, project_name)
}

fn repo_outline_display_labels(
    files: &[RepoOutlineFileView],
) -> std::collections::HashMap<String, String> {
    fn basename(path: &str) -> &str {
        path.rsplit('/').next().unwrap_or(path)
    }

    let mut by_basename: std::collections::HashMap<&str, Vec<&str>> =
        std::collections::HashMap::new();
    for file in files {
        by_basename
            .entry(basename(&file.relative_path))
            .or_default()
            .push(file.relative_path.as_str());
    }

    let mut labels = std::collections::HashMap::with_capacity(files.len());
    for paths in by_basename.into_values() {
        let split_paths: Vec<Vec<&str>> = paths
            .iter()
            .map(|path| {
                path.split('/')
                    .filter(|segment| !segment.is_empty())
                    .collect()
            })
            .collect();
        let max_depth = split_paths.iter().map(Vec::len).max().unwrap_or(1);
        let mut resolved: Vec<Option<String>> = vec![None; split_paths.len()];

        for depth in 1..=max_depth {
            let mut candidate_counts: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();
            for parts in &split_paths {
                let start = parts.len().saturating_sub(depth);
                let candidate = parts[start..].join("/");
                *candidate_counts.entry(candidate).or_insert(0) += 1;
            }

            for (idx, parts) in split_paths.iter().enumerate() {
                if resolved[idx].is_some() {
                    continue;
                }
                let start = parts.len().saturating_sub(depth);
                let candidate = parts[start..].join("/");
                if candidate_counts.get(&candidate) == Some(&1) || depth == max_depth {
                    resolved[idx] = Some(candidate);
                }
            }

            if resolved.iter().all(Option::is_some) {
                break;
            }
        }

        for (path, label) in paths.iter().zip(resolved.into_iter()) {
            labels.insert(
                (*path).to_string(),
                label.unwrap_or_else(|| (*path).to_string()),
            );
        }
    }

    labels
}

pub fn repo_outline_view(view: &RepoOutlineView, project_name: &str) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "{project_name}  ({} files, {} symbols)",
        view.total_files, view.total_symbols
    ));

    let labels = repo_outline_display_labels(&view.files);
    let label_width = labels
        .values()
        .map(|label| label.len())
        .max()
        .unwrap_or(20)
        .clamp(20, 32);

    for file in &view.files {
        let label = labels
            .get(&file.relative_path)
            .expect("repo outline label should exist");
        lines.push(format!(
            "  {:<width$} {:<12} {} symbols",
            label,
            file.language.to_string(),
            file.symbol_count,
            width = label_width
        ));
    }

    lines.join("\n")
}

/// Generate a health report for the index.
///
/// Watcher state is read from `health_stats()` (Off defaults when no watcher is active).
/// Use `health_report_with_watcher` when the live `WatcherInfo` should be reflected.
///
/// Format:
/// ```text
/// Status: {Ready|Empty|Degraded}
/// Files:  {N} indexed ({P} parsed, {PP} partial, {F} failed)
/// Symbols: {S}
/// Loaded in: {D}ms
/// Watcher: active ({E} events, last: {T}, debounce: {D}ms)
///     or: degraded ({E} events processed before failure)
///     or: off
/// ```
pub fn health_report(index: &LiveIndex) -> String {
    use crate::live_index::IndexState;

    let state = index.index_state();
    let status = match state {
        IndexState::Empty => "Empty",
        IndexState::Ready => "Ready",
        IndexState::Loading => "Loading",
        IndexState::CircuitBreakerTripped { .. } => "Degraded",
    };
    let stats = index.health_stats();
    health_report_from_stats(status, &stats)
}

/// Generate a health report for the index with live watcher state.
///
/// Uses `health_stats_with_watcher` to incorporate the live `WatcherInfo` into the report.
/// Called by the `health` tool handler in production (watcher is always available there).
pub fn health_report_with_watcher(
    index: &LiveIndex,
    watcher: &crate::watcher::WatcherInfo,
) -> String {
    use crate::live_index::IndexState;

    let state = index.index_state();
    let status = match state {
        IndexState::Empty => "Empty",
        IndexState::Ready => "Ready",
        IndexState::Loading => "Loading",
        IndexState::CircuitBreakerTripped { .. } => "Degraded",
    };
    let stats = index.health_stats_with_watcher(watcher);
    health_report_from_stats(status, &stats)
}

pub fn health_report_from_published_state(
    published: &PublishedIndexState,
    watcher: &crate::watcher::WatcherInfo,
) -> String {
    let mut stats = HealthStats {
        file_count: published.file_count,
        symbol_count: published.symbol_count,
        parsed_count: published.parsed_count,
        partial_parse_count: published.partial_parse_count,
        failed_count: published.failed_count,
        load_duration: published.load_duration,
        watcher_state: watcher.state.clone(),
        events_processed: watcher.events_processed,
        last_event_at: watcher.last_event_at,
        debounce_window_ms: watcher.debounce_window_ms,
    };
    // Preserve the existing formatter shape by reusing HealthStats.
    if matches!(stats.watcher_state, crate::watcher::WatcherState::Off) {
        stats.events_processed = 0;
        stats.last_event_at = None;
    }
    health_report_from_stats(published.status_label(), &stats)
}

pub fn health_report_from_stats(status: &str, stats: &HealthStats) -> String {
    use crate::watcher::WatcherState;

    let watcher_line = match &stats.watcher_state {
        WatcherState::Active => {
            let last = match stats.last_event_at {
                None => "never".to_string(),
                Some(t) => {
                    let secs = t.elapsed().map(|d| d.as_secs()).unwrap_or(0);
                    format!("{secs}s ago")
                }
            };
            format!(
                "Watcher: active ({} events, last: {}, debounce: {}ms)",
                stats.events_processed, last, stats.debounce_window_ms
            )
        }
        WatcherState::Degraded => {
            format!(
                "Watcher: degraded ({} events processed before failure)",
                stats.events_processed
            )
        }
        WatcherState::Off => "Watcher: off".to_string(),
    };

    format!(
        "Status: {}\nFiles:  {} indexed ({} parsed, {} partial, {} failed)\nSymbols: {}\nLoaded in: {}ms\n{}",
        status,
        stats.file_count,
        stats.parsed_count,
        stats.partial_parse_count,
        stats.failed_count,
        stats.symbol_count,
        stats.load_duration.as_millis(),
        watcher_line
    )
}

/// List files changed since the given Unix timestamp.
///
/// If since_ts < loaded_at: return list of all files (entire index is "newer")
/// If since_ts >= loaded_at: return "No changes detected since last index load."
pub fn what_changed_result(index: &LiveIndex, since_ts: i64) -> String {
    let view = index.capture_what_changed_timestamp_view();
    what_changed_timestamp_view(&view, since_ts)
}

pub fn what_changed_timestamp_view(view: &WhatChangedTimestampView, since_ts: i64) -> String {
    if since_ts < view.loaded_secs {
        // Entire index is newer — list all files
        if view.paths.is_empty() {
            return "No changes detected since last index load.".to_string();
        }
        view.paths.join("\n")
    } else {
        "No changes detected since last index load.".to_string()
    }
}

pub fn what_changed_paths_result(paths: &[String], empty_message: &str) -> String {
    let mut normalized_paths: Vec<String> =
        paths.iter().map(|path| path.replace('\\', "/")).collect();
    normalized_paths.sort();
    normalized_paths.dedup();

    if normalized_paths.is_empty() {
        return empty_message.to_string();
    }

    normalized_paths.join("\n")
}

pub fn resolve_path_result(index: &LiveIndex, hint: &str) -> String {
    let view = index.capture_resolve_path_view(hint);
    resolve_path_result_view(&view)
}

pub fn resolve_path_result_view(view: &ResolvePathView) -> String {
    match view {
        ResolvePathView::EmptyHint => "Path hint must not be empty.".to_string(),
        ResolvePathView::Resolved { path } => path.clone(),
        ResolvePathView::NotFound { hint } => {
            format!("No indexed source path matched '{hint}'")
        }
        ResolvePathView::Ambiguous {
            hint,
            matches,
            overflow_count,
        } => {
            let mut lines = vec![format!(
                "Ambiguous path hint '{hint}' ({} matches)",
                matches.len() + overflow_count
            )];
            lines.extend(matches.iter().map(|path| format!("  {path}")));
            if *overflow_count > 0 {
                lines.push(format!("  ... and {} more", overflow_count));
            }
            lines.join("\n")
        }
    }
}

pub fn search_files(index: &LiveIndex, query: &str, limit: usize) -> String {
    let view = index.capture_search_files_view(query, limit, None);
    search_files_result_view(&view)
}

pub fn search_files_result_view(view: &SearchFilesView) -> String {
    match view {
        SearchFilesView::EmptyQuery => "Path search requires a non-empty query.".to_string(),
        SearchFilesView::NotFound { query } => {
            format!("No indexed source files matching '{query}'")
        }
        SearchFilesView::Found {
            total_matches,
            overflow_count,
            hits,
            ..
        } => {
            let mut lines = vec![if *total_matches == 1 {
                "1 matching file".to_string()
            } else {
                format!("{total_matches} matching files")
            }];

            let mut last_tier: Option<SearchFilesTier> = None;
            for hit in hits {
                if last_tier != Some(hit.tier) {
                    last_tier = Some(hit.tier);
                    let header = match hit.tier {
                        SearchFilesTier::CoChange => {
                            "── Co-changed files (git temporal coupling) ──"
                        }
                        SearchFilesTier::StrongPath => "── Strong path matches ──",
                        SearchFilesTier::Basename => "── Basename matches ──",
                        SearchFilesTier::LoosePath => "── Loose path matches ──",
                    };
                    if lines.len() > 1 {
                        lines.push(String::new());
                    }
                    lines.push(header.to_string());
                }
                if let (Some(score), Some(shared)) = (hit.coupling_score, hit.shared_commits) {
                    lines.push(format!(
                        "  {}  ({:.0}% coupled, {} shared commits)",
                        hit.path,
                        score * 100.0,
                        shared
                    ));
                } else {
                    lines.push(format!("  {}", hit.path));
                }
            }

            if *overflow_count > 0 {
                lines.push(format!("... and {} more", overflow_count));
            }

            lines.join("\n")
        }
    }
}

/// Return raw file content, optionally sliced by 1-indexed line range.
///
/// Not-found: "File not found: {path}"
pub fn file_content(
    index: &LiveIndex,
    path: &str,
    start_line: Option<u32>,
    end_line: Option<u32>,
) -> String {
    let options = search::FileContentOptions::for_explicit_path_read(path, start_line, end_line);
    match index.capture_shared_file_for_scope(&options.path_scope) {
        Some(file) => {
            file_content_from_indexed_file_with_context(file.as_ref(), options.content_context)
        }
        None => not_found_file(path),
    }
}

pub fn file_content_from_indexed_file(
    file: &IndexedFile,
    start_line: Option<u32>,
    end_line: Option<u32>,
) -> String {
    file_content_from_indexed_file_with_context(
        file,
        search::ContentContext::line_range(start_line, end_line),
    )
}

pub fn file_content_from_indexed_file_with_context(
    file: &IndexedFile,
    context: search::ContentContext,
) -> String {
    if let Some(chunk_index) = context.chunk_index {
        let max_lines = match context.max_lines {
            Some(ml) => ml,
            None => {
                return format!(
                    "{} [error: chunked read requires max_lines parameter]",
                    file.relative_path
                );
            }
        };
        return render_numbered_chunk_excerpt(file, chunk_index, max_lines);
    }

    if let Some(around_symbol) = context.around_symbol.as_deref() {
        return render_numbered_around_symbol_excerpt(
            file,
            around_symbol,
            context.symbol_line,
            context
                .context_lines
                .unwrap_or(DEFAULT_AROUND_LINE_CONTEXT_LINES),
        );
    }

    if let Some(around_match) = context.around_match.as_deref() {
        return render_numbered_around_match_excerpt(
            file,
            around_match,
            context
                .context_lines
                .unwrap_or(DEFAULT_AROUND_LINE_CONTEXT_LINES),
        );
    }

    render_file_content_bytes(&file.relative_path, &file.content, context)
}

/// Compatibility renderer for `FileContentView`.
///
/// Main hot-path readers should prefer `file_content_from_indexed_file()`.
pub fn file_content_view(
    view: &FileContentView,
    start_line: Option<u32>,
    end_line: Option<u32>,
) -> String {
    render_file_content_bytes(
        &view.relative_path,
        &view.content,
        search::ContentContext::line_range(start_line, end_line),
    )
}

const DEFAULT_AROUND_LINE_CONTEXT_LINES: u32 = 2;

pub(crate) fn render_file_content_bytes(
    path: &str,
    content: &[u8],
    context: search::ContentContext,
) -> String {
    let content = String::from_utf8_lossy(content);
    let lines: Vec<&str> = content.lines().collect();
    let line_count = lines.len() as u32;

    // Validate explicit line range against file length.
    if let Some(start) = context.start_line {
        if start > line_count {
            return format!(
                "{path} [error: requested range (lines {start}-{}) exceeds file length ({line_count} lines)]",
                context.end_line.unwrap_or(start),
            );
        }
    }

    if let Some(around_line) = context.around_line {
        if around_line > line_count {
            return format!(
                "{path} [error: around_line={around_line} exceeds file length ({line_count} lines)]",
            );
        }
        return render_numbered_around_line_excerpt(
            &lines,
            around_line,
            context
                .context_lines
                .unwrap_or(DEFAULT_AROUND_LINE_CONTEXT_LINES),
        );
    }

    if !context.show_line_numbers && !context.header {
        return match (context.start_line, context.end_line) {
            (None, None) => content.into_owned(),
            (start, end) => render_raw_line_slice(&lines, start, end),
        };
    }

    render_ordinary_read(
        path,
        &lines,
        context.start_line,
        context.end_line,
        context.show_line_numbers,
        context.header,
    )
}

fn render_raw_line_slice(lines: &[&str], start_line: Option<u32>, end_line: Option<u32>) -> String {
    slice_lines(lines, start_line, end_line)
        .into_iter()
        .map(|(_, line)| line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_ordinary_read(
    path: &str,
    lines: &[&str],
    start_line: Option<u32>,
    end_line: Option<u32>,
    show_line_numbers: bool,
    header: bool,
) -> String {
    let selected = slice_lines(lines, start_line, end_line);
    let body = if show_line_numbers {
        selected
            .iter()
            .map(|(line_number, line)| format!("{line_number}: {line}"))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        selected
            .iter()
            .map(|(_, line)| *line)
            .collect::<Vec<_>>()
            .join("\n")
    };

    if !header {
        return body;
    }

    let header_line = if start_line.is_some() || end_line.is_some() {
        render_ordinary_read_header(path, &selected)
    } else {
        path.to_string()
    };

    if body.is_empty() {
        header_line
    } else {
        format!("{header_line}\n{body}")
    }
}

fn slice_lines<'a>(
    lines: &'a [&'a str],
    start_line: Option<u32>,
    end_line: Option<u32>,
) -> Vec<(u32, &'a str)> {
    let start_idx = start_line
        .map(|start| start.saturating_sub(1) as usize)
        .unwrap_or(0);
    let end_idx = end_line.map(|end| end as usize).unwrap_or(usize::MAX);

    lines
        .iter()
        .enumerate()
        .filter_map(|(idx, line)| {
            if idx >= start_idx && idx < end_idx {
                Some((idx as u32 + 1, *line))
            } else {
                None
            }
        })
        .collect()
}

fn render_ordinary_read_header(path: &str, selected: &[(u32, &str)]) -> String {
    match (selected.first(), selected.last()) {
        (Some((first, _)), Some((last, _))) => format!("{path} [lines {first}-{last}]"),
        _ => format!("{path} [lines empty]"),
    }
}

fn render_numbered_chunk_excerpt(file: &IndexedFile, chunk_index: u32, max_lines: u32) -> String {
    let content = String::from_utf8_lossy(&file.content);
    let lines: Vec<&str> = content.lines().collect();
    let chunk_size = max_lines as usize;

    if chunk_index == 0 || chunk_size == 0 {
        return out_of_range_file_chunk(&file.relative_path, chunk_index, 0);
    }

    let total_chunks = lines.len().div_ceil(chunk_size);
    if total_chunks == 0 {
        return out_of_range_file_chunk(&file.relative_path, chunk_index, 0);
    }

    let chunk_number = chunk_index as usize;
    if chunk_number > total_chunks {
        return out_of_range_file_chunk(&file.relative_path, chunk_index, total_chunks);
    }

    let start_idx = (chunk_number - 1) * chunk_size;
    let end_idx = (start_idx + chunk_size).min(lines.len());
    let start_line = start_idx + 1;
    let end_line = end_idx;

    let body = lines[start_idx..end_idx]
        .iter()
        .enumerate()
        .map(|(offset, line)| format!("{}: {line}", start_line + offset))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "{} [chunk {}/{}, lines {}-{}]\n{}",
        file.relative_path, chunk_index, total_chunks, start_line, end_line, body
    )
}

fn render_numbered_around_symbol_excerpt(
    file: &IndexedFile,
    around_symbol: &str,
    symbol_line: Option<u32>,
    context_lines: u32,
) -> String {
    let content = String::from_utf8_lossy(&file.content);
    let lines: Vec<&str> = content.lines().collect();

    match resolve_around_symbol_line(file, around_symbol, symbol_line) {
        Ok(around_line) => render_numbered_around_line_excerpt(&lines, around_line, context_lines),
        Err(AroundSymbolResolutionError::NotFound) => {
            render_not_found_symbol(&file.relative_path, &file.symbols, around_symbol)
        }
        Err(AroundSymbolResolutionError::SelectorNotFound(symbol_line)) => {
            format!(
                "Symbol not found in {}: {} at line {}",
                file.relative_path, around_symbol, symbol_line
            )
        }
        Err(AroundSymbolResolutionError::Ambiguous(candidate_lines)) => {
            let candidate_lines = candidate_lines
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "Ambiguous symbol selector for {around_symbol} in {}; pass `symbol_line` to disambiguate. Candidates: {candidate_lines}",
                file.relative_path
            )
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum AroundSymbolResolutionError {
    NotFound,
    SelectorNotFound(u32),
    Ambiguous(Vec<u32>),
}

fn resolve_around_symbol_line(
    file: &IndexedFile,
    around_symbol: &str,
    symbol_line: Option<u32>,
) -> Result<u32, AroundSymbolResolutionError> {
    let matching_symbols: Vec<&crate::domain::SymbolRecord> = file
        .symbols
        .iter()
        .filter(|symbol| symbol.name == around_symbol)
        .collect();

    if matching_symbols.is_empty() {
        return Err(AroundSymbolResolutionError::NotFound);
    }

    if let Some(symbol_line) = symbol_line {
        let exact_matches: Vec<&crate::domain::SymbolRecord> = matching_symbols
            .iter()
            .copied()
            .filter(|symbol| symbol.line_range.0 == symbol_line)
            .collect();

        return match exact_matches.as_slice() {
            [symbol] => Ok(symbol.line_range.0.saturating_add(1)),
            [] => Err(AroundSymbolResolutionError::SelectorNotFound(symbol_line)),
            _ => Err(AroundSymbolResolutionError::Ambiguous(
                dedup_symbol_candidate_lines(&exact_matches),
            )),
        };
    }

    match matching_symbols.as_slice() {
        [symbol] => Ok(symbol.line_range.0.saturating_add(1)),
        _ => Err(AroundSymbolResolutionError::Ambiguous(
            dedup_symbol_candidate_lines(&matching_symbols),
        )),
    }
}

fn dedup_symbol_candidate_lines(symbols: &[&crate::domain::SymbolRecord]) -> Vec<u32> {
    let mut candidate_lines: Vec<u32> = symbols.iter().map(|symbol| symbol.line_range.0).collect();
    candidate_lines.sort_unstable();
    candidate_lines.dedup();
    candidate_lines
}

fn render_numbered_around_match_excerpt(
    file: &IndexedFile,
    around_match: &str,
    context_lines: u32,
) -> String {
    let content = String::from_utf8_lossy(&file.content);
    let lines: Vec<&str> = content.lines().collect();

    let Some(around_line) = find_first_case_insensitive_match_line(&lines, around_match) else {
        return not_found_file_match(&file.relative_path, around_match);
    };

    render_numbered_around_line_excerpt(&lines, around_line, context_lines)
}

fn find_first_case_insensitive_match_line(lines: &[&str], around_match: &str) -> Option<u32> {
    let needle = around_match.to_lowercase();

    lines
        .iter()
        .position(|line| line.to_lowercase().contains(&needle))
        .map(|index| (index + 1) as u32)
}

fn render_numbered_around_line_excerpt(
    lines: &[&str],
    around_line: u32,
    context_lines: u32,
) -> String {
    if lines.is_empty() {
        return String::new();
    }

    let anchor = around_line.max(1) as usize;
    let context = context_lines as usize;
    let start = anchor.saturating_sub(context).max(1);
    let end = anchor.saturating_add(context).min(lines.len());

    if start > end || start > lines.len() {
        return String::new();
    }

    (start..=end)
        .map(|line_number| format!("{line_number}: {}", lines[line_number - 1]))
        .collect::<Vec<_>>()
        .join("\n")
}

/// "File not found: {path}"
pub fn not_found_file(path: &str) -> String {
    format!("File not found: {path}")
}

/// "No matches for '{query}' in {path}"
pub fn not_found_file_match(path: &str, query: &str) -> String {
    format!("No matches for '{query}' in {path}")
}

fn out_of_range_file_chunk(path: &str, chunk_index: u32, total_chunks: usize) -> String {
    format!("Chunk {chunk_index} out of range for {path} ({total_chunks} chunks)")
}

/// "No symbol {name} in {path}. Close matches: {top 5 fuzzy matches}. Use get_file_context with sections=['outline'] for the full list."
pub fn not_found_symbol(index: &LiveIndex, path: &str, name: &str) -> String {
    match index.capture_shared_file(path) {
        None => not_found_file(path),
        Some(file) => render_not_found_symbol(&file.relative_path, &file.symbols, name),
    }
}

fn render_not_found_symbol(
    relative_path: &str,
    symbols: &[crate::domain::SymbolRecord],
    name: &str,
) -> String {
    let symbol_names: Vec<String> = symbols.iter().map(|s| s.name.clone()).collect();
    not_found_symbol_names(relative_path, &symbol_names, name)
}

/// Simple edit-distance score for fuzzy matching (lower is closer).
fn fuzzy_distance(a: &str, b: &str) -> usize {
    let a_lower = a.to_lowercase();
    let b_lower = b.to_lowercase();

    // Substring match gets highest priority (distance 0).
    if b_lower.contains(&a_lower) || a_lower.contains(&b_lower) {
        return 0;
    }

    // Prefix match gets second priority.
    let prefix_len = a_lower
        .chars()
        .zip(b_lower.chars())
        .take_while(|(x, y)| x == y)
        .count();
    if prefix_len > 0 {
        return a.len().max(b.len()) - prefix_len;
    }

    // Fall back to simple character overlap distance.
    let a_chars: std::collections::HashSet<char> = a_lower.chars().collect();
    let b_chars: std::collections::HashSet<char> = b_lower.chars().collect();
    let intersection = a_chars.intersection(&b_chars).count();
    if intersection == 0 {
        return usize::MAX;
    }
    a.len().max(b.len()) - intersection
}

fn not_found_symbol_names(relative_path: &str, symbol_names: &[String], name: &str) -> String {
    if symbol_names.is_empty() {
        return format!("No symbol {name} in {relative_path}. No symbols in that file.");
    }

    // Rank by fuzzy distance and take top 5.
    let mut scored: Vec<(&String, usize)> = symbol_names
        .iter()
        .map(|s| (s, fuzzy_distance(name, s)))
        .collect();
    scored.sort_by_key(|(_, d)| *d);

    let close_matches: Vec<&str> = scored
        .iter()
        .take(5)
        .filter(|(_, d)| *d < usize::MAX)
        .map(|(s, _)| s.as_str())
        .collect();

    if close_matches.is_empty() {
        format!(
            "No symbol {name} in {relative_path}. No close matches found. \
             Use get_file_context with sections=['outline'] to see all {} symbols in this file.",
            symbol_names.len()
        )
    } else {
        format!(
            "No symbol {name} in {relative_path}. Close matches: {}. \
             Use get_file_context with sections=['outline'] for the full list ({} symbols).",
            close_matches.join(", "),
            symbol_names.len()
        )
    }
}

/// Find all references for a name across the repo, grouped by file with 3-line context.
///
/// kind_filter: "call" | "import" | "type_usage" | "all" | None (all)
/// Output format matches CONTEXT.md decision AD-6 (compact human-readable).
pub fn find_references_result(index: &LiveIndex, name: &str, kind_filter: Option<&str>) -> String {
    let limits = OutputLimits::default();
    let view = index.capture_find_references_view(name, kind_filter, limits.total_hits);
    find_references_result_view(&view, name, &limits)
}

pub fn find_references_result_view(
    view: &FindReferencesView,
    name: &str,
    limits: &OutputLimits,
) -> String {
    if view.total_refs == 0 {
        return format!("No references found for \"{name}\"");
    }

    let total = view.total_refs;
    let total_files = view.total_files;
    let shown_files = view.files.len().min(limits.max_files);
    let mut lines = if shown_files < total_files {
        vec![format!(
            "{total} references across {total_files} files (showing {shown_files})"
        )]
    } else {
        vec![format!("{total} references in {total_files} files")]
    };
    if view.total_refs > 100 && name.len() <= 4 {
        lines.push(format!(
            "Note: '{}' is a very common identifier — results may include unrelated symbols. \
             Add path or symbol_kind to scope the search.",
            name
        ));
    }
    lines.push(String::new()); // blank line

    let mut total_emitted = 0usize;
    for file in view.files.iter().take(limits.max_files) {
        if total_emitted >= limits.total_hits {
            break;
        }
        lines.push(file.file_path.clone());
        let mut hit_count = 0usize;
        let mut truncated_hits = 0usize;
        for hit in &file.hits {
            if hit_count >= limits.max_per_file || total_emitted >= limits.total_hits {
                truncated_hits += 1;
                continue;
            }
            for line in &hit.context_lines {
                if line.is_reference_line {
                    if let Some(annotation) = &line.enclosing_annotation {
                        lines.push(format!(
                            "  {}: {:<40}{}",
                            line.line_number, line.text, annotation
                        ));
                    } else {
                        lines.push(format!("  {}: {}", line.line_number, line.text));
                    }
                } else {
                    lines.push(format!("  {}: {}", line.line_number, line.text));
                }
            }
            hit_count += 1;
            total_emitted += 1;
        }
        if truncated_hits > 0 {
            lines.push(format!("  ... and {truncated_hits} more references"));
        }
        lines.push(String::new()); // blank line between files
    }

    let remaining_files = total_files.saturating_sub(shown_files);
    if remaining_files > 0 {
        lines.push(format!("... and {remaining_files} more files"));
    }

    while lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        lines.pop();
    }

    lines.join("\n")
}

/// Render a compact find_references result: file:line [kind] in symbol — no source text.
pub fn find_references_compact_view(
    view: &FindReferencesView,
    name: &str,
    limits: &OutputLimits,
) -> String {
    if view.total_refs == 0 {
        return format!("No references found for \"{name}\"");
    }

    let total_files = view.total_files;
    let shown_files = view.files.len().min(limits.max_files);
    let mut lines = if shown_files < total_files {
        vec![format!(
            "{} references to \"{}\" across {} files (showing {})",
            view.total_refs, name, total_files, shown_files
        )]
    } else {
        vec![format!(
            "{} references to \"{}\" in {} files",
            view.total_refs, name, total_files
        )]
    };
    if view.total_refs > 100 && name.len() <= 4 {
        lines.push(format!(
            "Note: '{}' is a very common identifier — results may include unrelated symbols. \
             Add path or symbol_kind to scope the search.",
            name
        ));
    }

    let mut total_emitted = 0usize;
    for file in view.files.iter().take(limits.max_files) {
        if total_emitted >= limits.total_hits {
            break;
        }
        lines.push(file.file_path.clone());
        let mut hit_count = 0usize;
        let mut truncated_hits = 0usize;
        for hit in &file.hits {
            if hit_count >= limits.max_per_file || total_emitted >= limits.total_hits {
                truncated_hits += 1;
                continue;
            }
            for line in &hit.context_lines {
                if line.is_reference_line {
                    let annotation = line.enclosing_annotation.as_deref().unwrap_or("");
                    lines.push(format!("  :{} {}", line.line_number, annotation));
                }
            }
            hit_count += 1;
            total_emitted += 1;
        }
        if truncated_hits > 0 {
            lines.push(format!("  ... and {truncated_hits} more"));
        }
    }

    let remaining_files = total_files.saturating_sub(shown_files);
    if remaining_files > 0 {
        lines.push(format!("... and {remaining_files} more files"));
    }

    lines.join("\n")
}

/// Format results of `find_implementations`.
pub fn find_implementations_result_view(
    view: &FindImplementationsView,
    name: &str,
    limits: &OutputLimits,
) -> String {
    if view.entries.is_empty() {
        return format!("No implementations found for \"{name}\"");
    }

    let total = view.entries.len();
    let shown = total.min(limits.max_files * limits.max_per_file);
    let mut lines = vec![format!("{total} implementation(s) found for \"{name}\"")];
    lines.push(String::new());

    // Group by trait name for readable output
    let mut current_trait: Option<&str> = None;
    for (i, entry) in view.entries.iter().enumerate() {
        if i >= shown {
            break;
        }
        if current_trait != Some(&entry.trait_name) {
            if current_trait.is_some() {
                lines.push(String::new());
            }
            lines.push(format!("trait/interface {}:", entry.trait_name));
            current_trait = Some(&entry.trait_name);
        }
        lines.push(format!(
            "  {} ({}:{})",
            entry.implementor,
            entry.file_path,
            entry.line + 1
        ));
    }

    let remaining = total.saturating_sub(shown);
    if remaining > 0 {
        lines.push(String::new());
        lines.push(format!("... and {remaining} more"));
    }

    lines.join("\n")
}

/// Find all files that import (depend on) the given path.
///
/// Output format: compact list grouped by importing file, each with import line.
pub fn find_dependents_result(index: &LiveIndex, path: &str) -> String {
    let view = index.capture_find_dependents_view(path);
    find_dependents_result_view(&view, path, &OutputLimits::default())
}

pub fn find_dependents_result_view(
    view: &FindDependentsView,
    path: &str,
    limits: &OutputLimits,
) -> String {
    if view.files.is_empty() {
        return format!("No dependents found for \"{path}\"");
    }

    let total_files = view.files.len();
    let shown_files = total_files.min(limits.max_files);
    let mut lines = vec![format!("{total_files} files depend on {path}")];
    lines.push(String::new()); // blank line

    for file in view.files.iter().take(limits.max_files) {
        lines.push(file.file_path.clone());
        let total_refs = file.lines.len();
        let shown_refs = total_refs.min(limits.max_per_file);
        for line in file.lines.iter().take(limits.max_per_file) {
            lines.push(format!(
                "  {}: {}   [{}]",
                line.line_number, line.line_content, line.kind
            ));
        }
        let remaining_refs = total_refs.saturating_sub(shown_refs);
        if remaining_refs > 0 {
            lines.push(format!("  ... and {remaining_refs} more references"));
        }
        lines.push(String::new()); // blank line between files
    }

    let remaining_files = total_files.saturating_sub(shown_files);
    if remaining_files > 0 {
        lines.push(format!("... and {remaining_files} more files"));
    }

    // Remove trailing blank line
    while lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        lines.pop();
    }

    lines.join("\n")
}

/// Render a compact find_dependents result: file:line [kind] without source text.
pub fn find_dependents_compact_view(
    view: &FindDependentsView,
    path: &str,
    limits: &OutputLimits,
) -> String {
    if view.files.is_empty() {
        return format!("No dependents found for \"{path}\"");
    }

    let total_files = view.files.len();
    let shown_files = total_files.min(limits.max_files);
    let mut lines = vec![format!("{total_files} files depend on {path}")];

    for file in view.files.iter().take(limits.max_files) {
        let total_refs = file.lines.len();
        let shown_refs = total_refs.min(limits.max_per_file);
        let kinds: Vec<&str> = file
            .lines
            .iter()
            .take(limits.max_per_file)
            .map(|l| l.kind.as_str())
            .collect();
        let summary = if kinds.is_empty() {
            file.file_path.clone()
        } else {
            let unique_kinds: Vec<&str> = {
                let mut k = kinds.clone();
                k.sort_unstable();
                k.dedup();
                k
            };
            format!(
                "  {}  ({} refs: {})",
                file.file_path,
                total_refs,
                unique_kinds.join(", ")
            )
        };
        lines.push(summary);
        let remaining = total_refs.saturating_sub(shown_refs);
        if remaining > 0 {
            // still count but don't show individual lines
        }
    }

    let remaining_files = total_files.saturating_sub(shown_files);
    if remaining_files > 0 {
        lines.push(format!("... and {remaining_files} more files"));
    }

    lines.join("\n")
}

/// Render a find_dependents result as a Mermaid flowchart.
pub fn find_dependents_mermaid(
    view: &FindDependentsView,
    path: &str,
    limits: &OutputLimits,
) -> String {
    if view.files.is_empty() {
        return format!("No dependents found for \"{path}\"");
    }

    let mut lines = vec!["flowchart LR".to_string()];
    let target_id = mermaid_node_id(path);
    lines.push(format!("    {target_id}[\"{path}\"]"));

    for file in view.files.iter().take(limits.max_files) {
        let dep_id = mermaid_node_id(&file.file_path);
        let ref_count = file.lines.len().min(limits.max_per_file);
        lines.push(format!(
            "    {dep_id}[\"{}\"] -->|{} refs| {target_id}",
            file.file_path, ref_count
        ));
    }

    let remaining = view.files.len().saturating_sub(limits.max_files);
    if remaining > 0 {
        lines.push(format!(
            "    more[\"... and {remaining} more files\"] --> {target_id}"
        ));
    }

    lines.join("\n")
}

/// Render a find_dependents result as a Graphviz DOT digraph.
pub fn find_dependents_dot(view: &FindDependentsView, path: &str, limits: &OutputLimits) -> String {
    if view.files.is_empty() {
        return format!("No dependents found for \"{path}\"");
    }

    let mut lines = vec!["digraph dependents {".to_string()];
    lines.push("    rankdir=LR;".to_string());
    lines.push(format!(
        "    \"{}\" [shape=box, style=bold];",
        dot_escape(path)
    ));

    for file in view.files.iter().take(limits.max_files) {
        let ref_count = file.lines.len().min(limits.max_per_file);
        lines.push(format!(
            "    \"{}\" -> \"{}\" [label=\"{} refs\"];",
            dot_escape(&file.file_path),
            dot_escape(path),
            ref_count
        ));
    }

    let remaining = view.files.len().saturating_sub(limits.max_files);
    if remaining > 0 {
        lines.push(format!(
            "    \"... and {} more\" -> \"{}\" [style=dashed];",
            remaining,
            dot_escape(path)
        ));
    }

    lines.push("}".to_string());
    lines.join("\n")
}

/// Sanitize a file path into a valid Mermaid node ID (alphanumeric + underscores).
fn mermaid_node_id(path: &str) -> String {
    path.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

/// Escape a string for DOT label/node usage.
fn dot_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Get full context bundle for a symbol: definition body + callers + callees + type usages.
///
/// Each section is capped at 20 entries with "...and N more" overflow.
pub fn context_bundle_result(
    index: &LiveIndex,
    path: &str,
    name: &str,
    kind_filter: Option<&str>,
) -> String {
    let view = index.capture_context_bundle_view(path, name, kind_filter, None);
    context_bundle_result_view(&view, "full")
}

pub fn context_bundle_result_view(view: &ContextBundleView, verbosity: &str) -> String {
    match view {
        ContextBundleView::FileNotFound { path } => not_found_file(path),
        ContextBundleView::AmbiguousSymbol {
            path,
            name,
            candidate_lines,
        } => format!(
            "Ambiguous symbol selector for {name} in {path}; pass `symbol_line` to disambiguate. Candidates: {}",
            candidate_lines
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ContextBundleView::SymbolNotFound {
            relative_path,
            symbol_names,
            name,
        } => not_found_symbol_names(relative_path, symbol_names, name),
        ContextBundleView::Found(view) => render_context_bundle_found(view, verbosity),
    }
}

fn render_context_bundle_found(view: &ContextBundleFoundView, verbosity: &str) -> String {
    let body = apply_verbosity(&view.body, verbosity);
    let mut output = format!(
        "{}\n[{}, {}:{}-{}, {} bytes]\n",
        body,
        view.kind_label,
        view.file_path,
        view.line_range.0 + 1,
        view.line_range.1 + 1,
        view.byte_count
    );
    output.push_str(&format_context_bundle_section("Callers", &view.callers));
    output.push_str(&format_context_bundle_section("Callees", &view.callees));
    output.push_str(&format_context_bundle_section(
        "Type usages",
        &view.type_usages,
    ));
    if !view.dependencies.is_empty() {
        output.push_str(&format_type_dependencies(&view.dependencies));
    }
    // Hint: when a struct/enum has 0 callers, suggest looking at impl blocks instead.
    let is_struct_like = matches!(
        view.kind_label.as_str(),
        "struct" | "enum" | "class" | "interface" | "trait"
    );
    if is_struct_like && view.callers.total_count == 0 && view.callees.total_count == 0 {
        // Extract the type name from the body's first line. Handles:
        //   "pub struct Foo {" → "Foo"
        //   "struct Foo<T>" → "Foo"
        //   "pub(crate) struct Foo" → "Foo"
        // Falls back to "..." if extraction produces something garbled.
        let extracted_name = view
            .body
            .lines()
            .next()
            .and_then(|line| {
                // Find the keyword (struct/enum/class/trait/interface), take the token after it
                let words: Vec<&str> = line.split_whitespace().collect();
                let keyword_pos = words.iter().position(|w| {
                    matches!(*w, "struct" | "enum" | "class" | "trait" | "interface")
                })?;
                words.get(keyword_pos + 1).copied()
            })
            .map(|n| n.trim_end_matches(|c: char| !c.is_alphanumeric() && c != '_'))
            .filter(|n| !n.is_empty() && !n.contains('(') && !n.contains('#'))
            .unwrap_or("...");
        output.push_str(&format!(
            "\nTip: This {} has 0 direct callers/callees. Try `get_symbol_context` on its `impl` block or use `find_references(name=\"{}\")` to find usages.\n",
            view.kind_label, extracted_name
        ));
    }
    output
}

/// Format results of `trace_symbol`.
pub fn trace_symbol_result_view(
    view: &crate::live_index::TraceSymbolView,
    name: &str,
    verbosity: &str,
) -> String {
    match view {
        crate::live_index::TraceSymbolView::FileNotFound { path } => not_found_file(path),
        crate::live_index::TraceSymbolView::AmbiguousSymbol {
            path,
            name,
            candidate_lines,
        } => format!(
            "Ambiguous symbol selector for {name} in {path}; pass `symbol_line` to disambiguate. Candidates: {}",
            candidate_lines
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        crate::live_index::TraceSymbolView::SymbolNotFound {
            relative_path,
            symbol_names,
            name,
        } => not_found_symbol_names(relative_path, symbol_names, name),
        crate::live_index::TraceSymbolView::Found(found) => {
            let mut output = render_context_bundle_found(&found.context_bundle, verbosity);

            if !found.siblings.is_empty() {
                output.push_str(&format_siblings(&found.siblings));
            }

            if !found.dependents.files.is_empty() {
                output.push_str("\n\n");
                let dependents_fn = if verbosity == "full" {
                    find_dependents_result_view
                } else {
                    find_dependents_compact_view
                };
                output.push_str(&dependents_fn(
                    &found.dependents,
                    &found.context_bundle.file_path,
                    &OutputLimits::default(),
                ));
            }

            if !found.implementations.entries.is_empty() {
                output.push_str("\n\n");
                output.push_str(&find_implementations_result_view(
                    &found.implementations,
                    name,
                    &OutputLimits::default(),
                ));
            }

            if let Some(git) = &found.git_activity {
                output.push_str(&format_trace_git_activity(git));
            }

            output
        }
    }
}

fn format_siblings(siblings: &[crate::live_index::SiblingSymbolView]) -> String {
    let mut lines = vec!["\nNearby siblings:".to_string()];
    for sib in siblings {
        lines.push(format!(
            "  {:<12} {:<30} {}-{}",
            sib.kind_label, sib.name, sib.line_range.0, sib.line_range.1
        ));
    }
    lines.join("\n")
}

fn format_trace_git_activity(git: &crate::live_index::GitActivityView) -> String {
    let mut lines = vec![String::new()];
    lines.push(format!(
        "Git activity:  {} {:.2} ({})    {} commits, last {}",
        git.churn_bar, git.churn_score, git.churn_label, git.commit_count, git.last_relative,
    ));
    lines.push(format!(
        "  Last:  {} \"{}\" ({}, {})",
        git.last_hash, git.last_message, git.last_author, git.last_timestamp,
    ));
    if !git.owners.is_empty() {
        lines.push(format!("  Owners: {}", git.owners.join(", ")));
    }
    if !git.co_changes.is_empty() {
        lines.push("  Co-changes:".to_string());
        for (path, coupling, shared) in &git.co_changes {
            lines.push(format!(
                "    {}  ({:.2} coupling, {} shared commits)",
                path, coupling, shared,
            ));
        }
    }
    lines.join("\n")
}

/// Format results of `inspect_match`.
pub fn inspect_match_result_view(view: &InspectMatchView) -> String {
    match view {
        InspectMatchView::FileNotFound { path } => not_found_file(path),
        InspectMatchView::LineOutOfBounds {
            path,
            line,
            total_lines,
        } => {
            format!("Line {line} is out of bounds for {path} (file has {total_lines} lines).")
        }
        InspectMatchView::Found(found) => {
            let mut output = String::new();

            // 1. Excerpt
            output.push_str(&found.excerpt);
            output.push('\n');

            // 2. Enclosing symbol
            if let Some(enclosing) = &found.enclosing {
                output.push_str(&format_enclosing(enclosing));
            } else {
                output.push_str("\n(No enclosing symbol)");
            }

            // 3. Siblings
            if !found.siblings.is_empty() {
                output.push_str(&format_siblings(&found.siblings));
            }

            output
        }
    }
}

fn format_enclosing(enclosing: &crate::live_index::EnclosingSymbolView) -> String {
    format!(
        "\nEnclosing symbol: {} {} (lines {}-{})",
        enclosing.kind_label, enclosing.name, enclosing.line_range.0, enclosing.line_range.1
    )
}

fn format_context_bundle_section(title: &str, section: &ContextBundleSectionView) -> String {
    let mut lines = vec![format!("\n{title} ({}):", section.total_count)];

    let mut external_count = 0usize;

    for entry in &section.entries {
        if is_external_symbol(&entry.display_name, &entry.file_path) {
            external_count += 1;
        }
        if let Some(enclosing) = &entry.enclosing {
            lines.push(format!(
                "  {:<20} {}:{}  {}",
                entry.display_name, entry.file_path, entry.line_number, enclosing
            ));
        } else {
            lines.push(format!(
                "  {:<20} {}:{}",
                entry.display_name, entry.file_path, entry.line_number
            ));
        }
    }

    if section.overflow_count > 0 {
        // Estimate external ratio from shown entries and extrapolate
        let shown = section.entries.len();
        let est_external = if shown > 0 {
            (external_count as f64 / shown as f64 * section.overflow_count as f64).round() as usize
        } else {
            0
        };
        let est_project = section.overflow_count.saturating_sub(est_external);
        if est_external > 0 {
            lines.push(format!(
                "  ...and {} more {} ({} project, ~{} stdlib/framework)",
                section.overflow_count,
                title.to_lowercase(),
                est_project,
                est_external
            ));
        } else {
            lines.push(format!(
                "  ...and {} more {}",
                section.overflow_count,
                title.to_lowercase()
            ));
        }
    }

    lines.join("\n")
}

/// Heuristic: classify a symbol reference as external (stdlib/framework) vs project-defined.
fn is_external_symbol(name: &str, file_path: &str) -> bool {
    // No file path means it's a builtin/external
    if file_path.is_empty() {
        return true;
    }
    // Common stdlib/framework patterns across languages
    let external_prefixes = [
        "std::",
        "core::",
        "alloc::",
        "System.",
        "Microsoft.",
        "java.",
        "javax.",
        "kotlin.",
        "android.",
        "console.",
        "JSON.",
        "Math.",
        "Object.",
        "Array.",
        "String.",
        "Promise.",
        "Map.",
        "Set.",
        "Error.",
    ];
    for prefix in &external_prefixes {
        if name.starts_with(prefix) {
            return true;
        }
    }
    // Single-word lowercase names that are very common builtins
    let common_builtins = [
        "println",
        "print",
        "eprintln",
        "format",
        "vec",
        "to_string",
        "clone",
        "unwrap",
        "expect",
        "push",
        "pop",
        "len",
        "is_empty",
        "iter",
        "map",
        "filter",
        "collect",
        "into",
        "from",
        "default",
        "new",
        "Add",
        "Sub",
        "Display",
        "Debug",
        "ToString",
        "log",
        "warn",
        "error",
        "info",
        "LogWarning",
        "LogError",
        "LogInformation",
        "Console",
    ];
    common_builtins.contains(&name)
}

/// Extract the signature (first meaningful line) from a symbol body.
///
/// Handles common patterns: `fn foo(...)`, `pub struct Foo`, `class Bar`, etc.
/// Returns the first non-empty, non-comment line. If the body is a single line
/// or empty, returns it as-is.
fn extract_signature(body: &str) -> String {
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') {
            continue;
        }
        return line.to_string();
    }
    body.lines().next().unwrap_or("").to_string()
}

/// Extract the first doc-comment line from a symbol body.
///
/// Looks for `///`, `//!`, `/** ... */`, `# ...` (Python docstring-adjacent),
/// or `/* ... */` style comments immediately before/after the signature.
fn extract_first_doc_line(body: &str) -> Option<String> {
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Rust doc comments
        if let Some(rest) = trimmed.strip_prefix("///") {
            let doc = rest.trim();
            if !doc.is_empty() {
                return Some(doc.to_string());
            }
        }
        // Rust inner doc comments
        if let Some(rest) = trimmed.strip_prefix("//!") {
            let doc = rest.trim();
            if !doc.is_empty() {
                return Some(doc.to_string());
            }
        }
        // C-style block doc comments
        if let Some(rest) = trimmed.strip_prefix("/**") {
            let doc = rest.trim_end_matches("*/").trim();
            if !doc.is_empty() {
                return Some(doc.to_string());
            }
        }
        // XML doc comments (C#)
        if trimmed.starts_with("/// <summary>") || trimmed.starts_with("/// <remarks>") {
            continue; // skip XML tags, look for actual text
        }
        // Python/JS docstrings
        if trimmed.starts_with("\"\"\"") || trimmed.starts_with("'''") {
            let doc = trimmed
                .trim_start_matches("\"\"\"")
                .trim_start_matches("'''")
                .trim_end_matches("\"\"\"")
                .trim_end_matches("'''")
                .trim();
            if !doc.is_empty() {
                return Some(doc.to_string());
            }
        }
        // If we hit a non-comment line, stop looking
        if !trimmed.starts_with("//")
            && !trimmed.starts_with("/*")
            && !trimmed.starts_with('*')
            && !trimmed.starts_with('#')
        {
            break;
        }
    }
    None
}

/// Apply verbosity filter to a symbol body.
///
/// - `"signature"`: first meaningful line only (~80% smaller).
/// - `"compact"`: signature + first doc-comment line.
/// - `"full"` or anything else: complete body (default).
pub(crate) fn apply_verbosity(body: &str, verbosity: &str) -> String {
    match verbosity {
        "signature" => extract_signature(body),
        "compact" => {
            let sig = extract_signature(body);
            if let Some(doc) = extract_first_doc_line(body) {
                format!("{sig}\n  // {doc}")
            } else {
                sig
            }
        }
        _ => body.to_string(),
    }
}

fn format_type_dependencies(deps: &[TypeDependencyView]) -> String {
    let mut output = format!("\nDependencies ({}):", deps.len());
    for dep in deps {
        let depth_marker = if dep.depth > 0 {
            format!(" (depth {})", dep.depth)
        } else {
            String::new()
        };
        output.push_str(&format!(
            "\n── {} [{}, {}:{}-{}{}] ──\n{}",
            dep.name,
            dep.kind_label,
            dep.file_path,
            dep.line_range.0 + 1,
            dep.line_range.1 + 1,
            depth_marker,
            dep.body
        ));
    }
    output
}

/// "Index is loading... try again shortly."
pub fn loading_guard_message() -> String {
    "Index is loading... try again shortly.".to_string()
}

/// "Index not loaded. Call index_folder to index a directory."
pub fn empty_guard_message() -> String {
    "Index not loaded. Call index_folder to index a directory.".to_string()
}

/// Format a "Token Savings (this session)" section from a `StatsSnapshot`.
///
/// Input: `snap` — the `StatsSnapshot` from `TokenStats::summary()`.
/// Output: a multi-line string listing per-hook-type fire counts and token savings.
///
/// If all counters are zero, returns an empty string (no savings section shown).
/// This is a fail-open function — callers can append the result without checking emptiness.
///
/// ```text
/// ── Token Savings (this session) ──
/// Read:  N fires, ~M tokens saved
/// Edit:  N fires, ~M tokens saved
/// Write: N fires
/// Grep:  N fires, ~M tokens saved
/// Total: ~T tokens saved
/// ```
pub fn format_token_savings(snap: &crate::sidecar::StatsSnapshot) -> String {
    let total_saved = snap.read_saved_tokens + snap.edit_saved_tokens + snap.grep_saved_tokens;

    // Show section only when at least one hook has fired.
    let any_fires =
        snap.read_fires > 0 || snap.edit_fires > 0 || snap.write_fires > 0 || snap.grep_fires > 0;

    if !any_fires {
        return String::new();
    }

    let mut lines = vec!["── Token Savings (this session) ──".to_string()];

    if snap.read_fires > 0 {
        lines.push(format!(
            "Read:  {} fires, ~{} tokens saved",
            snap.read_fires, snap.read_saved_tokens
        ));
    }
    if snap.edit_fires > 0 {
        lines.push(format!(
            "Edit:  {} fires, ~{} tokens saved",
            snap.edit_fires, snap.edit_saved_tokens
        ));
    }
    if snap.write_fires > 0 {
        lines.push(format!("Write: {} fires", snap.write_fires));
    }
    if snap.grep_fires > 0 {
        lines.push(format!(
            "Grep:  {} fires, ~{} tokens saved",
            snap.grep_fires, snap.grep_saved_tokens
        ));
    }

    lines.push(format!("Total: ~{} tokens saved", total_saved));

    lines.join("\n")
}

/// Estimate tokens saved by a structured response vs raw file content.
/// Returns a one-line footer string, or empty string if no meaningful savings.
pub fn compact_savings_footer(response_chars: usize, raw_chars: usize) -> String {
    if raw_chars <= response_chars || raw_chars < 200 {
        return String::new();
    }
    // Rough token estimate: ~4 chars per token for code
    let response_tokens = response_chars / 4;
    let raw_tokens = raw_chars / 4;
    let saved = raw_tokens.saturating_sub(response_tokens);
    if saved < 50 {
        return String::new();
    }
    format!("\n\n~{saved} tokens saved vs raw file read")
}

/// Format a one-line git temporal summary for the health report.
pub fn git_temporal_health_line(
    temporal: &crate::live_index::git_temporal::GitTemporalIndex,
) -> String {
    use crate::live_index::git_temporal::GitTemporalState;

    match &temporal.state {
        GitTemporalState::Pending => "Git temporal: pending".to_string(),
        GitTemporalState::Computing => "Git temporal: computing...".to_string(),
        GitTemporalState::Unavailable(reason) => {
            format!("Git temporal: unavailable ({reason})")
        }
        GitTemporalState::Ready => {
            let stats = &temporal.stats;
            let mut lines = vec![format!(
                "Git temporal: ready ({} commits over {}d, computed in {}ms)",
                stats.total_commits_analyzed,
                stats.analysis_window_days,
                stats.compute_duration.as_millis(),
            )];

            if !stats.hotspots.is_empty() {
                let top: Vec<String> = stats
                    .hotspots
                    .iter()
                    .take(5)
                    .map(|(path, score)| format!("{path} ({score:.2})"))
                    .collect();
                lines.push(format!("  Hotspots: {}", top.join(", ")));
            }

            if !stats.most_coupled.is_empty() {
                let (a, b, score) = &stats.most_coupled[0];
                lines.push(format!(
                    "  Strongest coupling: {a} \u{2194} {b} ({score:.2})"
                ));
            }

            lines.join("\n")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{LanguageId, SymbolKind, SymbolRecord};
    use crate::live_index::store::{CircuitBreakerState, IndexedFile, LiveIndex, ParseStatus};
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    // --- Test helpers ---

    fn make_symbol(
        name: &str,
        kind: SymbolKind,
        depth: u32,
        line_start: u32,
        line_end: u32,
    ) -> SymbolRecord {
        SymbolRecord {
            name: name.to_string(),
            kind,
            depth,
            sort_order: 0,
            byte_range: (0, 10),
            line_range: (line_start, line_end),
            doc_byte_range: None,
        }
    }

    fn make_symbol_with_bytes(
        name: &str,
        kind: SymbolKind,
        depth: u32,
        line_start: u32,
        line_end: u32,
        byte_start: u32,
        byte_end: u32,
    ) -> SymbolRecord {
        SymbolRecord {
            name: name.to_string(),
            kind,
            depth,
            sort_order: 0,
            byte_range: (byte_start, byte_end),
            line_range: (line_start, line_end),
            doc_byte_range: None,
        }
    }

    fn make_file(path: &str, content: &[u8], symbols: Vec<SymbolRecord>) -> (String, IndexedFile) {
        (
            path.to_string(),
            IndexedFile {
                relative_path: path.to_string(),
                language: LanguageId::Rust,
                classification: crate::domain::FileClassification::for_code_path(path),
                content: content.to_vec(),
                symbols,
                parse_status: ParseStatus::Parsed,
                byte_len: content.len() as u64,
                content_hash: "test".to_string(),
                references: vec![],
                alias_map: std::collections::HashMap::new(),
            },
        )
    }

    fn make_index(files: Vec<(String, IndexedFile)>) -> LiveIndex {
        use crate::live_index::trigram::TrigramIndex;
        let cb = CircuitBreakerState::new(0.20);
        let files_map = files
            .into_iter()
            .map(|(path, file)| (path, std::sync::Arc::new(file)))
            .collect::<HashMap<_, _>>();
        let trigram_index = TrigramIndex::build_from_files(&files_map);
        let mut index = LiveIndex {
            files: files_map,
            loaded_at: Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: Duration::from_millis(42),
            cb_state: cb,
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

    fn empty_index() -> LiveIndex {
        make_index(vec![])
    }

    // --- file_outline tests ---

    #[test]
    fn test_file_outline_header_shows_path_and_count() {
        let (key, file) = make_file(
            "src/main.rs",
            b"fn main() {}",
            vec![make_symbol("main", SymbolKind::Function, 0, 1, 1)],
        );
        let index = make_index(vec![(key, file)]);
        let result = file_outline(&index, "src/main.rs");
        assert!(
            result.starts_with("src/main.rs  (1 symbols)"),
            "header should show path and count, got: {result}"
        );
    }

    #[test]
    fn test_file_outline_symbol_line_with_kind_and_range() {
        let (key, file) = make_file(
            "src/main.rs",
            b"fn main() {}",
            vec![make_symbol("main", SymbolKind::Function, 0, 0, 4)],
        );
        let index = make_index(vec![(key, file)]);
        let result = file_outline(&index, "src/main.rs");
        assert!(result.contains("fn"), "should contain fn kind");
        assert!(result.contains("main"), "should contain symbol name");
        assert!(result.contains("1-5"), "should contain 1-based line range");
    }

    #[test]
    fn test_file_outline_depth_indentation() {
        let symbols = vec![
            make_symbol("MyStruct", SymbolKind::Struct, 0, 1, 10),
            make_symbol("my_method", SymbolKind::Method, 1, 2, 5),
        ];
        let (key, file) = make_file(
            "src/lib.rs",
            b"struct MyStruct { fn my_method() {} }",
            symbols,
        );
        let index = make_index(vec![(key, file)]);
        let result = file_outline(&index, "src/lib.rs");
        let lines: Vec<&str> = result.lines().collect();
        // Method at depth 1 should be indented by 2 spaces
        let method_line = lines.iter().find(|l| l.contains("my_method")).unwrap();
        assert!(
            method_line.starts_with("  "),
            "depth-1 symbol should be indented by 2 spaces"
        );
    }

    #[test]
    fn test_file_outline_not_found() {
        let index = empty_index();
        let result = file_outline(&index, "nonexistent.rs");
        assert_eq!(result, "File not found: nonexistent.rs");
    }

    #[test]
    fn test_file_outline_empty_symbols() {
        let (key, file) = make_file("src/main.rs", b"", vec![]);
        let index = make_index(vec![(key, file)]);
        let result = file_outline(&index, "src/main.rs");
        assert!(result.contains("(0 symbols)"), "should show 0 symbols");
    }

    #[test]
    fn test_file_outline_view_matches_live_index_output() {
        let (key, file) = make_file(
            "src/main.rs",
            b"fn main() {}",
            vec![make_symbol("main", SymbolKind::Function, 0, 1, 5)],
        );
        let index = make_index(vec![(key, file)]);

        let live_result = file_outline(&index, "src/main.rs");
        let captured_result =
            file_outline_view(&index.capture_file_outline_view("src/main.rs").unwrap());

        assert_eq!(captured_result, live_result);
    }

    // --- symbol_detail tests ---

    #[test]
    fn test_symbol_detail_returns_body_and_footer() {
        let content = b"fn hello() { println!(\"hi\"); }";
        let sym = make_symbol_with_bytes("hello", SymbolKind::Function, 0, 0, 0, 0, 30);
        let (key, file) = make_file("src/lib.rs", content, vec![sym]);
        let index = make_index(vec![(key, file)]);
        let result = symbol_detail(&index, "src/lib.rs", "hello", None);
        assert!(result.contains("fn hello"), "should contain body");
        assert!(
            result.contains("[fn, lines 1-1, 30 bytes]"),
            "should contain footer (0-based line_range 0-0 displayed as 1-based 1-1)"
        );
    }

    #[test]
    fn test_symbol_detail_not_found_lists_available_symbols() {
        let sym = make_symbol("real_fn", SymbolKind::Function, 0, 1, 5);
        let (key, file) = make_file("src/lib.rs", b"fn real_fn() {}", vec![sym]);
        let index = make_index(vec![(key, file)]);
        let result = symbol_detail(&index, "src/lib.rs", "missing_fn", None);
        assert!(result.contains("No symbol missing_fn in src/lib.rs"));
        assert!(result.contains("real_fn"), "should list available symbols");
    }

    #[test]
    fn test_symbol_detail_file_not_found() {
        let index = empty_index();
        let result = symbol_detail(&index, "nonexistent.rs", "foo", None);
        assert_eq!(result, "File not found: nonexistent.rs");
    }

    #[test]
    fn test_symbol_detail_kind_filter_matches() {
        let symbols = vec![
            make_symbol("foo", SymbolKind::Function, 0, 0, 0),
            make_symbol("foo", SymbolKind::Struct, 0, 4, 9),
        ];
        let content = b"fn foo() {} struct foo {}";
        let (key, file) = make_file("src/lib.rs", content, symbols);
        let index = make_index(vec![(key, file)]);
        // Filter for struct kind (0-based 4-9 displays as 1-based 5-10)
        let result = symbol_detail(&index, "src/lib.rs", "foo", Some("struct"));
        assert!(
            result.contains("[struct, lines 5-10"),
            "footer should show struct kind"
        );
    }

    #[test]
    fn test_symbol_detail_view_matches_live_index_output() {
        let content = b"fn hello() { println!(\"hi\"); }";
        let sym = make_symbol_with_bytes("hello", SymbolKind::Function, 0, 1, 1, 0, 30);
        let (key, file) = make_file("src/lib.rs", content, vec![sym]);
        let index = make_index(vec![(key, file)]);

        let live_result = symbol_detail(&index, "src/lib.rs", "hello", None);
        let captured_result = symbol_detail_view(
            &index.capture_symbol_detail_view("src/lib.rs").unwrap(),
            "hello",
            None,
        );

        assert_eq!(captured_result, live_result);
    }

    #[test]
    fn test_code_slice_view_formats_path_and_slice_text() {
        let result = code_slice_view("src/lib.rs", b"fn foo()");
        assert_eq!(result, "src/lib.rs\nfn foo()");
    }

    #[test]
    fn test_code_slice_from_indexed_file_clamps_and_formats() {
        let (key, file) = make_file("src/lib.rs", b"fn foo() { bar(); }", vec![]);
        let index = make_index(vec![(key, file)]);

        let result = code_slice_from_indexed_file(
            index.capture_shared_file("src/lib.rs").unwrap().as_ref(),
            0,
            Some(200),
        );

        assert_eq!(result, "src/lib.rs\nfn foo() { bar(); }");
    }

    // --- search_symbols_result tests ---

    #[test]
    fn test_search_symbols_summary_header() {
        let symbols = vec![
            make_symbol("get_user", SymbolKind::Function, 0, 1, 5),
            make_symbol("get_role", SymbolKind::Function, 0, 6, 10),
        ];
        let (key, file) = make_file("src/lib.rs", b"fn get_user() {} fn get_role() {}", symbols);
        let index = make_index(vec![(key, file)]);
        let result = search_symbols_result(&index, "get");
        assert!(
            result.starts_with("2 matches in 1 files"),
            "should start with summary"
        );
    }

    #[test]
    fn test_search_symbols_case_insensitive() {
        let sym = make_symbol("GetUser", SymbolKind::Function, 0, 1, 5);
        let (key, file) = make_file("src/lib.rs", b"fn GetUser() {}", vec![sym]);
        let index = make_index(vec![(key, file)]);
        let result = search_symbols_result(&index, "getuser");
        assert!(
            !result.starts_with("No symbols"),
            "should find case-insensitive match"
        );
    }

    #[test]
    fn test_search_symbols_no_match() {
        let sym = make_symbol("unrelated", SymbolKind::Function, 0, 1, 5);
        let (key, file) = make_file("src/lib.rs", b"fn unrelated() {}", vec![sym]);
        let index = make_index(vec![(key, file)]);
        let result = search_symbols_result(&index, "xyz_no_match");
        assert_eq!(result, "No symbols matching 'xyz_no_match'");
    }

    #[test]
    fn test_search_symbols_result_view_matches_live_index_output() {
        let symbols = vec![
            make_symbol("get_user", SymbolKind::Function, 0, 1, 5),
            make_symbol("get_role", SymbolKind::Function, 0, 6, 10),
        ];
        let (key, file) = make_file("src/lib.rs", b"fn get_user() {} fn get_role() {}", symbols);
        let index = make_index(vec![(key, file)]);

        let live_result = search_symbols_result(&index, "get");
        let captured_result =
            search_symbols_result_view(&search::search_symbols(&index, "get", None, 50), "get");

        assert_eq!(captured_result, live_result);
    }

    #[test]
    fn test_search_symbols_grouped_by_file() {
        let sym1 = make_symbol("foo", SymbolKind::Function, 0, 1, 5);
        let sym2 = make_symbol("foo_bar", SymbolKind::Function, 0, 1, 5);
        let (key1, file1) = make_file("a.rs", b"fn foo() {}", vec![sym1]);
        let (key2, file2) = make_file("b.rs", b"fn foo_bar() {}", vec![sym2]);
        let index = make_index(vec![(key1, file1), (key2, file2)]);
        let result = search_symbols_result(&index, "foo");
        assert!(
            result.contains("2 matches in 2 files"),
            "should show 2 files"
        );
        assert!(result.contains("a.rs"), "should contain file a.rs");
        assert!(result.contains("b.rs"), "should contain file b.rs");
    }

    #[test]
    fn test_search_symbols_kind_filter_limits_results() {
        let function = make_symbol("JobRunner", SymbolKind::Function, 0, 1, 5);
        let class = make_symbol("Job", SymbolKind::Class, 0, 6, 10);
        let (key, file) = make_file(
            "src/lib.rs",
            b"fn JobRunner() {} struct Job {}",
            vec![function, class],
        );
        let index = make_index(vec![(key, file)]);
        let result = search_symbols_result_with_kind(&index, "job", Some("class"));
        assert!(
            result.contains("class Job"),
            "class result should remain visible: {result}"
        );
        assert!(
            !result.contains("fn JobRunner"),
            "function result should be filtered out: {result}"
        );
    }

    // --- search_text_result tests ---

    #[test]
    fn test_search_text_summary_header() {
        let (key, file) = make_file("src/lib.rs", b"let x = 1;\nlet y = 2;", vec![]);
        let index = make_index(vec![(key, file)]);
        let result = search_text_result(&index, "let");
        assert!(result.starts_with("2 matches in 1 files"), "got: {result}");
    }

    #[test]
    fn test_search_text_shows_line_numbers() {
        let content = b"line one\nline two\nline three";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let index = make_index(vec![(key, file)]);
        let result = search_text_result(&index, "line two");
        assert!(
            result.contains("  2:"),
            "should show 1-indexed line number 2"
        );
    }

    #[test]
    fn test_search_text_case_insensitive() {
        let (key, file) = make_file("src/lib.rs", b"Hello World", vec![]);
        let index = make_index(vec![(key, file)]);
        let result = search_text_result(&index, "hello world");
        assert!(
            !result.starts_with("No matches"),
            "should find case-insensitive"
        );
    }

    #[test]
    fn test_search_text_no_match() {
        let (key, file) = make_file("src/lib.rs", b"fn main() {}", vec![]);
        let index = make_index(vec![(key, file)]);
        let result = search_text_result(&index, "xyz_totally_absent");
        assert_eq!(result, "No matches for 'xyz_totally_absent'");
    }

    #[test]
    fn test_search_text_result_view_matches_live_index_output() {
        let (key, file) = make_file("src/lib.rs", b"let x = 1;\nlet y = 2;", vec![]);
        let index = make_index(vec![(key, file)]);

        let live_result = search_text_result(&index, "let");
        let captured_result =
            search_text_result_view(search::search_text(&index, Some("let"), None, false), None);

        assert_eq!(captured_result, live_result);
    }

    #[test]
    fn test_search_text_crlf_handling() {
        let content = b"fn foo() {\r\n    let x = 1;\r\n}";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let index = make_index(vec![(key, file)]);
        let result = search_text_result(&index, "let x");
        assert!(
            result.contains("let x = 1"),
            "should find content without \\r"
        );
    }

    #[test]
    fn test_search_text_terms_or_matches_multiple_needles() {
        let (key, file) = make_file(
            "src/lib.rs",
            b"// TODO: first\n// FIXME: second\n// NOTE: ignored",
            vec![],
        );
        let index = make_index(vec![(key, file)]);
        let terms = vec!["TODO".to_string(), "FIXME".to_string()];
        let result = search_text_result_with_options(&index, None, Some(&terms), false);
        assert!(
            result.contains("TODO: first"),
            "TODO line should match: {result}"
        );
        assert!(
            result.contains("FIXME: second"),
            "FIXME line should match: {result}"
        );
        assert!(
            !result.contains("NOTE: ignored"),
            "non-matching line should be absent: {result}"
        );
    }

    #[test]
    fn test_search_text_regex_mode_matches_pattern() {
        let (key, file) = make_file(
            "src/lib.rs",
            b"// TODO: first\n// FIXME: second\n// NOTE: ignored",
            vec![],
        );
        let index = make_index(vec![(key, file)]);
        let result = search_text_result_with_options(&index, Some("TODO|FIXME"), None, true);
        assert!(
            result.contains("TODO: first"),
            "TODO line should match regex: {result}"
        );
        assert!(
            result.contains("FIXME: second"),
            "FIXME line should match regex: {result}"
        );
        assert!(
            !result.contains("NOTE: ignored"),
            "non-matching line should be absent: {result}"
        );
    }

    #[test]
    fn test_search_text_result_view_renders_context_windows_with_separators() {
        let (key, file) = make_file(
            "src/lib.rs",
            b"line 1\nline 2\nneedle 3\nline 4\nneedle 5\nline 6\nline 7\nline 8\nneedle 9\nline 10\n",
            vec![],
        );
        let index = make_index(vec![(key, file)]);
        let result = search::search_text_with_options(
            &index,
            Some("needle"),
            None,
            false,
            &search::TextSearchOptions {
                context: Some(1),
                ..search::TextSearchOptions::for_current_code_search()
            },
        );

        let rendered = search_text_result_view(result, None);

        assert!(
            rendered.contains("src/lib.rs"),
            "file header missing: {rendered}"
        );
        assert!(
            rendered.contains("  2: line 2"),
            "context line missing: {rendered}"
        );
        assert!(
            rendered.contains("> 3: needle 3"),
            "match marker missing: {rendered}"
        );
        assert!(
            rendered.contains("  ..."),
            "window separator missing: {rendered}"
        );
        assert!(
            rendered.contains("> 9: needle 9"),
            "later match missing: {rendered}"
        );
    }

    // --- repo_outline tests ---

    #[test]
    fn test_repo_outline_header_totals() {
        let sym = make_symbol("main", SymbolKind::Function, 0, 1, 5);
        let (key, file) = make_file("src/main.rs", b"fn main() {}", vec![sym]);
        let index = make_index(vec![(key, file)]);
        let result = repo_outline(&index, "myproject");
        assert!(
            result.starts_with("myproject  (1 files, 1 symbols)"),
            "got: {result}"
        );
    }

    #[test]
    fn test_repo_outline_shows_filename_language_count() {
        let sym = make_symbol("foo", SymbolKind::Function, 0, 1, 3);
        let (key, file) = make_file("src/lib.rs", b"fn foo() {}", vec![sym]);
        let index = make_index(vec![(key, file)]);
        let result = repo_outline(&index, "proj");
        assert!(result.contains("lib.rs"), "should show filename");
        assert!(result.contains("Rust"), "should show language");
        assert!(result.contains("1 symbols"), "should show symbol count");
    }

    #[test]
    fn test_repo_outline_repeated_basenames_use_shortest_unique_suffixes() {
        let sym = make_symbol("foo", SymbolKind::Function, 0, 1, 3);
        let index = make_index(vec![
            make_file("src/live_index/mod.rs", b"fn foo() {}", vec![sym.clone()]),
            make_file("src/protocol/mod.rs", b"fn foo() {}", vec![sym.clone()]),
            make_file("src/parsing/languages/mod.rs", b"fn foo() {}", vec![sym]),
        ]);

        let result = repo_outline(&index, "proj");

        assert!(result.contains("live_index/mod.rs"), "got: {result}");
        assert!(result.contains("protocol/mod.rs"), "got: {result}");
        assert!(result.contains("languages/mod.rs"), "got: {result}");
        assert!(!result.contains("\n  mod.rs"), "got: {result}");
    }

    #[test]
    fn test_repo_outline_deeper_collisions_expand_beyond_one_parent() {
        let sym = make_symbol("foo", SymbolKind::Function, 0, 1, 3);
        let index = make_index(vec![
            make_file("src/alpha/shared/mod.rs", b"fn foo() {}", vec![sym.clone()]),
            make_file("tests/beta/shared/mod.rs", b"fn foo() {}", vec![sym]),
        ]);

        let result = repo_outline(&index, "proj");

        assert!(result.contains("alpha/shared/mod.rs"), "got: {result}");
        assert!(result.contains("beta/shared/mod.rs"), "got: {result}");
    }

    #[test]
    fn test_repo_outline_view_matches_live_index_output() {
        let alpha = make_symbol("alpha", SymbolKind::Function, 0, 1, 3);
        let beta = make_symbol("beta", SymbolKind::Function, 0, 5, 7);
        let (k1, f1) = make_file("src/zeta.rs", b"fn beta() {}", vec![beta]);
        let (k2, f2) = make_file("src/alpha.rs", b"fn alpha() {}", vec![alpha]);
        let index = make_index(vec![(k1, f1), (k2, f2)]);

        let live_result = repo_outline(&index, "proj");
        let captured_result = repo_outline_view(&index.capture_repo_outline_view(), "proj");

        assert_eq!(captured_result, live_result);
    }

    // --- health_report tests ---

    #[test]
    fn test_health_report_ready_state() {
        let sym = make_symbol("foo", SymbolKind::Function, 0, 1, 3);
        let (key, file) = make_file("src/lib.rs", b"fn foo() {}", vec![sym]);
        let index = make_index(vec![(key, file)]);
        let result = health_report(&index);
        assert!(result.contains("Status: Ready"), "got: {result}");
        assert!(result.contains("Files:"), "should have Files line");
        assert!(result.contains("Symbols:"), "should have Symbols line");
        assert!(result.contains("Loaded in:"), "should have Loaded in line");
        assert!(
            result.contains("Watcher: off"),
            "should have Watcher: off line (no watcher active)"
        );
    }

    #[test]
    fn test_health_report_empty_state() {
        let index = LiveIndex {
            files: HashMap::new(),
            loaded_at: Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: Duration::from_millis(0),
            cb_state: CircuitBreakerState::new(0.20),
            is_empty: true,
            load_source: crate::live_index::store::IndexLoadSource::EmptyBootstrap,
            snapshot_verify_state: crate::live_index::store::SnapshotVerifyState::NotNeeded,
            reverse_index: HashMap::new(),
            files_by_basename: HashMap::new(),
            files_by_dir_component: HashMap::new(),
            trigram_index: crate::live_index::trigram::TrigramIndex::new(),
        };
        let result = health_report(&index);
        assert!(result.contains("Status: Empty"), "got: {result}");
    }

    #[test]
    fn test_health_report_shows_watcher_off() {
        // health_report with no watcher active should show "Watcher: off"
        let index = make_index(vec![]);
        let result = health_report(&index);
        assert!(result.contains("Watcher: off"), "got: {result}");
        assert!(
            !result.contains("events"),
            "off watcher should not mention events"
        );
    }

    #[test]
    fn test_health_report_shows_watcher_active() {
        use crate::watcher::{WatcherInfo, WatcherState};
        // Verify health_stats_with_watcher populates Active state correctly;
        // we test the stats fields here since health_report calls health_stats() not
        // health_stats_with_watcher(). The format function is fully tested via the
        // watcher_state field on HealthStats.
        let index = make_index(vec![]);
        let watcher = WatcherInfo {
            state: WatcherState::Active,
            events_processed: 7,
            last_event_at: None,
            debounce_window_ms: 200,
        };
        let stats = index.health_stats_with_watcher(&watcher);
        assert_eq!(stats.watcher_state, WatcherState::Active);
        assert_eq!(stats.events_processed, 7);
    }

    #[test]
    fn test_health_report_from_stats_matches_live_index_output() {
        use crate::watcher::{WatcherInfo, WatcherState};

        let sym = make_symbol("foo", SymbolKind::Function, 0, 1, 3);
        let (key, file) = make_file("src/lib.rs", b"fn foo() {}", vec![sym]);
        let index = make_index(vec![(key, file)]);
        let watcher = WatcherInfo {
            state: WatcherState::Active,
            events_processed: 7,
            last_event_at: None,
            debounce_window_ms: 200,
        };

        let live_result = health_report_with_watcher(&index, &watcher);
        let captured_result =
            health_report_from_stats("Ready", &index.health_stats_with_watcher(&watcher));

        assert_eq!(captured_result, live_result);
    }

    #[test]
    fn test_health_report_from_published_state_matches_live_index_output() {
        use crate::watcher::{WatcherInfo, WatcherState};

        let sym = make_symbol("foo", SymbolKind::Function, 0, 1, 3);
        let (key, file) = make_file("src/lib.rs", b"fn foo() {}", vec![sym]);
        let index = make_index(vec![(key, file)]);
        let watcher = WatcherInfo {
            state: WatcherState::Active,
            events_processed: 7,
            last_event_at: None,
            debounce_window_ms: 200,
        };

        let live_result = health_report_with_watcher(&index, &watcher);
        let shared = crate::live_index::SharedIndexHandle::shared(index);
        let captured_result =
            health_report_from_published_state(&shared.published_state(), &watcher);

        assert_eq!(captured_result, live_result);
    }

    // --- what_changed_result tests ---

    #[test]
    fn test_what_changed_since_far_past_lists_all_files() {
        let sym = make_symbol("foo", SymbolKind::Function, 0, 1, 3);
        let (key, file) = make_file("src/lib.rs", b"fn foo() {}", vec![sym]);
        let index = make_index(vec![(key, file)]);
        // since_ts=0 (epoch) is before index was loaded
        let result = what_changed_result(&index, 0);
        assert!(
            result.contains("src/lib.rs"),
            "should list all files: {result}"
        );
    }

    #[test]
    fn test_what_changed_since_far_future_returns_no_changes() {
        let (key, file) = make_file("src/lib.rs", b"fn foo() {}", vec![]);
        let index = make_index(vec![(key, file)]);
        // since_ts=far future — no changes
        let result = what_changed_result(&index, i64::MAX);
        assert_eq!(result, "No changes detected since last index load.");
    }

    #[test]
    fn test_what_changed_timestamp_view_matches_live_index_output() {
        let (k1, f1) = make_file("src/z.rs", b"fn z() {}", vec![]);
        let (k2, f2) = make_file("src/a.rs", b"fn a() {}", vec![]);
        let index = make_index(vec![(k1, f1), (k2, f2)]);

        let live_result = what_changed_result(&index, 0);
        let captured_result =
            what_changed_timestamp_view(&index.capture_what_changed_timestamp_view(), 0);

        assert_eq!(captured_result, live_result);
    }

    #[test]
    fn test_what_changed_paths_result_sorts_and_deduplicates() {
        let result = what_changed_paths_result(
            &[
                "src\\b.rs".to_string(),
                "src/a.rs".to_string(),
                "src/a.rs".to_string(),
            ],
            "No git changes detected.",
        );
        assert_eq!(result, "src/a.rs\nsrc/b.rs");
    }

    #[test]
    fn test_resolve_path_result_view_returns_exact_path() {
        let view = ResolvePathView::Resolved {
            path: "src/protocol/tools.rs".to_string(),
        };

        assert_eq!(resolve_path_result_view(&view), "src/protocol/tools.rs");
    }

    #[test]
    fn test_resolve_path_result_view_formats_ambiguous_output() {
        let view = ResolvePathView::Ambiguous {
            hint: "lib.rs".to_string(),
            matches: vec!["src/lib.rs".to_string(), "tests/lib.rs".to_string()],
            overflow_count: 1,
        };

        let result = resolve_path_result_view(&view);

        assert!(result.contains("Ambiguous path hint 'lib.rs' (3 matches)"));
        assert!(result.contains("  src/lib.rs"));
        assert!(result.contains("  tests/lib.rs"));
        assert!(result.contains("  ... and 1 more"));
    }

    #[test]
    fn test_resolve_path_result_view_not_found() {
        let view = ResolvePathView::NotFound {
            hint: "README.md".to_string(),
        };

        assert_eq!(
            resolve_path_result_view(&view),
            "No indexed source path matched 'README.md'"
        );
    }

    #[test]
    fn test_search_files_result_view_groups_ranked_paths() {
        let view = SearchFilesView::Found {
            query: "tools.rs".to_string(),
            total_matches: 3,
            overflow_count: 1,
            hits: vec![
                crate::live_index::SearchFilesHit {
                    tier: SearchFilesTier::StrongPath,
                    path: "src/protocol/tools.rs".to_string(),
                    coupling_score: None,
                    shared_commits: None,
                },
                crate::live_index::SearchFilesHit {
                    tier: SearchFilesTier::Basename,
                    path: "src/sidecar/tools.rs".to_string(),
                    coupling_score: None,
                    shared_commits: None,
                },
                crate::live_index::SearchFilesHit {
                    tier: SearchFilesTier::LoosePath,
                    path: "src/protocol/tools_helper.rs".to_string(),
                    coupling_score: None,
                    shared_commits: None,
                },
            ],
        };

        let result = search_files_result_view(&view);

        assert!(result.contains("3 matching files"));
        assert!(result.contains("── Strong path matches ──"));
        assert!(result.contains("  src/protocol/tools.rs"));
        assert!(result.contains("── Basename matches ──"));
        assert!(result.contains("  src/sidecar/tools.rs"));
        assert!(result.contains("── Loose path matches ──"));
        assert!(result.contains("  src/protocol/tools_helper.rs"));
        assert!(result.contains("... and 1 more"));
    }

    #[test]
    fn test_search_files_result_view_not_found() {
        let view = SearchFilesView::NotFound {
            query: "README.md".to_string(),
        };

        assert_eq!(
            search_files_result_view(&view),
            "No indexed source files matching 'README.md'"
        );
    }

    // --- file_content tests ---

    #[test]
    fn test_file_content_full() {
        let content = b"fn main() {\n    println!(\"hi\");\n}";
        let (key, file) = make_file("src/main.rs", content, vec![]);
        let index = make_index(vec![(key, file)]);
        let result = file_content(&index, "src/main.rs", None, None);
        assert!(result.contains("fn main()"), "should return full content");
        assert!(result.contains("println!"), "should return full content");
    }

    #[test]
    fn test_file_content_line_range() {
        let content = b"line 1\nline 2\nline 3\nline 4\nline 5";
        let (key, file) = make_file("src/main.rs", content, vec![]);
        let index = make_index(vec![(key, file)]);
        // Lines 2-4 (1-indexed)
        let result = file_content(&index, "src/main.rs", Some(2), Some(4));
        assert!(!result.contains("line 1"), "should not include line 1");
        assert!(result.contains("line 2"), "should include line 2");
        assert!(result.contains("line 3"), "should include line 3");
        assert!(result.contains("line 4"), "should include line 4");
        assert!(!result.contains("line 5"), "should not include line 5");
    }

    #[test]
    fn test_file_content_not_found() {
        let index = empty_index();
        let result = file_content(&index, "nonexistent.rs", None, None);
        assert_eq!(result, "File not found: nonexistent.rs");
    }

    #[test]
    fn test_file_outline_from_indexed_file_matches_live_index_output() {
        let (key, file) = make_file(
            "src/main.rs",
            b"fn main() {}",
            vec![
                make_symbol("main", SymbolKind::Function, 0, 0, 0),
                make_symbol("helper", SymbolKind::Function, 1, 1, 1),
            ],
        );
        let index = make_index(vec![(key, file)]);

        let live_result = file_outline(&index, "src/main.rs");
        let shared_result = file_outline_from_indexed_file(
            index.capture_shared_file("src/main.rs").unwrap().as_ref(),
        );

        assert_eq!(shared_result, live_result);
    }

    #[test]
    fn test_symbol_detail_from_indexed_file_matches_live_index_output() {
        let content = b"fn helper() {}\nfn target() {}\n";
        let (key, file) = make_file(
            "src/main.rs",
            content,
            vec![
                make_symbol_with_bytes("helper", SymbolKind::Function, 0, 0, 0, 0, 13),
                make_symbol_with_bytes("target", SymbolKind::Function, 0, 1, 1, 14, 27),
            ],
        );
        let index = make_index(vec![(key, file)]);

        let live_result = symbol_detail(&index, "src/main.rs", "target", None);
        let shared_result = symbol_detail_from_indexed_file(
            index.capture_shared_file("src/main.rs").unwrap().as_ref(),
            "target",
            None,
        );

        assert_eq!(shared_result, live_result);
    }

    #[test]
    fn test_file_content_view_matches_live_index_output() {
        let content = b"line 1\nline 2\nline 3\nline 4\nline 5";
        let (key, file) = make_file("src/main.rs", content, vec![]);
        let index = make_index(vec![(key, file)]);

        let live_result = file_content(&index, "src/main.rs", Some(2), Some(4));
        let captured_result = file_content_view(
            &index.capture_file_content_view("src/main.rs").unwrap(),
            Some(2),
            Some(4),
        );

        assert_eq!(captured_result, live_result);
    }

    #[test]
    fn test_file_content_from_indexed_file_matches_live_index_output() {
        let content = b"line 1\nline 2\nline 3\nline 4\nline 5";
        let (key, file) = make_file("src/main.rs", content, vec![]);
        let index = make_index(vec![(key, file)]);

        let live_result = file_content(&index, "src/main.rs", Some(2), Some(4));
        let shared_result = file_content_from_indexed_file(
            index.capture_shared_file("src/main.rs").unwrap().as_ref(),
            Some(2),
            Some(4),
        );

        assert_eq!(shared_result, live_result);
    }

    #[test]
    fn test_file_content_from_indexed_file_with_context_renders_numbered_full_read() {
        let content = b"line 1\nline 2\nline 3";
        let (key, file) = make_file("src/main.rs", content, vec![]);
        let index = make_index(vec![(key, file)]);

        let result = file_content_from_indexed_file_with_context(
            index.capture_shared_file("src/main.rs").unwrap().as_ref(),
            search::ContentContext::line_range_with_format(None, None, true, false),
        );

        assert_eq!(result, "1: line 1\n2: line 2\n3: line 3");
    }

    #[test]
    fn test_file_content_from_indexed_file_with_context_renders_headered_range_read() {
        let content = b"line 1\nline 2\nline 3\nline 4";
        let (key, file) = make_file("src/main.rs", content, vec![]);
        let index = make_index(vec![(key, file)]);

        let result = file_content_from_indexed_file_with_context(
            index.capture_shared_file("src/main.rs").unwrap().as_ref(),
            search::ContentContext::line_range_with_format(Some(2), Some(3), true, true),
        );

        assert_eq!(result, "src/main.rs [lines 2-3]\n2: line 2\n3: line 3");
    }

    #[test]
    fn test_file_content_from_indexed_file_with_context_renders_numbered_around_line_excerpt() {
        let content = b"line 1\nline 2\nline 3\nline 4\nline 5";
        let (key, file) = make_file("src/main.rs", content, vec![]);
        let index = make_index(vec![(key, file)]);

        let result = file_content_from_indexed_file_with_context(
            index.capture_shared_file("src/main.rs").unwrap().as_ref(),
            search::ContentContext::around_line(3, Some(1)),
        );

        assert_eq!(result, "2: line 2\n3: line 3\n4: line 4");
    }

    #[test]
    fn test_file_content_from_indexed_file_with_context_renders_numbered_around_match_excerpt() {
        let content = b"line 1\nTODO first\nline 3\nTODO second\nline 5";
        let (key, file) = make_file("src/main.rs", content, vec![]);
        let index = make_index(vec![(key, file)]);

        let result = file_content_from_indexed_file_with_context(
            index.capture_shared_file("src/main.rs").unwrap().as_ref(),
            search::ContentContext::around_match("todo", Some(1)),
        );

        assert_eq!(result, "1: line 1\n2: TODO first\n3: line 3");
    }

    #[test]
    fn test_file_content_from_indexed_file_with_context_renders_chunked_excerpt_header() {
        let content = b"line 1\nline 2\nline 3\nline 4\nline 5";
        let (key, file) = make_file("src/main.rs", content, vec![]);
        let index = make_index(vec![(key, file)]);

        let result = file_content_from_indexed_file_with_context(
            index.capture_shared_file("src/main.rs").unwrap().as_ref(),
            search::ContentContext::chunk(2, 2),
        );

        assert_eq!(
            result,
            "src/main.rs [chunk 2/3, lines 3-4]\n3: line 3\n4: line 4"
        );
    }

    #[test]
    fn test_file_content_from_indexed_file_with_context_reports_out_of_range_chunk() {
        let content = b"line 1\nline 2\nline 3";
        let (key, file) = make_file("src/main.rs", content, vec![]);
        let index = make_index(vec![(key, file)]);

        let result = file_content_from_indexed_file_with_context(
            index.capture_shared_file("src/main.rs").unwrap().as_ref(),
            search::ContentContext::chunk(3, 2),
        );

        assert_eq!(result, "Chunk 3 out of range for src/main.rs (2 chunks)");
    }

    #[test]
    fn test_file_content_from_indexed_file_with_context_renders_around_symbol_excerpt() {
        let content = b"line 1\nfn connect() {}\nline 3";
        let (key, file) = make_file(
            "src/main.rs",
            content,
            vec![make_symbol("connect", SymbolKind::Function, 0, 1, 1)],
        );
        let index = make_index(vec![(key, file)]);

        let result = file_content_from_indexed_file_with_context(
            index.capture_shared_file("src/main.rs").unwrap().as_ref(),
            search::ContentContext::around_symbol("connect", None, Some(1)),
        );

        assert_eq!(result, "1: line 1\n2: fn connect() {}\n3: line 3");
    }

    #[test]
    fn test_file_content_from_indexed_file_with_context_reports_ambiguous_around_symbol() {
        let content = b"fn connect() {}\nline 2\nfn connect() {}";
        let (key, file) = make_file(
            "src/main.rs",
            content,
            vec![
                make_symbol("connect", SymbolKind::Function, 0, 0, 0),
                make_symbol("connect", SymbolKind::Function, 0, 2, 2),
            ],
        );
        let index = make_index(vec![(key, file)]);

        let result = file_content_from_indexed_file_with_context(
            index.capture_shared_file("src/main.rs").unwrap().as_ref(),
            search::ContentContext::around_symbol("connect", None, Some(1)),
        );

        assert_eq!(
            result,
            "Ambiguous symbol selector for connect in src/main.rs; pass `symbol_line` to disambiguate. Candidates: 0, 2"
        );
    }

    #[test]
    fn test_file_content_from_indexed_file_with_context_around_symbol_line_selects_exact_match() {
        let content = b"fn connect() {}\nline 2\nfn connect() {}";
        let (key, file) = make_file(
            "src/main.rs",
            content,
            vec![
                make_symbol("connect", SymbolKind::Function, 0, 0, 0),
                make_symbol("connect", SymbolKind::Function, 0, 2, 2),
            ],
        );
        let index = make_index(vec![(key, file)]);

        let result = file_content_from_indexed_file_with_context(
            index.capture_shared_file("src/main.rs").unwrap().as_ref(),
            search::ContentContext::around_symbol("connect", Some(2), Some(0)),
        );

        assert_eq!(result, "3: fn connect() {}");
    }

    // --- guard messages ---

    #[test]
    fn test_loading_guard_message() {
        assert_eq!(
            loading_guard_message(),
            "Index is loading... try again shortly."
        );
    }

    #[test]
    fn test_empty_guard_message() {
        assert_eq!(
            empty_guard_message(),
            "Index not loaded. Call index_folder to index a directory."
        );
    }

    // --- not_found helpers ---

    #[test]
    fn test_not_found_file_format() {
        assert_eq!(not_found_file("src/foo.rs"), "File not found: src/foo.rs");
    }

    #[test]
    fn test_not_found_symbol_lists_available() {
        let sym = make_symbol("existing_fn", SymbolKind::Function, 0, 1, 5);
        let (key, file) = make_file("src/lib.rs", b"fn existing_fn() {}", vec![sym]);
        let index = make_index(vec![(key, file)]);
        let result = not_found_symbol(&index, "src/lib.rs", "missing_fn");
        assert!(result.contains("No symbol missing_fn in src/lib.rs"));
        assert!(result.contains("existing_fn"));
    }

    #[test]
    fn test_not_found_symbol_no_symbols_in_file() {
        let (key, file) = make_file("src/lib.rs", b"", vec![]);
        let index = make_index(vec![(key, file)]);
        let result = not_found_symbol(&index, "src/lib.rs", "foo");
        assert!(result.contains("No symbols in that file"));
    }

    // ─── find_references_result tests ─────────────────────────────────────

    use crate::domain::{ReferenceKind, ReferenceRecord};

    fn make_ref(
        name: &str,
        kind: ReferenceKind,
        line: u32,
        enclosing: Option<u32>,
    ) -> ReferenceRecord {
        ReferenceRecord {
            name: name.to_string(),
            qualified_name: None,
            kind,
            byte_range: (0, 1),
            line_range: (line, line),
            enclosing_symbol_index: enclosing,
        }
    }

    fn make_file_with_refs(
        path: &str,
        content: &[u8],
        symbols: Vec<SymbolRecord>,
        references: Vec<ReferenceRecord>,
    ) -> (String, IndexedFile) {
        (
            path.to_string(),
            IndexedFile {
                relative_path: path.to_string(),
                language: LanguageId::Rust,
                classification: crate::domain::FileClassification::for_code_path(path),
                content: content.to_vec(),
                symbols,
                parse_status: ParseStatus::Parsed,
                byte_len: content.len() as u64,
                content_hash: "test".to_string(),
                references,
                alias_map: std::collections::HashMap::new(),
            },
        )
    }

    fn make_index_with_reverse(files: Vec<(String, IndexedFile)>) -> LiveIndex {
        use crate::live_index::trigram::TrigramIndex;
        let cb = CircuitBreakerState::new(0.20);
        let files_map = files
            .into_iter()
            .map(|(path, file)| (path, std::sync::Arc::new(file)))
            .collect::<HashMap<_, _>>();
        let trigram_index = TrigramIndex::build_from_files(&files_map);
        let mut index = LiveIndex {
            files: files_map,
            loaded_at: Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: Duration::from_millis(42),
            cb_state: cb,
            is_empty: false,
            load_source: crate::live_index::store::IndexLoadSource::FreshLoad,
            snapshot_verify_state: crate::live_index::store::SnapshotVerifyState::NotNeeded,
            reverse_index: HashMap::new(),
            files_by_basename: HashMap::new(),
            files_by_dir_component: HashMap::new(),
            trigram_index,
        };
        index.rebuild_reverse_index();
        index.rebuild_path_indices();
        index
    }

    #[test]
    fn test_find_references_result_groups_by_file_and_shows_context() {
        // Content: 3 lines so we can test context extraction
        let content = b"fn handle() {\n    process(x);\n}\n";
        let sym = make_symbol_with_bytes("handle", SymbolKind::Function, 0, 1, 3, 0, 30);
        let r = make_ref("process", ReferenceKind::Call, 2, Some(0));
        let (key, file) = make_file_with_refs("src/handler.rs", content, vec![sym], vec![r]);
        let index = make_index_with_reverse(vec![(key, file)]);
        let result = find_references_result(&index, "process", None);
        assert!(
            result.contains("1 references in 1 files"),
            "header missing, got: {result}"
        );
        assert!(
            result.contains("src/handler.rs"),
            "file path missing, got: {result}"
        );
        assert!(
            result.contains("process"),
            "reference name missing, got: {result}"
        );
        assert!(
            result.contains("[in fn handle]"),
            "enclosing annotation missing, got: {result}"
        );
    }

    #[test]
    fn test_find_references_result_zero_results() {
        let index = make_index_with_reverse(vec![]);
        let result = find_references_result(&index, "nobody", None);
        assert_eq!(result, "No references found for \"nobody\"");
    }

    #[test]
    fn test_find_references_result_kind_filter_call_only() {
        let content = b"use foo;\nfoo();\n";
        let r_import = make_ref("foo", ReferenceKind::Import, 1, None);
        let r_call = make_ref("foo", ReferenceKind::Call, 2, None);
        let (key, file) =
            make_file_with_refs("src/lib.rs", content, vec![], vec![r_import, r_call]);
        let index = make_index_with_reverse(vec![(key, file)]);
        let result = find_references_result(&index, "foo", Some("call"));
        // Should only show the call reference, not the import
        assert!(
            result.contains("1 references"),
            "expected only 1 reference, got: {result}"
        );
    }

    #[test]
    fn test_find_references_result_view_matches_live_index_output() {
        let content = b"fn handle() {\n    process(x);\n}\n";
        let sym = make_symbol_with_bytes("handle", SymbolKind::Function, 0, 1, 3, 0, 30);
        let r = make_ref("process", ReferenceKind::Call, 2, Some(0));
        let (key, file) = make_file_with_refs("src/handler.rs", content, vec![sym], vec![r]);
        let index = make_index_with_reverse(vec![(key, file)]);

        let live_result = find_references_result(&index, "process", None);
        let limits = OutputLimits::default();
        let captured_result = find_references_result_view(
            &index.capture_find_references_view("process", None, limits.total_hits),
            "process",
            &limits,
        );

        assert_eq!(captured_result, live_result);
    }

    #[test]
    fn test_find_references_result_view_total_limit_caps_across_files() {
        // 3 files, each with 10 references → 30 total, but total_limit=15
        let mut all_files = Vec::new();
        for i in 0..3 {
            let path = format!("src/file_{i}.rs");
            let content = b"fn f() {}\nfn g() {}\nfn h() {}\n";
            let refs: Vec<ReferenceRecord> = (0..10)
                .map(|j| make_ref("target", ReferenceKind::Call, (j % 3) + 1, None))
                .collect();
            let (key, file) = make_file_with_refs(&path, content, vec![], refs);
            all_files.push((key, file));
        }
        let index = make_index_with_reverse(all_files);
        let view = index.capture_find_references_view("target", None, 200);

        // Without total_hits limit, all 30 refs would be shown (max_per_file is high)
        let unlimited = OutputLimits {
            max_files: 100,
            max_per_file: 100,
            total_hits: usize::MAX,
        };
        let unlimited_result = find_references_result_view(&view, "target", &unlimited);
        assert!(
            !unlimited_result.contains("more references"),
            "unlimited should show all refs"
        );

        // With total_hits=15, only 15 refs should be emitted
        let limits = OutputLimits {
            max_files: 100,
            max_per_file: 100,
            total_hits: 15,
        };
        let result = find_references_result_view(&view, "target", &limits);

        // file_0 gets 10 hits, file_1 gets 5 hits before total_limit reached,
        // file_1 has 5 truncated, file_2 is skipped entirely
        assert!(
            result.contains("... and 5 more references"),
            "file_1 should show 5 truncated hits, got:\n{result}"
        );
        // file_2 should not appear (total_limit already reached before it)
        assert!(
            !result.contains("src/file_2.rs"),
            "file_2 should be skipped, got:\n{result}"
        );
    }

    #[test]
    fn test_find_references_result_view_per_file_limit_within_total() {
        // 1 file with 20 references, max_per_file=5, total_hits=100
        let content = b"fn a() {}\nfn b() {}\nfn c() {}\n";
        let refs: Vec<ReferenceRecord> = (0..20)
            .map(|j| make_ref("target", ReferenceKind::Call, (j % 3) + 1, None))
            .collect();
        let (key, file) = make_file_with_refs("src/lib.rs", content, vec![], refs);
        let index = make_index_with_reverse(vec![(key, file)]);
        let view = index.capture_find_references_view("target", None, 200);

        let limits = OutputLimits {
            max_files: 100,
            max_per_file: 5,
            total_hits: 100,
        };
        let result = find_references_result_view(&view, "target", &limits);

        // Should show 5 refs and truncate 15
        assert!(
            result.contains("... and 15 more references"),
            "expected per-file truncation, got:\n{result}"
        );
    }

    #[test]
    fn test_find_references_compact_view_total_limit_caps_across_files() {
        let mut all_files = Vec::new();
        for i in 0..3 {
            let path = format!("src/file_{i}.rs");
            let content = b"fn f() {}\nfn g() {}\nfn h() {}\n";
            let refs: Vec<ReferenceRecord> = (0..10)
                .map(|j| make_ref("target", ReferenceKind::Call, (j % 3) + 1, None))
                .collect();
            let (key, file) = make_file_with_refs(&path, content, vec![], refs);
            all_files.push((key, file));
        }
        let index = make_index_with_reverse(all_files);
        let view = index.capture_find_references_view("target", None, 200);

        let limits = OutputLimits {
            max_files: 100,
            max_per_file: 100,
            total_hits: 15,
        };
        let result = find_references_compact_view(&view, "target", &limits);

        // file_0 gets 10 hits, file_1 gets 5 hits, file_1 truncates 5, file_2 skipped
        assert!(
            result.contains("... and 5 more"),
            "file_1 should show 5 truncated in compact view, got:\n{result}"
        );
        assert!(
            !result.contains("src/file_2.rs"),
            "file_2 should be skipped in compact view, got:\n{result}"
        );
    }

    // ─── find_dependents_result tests ─────────────────────────────────────

    #[test]
    fn test_find_dependents_result_shows_importers() {
        let content_b = b"use crate::db;\n";
        let r = make_ref("db", ReferenceKind::Import, 1, None);
        let (key_b, file_b) = make_file_with_refs("src/handler.rs", content_b, vec![], vec![r]);
        // Also need "src/db.rs" in the index for find_dependents_for_file to work
        let (key_a, file_a) = make_file("src/db.rs", b"pub fn connect() {}", vec![]);
        let index = make_index_with_reverse(vec![(key_a, file_a), (key_b, file_b)]);
        let result = find_dependents_result(&index, "src/db.rs");
        assert!(
            result.contains("1 files depend on src/db.rs"),
            "header wrong, got: {result}"
        );
        assert!(
            result.contains("src/handler.rs"),
            "importer missing, got: {result}"
        );
        assert!(
            result.contains("[import]"),
            "import annotation missing, got: {result}"
        );
    }

    #[test]
    fn test_find_dependents_result_zero_dependents() {
        let (key, file) = make_file("src/db.rs", b"", vec![]);
        let index = make_index_with_reverse(vec![(key, file)]);
        let result = find_dependents_result(&index, "src/db.rs");
        assert_eq!(result, "No dependents found for \"src/db.rs\"");
    }

    #[test]
    fn test_find_dependents_result_view_matches_live_index_output() {
        let content_b = b"use crate::db;\n";
        let r = make_ref("db", ReferenceKind::Import, 1, None);
        let (key_b, file_b) = make_file_with_refs("src/handler.rs", content_b, vec![], vec![r]);
        let (key_a, file_a) = make_file("src/db.rs", b"pub fn connect() {}", vec![]);
        let index = make_index_with_reverse(vec![(key_a, file_a), (key_b, file_b)]);

        let live_result = find_dependents_result(&index, "src/db.rs");
        let captured_result = find_dependents_result_view(
            &index.capture_find_dependents_view("src/db.rs"),
            "src/db.rs",
            &OutputLimits::default(),
        );

        assert_eq!(captured_result, live_result);
    }

    #[test]
    fn test_find_dependents_mermaid_shows_flowchart() {
        let content_b = b"use crate::db;\n";
        let r = make_ref("db", ReferenceKind::Import, 1, None);
        let (key_b, file_b) = make_file_with_refs("src/handler.rs", content_b, vec![], vec![r]);
        let (key_a, file_a) = make_file("src/db.rs", b"pub fn connect() {}", vec![]);
        let index = make_index_with_reverse(vec![(key_a, file_a), (key_b, file_b)]);
        let view = index.capture_find_dependents_view("src/db.rs");
        let result = find_dependents_mermaid(&view, "src/db.rs", &OutputLimits::default());
        assert!(
            result.starts_with("flowchart LR"),
            "should start with flowchart, got: {result}"
        );
        assert!(result.contains("src/db.rs"), "should mention target file");
        assert!(
            result.contains("src/handler.rs"),
            "should mention dependent"
        );
        assert!(result.contains("refs"), "should show ref count");
    }

    #[test]
    fn test_find_dependents_mermaid_empty() {
        let (key, file) = make_file("src/db.rs", b"", vec![]);
        let index = make_index_with_reverse(vec![(key, file)]);
        let view = index.capture_find_dependents_view("src/db.rs");
        let result = find_dependents_mermaid(&view, "src/db.rs", &OutputLimits::default());
        assert_eq!(result, "No dependents found for \"src/db.rs\"");
    }

    #[test]
    fn test_find_dependents_dot_shows_digraph() {
        let content_b = b"use crate::db;\n";
        let r = make_ref("db", ReferenceKind::Import, 1, None);
        let (key_b, file_b) = make_file_with_refs("src/handler.rs", content_b, vec![], vec![r]);
        let (key_a, file_a) = make_file("src/db.rs", b"pub fn connect() {}", vec![]);
        let index = make_index_with_reverse(vec![(key_a, file_a), (key_b, file_b)]);
        let view = index.capture_find_dependents_view("src/db.rs");
        let result = find_dependents_dot(&view, "src/db.rs", &OutputLimits::default());
        assert!(
            result.starts_with("digraph dependents {"),
            "should start with digraph, got: {result}"
        );
        assert!(result.contains("src/db.rs"), "should mention target file");
        assert!(
            result.contains("src/handler.rs"),
            "should mention dependent"
        );
        assert!(result.ends_with('}'), "should end with closing brace");
    }

    #[test]
    fn test_find_dependents_dot_empty() {
        let (key, file) = make_file("src/db.rs", b"", vec![]);
        let index = make_index_with_reverse(vec![(key, file)]);
        let view = index.capture_find_dependents_view("src/db.rs");
        let result = find_dependents_dot(&view, "src/db.rs", &OutputLimits::default());
        assert_eq!(result, "No dependents found for \"src/db.rs\"");
    }

    // ─── context_bundle_result tests ──────────────────────────────────────

    #[test]
    fn test_context_bundle_result_includes_body_and_sections() {
        let content = b"fn process(x: i32) -> i32 {\n    x + 1\n}\n";
        let sym = make_symbol_with_bytes("process", SymbolKind::Function, 0, 1, 3, 0, 41);
        let (key, file) = make_file_with_refs("src/lib.rs", content, vec![sym], vec![]);
        let index = make_index_with_reverse(vec![(key, file)]);
        let result = context_bundle_result(&index, "src/lib.rs", "process", None);
        assert!(result.contains("fn process"), "body missing, got: {result}");
        assert!(
            result.contains("[fn, src/lib.rs:"),
            "footer missing, got: {result}"
        );
        assert!(
            result.contains("Callers"),
            "Callers section missing, got: {result}"
        );
        assert!(
            result.contains("Callees"),
            "Callees section missing, got: {result}"
        );
        assert!(
            result.contains("Type usages"),
            "Type usages section missing, got: {result}"
        );
    }

    #[test]
    fn test_context_bundle_result_caps_callers_at_20() {
        // Build 25 Call references to "process" from different positions
        let refs: Vec<ReferenceRecord> = (0u32..25)
            .map(|i| make_ref("process", ReferenceKind::Call, i + 100, None))
            .collect();
        let content = b"fn caller() {} fn process() {}";
        let sym_caller = make_symbol_with_bytes("caller", SymbolKind::Function, 0, 1, 1, 0, 14);
        let sym_process = make_symbol_with_bytes("process", SymbolKind::Function, 0, 1, 1, 15, 30);
        // Add a process symbol as the target
        let (key, file) =
            make_file_with_refs("src/lib.rs", content, vec![sym_caller, sym_process], refs);
        let index = make_index_with_reverse(vec![(key, file)]);
        let result = context_bundle_result(&index, "src/lib.rs", "process", None);
        assert!(
            result.contains("...and"),
            "overflow message missing, got: {result}"
        );
        assert!(
            result.contains("more callers"),
            "overflow count missing, got: {result}"
        );
    }

    #[test]
    fn test_context_bundle_result_symbol_not_found() {
        let (key, file) = make_file("src/lib.rs", b"fn foo() {}", vec![]);
        let index = make_index_with_reverse(vec![(key, file)]);
        let result = context_bundle_result(&index, "src/lib.rs", "nonexistent", None);
        assert!(
            result.contains("No symbol nonexistent in src/lib.rs"),
            "got: {result}"
        );
    }

    #[test]
    fn test_context_bundle_result_empty_sections_show_zero() {
        let content = b"fn process() {}";
        let sym = make_symbol_with_bytes("process", SymbolKind::Function, 0, 1, 1, 0, 15);
        let (key, file) = make_file_with_refs("src/lib.rs", content, vec![sym], vec![]);
        let index = make_index_with_reverse(vec![(key, file)]);
        let result = context_bundle_result(&index, "src/lib.rs", "process", None);
        assert!(
            result.contains("Callers (0)"),
            "zero callers section missing, got: {result}"
        );
        assert!(
            result.contains("Callees (0)"),
            "zero callees section missing, got: {result}"
        );
        assert!(
            result.contains("Type usages (0)"),
            "zero type usages section missing, got: {result}"
        );
    }

    #[test]
    fn test_context_bundle_result_view_matches_live_index_output() {
        let content = b"fn process(x: i32) -> i32 {\n    x + 1\n}\n";
        let sym = make_symbol_with_bytes("process", SymbolKind::Function, 0, 1, 3, 0, 41);
        let (key, file) = make_file_with_refs("src/lib.rs", content, vec![sym], vec![]);
        let index = make_index_with_reverse(vec![(key, file)]);

        let live_result = context_bundle_result(&index, "src/lib.rs", "process", None);
        let captured_result = context_bundle_result_view(
            &index.capture_context_bundle_view("src/lib.rs", "process", None, None),
            "full",
        );

        assert_eq!(captured_result, live_result);
    }

    #[test]
    fn test_context_bundle_result_view_ambiguous_symbol() {
        let result = context_bundle_result_view(
            &ContextBundleView::AmbiguousSymbol {
                path: "src/lib.rs".to_string(),
                name: "process".to_string(),
                candidate_lines: vec![1, 10],
            },
            "full",
        );

        assert!(
            result.contains("Ambiguous symbol selector"),
            "got: {result}"
        );
        assert!(result.contains("1"), "got: {result}");
        assert!(result.contains("10"), "got: {result}");
    }

    // --- format_token_savings tests ---

    #[test]
    fn test_format_token_savings_all_zeros_returns_empty() {
        let snap = crate::sidecar::StatsSnapshot {
            read_fires: 0,
            read_saved_tokens: 0,
            edit_fires: 0,
            edit_saved_tokens: 0,
            write_fires: 0,
            grep_fires: 0,
            grep_saved_tokens: 0,
        };
        let result = format_token_savings(&snap);
        assert!(
            result.is_empty(),
            "all-zero snapshot should return empty string; got: {result}"
        );
    }

    #[test]
    fn test_format_token_savings_shows_section_header() {
        let snap = crate::sidecar::StatsSnapshot {
            read_fires: 1,
            read_saved_tokens: 250,
            edit_fires: 0,
            edit_saved_tokens: 0,
            write_fires: 0,
            grep_fires: 0,
            grep_saved_tokens: 0,
        };
        let result = format_token_savings(&snap);
        assert!(
            result.contains("Token Savings"),
            "result must contain 'Token Savings' header; got: {result}"
        );
    }

    #[test]
    fn test_format_token_savings_read_fires_and_tokens() {
        let snap = crate::sidecar::StatsSnapshot {
            read_fires: 3,
            read_saved_tokens: 750,
            edit_fires: 0,
            edit_saved_tokens: 0,
            write_fires: 0,
            grep_fires: 0,
            grep_saved_tokens: 0,
        };
        let result = format_token_savings(&snap);
        assert!(
            result.contains("Read"),
            "should show Read line; got: {result}"
        );
        assert!(
            result.contains("3 fires"),
            "should show fire count; got: {result}"
        );
        assert!(
            result.contains("750"),
            "should show saved tokens; got: {result}"
        );
    }

    #[test]
    fn test_format_token_savings_total_is_sum_of_parts() {
        let snap = crate::sidecar::StatsSnapshot {
            read_fires: 2,
            read_saved_tokens: 100,
            edit_fires: 1,
            edit_saved_tokens: 50,
            write_fires: 0,
            grep_fires: 3,
            grep_saved_tokens: 200,
        };
        let result = format_token_savings(&snap);
        // Total = 100 + 50 + 200 = 350
        assert!(
            result.contains("350"),
            "total should be sum of read+edit+grep savings (350); got: {result}"
        );
        assert!(
            result.contains("Total:"),
            "should have Total line; got: {result}"
        );
    }

    #[test]
    fn test_format_token_savings_write_fires_no_savings_field() {
        let snap = crate::sidecar::StatsSnapshot {
            read_fires: 0,
            read_saved_tokens: 0,
            edit_fires: 0,
            edit_saved_tokens: 0,
            write_fires: 2,
            grep_fires: 0,
            grep_saved_tokens: 0,
        };
        let result = format_token_savings(&snap);
        assert!(
            result.contains("Write"),
            "should show Write line; got: {result}"
        );
        assert!(
            result.contains("2 fires"),
            "should show write fire count; got: {result}"
        );
        // Write has no savings — just fire count
        assert!(
            !result.contains("tokens saved\nTotal"),
            "write line should not show saved tokens"
        );
    }

    #[test]
    fn test_format_token_savings_omits_zero_hook_types() {
        // Only read fired — edit and grep should not appear.
        let snap = crate::sidecar::StatsSnapshot {
            read_fires: 1,
            read_saved_tokens: 100,
            edit_fires: 0,
            edit_saved_tokens: 0,
            write_fires: 0,
            grep_fires: 0,
            grep_saved_tokens: 0,
        };
        let result = format_token_savings(&snap);
        assert!(result.contains("Read"), "should show Read; got: {result}");
        assert!(
            !result.contains("Edit:"),
            "Edit should be omitted when zero; got: {result}"
        );
        assert!(
            !result.contains("Grep:"),
            "Grep should be omitted when zero; got: {result}"
        );
        assert!(
            !result.contains("Write:"),
            "Write should be omitted when zero; got: {result}"
        );
    }

    // --- compact_savings_footer tests ---

    #[test]
    fn test_compact_savings_footer_shows_savings() {
        let footer = compact_savings_footer(200, 2000);
        assert!(footer.contains("tokens saved"), "got: {footer}");
    }

    #[test]
    fn test_compact_savings_footer_empty_when_no_savings() {
        let footer = compact_savings_footer(2000, 200);
        assert!(footer.is_empty());
    }

    #[test]
    fn test_compact_savings_footer_empty_for_small_files() {
        let footer = compact_savings_footer(50, 100);
        assert!(footer.is_empty());
    }

    // ── search_symbols tier ordering tests ───────────────────────────────────

    #[test]
    fn test_search_symbols_exact_match_tier_header() {
        let sym = make_symbol("parse", SymbolKind::Function, 0, 1, 5);
        let (key, file) = make_file("src/lib.rs", b"fn parse() {}", vec![sym]);
        let index = make_index(vec![(key, file)]);
        let result = search_symbols_result(&index, "parse");
        assert!(
            result.contains("Exact matches"),
            "should show 'Exact matches' tier header; got: {result}"
        );
    }

    #[test]
    fn test_search_symbols_prefix_match_tier_header() {
        let sym = make_symbol("parse_file", SymbolKind::Function, 0, 1, 5);
        let (key, file) = make_file("src/lib.rs", b"fn parse_file() {}", vec![sym]);
        let index = make_index(vec![(key, file)]);
        let result = search_symbols_result(&index, "parse");
        assert!(
            result.contains("Prefix matches"),
            "should show 'Prefix matches' tier header; got: {result}"
        );
    }

    #[test]
    fn test_search_symbols_substring_match_tier_header() {
        let sym = make_symbol("do_parse_now", SymbolKind::Function, 0, 1, 5);
        let (key, file) = make_file("src/lib.rs", b"fn do_parse_now() {}", vec![sym]);
        let index = make_index(vec![(key, file)]);
        let result = search_symbols_result(&index, "parse");
        assert!(
            result.contains("Substring matches"),
            "should show 'Substring matches' tier header; got: {result}"
        );
    }

    #[test]
    fn test_search_symbols_exact_before_prefix_before_substring() {
        // exact: "parse", prefix: "parse_file", substring: "do_parse"
        let symbols = vec![
            make_symbol("do_parse", SymbolKind::Function, 0, 1, 2),
            make_symbol("parse_file", SymbolKind::Function, 0, 3, 4),
            make_symbol("parse", SymbolKind::Function, 0, 5, 6),
        ];
        let (key, file) = make_file(
            "src/lib.rs",
            b"fn do_parse() {} fn parse_file() {} fn parse() {}",
            symbols,
        );
        let index = make_index(vec![(key, file)]);
        let result = search_symbols_result(&index, "parse");

        let exact_pos = result
            .find("Exact matches")
            .expect("missing Exact matches header");
        let prefix_pos = result
            .find("Prefix matches")
            .expect("missing Prefix matches header");
        let substr_pos = result
            .find("Substring matches")
            .expect("missing Substring matches header");

        assert!(exact_pos < prefix_pos, "Exact must appear before Prefix");
        assert!(
            prefix_pos < substr_pos,
            "Prefix must appear before Substring"
        );

        // "parse" must appear after "Exact matches" and before "Prefix matches"
        let parse_pos = result[exact_pos..]
            .find("\n  ")
            .map(|p| exact_pos + p)
            .expect("no symbol line after Exact header");
        assert!(
            parse_pos < prefix_pos,
            "exact match 'parse' must be in Exact section"
        );
    }

    #[test]
    fn test_search_symbols_omits_empty_tier_sections() {
        // Only exact match — prefix and substring headers must NOT appear
        let sym = make_symbol("search", SymbolKind::Function, 0, 1, 5);
        let (key, file) = make_file("src/lib.rs", b"fn search() {}", vec![sym]);
        let index = make_index(vec![(key, file)]);
        let result = search_symbols_result(&index, "search");
        assert!(
            !result.contains("Prefix matches"),
            "no prefix matches: header must be omitted; got: {result}"
        );
        assert!(
            !result.contains("Substring matches"),
            "no substring matches: header must be omitted; got: {result}"
        );
    }

    #[test]
    fn test_search_symbols_within_exact_tier_alphabetical() {
        let symbols = vec![
            make_symbol("z_fn", SymbolKind::Function, 0, 1, 2),
            make_symbol("a_fn", SymbolKind::Function, 0, 3, 4),
            make_symbol("m_fn", SymbolKind::Function, 0, 5, 6),
        ];
        let (key, file) = make_file(
            "src/lib.rs",
            b"fn z_fn() {} fn a_fn() {} fn m_fn() {}",
            symbols,
        );
        let index = make_index(vec![(key, file)]);
        let result = search_symbols_result(&index, "a_fn");
        // Only "a_fn" matches exactly — just verify it shows up in Exact
        assert!(result.contains("Exact matches"), "got: {result}");
        assert!(result.contains("a_fn"), "got: {result}");
    }

    #[test]
    fn test_search_symbols_within_prefix_tier_shorter_names_first() {
        // "parse" is query, "parse_x" (7 chars) should come before "parse_longer" (12 chars)
        let symbols = vec![
            make_symbol("parse_longer", SymbolKind::Function, 0, 1, 2),
            make_symbol("parse_x", SymbolKind::Function, 0, 3, 4),
        ];
        let (key, file) = make_file(
            "src/lib.rs",
            b"fn parse_longer() {} fn parse_x() {}",
            symbols,
        );
        let index = make_index(vec![(key, file)]);
        let result = search_symbols_result(&index, "parse");

        // In the prefix section, parse_x must appear before parse_longer
        let prefix_pos = result
            .find("Prefix matches")
            .expect("missing Prefix matches");
        let section_after = &result[prefix_pos..];
        let x_pos = section_after
            .find("parse_x")
            .expect("parse_x not in prefix section");
        let longer_pos = section_after
            .find("parse_longer")
            .expect("parse_longer not in prefix section");
        assert!(
            x_pos < longer_pos,
            "shorter prefix match 'parse_x' must appear before 'parse_longer'"
        );
    }

    // ── file_tree tests ───────────────────────────────────────────────────────

    fn make_file_with_lang(
        path: &str,
        content: &[u8],
        symbols: Vec<SymbolRecord>,
        lang: crate::domain::LanguageId,
    ) -> (String, IndexedFile) {
        (
            path.to_string(),
            IndexedFile {
                relative_path: path.to_string(),
                language: lang,
                classification: crate::domain::FileClassification::for_code_path(path),
                content: content.to_vec(),
                symbols,
                parse_status: ParseStatus::Parsed,
                byte_len: content.len() as u64,
                content_hash: "test".to_string(),
                references: vec![],
                alias_map: std::collections::HashMap::new(),
            },
        )
    }

    #[test]
    fn test_file_tree_shows_files_with_symbol_count() {
        let sym = make_symbol("main", SymbolKind::Function, 0, 1, 5);
        let (key, file) = make_file_with_lang(
            "src/main.rs",
            b"fn main() {}",
            vec![sym],
            crate::domain::LanguageId::Rust,
        );
        let index = make_index(vec![(key, file)]);
        let result = file_tree(&index, "", 2);
        assert!(
            result.contains("main.rs"),
            "should show filename; got: {result}"
        );
        assert!(
            result.contains("1 symbol"),
            "should show symbol count; got: {result}"
        );
    }

    #[test]
    fn test_file_tree_view_matches_live_index_output() {
        let sym1 = make_symbol("foo", SymbolKind::Function, 0, 1, 3);
        let sym2 = make_symbol("bar", SymbolKind::Function, 0, 1, 3);
        let (k1, f1) = make_file_with_lang(
            "src/a.rs",
            b"fn foo() {}",
            vec![sym1],
            crate::domain::LanguageId::Rust,
        );
        let (k2, f2) = make_file_with_lang(
            "tests/b.rs",
            b"fn bar() {}",
            vec![sym2],
            crate::domain::LanguageId::Rust,
        );
        let index = make_index(vec![(k1, f1), (k2, f2)]);

        let live_result = file_tree(&index, "", 3);
        let captured_result = file_tree_view(&index.capture_repo_outline_view().files, "", 3);

        assert_eq!(captured_result, live_result);
    }

    #[test]
    fn test_file_tree_shows_directory_with_file_counts() {
        let sym1 = make_symbol("foo", SymbolKind::Function, 0, 1, 3);
        let sym2 = make_symbol("bar", SymbolKind::Function, 0, 1, 3);
        let (k1, f1) = make_file_with_lang(
            "src/a.rs",
            b"fn foo() {}",
            vec![sym1],
            crate::domain::LanguageId::Rust,
        );
        let (k2, f2) = make_file_with_lang(
            "src/b.rs",
            b"fn bar() {}",
            vec![sym2],
            crate::domain::LanguageId::Rust,
        );
        let index = make_index(vec![(k1, f1), (k2, f2)]);
        let result = file_tree(&index, "", 1);
        // At depth 1, "src" directory should be shown collapsed with file/symbol counts
        assert!(
            result.contains("src"),
            "should show src directory; got: {result}"
        );
    }

    #[test]
    fn test_file_tree_footer_shows_totals() {
        let sym = make_symbol("foo", SymbolKind::Function, 0, 1, 3);
        let (k1, f1) = make_file_with_lang(
            "src/a.rs",
            b"fn foo() {}",
            vec![sym],
            crate::domain::LanguageId::Rust,
        );
        let (k2, f2) = make_file_with_lang(
            "lib/b.rs",
            b"fn bar() {}",
            vec![],
            crate::domain::LanguageId::Rust,
        );
        let index = make_index(vec![(k1, f1), (k2, f2)]);
        let result = file_tree(&index, "", 3);
        // Footer must show directories, files, symbols totals
        assert!(
            result.contains("files"),
            "footer should mention files; got: {result}"
        );
        assert!(
            result.contains("symbols"),
            "footer should mention symbols; got: {result}"
        );
    }

    #[test]
    fn test_file_tree_respects_path_filter() {
        let sym = make_symbol("foo", SymbolKind::Function, 0, 1, 3);
        let (k1, f1) = make_file_with_lang(
            "src/a.rs",
            b"fn foo() {}",
            vec![sym],
            crate::domain::LanguageId::Rust,
        );
        let (k2, f2) = make_file_with_lang(
            "tests/b.rs",
            b"fn test_b() {}",
            vec![],
            crate::domain::LanguageId::Rust,
        );
        let index = make_index(vec![(k1, f1), (k2, f2)]);
        let result = file_tree(&index, "src", 3);
        assert!(
            result.contains("a.rs"),
            "src filter should show a.rs; got: {result}"
        );
        assert!(
            !result.contains("b.rs"),
            "src filter should not show tests/b.rs; got: {result}"
        );
    }

    #[test]
    fn test_file_tree_repeated_basenames_remain_hierarchical() {
        let sym = make_symbol("foo", SymbolKind::Function, 0, 1, 3);
        let index = make_index(vec![
            make_file_with_lang(
                "src/live_index/mod.rs",
                b"fn foo() {}",
                vec![sym.clone()],
                crate::domain::LanguageId::Rust,
            ),
            make_file_with_lang(
                "src/protocol/mod.rs",
                b"fn foo() {}",
                vec![sym],
                crate::domain::LanguageId::Rust,
            ),
        ]);
        let result = file_tree(&index, "", 3);
        assert!(result.contains("live_index/"), "got: {result}");
        assert!(result.contains("protocol/"), "got: {result}");
        assert!(!result.contains("live_index/mod.rs"), "got: {result}");
        assert!(!result.contains("protocol/mod.rs"), "got: {result}");
    }

    #[test]
    fn test_file_tree_depth_collapses_deep_directories() {
        // At depth=1, nested directories beyond root level should be collapsed
        let sym = make_symbol("deep", SymbolKind::Function, 0, 1, 3);
        let (k1, f1) = make_file_with_lang(
            "src/deep/nested/file.rs",
            b"fn deep() {}",
            vec![sym],
            crate::domain::LanguageId::Rust,
        );
        let index = make_index(vec![(k1, f1)]);
        let result = file_tree(&index, "", 1);
        // file.rs should not be individually listed at depth=1
        assert!(
            !result.contains("file.rs"),
            "file.rs should be collapsed at depth=1; got: {result}"
        );
    }

    #[test]
    fn test_file_tree_empty_index() {
        let index = make_index(vec![]);
        let result = file_tree(&index, "", 2);
        assert!(
            result.contains("0 files") || result.contains("No source files"),
            "got: {result}"
        );
    }

    #[test]
    fn test_format_type_dependencies_renders_bodies_and_depth() {
        let deps = vec![
            TypeDependencyView {
                name: "UserConfig".to_string(),
                kind_label: "struct".to_string(),
                file_path: "src/config.rs".to_string(),
                line_range: (0, 2),
                body: "pub struct UserConfig {\n    pub name: String,\n}".to_string(),
                depth: 0,
            },
            TypeDependencyView {
                name: "Address".to_string(),
                kind_label: "struct".to_string(),
                file_path: "src/address.rs".to_string(),
                line_range: (0, 1),
                body: "pub struct Address {\n    pub city: String,\n}".to_string(),
                depth: 1,
            },
        ];
        let result = format_type_dependencies(&deps);
        assert!(
            result.contains("Dependencies (2):"),
            "header missing, got: {result}"
        );
        assert!(
            result.contains("── UserConfig [struct, src/config.rs:1-3] ──"),
            "UserConfig entry missing (0-based 0-2 displayed as 1-based 1-3), got: {result}"
        );
        assert!(
            result.contains("pub struct UserConfig"),
            "UserConfig body missing, got: {result}"
        );
        assert!(
            result.contains("(depth 1)"),
            "depth marker missing for Address, got: {result}"
        );
        // Direct dependency (depth 0) should NOT have depth marker.
        assert!(
            !result.contains("(depth 0)"),
            "depth 0 should have no marker, got: {result}"
        );
    }

    #[test]
    fn test_extract_declaration_name_rust_fn() {
        assert_eq!(
            super::extract_declaration_name("pub fn hello_world() -> String {"),
            Some("hello_world".to_string())
        );
        assert_eq!(
            super::extract_declaration_name("fn main() {"),
            Some("main".to_string())
        );
        assert_eq!(
            super::extract_declaration_name("pub(crate) async fn process(x: u32) -> Result {"),
            Some("process".to_string())
        );
    }

    #[test]
    fn test_extract_declaration_name_struct() {
        assert_eq!(
            super::extract_declaration_name("pub struct Config {"),
            Some("Config".to_string())
        );
        assert_eq!(
            super::extract_declaration_name("struct Inner;"),
            Some("Inner".to_string())
        );
    }

    #[test]
    fn test_extract_declaration_name_non_declaration() {
        assert_eq!(super::extract_declaration_name("let x = 5;"), None);
        assert_eq!(
            super::extract_declaration_name("// fn commented_out()"),
            None
        );
        assert_eq!(
            super::extract_declaration_name("use std::collections::HashMap;"),
            None
        );
    }
}

/// Format the output of the `explore` tool.
pub fn explore_result_view(
    label: &str,
    symbol_hits: &[(String, String, String)], // (name, kind, path)
    text_hits: &[(String, String, usize)],    // (path, line, line_number)
    related_files: &[(String, usize)],        // (path, count)
    enriched_symbols: &[(String, String, String, Option<String>, Vec<String>)],
    symbol_impls: &[(String, Vec<String>)],
    symbol_deps: &[(String, Vec<String>)],
    depth: u32,
) -> String {
    let mut lines = vec![format!("── Exploring: {label} ──")];
    lines.push(String::new());

    if depth >= 2 && !enriched_symbols.is_empty() {
        // Depth 2+: show enriched symbols with signatures
        lines.push(format!("Symbols ({} found):", symbol_hits.len()));
        for (name, kind, path, signature, dependents) in enriched_symbols {
            if let Some(sig) = signature {
                // Show first line of signature only to keep it compact
                let first_line = sig.lines().next().unwrap_or(sig);
                lines.push(format!("  {first_line}  [{kind}, {path}]"));
            } else {
                lines.push(format!("  {kind} {name}  {path}"));
            }
            if !dependents.is_empty() {
                lines.push(format!("    <- used by: {}", dependents.join(", ")));
            }
        }
        // Show remaining non-enriched symbols in compact form
        if symbol_hits.len() > enriched_symbols.len() {
            for (name, kind, path) in &symbol_hits[enriched_symbols.len()..] {
                lines.push(format!("  {kind} {name}  {path}"));
            }
        }
        lines.push(String::new());
    } else if !symbol_hits.is_empty() {
        // Depth 1: original compact format
        lines.push(format!("Symbols ({} found):", symbol_hits.len()));
        for (name, kind, path) in symbol_hits {
            lines.push(format!("  {kind} {name}  {path}"));
        }
        lines.push(String::new());
    }

    // Depth 3: implementations + type dependencies
    if depth >= 3 && symbol_impls.is_empty() && symbol_deps.is_empty() {
        lines.push("No implementations or type dependencies found for top symbols.".to_string());
        lines.push(String::new());
    }
    if depth >= 3 && !symbol_impls.is_empty() {
        lines.push("Implementations:".to_string());
        for (name, impls) in symbol_impls {
            lines.push(format!("  {name}:"));
            for imp in impls {
                lines.push(format!("    -> {imp}"));
            }
        }
        lines.push(String::new());
    }

    if depth >= 3 && !symbol_deps.is_empty() {
        lines.push("Type dependencies:".to_string());
        for (name, deps) in symbol_deps {
            lines.push(format!("  {name}:"));
            for dep in deps {
                lines.push(format!("    -> {dep}"));
            }
        }
        lines.push(String::new());
    }

    if !text_hits.is_empty() {
        lines.push(format!("Code patterns ({} found):", text_hits.len()));
        let mut last_path: Option<&str> = None;
        for (path, line, line_number) in text_hits {
            if last_path != Some(path.as_str()) {
                lines.push(format!("  {path}"));
                last_path = Some(path.as_str());
            }
            lines.push(format!("    > {line_number}: {line}"));
        }
        lines.push(String::new());
    }

    if !related_files.is_empty() {
        lines.push("Related files:".to_string());
        for (path, count) in related_files {
            lines.push(format!("  {path}  ({count} matches)"));
        }
    }

    if symbol_hits.is_empty() && text_hits.is_empty() {
        lines.push("No matches found.".to_string());
    }

    lines.join("\n")
}

/// Format git temporal data for a single file: churn, ownership, co-changes, last commit.
pub fn get_co_changes_result_view(
    path: &str,
    history: &crate::live_index::git_temporal::GitFileHistory,
    limit: usize,
) -> String {
    let mut lines = Vec::new();

    lines.push(format!("Git temporal data for {path}"));
    lines.push(String::new());

    // Churn
    lines.push(format!(
        "Churn score: {:.2} ({} commits)",
        history.churn_score, history.commit_count
    ));

    // Last commit
    let c = &history.last_commit;
    lines.push(format!(
        "Last commit: {} {} — {} ({})",
        c.hash, c.timestamp, c.message_head, c.author
    ));
    lines.push(String::new());

    // Ownership
    if !history.contributors.is_empty() {
        lines.push("Ownership:".to_string());
        for contrib in &history.contributors {
            lines.push(format!(
                "  {}: {} commits ({:.0}%)",
                contrib.author, contrib.commit_count, contrib.percentage
            ));
        }
        lines.push(String::new());
    }

    // Co-changes
    if history.co_changes.is_empty() {
        lines.push("No co-changing files detected.".to_string());
    } else {
        lines.push(format!(
            "Co-changing files (top {}):",
            limit.min(history.co_changes.len())
        ));
        for entry in history.co_changes.iter().take(limit) {
            lines.push(format!(
                "  {:<50} coupling: {:.3}  ({} shared commits)",
                entry.path, entry.coupling_score, entry.shared_commits
            ));
        }
    }

    lines.join("\n")
}

/// Format symbol-level diff between two git refs.
pub fn diff_symbols_result_view(
    base: &str,
    target: &str,
    changed_files: &[&str],
    repo: &crate::git::GitRepo,
    compact: bool,
) -> String {
    use std::collections::HashMap;

    let mut lines = Vec::new();
    lines.push(format!("Symbol diff: {base}...{target}"));
    lines.push(format!("{} files changed", changed_files.len()));
    lines.push(String::new());

    let mut total_added = 0usize;
    let mut total_removed = 0usize;
    let mut total_modified = 0usize;

    for file_path in changed_files {
        // Get content at base and target refs
        let base_content = repo
            .file_at_ref(base, file_path)
            .unwrap_or_default()
            .unwrap_or_default();

        let target_content = repo
            .file_at_ref(target, file_path)
            .unwrap_or_default()
            .unwrap_or_default();

        // Extract symbol names from both versions
        let base_symbols = extract_symbol_signatures(&base_content);
        let target_symbols = extract_symbol_signatures(&target_content);

        let base_names: HashMap<&str, &str> = base_symbols
            .iter()
            .map(|(n, s)| (n.as_str(), s.as_str()))
            .collect();
        let target_names: HashMap<&str, &str> = target_symbols
            .iter()
            .map(|(n, s)| (n.as_str(), s.as_str()))
            .collect();

        let mut file_added = Vec::new();
        let mut file_removed = Vec::new();
        let mut file_modified = Vec::new();

        // Find added and modified
        for (name, sig) in &target_names {
            match base_names.get(name) {
                None => file_added.push(*name),
                Some(base_sig) if base_sig != sig => file_modified.push(*name),
                _ => {}
            }
        }

        // Find removed
        for name in base_names.keys() {
            if !target_names.contains_key(name) {
                file_removed.push(*name);
            }
        }

        if file_added.is_empty() && file_removed.is_empty() && file_modified.is_empty() {
            continue; // No symbol-level changes
        }

        total_added += file_added.len();
        total_removed += file_removed.len();
        total_modified += file_modified.len();

        if compact {
            // Compact mode: one line per file with counts only
            let mut parts = Vec::new();
            if !file_added.is_empty() {
                parts.push(format!("+{}", file_added.len()));
            }
            if !file_removed.is_empty() {
                parts.push(format!("-{}", file_removed.len()));
            }
            if !file_modified.is_empty() {
                parts.push(format!("~{}", file_modified.len()));
            }
            lines.push(format!("  {} ({})", file_path, parts.join(", ")));
        } else {
            lines.push(format!("── {} ──", file_path));
            if !file_added.is_empty() {
                let mut sorted = file_added.clone();
                sorted.sort_unstable();
                for name in &sorted {
                    lines.push(format!("  + {name}"));
                }
            }
            if !file_removed.is_empty() {
                let mut sorted = file_removed.clone();
                sorted.sort_unstable();
                for name in &sorted {
                    lines.push(format!("  - {name}"));
                }
            }
            if !file_modified.is_empty() {
                let mut sorted = file_modified.clone();
                sorted.sort_unstable();
                for name in &sorted {
                    lines.push(format!("  ~ {name}"));
                }
            }
            lines.push(String::new());
        }
    }

    // Summary
    lines.push(format!(
        "Summary: +{total_added} added, -{total_removed} removed, ~{total_modified} modified"
    ));
    let files_with_symbol_changes = total_added + total_removed + total_modified;
    if files_with_symbol_changes == 0 && !changed_files.is_empty() {
        lines.push(format!(
            "Note: {} file(s) changed but no symbol boundaries were affected (changes in comments, whitespace, or non-symbol code).",
            changed_files.len()
        ));
    }

    lines.join("\n")
}

/// Extract symbol name → signature pairs from source code using simple pattern matching.
/// Returns Vec<(name, signature_line)> for functions, classes, structs, enums, traits, interfaces.
fn extract_symbol_signatures(content: &str) -> Vec<(String, String)> {
    let mut symbols = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        // Skip empty, comments, imports
        if trimmed.is_empty()
            || trimmed.starts_with("//")
            || trimmed.starts_with('#')
            || trimmed.starts_with("/*")
            || trimmed.starts_with('*')
            || trimmed.starts_with("use ")
            || trimmed.starts_with("import ")
            || trimmed.starts_with("from ")
        {
            continue;
        }

        // Match common symbol declaration patterns
        let name = extract_declaration_name(trimmed);
        if let Some(name) = name {
            symbols.push((name, trimmed.to_string()));
        }
    }
    symbols
}

/// Try to extract a declaration name from a line of code.
pub(crate) fn extract_declaration_name(line: &str) -> Option<String> {
    // Strip leading visibility modifier generically: pub, pub(crate), pub(super), pub(in path).
    let stripped = if let Some(rest) = line.strip_prefix("pub") {
        if let Some(after_paren) = rest.strip_prefix('(') {
            // Skip balanced parens: pub(crate), pub(super), pub(in crate::foo)
            if let Some(close) = after_paren.find(')') {
                after_paren[close + 1..].trim_start()
            } else {
                rest.trim_start()
            }
        } else {
            rest.trim_start()
        }
    } else if let Some(rest) = line.strip_prefix("export default ") {
        rest
    } else if let Some(rest) = line.strip_prefix("export ") {
        rest
    } else {
        line
    };

    let keywords = [
        "async fn ",
        "fn ",
        "struct ",
        "enum ",
        "trait ",
        "type ",
        "const ",
        "static ",
        "class ",
        "interface ",
        "function ",
        "async function ",
        "async def ",
        "def ",
    ];

    for kw in &keywords {
        if let Some(rest) = stripped.strip_prefix(kw) {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}
