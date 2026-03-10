/// Pure formatting functions for all 10 tool responses.
///
/// All functions take `&LiveIndex` (or data derived from it) and return `String`.
/// No I/O, no async. Output matches the locked formats defined in CONTEXT.md.
use crate::live_index::LiveIndex;

/// Format the file outline for a given path.
///
/// Header: `{path}  ({N} symbols)`
/// Body: each symbol indented by `depth * 2` spaces, then `{kind:<12} {name:<30} {start}-{end}`
/// Not-found: "File not found: {path}"
pub fn file_outline(index: &LiveIndex, path: &str) -> String {
    let file = match index.get_file(path) {
        Some(f) => f,
        None => return not_found_file(path),
    };

    let mut lines = Vec::new();
    lines.push(format!("{}  ({} symbols)", path, file.symbols.len()));

    for sym in &file.symbols {
        let indent = "  ".repeat(sym.depth as usize);
        let kind_str = sym.kind.to_string();
        // Format: indent + kind (left-padded to 12) + name (left-padded to 30) + line range
        lines.push(format!(
            "{}{:<12} {:<30} {}-{}",
            indent,
            kind_str,
            sym.name,
            sym.line_range.0,
            sym.line_range.1
        ));
    }

    lines.join("\n")
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
    let file = match index.get_file(path) {
        Some(f) => f,
        None => return not_found_file(path),
    };

    let sym = file.symbols.iter().find(|s| {
        s.name == name
            && kind_filter
                .map(|k| s.kind.to_string().eq_ignore_ascii_case(k))
                .unwrap_or(true)
    });

    match sym {
        None => not_found_symbol(index, path, name),
        Some(s) => {
            let start = s.byte_range.0 as usize;
            let end = s.byte_range.1 as usize;
            let body = if end <= file.content.len() {
                String::from_utf8_lossy(&file.content[start..end]).into_owned()
            } else {
                String::from_utf8_lossy(&file.content).into_owned()
            };
            let byte_count = end.saturating_sub(start);
            format!(
                "{}\n[{}, lines {}-{}, {} bytes]",
                body,
                s.kind,
                s.line_range.0,
                s.line_range.1,
                byte_count
            )
        }
    }
}

/// Search for symbols matching a query (case-insensitive substring).
///
/// Header: `{N} matches in {M} files`
/// Body: grouped by file, each match: `  {line_start}: {kind} {name}`
/// Empty: "No symbols matching '{query}'"
pub fn search_symbols_result(index: &LiveIndex, query: &str) -> String {
    let query_lower = query.to_lowercase();

    // Collect matches grouped by file, sorted by path for determinism
    let mut by_file: Vec<(String, Vec<String>)> = Vec::new();
    let mut total_matches = 0usize;

    let mut paths: Vec<&String> = index.all_files().map(|(p, _)| p).collect();
    paths.sort();

    for path in paths {
        let file = index.get_file(path).unwrap();
        let matches: Vec<String> = file
            .symbols
            .iter()
            .filter(|s| s.name.to_lowercase().contains(&query_lower))
            .map(|s| format!("  {}: {} {}", s.line_range.0, s.kind, s.name))
            .collect();

        if !matches.is_empty() {
            total_matches += matches.len();
            by_file.push((path.clone(), matches));
        }
    }

    if by_file.is_empty() {
        return format!("No symbols matching '{query}'");
    }

    let file_count = by_file.len();
    let mut lines = Vec::new();
    lines.push(format!("{total_matches} matches in {file_count} files"));

    for (path, matches) in &by_file {
        lines.push(path.clone());
        lines.extend_from_slice(matches);
    }

    lines.join("\n")
}

