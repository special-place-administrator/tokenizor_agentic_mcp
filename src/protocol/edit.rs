use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::index::{LanguageId, SymbolRecord};
use crate::live_index::query::{
    render_symbol_selector, resolve_symbol_selector, SymbolSelectorMatch,
};
use crate::live_index::store::IndexedFile;
use crate::live_index::SharedIndex;

// ---------------------------------------------------------------------------
// Core splice
// ---------------------------------------------------------------------------

/// Splice `replacement` bytes into `content` at the given byte range [start, end).
pub(crate) fn apply_splice(content: &[u8], range: (u32, u32), replacement: &[u8]) -> Vec<u8> {
    let (start, end) = (range.0 as usize, range.1 as usize);
    let mut result = Vec::with_capacity(content.len() - (end - start) + replacement.len());
    result.extend_from_slice(&content[..start]);
    result.extend_from_slice(replacement);
    result.extend_from_slice(&content[end..]);
    result
}

// ---------------------------------------------------------------------------
// Atomic file write
// ---------------------------------------------------------------------------

/// Write content to a file atomically: write to a temp file, then rename.
pub(crate) fn atomic_write_file(path: &Path, content: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tokenizor_tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Reindex after write
// ---------------------------------------------------------------------------

/// Re-parse file content and update the live index. Call after writing to disk.
pub(crate) fn reindex_after_write(
    index: &SharedIndex,
    relative_path: &str,
    content: Vec<u8>,
    language: LanguageId,
) {
    let result = crate::parsing::process_file(relative_path, &content, language);
    let indexed = IndexedFile::from_parse_result(result, content);
    index.update_file(relative_path.to_string(), indexed);
}

// ---------------------------------------------------------------------------
// Symbol resolution wrapper
// ---------------------------------------------------------------------------

/// Resolve a symbol by name/kind/line, returning (index, cloned record) or user-friendly error.
pub(crate) fn resolve_or_error(
    file: &IndexedFile,
    name: &str,
    kind: Option<&str>,
    line: Option<u32>,
) -> Result<(usize, SymbolRecord), String> {
    match resolve_symbol_selector(file, name, kind, line) {
        SymbolSelectorMatch::Selected(idx, sym) => Ok((idx, sym.clone())),
        SymbolSelectorMatch::NotFound => {
            let label = render_symbol_selector(name, kind, line);
            Err(format!("Symbol not found: {label}"))
        }
        SymbolSelectorMatch::Ambiguous(candidate_lines) => {
            let candidates = candidate_lines
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            Err(format!(
                "Ambiguous: multiple definitions of `{name}`. \
                 Pass `symbol_line` to disambiguate. Candidate lines: {candidates}"
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// Indentation utilities
// ---------------------------------------------------------------------------

/// Detect the leading whitespace on the line containing `byte_offset`.
pub(crate) fn detect_indentation(content: &[u8], byte_offset: u32) -> Vec<u8> {
    let offset = byte_offset as usize;
    let line_start = content[..offset]
        .iter()
        .rposition(|&b| b == b'\n')
        .map(|p| p + 1)
        .unwrap_or(0);
    let indent_end = content[line_start..]
        .iter()
        .position(|b| !b.is_ascii_whitespace() || *b == b'\n')
        .unwrap_or(0);
    content[line_start..line_start + indent_end].to_vec()
}

/// Prefix each non-empty line of `text` with `indent`.
pub(crate) fn apply_indentation(text: &str, indent: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if i > 0 {
            result.push(b'\n');
        }
        if !line.is_empty() {
            result.extend_from_slice(indent);
            result.extend_from_slice(line.as_bytes());
        }
    }
    if text.ends_with('\n') {
        result.push(b'\n');
    }
    result
}

// ---------------------------------------------------------------------------
// Insert helpers
// ---------------------------------------------------------------------------

/// Build the bytes to insert before a symbol: indented content + blank line + existing content.
/// Splices at the start of the line (before existing indentation) so indentation isn't doubled.
pub(crate) fn build_insert_before(
    file_content: &[u8],
    sym: &SymbolRecord,
    new_code: &str,
) -> Vec<u8> {
    let sym_start = sym.byte_range.0 as usize;
    let line_start = file_content[..sym_start]
        .iter()
        .rposition(|&b| b == b'\n')
        .map(|p| p + 1)
        .unwrap_or(0) as u32;
    let indent = detect_indentation(file_content, sym.byte_range.0);
    let indented = apply_indentation(new_code, &indent);
    let mut insertion = indented;
    insertion.extend_from_slice(b"\n\n");
    apply_splice(file_content, (line_start, line_start), &insertion)
}

/// Build the bytes to insert after a symbol: existing content + blank line + indented content.
pub(crate) fn build_insert_after(
    file_content: &[u8],
    sym: &SymbolRecord,
    new_code: &str,
) -> Vec<u8> {
    let indent = detect_indentation(file_content, sym.byte_range.0);
    let indented = apply_indentation(new_code, &indent);
    let mut insertion = Vec::new();
    insertion.extend_from_slice(b"\n\n");
    insertion.extend_from_slice(&indented);
    apply_splice(
        file_content,
        (sym.byte_range.1, sym.byte_range.1),
        &insertion,
    )
}

// ---------------------------------------------------------------------------
// Input structs for tool handlers
// ---------------------------------------------------------------------------

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct ReplaceSymbolBodyInput {
    /// Relative file path.
    pub path: String,
    /// Symbol name to replace.
    pub name: String,
    /// Optional kind filter (e.g., "fn", "struct", "impl").
    pub kind: Option<String>,
    /// Line number to disambiguate when multiple symbols share the same name.
    #[serde(default, deserialize_with = "super::tools::lenient_u32")]
    pub symbol_line: Option<u32>,
    /// Complete new source code for the symbol (replaces the entire definition).
    pub new_body: String,
}

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct InsertSymbolInput {
    /// Relative file path.
    pub path: String,
    /// Name of the reference symbol to insert adjacent to.
    pub name: String,
    /// Optional kind filter.
    pub kind: Option<String>,
    /// Line number to disambiguate.
    #[serde(default, deserialize_with = "super::tools::lenient_u32")]
    pub symbol_line: Option<u32>,
    /// Code to insert. Will be indented to match the target symbol's indentation.
    pub content: String,
}

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct DeleteSymbolInput {
    /// Relative file path.
    pub path: String,
    /// Symbol name to delete.
    pub name: String,
    /// Optional kind filter.
    pub kind: Option<String>,
    /// Line number to disambiguate.
    #[serde(default, deserialize_with = "super::tools::lenient_u32")]
    pub symbol_line: Option<u32>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct EditWithinSymbolInput {
    /// Relative file path.
    pub path: String,
    /// Symbol name that scopes the edit.
    pub name: String,
    /// Optional kind filter.
    pub kind: Option<String>,
    /// Line number to disambiguate.
    #[serde(default, deserialize_with = "super::tools::lenient_u32")]
    pub symbol_line: Option<u32>,
    /// Old text to find within the symbol body (literal match).
    pub old_text: String,
    /// Replacement text.
    pub new_text: String,
    /// If true, replace all occurrences within the symbol. Default: false (first match only).
    #[serde(default)]
    pub replace_all: bool,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::index::SymbolKind;

    // -- apply_splice --

    #[test]
    fn test_apply_splice_replaces_middle() {
        let content = b"fn foo() { old_body }";
        let result = apply_splice(content, (11, 19), b"new_body");
        assert_eq!(result, b"fn foo() { new_body }");
    }

    #[test]
    fn test_apply_splice_replaces_at_start() {
        let content = b"old_start rest";
        let result = apply_splice(content, (0, 9), b"new");
        assert_eq!(result, b"new rest");
    }

    #[test]
    fn test_apply_splice_replaces_at_end() {
        let content = b"prefix old_end";
        let result = apply_splice(content, (7, 14), b"new_end");
        assert_eq!(result, b"prefix new_end");
    }

    #[test]
    fn test_apply_splice_empty_replacement_deletes() {
        let content = b"keep_this remove_this keep_that";
        let result = apply_splice(content, (10, 21), b"");
        assert_eq!(result, b"keep_this  keep_that");
    }

    #[test]
    fn test_apply_splice_empty_range_inserts() {
        let content = b"ab";
        let result = apply_splice(content, (1, 1), b"X");
        assert_eq!(result, b"aXb");
    }

    // -- atomic_write_file --

    #[test]
    fn test_atomic_write_file_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.rs");
        atomic_write_file(&path, b"fn main() {}").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"fn main() {}");
    }

    #[test]
    fn test_atomic_write_file_overwrites_existing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.rs");
        std::fs::write(&path, b"old content").unwrap();
        atomic_write_file(&path, b"new content").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new content");
    }

    #[test]
    fn test_atomic_write_file_no_leftover_tmp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.rs");
        atomic_write_file(&path, b"content").unwrap();
        let tmp = path.with_extension("tokenizor_tmp");
        assert!(!tmp.exists());
    }

    // -- reindex_after_write --

    #[test]
    fn test_reindex_after_write_updates_index() {
        let handle = crate::live_index::LiveIndex::empty();
        let content = b"fn hello() {}\nfn world() {}\n".to_vec();
        reindex_after_write(&handle, "src/lib.rs", content, LanguageId::Rust);
        let guard = handle.read().expect("lock");
        let file = guard.get_file("src/lib.rs");
        assert!(file.is_some());
        let symbols = &file.unwrap().symbols;
        assert!(symbols.iter().any(|s| s.name == "hello"));
        assert!(symbols.iter().any(|s| s.name == "world"));
    }

    #[test]
    fn test_reindex_after_write_replaces_existing_entry() {
        let handle = crate::live_index::LiveIndex::empty();
        let v1 = b"fn alpha() {}\n".to_vec();
        reindex_after_write(&handle, "src/lib.rs", v1, LanguageId::Rust);
        let v2 = b"fn beta() {}\n".to_vec();
        reindex_after_write(&handle, "src/lib.rs", v2, LanguageId::Rust);

        let guard = handle.read().expect("lock");
        let file = guard.get_file("src/lib.rs").unwrap();
        assert!(!file.symbols.iter().any(|s| s.name == "alpha"));
        assert!(file.symbols.iter().any(|s| s.name == "beta"));
    }

    // -- resolve_or_error --

    fn make_test_indexed_file(symbols: Vec<SymbolRecord>) -> IndexedFile {
        IndexedFile {
            relative_path: "test.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::index::FileClassification::for_code_path("test.rs"),
            content: Vec::new(),
            symbols,
            parse_status: crate::live_index::store::ParseStatus::Parsed,
            byte_len: 0,
            content_hash: String::new(),
            references: Vec::new(),
            alias_map: std::collections::HashMap::new(),
        }
    }

    fn make_test_symbol(
        name: &str,
        kind: SymbolKind,
        byte_range: (u32, u32),
        line_start: u32,
    ) -> SymbolRecord {
        SymbolRecord {
            name: name.to_string(),
            kind,
            depth: 0,
            sort_order: 0,
            byte_range,
            line_range: (line_start, line_start + 2),
        }
    }

    #[test]
    fn test_resolve_or_error_finds_exact() {
        let file = make_test_indexed_file(vec![
            make_test_symbol("foo", SymbolKind::Function, (0, 20), 1),
            make_test_symbol("bar", SymbolKind::Function, (22, 50), 5),
        ]);
        let result = resolve_or_error(&file, "foo", None, None);
        assert!(result.is_ok());
        let (idx, sym) = result.unwrap();
        assert_eq!(idx, 0);
        assert_eq!(sym.name, "foo");
    }

    #[test]
    fn test_resolve_or_error_not_found() {
        let file = make_test_indexed_file(vec![make_test_symbol(
            "foo",
            SymbolKind::Function,
            (0, 20),
            1,
        )]);
        let result = resolve_or_error(&file, "baz", None, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_resolve_or_error_ambiguous_shows_candidates() {
        let file = make_test_indexed_file(vec![
            make_test_symbol("foo", SymbolKind::Function, (0, 20), 1),
            make_test_symbol("foo", SymbolKind::Function, (22, 50), 5),
        ]);
        let result = resolve_or_error(&file, "foo", None, None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Ambiguous"), "error was: {err}");
        assert!(err.contains("symbol_line"), "error was: {err}");
    }

    #[test]
    fn test_resolve_or_error_disambiguates_by_kind() {
        let file = make_test_indexed_file(vec![
            make_test_symbol("Foo", SymbolKind::Struct, (0, 20), 1),
            make_test_symbol("Foo", SymbolKind::Impl, (22, 80), 5),
        ]);
        let result = resolve_or_error(&file, "Foo", Some("struct"), None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().1.kind, SymbolKind::Struct);
    }

    #[test]
    fn test_resolve_or_error_disambiguates_by_line() {
        let file = make_test_indexed_file(vec![
            make_test_symbol("foo", SymbolKind::Function, (0, 20), 1),
            make_test_symbol("foo", SymbolKind::Function, (22, 50), 5),
        ]);
        let result = resolve_or_error(&file, "foo", None, Some(5));
        assert!(result.is_ok());
        assert_eq!(result.unwrap().0, 1);
    }

    // -- indentation --

    #[test]
    fn test_detect_indentation_spaces() {
        let content = b"fn outer() {\n    fn inner() {}\n}";
        let indent = detect_indentation(content, 14);
        assert_eq!(indent, b"    ");
    }

    #[test]
    fn test_detect_indentation_tabs() {
        let content = b"fn outer() {\n\tfn inner() {}\n}";
        let indent = detect_indentation(content, 14);
        assert_eq!(indent, b"\t");
    }

    #[test]
    fn test_detect_indentation_no_indent() {
        let content = b"fn top_level() {}";
        let indent = detect_indentation(content, 0);
        assert_eq!(indent, b"");
    }

    #[test]
    fn test_detect_indentation_at_newline_boundary() {
        let content = b"line1\nline2";
        let indent = detect_indentation(content, 6);
        assert_eq!(indent, b"");
    }

    #[test]
    fn test_apply_indentation_adds_prefix() {
        let result = apply_indentation("fn new() {\n    body;\n}", b"    ");
        let text = std::str::from_utf8(&result).unwrap();
        assert_eq!(text, "    fn new() {\n        body;\n    }");
    }

    #[test]
    fn test_apply_indentation_preserves_empty_lines() {
        let result = apply_indentation("a\n\nb", b"  ");
        let text = std::str::from_utf8(&result).unwrap();
        assert_eq!(text, "  a\n\n  b");
    }

    #[test]
    fn test_apply_indentation_empty_indent_is_identity() {
        let result = apply_indentation("fn foo() {}", b"");
        assert_eq!(result, b"fn foo() {}");
    }

    // -- insert helpers --

    #[test]
    fn test_build_insert_before_adds_content_with_indent() {
        let content = b"    fn existing() {}\n";
        let sym = make_test_symbol("existing", SymbolKind::Function, (4, 20), 1);
        let result = build_insert_before(content, &sym, "fn new_fn() {}");
        let text = std::str::from_utf8(&result).unwrap();
        assert!(
            text.starts_with("    fn new_fn() {}\n\n    fn existing"),
            "got: {text}"
        );
    }

    #[test]
    fn test_build_insert_after_adds_content_with_indent() {
        let content = b"    fn existing() {}";
        let sym = make_test_symbol("existing", SymbolKind::Function, (4, 20), 1);
        let result = build_insert_after(content, &sym, "fn new_fn() {}");
        let text = std::str::from_utf8(&result).unwrap();
        assert!(
            text.contains("fn existing() {}\n\n    fn new_fn() {}"),
            "got: {text}"
        );
    }
}
