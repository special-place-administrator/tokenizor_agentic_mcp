/// Pure formatting functions for all 10 tool responses.
///
/// All functions take `&LiveIndex` (or data derived from it) and return `String`.
/// No I/O, no async. Output matches the locked formats defined in CONTEXT.md.
use crate::live_index::{
    ContextBundleSectionView, ContextBundleView, FileContentView, FileOutlineView,
    FindDependentsView, FindReferencesView, HealthStats, IndexedFile, LiveIndex,
    PublishedIndexState, RepoOutlineFileView, RepoOutlineView, ResolvePathView, SearchFilesTier,
    SearchFilesView, SymbolDetailView, WhatChangedTimestampView, search,
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
            indent, kind_str, sym.name, sym.line_range.0, sym.line_range.1
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
            let start = s.byte_range.0 as usize;
            let end = s.byte_range.1 as usize;
            let body = if end <= content.len() {
                String::from_utf8_lossy(&content[start..end]).into_owned()
            } else {
                String::from_utf8_lossy(content).into_owned()
            };
            let byte_count = end.saturating_sub(start);
            format!(
                "{}\n[{}, lines {}-{}, {} bytes]",
                body, s.kind, s.line_range.0, s.line_range.1, byte_count
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
    search_text_result_view(result)
}

pub fn search_text_result_view(
    result: Result<search::TextSearchResult, search::TextSearchError>,
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
            for line_match in &file.matches {
                lines.push(format!("  {}: {}", line_match.line_number, line_match.line));
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

pub fn search_files_result(index: &LiveIndex, query: &str, limit: usize) -> String {
    let view = index.capture_search_files_view(query, limit);
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
                        SearchFilesTier::StrongPath => "── Strong path matches ──",
                        SearchFilesTier::Basename => "── Basename matches ──",
                        SearchFilesTier::LoosePath => "── Loose path matches ──",
                    };
                    if lines.len() > 1 {
                        lines.push(String::new());
                    }
                    lines.push(header.to_string());
                }
                lines.push(format!("  {}", hit.path));
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
    if let Some(around_match) = context.around_match.as_deref() {
        return render_numbered_around_match_excerpt(
            file,
            around_match,
            context
                .context_lines
                .unwrap_or(DEFAULT_AROUND_LINE_CONTEXT_LINES),
        );
    }

    render_file_content_bytes(&file.content, context)
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
        &view.content,
        search::ContentContext::line_range(start_line, end_line),
    )
}

const DEFAULT_AROUND_LINE_CONTEXT_LINES: u32 = 2;

fn render_file_content_bytes(content: &[u8], context: search::ContentContext) -> String {
    let content = String::from_utf8_lossy(content);
    let lines: Vec<&str> = content.lines().collect();

    if let Some(around_line) = context.around_line {
        return render_numbered_around_line_excerpt(
            &lines,
            around_line,
            context
                .context_lines
                .unwrap_or(DEFAULT_AROUND_LINE_CONTEXT_LINES),
        );
    }

    match (context.start_line, context.end_line) {
        (None, None) => content.into_owned(),
        (start, end) => {
            let start_idx = start.map(|s| s.saturating_sub(1) as usize).unwrap_or(0);
            let end_idx = end.map(|e| e as usize).unwrap_or(usize::MAX);

            let sliced: Vec<&str> = lines
                .iter()
                .enumerate()
                .filter_map(|(i, line)| {
                    if i >= start_idx && i < end_idx {
                        Some(*line)
                    } else {
                        None
                    }
                })
                .collect();
            sliced.join("\n")
        }
    }
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

/// "No symbol {name} in {path}. Symbols in that file: {comma-separated list}"
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

fn not_found_symbol_names(relative_path: &str, symbol_names: &[String], name: &str) -> String {
    if symbol_names.is_empty() {
        format!("No symbol {name} in {relative_path}. No symbols in that file.")
    } else {
        format!(
            "No symbol {name} in {relative_path}. Symbols in that file: {}",
            symbol_names.join(", ")
        )
    }
}

/// Find all references for a name across the repo, grouped by file with 3-line context.
///
/// kind_filter: "call" | "import" | "type_usage" | "all" | None (all)
/// Output format matches CONTEXT.md decision AD-6 (compact human-readable).
pub fn find_references_result(index: &LiveIndex, name: &str, kind_filter: Option<&str>) -> String {
    let view = index.capture_find_references_view(name, kind_filter);
    find_references_result_view(&view, name)
}

pub fn find_references_result_view(view: &FindReferencesView, name: &str) -> String {
    if view.total_refs == 0 {
        return format!("No references found for \"{name}\"");
    }

    let total = view.total_refs;
    let file_count = view.files.len();
    let mut lines = vec![format!("{total} references in {file_count} files")];
    lines.push(String::new()); // blank line

    for file in &view.files {
        lines.push(file.file_path.clone());
        for hit in &file.hits {
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
        }
        lines.push(String::new()); // blank line between files
    }

    while lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        lines.pop();
    }

    lines.join("\n")
}