/// Search for text content matches (case-insensitive substring).
///
/// Header: `{N} matches in {M} files`
/// Body: grouped by file, each match: `  {line_number}: {line_content}`
/// Empty: "No matches for '{query}'"
pub fn search_text_result(index: &LiveIndex, query: &str) -> String {
    let query_lower = query.to_lowercase();

    let mut by_file: Vec<(String, Vec<String>)> = Vec::new();
    let mut total_matches = 0usize;

    let mut paths: Vec<&String> = index.all_files().map(|(p, _)| p).collect();
    paths.sort();

    for path in paths {
        let file = index.get_file(path).unwrap();
        let content_str = String::from_utf8_lossy(&file.content);

        let matches: Vec<String> = content_str
            .lines()
            .enumerate()
            .filter_map(|(i, line)| {
                // Trim \r for CRLF files
                let line = line.trim_end_matches('\r');
                if line.to_lowercase().contains(&query_lower) {
                    Some(format!("  {}: {}", i + 1, line))
                } else {
                    None
                }
            })
            .collect();

        if !matches.is_empty() {
            total_matches += matches.len();
            by_file.push((path.clone(), matches));
        }
    }

    if by_file.is_empty() {
        return format!("No matches for '{query}'");
    }

    let file_count = by_file.len();
    let mut lines = Vec::new();
    lines.push(format!("{total_matches} matches in {file_count} files"));

    for (path, matches) in &by_file {
        lines.push(path.clone());
        lines.extend_from_slice(matches);
    }

    lines.join("\n")
}

/// Generate a directory-tree overview of the repo.
///
/// Header: `{project_name}  ({N} files, {M} symbols)`
/// Body: sorted paths, each: `  {filename:<20} {language:<12} {symbol_count} symbols`
pub fn repo_outline(index: &LiveIndex, project_name: &str) -> String {
    let total_files = index.file_count();
    let total_symbols = index.symbol_count();

    let mut lines = Vec::new();
    lines.push(format!(
        "{project_name}  ({total_files} files, {total_symbols} symbols)"
    ));

    let mut paths: Vec<&String> = index.all_files().map(|(p, _)| p).collect();
    paths.sort();

    for path in paths {
        let file = index.get_file(path).unwrap();
        // Get just the filename for display
        let filename = path.rsplit('/').next().unwrap_or(path.as_str());
        lines.push(format!(
            "  {:<20} {:<12} {} symbols",
            filename,
            file.language.to_string(),
            file.symbols.len()
        ));
    }

    lines.join("\n")
}

/// Generate a health report for the index.
///
/// Format:
/// ```text
/// Status: {Ready|Empty|Degraded}
/// Files:  {N} indexed ({P} parsed, {PP} partial, {F} failed)
/// Symbols: {S}
/// Loaded in: {D}ms
/// Watcher: not active (Phase 3)
/// ```
pub fn health_report(index: &LiveIndex) -> String {
    use crate::live_index::IndexState;

    let state = index.index_state();
    let status = match state {
        IndexState::Empty => "Empty".to_string(),
        IndexState::Ready => "Ready".to_string(),
        IndexState::Loading => "Loading".to_string(),
        IndexState::CircuitBreakerTripped { .. } => "Degraded".to_string(),
    };

    let stats = index.health_stats();
    format!(
        "Status: {}\nFiles:  {} indexed ({} parsed, {} partial, {} failed)\nSymbols: {}\nLoaded in: {}ms\nWatcher: not active (Phase 3)",
        status,
        stats.file_count,
        stats.parsed_count,
        stats.partial_parse_count,
        stats.failed_count,
        stats.symbol_count,
        stats.load_duration.as_millis()
    )
}