/// Find all files that import (depend on) the given path.
///
/// Output format: compact list grouped by importing file, each with import line.
pub fn find_dependents_result(index: &LiveIndex, path: &str) -> String {
    let view = index.capture_find_dependents_view(path);
    find_dependents_result_view(&view, path)
}

pub fn find_dependents_result_view(view: &FindDependentsView, path: &str) -> String {
    if view.files.is_empty() {
        return format!("No dependents found for \"{path}\"");
    }

    let file_count = view.files.len();
    let mut lines = vec![format!("{file_count} files depend on {path}")];
    lines.push(String::new()); // blank line

    for file in &view.files {
        lines.push(file.file_path.clone());
        for line in &file.lines {
            lines.push(format!(
                "  {}: {}   [{}]",
                line.line_number, line.line_content, line.kind
            ));
        }
        lines.push(String::new()); // blank line between files
    }

    // Remove trailing blank line
    while lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        lines.pop();
    }

    lines.join("\n")
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
    context_bundle_result_view(&view)
}

pub fn context_bundle_result_view(view: &ContextBundleView) -> String {
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
        ContextBundleView::Found(view) => {
            let mut output = format!(
                "{}\n[{}, lines {}-{}, {} bytes]\n",
                view.body, view.kind_label, view.line_range.0, view.line_range.1, view.byte_count
            );
            output.push_str(&format_context_bundle_section("Callers", &view.callers));
            output.push_str(&format_context_bundle_section("Callees", &view.callees));
            output.push_str(&format_context_bundle_section(
                "Type usages",
                &view.type_usages,
            ));
            output
        }
    }
}

fn format_context_bundle_section(title: &str, section: &ContextBundleSectionView) -> String {
    let mut lines = vec![format!("\n{title} ({}):", section.total_count)];

    for entry in &section.entries {
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
        lines.push(format!(
            "  ...and {} more {}",
            section.overflow_count,
            title.to_lowercase()
        ));
    }

    lines.join("\n")
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
            vec![make_symbol("main", SymbolKind::Function, 0, 1, 5)],
        );
        let index = make_index(vec![(key, file)]);
        let result = file_outline(&index, "src/main.rs");
        assert!(result.contains("fn"), "should contain fn kind");
        assert!(result.contains("main"), "should contain symbol name");
        assert!(result.contains("1-5"), "should contain line range");
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
        let sym = make_symbol_with_bytes("hello", SymbolKind::Function, 0, 1, 1, 0, 30);
        let (key, file) = make_file("src/lib.rs", content, vec![sym]);
        let index = make_index(vec![(key, file)]);
        let result = symbol_detail(&index, "src/lib.rs", "hello", None);
        assert!(result.contains("fn hello"), "should contain body");
        assert!(
            result.contains("[fn, lines 1-1, 30 bytes]"),
            "should contain footer"
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
            make_symbol("foo", SymbolKind::Function, 0, 1, 1),
            make_symbol("foo", SymbolKind::Struct, 0, 5, 10),
        ];
        let content = b"fn foo() {} struct foo {}";
        let (key, file) = make_file("src/lib.rs", content, symbols);
        let index = make_index(vec![(key, file)]);
        // Filter for struct kind
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
            search_text_result_view(search::search_text(&index, Some("let"), None, false));

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

        let rendered = search_text_result_view(result);

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
                },
                crate::live_index::SearchFilesHit {
                    tier: SearchFilesTier::Basename,
                    path: "src/sidecar/tools.rs".to_string(),
                },
                crate::live_index::SearchFilesHit {
                    tier: SearchFilesTier::LoosePath,
                    path: "src/protocol/tools_helper.rs".to_string(),
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
        let captured_result = find_references_result_view(
            &index.capture_find_references_view("process", None),
            "process",
        );

        assert_eq!(captured_result, live_result);
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
        );

        assert_eq!(captured_result, live_result);
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
            result.contains("[fn, lines"),
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
        let captured_result = context_bundle_result_view(&index.capture_context_bundle_view(
            "src/lib.rs",
            "process",
            None,
            None,
        ));

        assert_eq!(captured_result, live_result);
    }

    #[test]
    fn test_context_bundle_result_view_ambiguous_symbol() {
        let result = context_bundle_result_view(&ContextBundleView::AmbiguousSymbol {
            path: "src/lib.rs".to_string(),
            name: "process".to_string(),
            candidate_lines: vec![1, 10],
        });

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
}