/// List files changed since the given Unix timestamp.
///
/// If since_ts < loaded_at: return list of all files (entire index is "newer")
/// If since_ts >= loaded_at: return "No changes detected since last index load."
pub fn what_changed_result(index: &LiveIndex, since_ts: i64) -> String {
    use std::time::UNIX_EPOCH;

    let loaded_at = index.loaded_at_system();
    let loaded_secs = loaded_at
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    if since_ts < loaded_secs {
        // Entire index is newer — list all files
        let mut paths: Vec<&String> = index.all_files().map(|(p, _)| p).collect();
        paths.sort();
        if paths.is_empty() {
            return "No changes detected since last index load.".to_string();
        }
        paths.iter().map(|p| p.as_str()).collect::<Vec<_>>().join("\n")
    } else {
        "No changes detected since last index load.".to_string()
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
    let file = match index.get_file(path) {
        Some(f) => f,
        None => return not_found_file(path),
    };

    let content = String::from_utf8_lossy(&file.content);

    match (start_line, end_line) {
        (None, None) => content.into_owned(),
        (start, end) => {
            let start_idx = start.map(|s| s.saturating_sub(1) as usize).unwrap_or(0);
            let end_idx = end.map(|e| e as usize).unwrap_or(usize::MAX);

            let lines: Vec<&str> = content.lines().collect();
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

/// "File not found: {path}"
pub fn not_found_file(path: &str) -> String {
    format!("File not found: {path}")
}

/// "No symbol {name} in {path}. Symbols in that file: {comma-separated list}"
pub fn not_found_symbol(index: &LiveIndex, path: &str, name: &str) -> String {
    match index.get_file(path) {
        None => not_found_file(path),
        Some(file) => {
            let symbol_names: Vec<&str> = file.symbols.iter().map(|s| s.name.as_str()).collect();
            if symbol_names.is_empty() {
                format!("No symbol {name} in {path}. No symbols in that file.")
            } else {
                format!(
                    "No symbol {name} in {path}. Symbols in that file: {}",
                    symbol_names.join(", ")
                )
            }
        }
    }
}

/// "Index is loading... try again shortly."
pub fn loading_guard_message() -> String {
    "Index is loading... try again shortly.".to_string()
}

/// "Index not loaded. Call index_folder to index a directory."
pub fn empty_guard_message() -> String {
    "Index not loaded. Call index_folder to index a directory.".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{LanguageId, SymbolKind, SymbolRecord};
    use crate::live_index::store::{CircuitBreakerState, IndexedFile, LiveIndex, ParseStatus};
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    // --- Test helpers ---

    fn make_symbol(name: &str, kind: SymbolKind, depth: u32, line_start: u32, line_end: u32) -> SymbolRecord {
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
                content: content.to_vec(),
                symbols,
                parse_status: ParseStatus::Parsed,
                byte_len: content.len() as u64,
                content_hash: "test".to_string(),
            },
        )
    }

    fn make_index(files: Vec<(String, IndexedFile)>) -> LiveIndex {
        let cb = CircuitBreakerState::new(0.20);
        LiveIndex {
            files: files.into_iter().collect::<HashMap<_, _>>(),
            loaded_at: Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: Duration::from_millis(42),
            cb_state: cb,
            is_empty: false,
        }
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
        let (key, file) = make_file("src/lib.rs", b"struct MyStruct { fn my_method() {} }", symbols);
        let index = make_index(vec![(key, file)]);
        let result = file_outline(&index, "src/lib.rs");
        let lines: Vec<&str> = result.lines().collect();
        // Method at depth 1 should be indented by 2 spaces
        let method_line = lines.iter().find(|l| l.contains("my_method")).unwrap();
        assert!(method_line.starts_with("  "), "depth-1 symbol should be indented by 2 spaces");
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

    // --- symbol_detail tests ---

    #[test]
    fn test_symbol_detail_returns_body_and_footer() {
        let content = b"fn hello() { println!(\"hi\"); }";
        let sym = make_symbol_with_bytes("hello", SymbolKind::Function, 0, 1, 1, 0, 30);
        let (key, file) = make_file("src/lib.rs", content, vec![sym]);
        let index = make_index(vec![(key, file)]);
        let result = symbol_detail(&index, "src/lib.rs", "hello", None);
        assert!(result.contains("fn hello"), "should contain body");
        assert!(result.contains("[fn, lines 1-1, 30 bytes]"), "should contain footer");
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
        assert!(result.contains("[struct, lines 5-10"), "footer should show struct kind");
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
        assert!(result.starts_with("2 matches in 1 files"), "should start with summary");
    }

    #[test]
    fn test_search_symbols_case_insensitive() {
        let sym = make_symbol("GetUser", SymbolKind::Function, 0, 1, 5);
        let (key, file) = make_file("src/lib.rs", b"fn GetUser() {}", vec![sym]);
        let index = make_index(vec![(key, file)]);
        let result = search_symbols_result(&index, "getuser");
        assert!(!result.starts_with("No symbols"), "should find case-insensitive match");
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
    fn test_search_symbols_grouped_by_file() {
        let sym1 = make_symbol("foo", SymbolKind::Function, 0, 1, 5);
        let sym2 = make_symbol("foo_bar", SymbolKind::Function, 0, 1, 5);
        let (key1, file1) = make_file("a.rs", b"fn foo() {}", vec![sym1]);
        let (key2, file2) = make_file("b.rs", b"fn foo_bar() {}", vec![sym2]);
        let index = make_index(vec![(key1, file1), (key2, file2)]);
        let result = search_symbols_result(&index, "foo");
        assert!(result.contains("2 matches in 2 files"), "should show 2 files");
        assert!(result.contains("a.rs"), "should contain file a.rs");
        assert!(result.contains("b.rs"), "should contain file b.rs");
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
        assert!(result.contains("  2:"), "should show 1-indexed line number 2");
    }

    #[test]
    fn test_search_text_case_insensitive() {
        let (key, file) = make_file("src/lib.rs", b"Hello World", vec![]);
        let index = make_index(vec![(key, file)]);
        let result = search_text_result(&index, "hello world");
        assert!(!result.starts_with("No matches"), "should find case-insensitive");
    }

    #[test]
    fn test_search_text_no_match() {
        let (key, file) = make_file("src/lib.rs", b"fn main() {}", vec![]);
        let index = make_index(vec![(key, file)]);
        let result = search_text_result(&index, "xyz_totally_absent");
        assert_eq!(result, "No matches for 'xyz_totally_absent'");
    }

    #[test]
    fn test_search_text_crlf_handling() {
        let content = b"fn foo() {\r\n    let x = 1;\r\n}";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let index = make_index(vec![(key, file)]);
        let result = search_text_result(&index, "let x");
        assert!(result.contains("let x = 1"), "should find content without \\r");
    }

    // --- repo_outline tests ---

    #[test]
    fn test_repo_outline_header_totals() {
        let sym = make_symbol("main", SymbolKind::Function, 0, 1, 5);
        let (key, file) = make_file("src/main.rs", b"fn main() {}", vec![sym]);
        let index = make_index(vec![(key, file)]);
        let result = repo_outline(&index, "myproject");
        assert!(result.starts_with("myproject  (1 files, 1 symbols)"), "got: {result}");
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
        assert!(result.contains("Watcher: not active (Phase 3)"), "should have Watcher line");
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
        };
        let result = health_report(&index);
        assert!(result.contains("Status: Empty"), "got: {result}");
    }

    // --- what_changed_result tests ---

    #[test]
    fn test_what_changed_since_far_past_lists_all_files() {
        let sym = make_symbol("foo", SymbolKind::Function, 0, 1, 3);
        let (key, file) = make_file("src/lib.rs", b"fn foo() {}", vec![sym]);
        let index = make_index(vec![(key, file)]);
        // since_ts=0 (epoch) is before index was loaded
        let result = what_changed_result(&index, 0);
        assert!(result.contains("src/lib.rs"), "should list all files: {result}");
    }

    #[test]
    fn test_what_changed_since_far_future_returns_no_changes() {
        let (key, file) = make_file("src/lib.rs", b"fn foo() {}", vec![]);
        let index = make_index(vec![(key, file)]);
        // since_ts=far future — no changes
        let result = what_changed_result(&index, i64::MAX);
        assert_eq!(result, "No changes detected since last index load.");
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
}
