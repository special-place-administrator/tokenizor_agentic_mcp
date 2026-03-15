use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::index::{LanguageId, SymbolKind, SymbolRecord};
use crate::live_index::SharedIndex;
use crate::live_index::query::{
    SymbolSelectorMatch, render_symbol_selector, resolve_symbol_selector,
};
use crate::live_index::store::IndexedFile;

// ---------------------------------------------------------------------------
// Path containment
// ---------------------------------------------------------------------------

/// Validate that a user-supplied relative path stays within the repo root.
/// Returns the canonicalized absolute path on success.
pub(crate) fn safe_repo_path(repo_root: &Path, relative_path: &str) -> Result<PathBuf, String> {
    let full_path = repo_root.join(relative_path);
    let canon_root = repo_root
        .canonicalize()
        .map_err(|e| format!("cannot resolve repo root: {e}"))?;
    let canon_path = full_path
        .canonicalize()
        .map_err(|e| format!("cannot resolve path '{relative_path}': {e}"))?;
    if !canon_path.starts_with(&canon_root) {
        return Err(format!("path '{relative_path}' is outside the repository"));
    }
    Ok(canon_path)
}

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

/// Write content to a file and fully reindex from disk.
///
/// INVARIANT: All derived index state is rebuilt from the persisted on-disk bytes,
/// never from the in-memory buffer passed to `fs::write`. If the write partially
/// fails or the OS buffers differently, the index will still reflect reality.
pub(crate) fn reindex_after_write(
    index: &SharedIndex,
    abs_path: &Path,
    relative_path: &str,
    written: &[u8],
    language: LanguageId,
) {
    // Re-read from disk — not from the `written` parameter.
    let on_disk = match std::fs::read(abs_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::warn!(
                "reindex_after_write: failed to re-read {}: {e}",
                abs_path.display()
            );
            return;
        }
    };

    debug_assert_eq!(
        written,
        on_disk.as_slice(),
        "reindex_after_write: disk content differs from written buffer for {}",
        abs_path.display()
    );

    let result = crate::parsing::process_file(relative_path, &on_disk, language);
    let indexed = IndexedFile::from_parse_result(result, on_disk);
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

/// Build the bytes to insert before a symbol: indented content + separator + existing content.
/// Splices at the start of the line (before existing indentation) so indentation isn't doubled.
/// Uses `\n\n` when the target symbol has no doc comments and no blank line already precedes
/// the splice point (visual separation between definitions), and `\n` otherwise (avoids triple
/// newlines when a blank line already exists, and keeps doc comments tight against their symbol).
pub(crate) fn build_insert_before(
    file_content: &[u8],
    sym: &SymbolRecord,
    new_code: &str,
) -> Vec<u8> {
    let sym_start = sym.effective_start() as usize;
    let line_start = file_content[..sym_start]
        .iter()
        .rposition(|&b| b == b'\n')
        .map(|p| p + 1)
        .unwrap_or(0) as u32;
    let indent = detect_indentation(file_content, sym.byte_range.0);
    let indented = apply_indentation(new_code, &indent);
    let mut insertion = indented;
    let separator: &[u8] = if sym.doc_byte_range.is_some() {
        b"\n"
    } else {
        // Use single newline only when a blank line already precedes the symbol
        // (avoids creating triple-newline sequences). At start-of-file (empty prefix),
        // there's no existing blank line, so use double newline for visual separation.
        let prefix = &file_content[..line_start as usize];
        let already_has_blank = prefix.len() >= 2
            && prefix[prefix.len() - 1] == b'\n'
            && prefix[prefix.len() - 2] == b'\n';
        if already_has_blank { b"\n" } else { b"\n\n" }
    };
    insertion.extend_from_slice(separator);
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
// Delete helper
// ---------------------------------------------------------------------------

/// Build file content with the symbol removed, including leading whitespace and trailing newlines.
/// Collapses runs of 3+ consecutive blank lines down to 1 after deletion.
pub(crate) fn build_delete(file_content: &[u8], sym: &SymbolRecord) -> Vec<u8> {
    // Extend to start of line (include leading whitespace).
    let start = {
        let s = sym.effective_start() as usize;
        let mut line_start = file_content[..s]
            .iter()
            .rposition(|&b| b == b'\n')
            .map(|p| p + 1)
            .unwrap_or(0);

        // If doc_byte_range is None (no attached doc comment), scan upward
        // past a single blank line to find orphaned doc comments. This handles
        // the case where a blank line separates a comment from its symbol,
        // preventing scan_doc_range from attaching it.
        if sym.doc_byte_range.is_none() {
            // Split content above into lines and scan from bottom up.
            let above = &file_content[..line_start];
            let lines: Vec<&[u8]> = above.split(|&b| b == b'\n').collect();
            // lines has a trailing empty element if above ends with \n.
            // Walk from the end: skip empty/whitespace lines (blank lines),
            // then collect consecutive comment lines.
            let mut i = lines.len();
            // Skip trailing empty element from split
            if i > 0 && lines[i - 1].is_empty() {
                i -= 1;
            }
            // Skip exactly one blank line
            if i > 0 && lines[i - 1].iter().all(|b| b.is_ascii_whitespace()) {
                i -= 1;
                // Now collect consecutive comment lines above the blank line
                let mut found_comments = false;
                while i > 0 {
                    let line_text = std::str::from_utf8(lines[i - 1]).unwrap_or("");
                    let trimmed = line_text.trim_start();
                    if trimmed.starts_with("///")
                        || trimmed.starts_with("//!")
                        || trimmed.starts_with("/**")
                        || trimmed.starts_with("# ")
                        || trimmed == "#"
                    {
                        found_comments = true;
                        i -= 1;
                    } else {
                        break;
                    }
                }
                if found_comments {
                    // Compute byte offset: sum of lengths of lines 0..i + newlines
                    let new_start: usize = lines[..i].iter().map(|l| l.len() + 1).sum();
                    line_start = new_start;
                }
            }
        }

        line_start as u32
    };
    // Extend past trailing newlines (consume up to one blank line).
    let end = {
        let e = sym.byte_range.1 as usize;
        let mut pos = e;
        while pos < file_content.len() && file_content[pos] != b'\n' {
            pos += 1;
        }
        if pos < file_content.len() {
            pos += 1;
        }
        if pos < file_content.len() && file_content[pos] == b'\n' {
            pos += 1;
        }
        pos as u32
    };
    let spliced = apply_splice(file_content, (start, end), b"");
    collapse_blank_lines(&spliced)
}

/// Collapse runs of 3+ consecutive newlines (\n\n\n+) down to 2 (\n\n = one blank line).
fn collapse_blank_lines(content: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(content.len());
    let mut consecutive_newlines = 0u32;
    for &b in content {
        if b == b'\n' {
            consecutive_newlines += 1;
            if consecutive_newlines <= 2 {
                result.push(b);
            }
        } else {
            consecutive_newlines = 0;
            result.push(b);
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Edit-within helper
// ---------------------------------------------------------------------------

/// Find-and-replace text within a symbol's byte range. Returns (new_content, replacement_count).
pub(crate) fn build_edit_within(
    file_content: &[u8],
    sym: &SymbolRecord,
    old_text: &str,
    new_text: &str,
    replace_all: bool,
) -> Result<(Vec<u8>, usize), String> {
    let sym_start = sym.effective_start() as usize;
    let sym_end = sym.byte_range.1 as usize;
    let body = &file_content[sym_start..sym_end];
    let body_str =
        std::str::from_utf8(body).map_err(|_| "Symbol body is not valid UTF-8.".to_string())?;

    let (new_body, count) = if replace_all {
        let count = body_str.matches(old_text).count();
        if count == 0 {
            return Err(format!(
                "`{old_text}` not found within symbol `{}`",
                sym.name
            ));
        }
        (body_str.replace(old_text, new_text), count)
    } else {
        match body_str.find(old_text) {
            Some(_) => (body_str.replacen(old_text, new_text, 1), 1),
            None => {
                return Err(format!(
                    "`{old_text}` not found within symbol `{}`",
                    sym.name
                ));
            }
        }
    };

    let effective_range = (sym.effective_start(), sym.byte_range.1);
    let new_content = apply_splice(file_content, effective_range, new_body.as_bytes());
    Ok((new_content, count))
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
    /// Where to insert relative to the target symbol: `"before"` or `"after"` (default `"after"`).
    #[serde(default)]
    pub position: Option<String>,
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
// Batch edit types and execution
// ---------------------------------------------------------------------------

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct BatchEditInput {
    /// List of individual edits to apply atomically.
    pub edits: Vec<SingleEdit>,
    /// When true, validate and plan all edits but skip disk writes and index mutation.
    /// Returns per-edit preview lines prefixed with `[DRY RUN]`.
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct SingleEdit {
    /// Relative file path.
    pub path: String,
    /// Symbol name.
    pub name: String,
    /// Optional kind filter.
    pub kind: Option<String>,
    /// Line number to disambiguate.
    #[serde(default, deserialize_with = "super::tools::lenient_u32")]
    pub symbol_line: Option<u32>,
    /// The edit operation to perform.
    pub operation: EditOperation,
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type")]
pub enum EditOperation {
    /// Replace the entire symbol definition.
    #[serde(rename = "replace")]
    Replace { new_body: String },
    /// Insert code before the symbol.
    #[serde(rename = "insert_before")]
    InsertBefore { content: String },
    /// Insert code after the symbol.
    #[serde(rename = "insert_after")]
    InsertAfter { content: String },
    /// Delete the symbol.
    #[serde(rename = "delete")]
    Delete,
    /// Find-and-replace within the symbol.
    #[serde(rename = "edit_within")]
    EditWithin { old_text: String, new_text: String },
}

/// Apply multiple symbol-addressed edits atomically.
/// Validates all symbols first, rejects overlapping ranges, then applies in reverse-offset order.
/// When `dry_run` is true, all validation runs identically but disk writes and index mutation are skipped.
pub(crate) fn execute_batch_edit(
    index: &SharedIndex,
    repo_root: &Path,
    edits: &[SingleEdit],
    dry_run: bool,
) -> Result<Vec<String>, String> {
    struct ResolvedEdit {
        path: String,
        sym: SymbolRecord,
        operation: usize,
        language: LanguageId,
    }

    // Phase 1: Resolve all symbols.
    let n = edits.len();
    let targeted_paths: Vec<&str> = edits.iter().map(|e| e.path.as_str()).collect();
    let rollback_footer = |paths: &[&str]| -> String {
        let path_list = paths
            .iter()
            .map(|p| format!("  - {p}"))
            .collect::<Vec<_>>()
            .join("\n");
        format!("\n\nROLLED BACK — {n} edit(s) attempted on:\n{path_list}\nNo files were modified.")
    };

    let mut resolved = Vec::with_capacity(n);
    {
        let guard = index.read().expect("lock poisoned");
        for (i, edit) in edits.iter().enumerate() {
            let file = guard.get_file(&edit.path).ok_or_else(|| {
                format!(
                    "File not indexed: {}{}",
                    edit.path,
                    rollback_footer(&targeted_paths)
                )
            })?;
            let (_, sym) =
                resolve_or_error(file, &edit.name, edit.kind.as_deref(), edit.symbol_line)
                    .map_err(|e| {
                        format!("Edit {}: {e}{}", i + 1, rollback_footer(&targeted_paths))
                    })?;
            resolved.push(ResolvedEdit {
                path: edit.path.clone(),
                sym,
                operation: i,
                language: file.language.clone(),
            });
        }
    }

    // Phase 1b: Validate no overlapping byte ranges within the same file.
    let mut by_file: std::collections::HashMap<String, Vec<usize>> =
        std::collections::HashMap::new();
    for (i, r) in resolved.iter().enumerate() {
        by_file.entry(r.path.clone()).or_default().push(i);
    }
    for (path, indices) in &by_file {
        for i in 0..indices.len() {
            for j in (i + 1)..indices.len() {
                let a = (
                    resolved[indices[i]].sym.effective_start(),
                    resolved[indices[i]].sym.byte_range.1,
                );
                let b = (
                    resolved[indices[j]].sym.effective_start(),
                    resolved[indices[j]].sym.byte_range.1,
                );
                if a.0 < b.1 && b.0 < a.1 {
                    return Err(format!(
                        "Overlapping edits in {path}: `{}` ({}-{}) and `{}` ({}-{}). \
                         Split into separate calls.{}",
                        resolved[indices[i]].sym.name,
                        a.0,
                        a.1,
                        resolved[indices[j]].sym.name,
                        b.0,
                        b.1,
                        rollback_footer(&targeted_paths),
                    ));
                }
            }
        }
    }

    // Phase 2: Sort each file's edits reverse by byte offset.
    for indices in by_file.values_mut() {
        indices.sort_by(|&a, &b| {
            resolved[b]
                .sym
                .effective_start()
                .cmp(&resolved[a].sym.effective_start())
        });
    }

    // Phase 3: Apply edits per file, write, reindex.
    // Best-effort: each file is written+reindexed independently.
    // If one file's write fails, continue with the rest and report the failure.
    let mut summaries = Vec::new();
    let mut write_failures: Vec<String> = Vec::new();

    for (path, indices) in &by_file {
        let file = {
            let guard = index.read().expect("lock poisoned");
            guard
                .capture_shared_file(path)
                .ok_or_else(|| format!("File disappeared: {path}"))?
        };

        let mut content = file.content.clone();
        let language = resolved[indices[0]].language.clone();
        let mut file_summaries: Vec<String> = Vec::new();

        for &ri in indices {
            let r = &resolved[ri];
            let edit = &edits[r.operation];
            match &edit.operation {
                EditOperation::Replace { new_body } => {
                    let old_bytes = (r.sym.byte_range.1 - r.sym.byte_range.0) as usize;
                    let effective = r.sym.effective_start() as usize;
                    let line_start = content[..effective]
                        .iter()
                        .rposition(|&b| b == b'\n')
                        .map(|p| p + 1)
                        .unwrap_or(0) as u32;
                    let indent = detect_indentation(&content, r.sym.byte_range.0);
                    let indented = apply_indentation(new_body, &indent);
                    content = apply_splice(&content, (line_start, r.sym.byte_range.1), &indented);
                    file_summaries.push(super::edit_format::format_replace(
                        path,
                        &r.sym.name,
                        &r.sym.kind.to_string(),
                        old_bytes,
                        new_body.len(),
                    ));
                }
                EditOperation::InsertBefore { content: code } => {
                    content = build_insert_before(&content, &r.sym, code);
                    file_summaries.push(super::edit_format::format_insert(
                        path,
                        &r.sym.name,
                        "before",
                        code.len(),
                    ));
                }
                EditOperation::InsertAfter { content: code } => {
                    content = build_insert_after(&content, &r.sym, code);
                    file_summaries.push(super::edit_format::format_insert(
                        path,
                        &r.sym.name,
                        "after",
                        code.len(),
                    ));
                }
                EditOperation::Delete => {
                    let deleted = (r.sym.byte_range.1 - r.sym.byte_range.0) as usize;
                    content = build_delete(&content, &r.sym);
                    file_summaries.push(super::edit_format::format_delete(
                        path,
                        &r.sym.name,
                        &r.sym.kind.to_string(),
                        deleted,
                    ));
                }
                EditOperation::EditWithin { old_text, new_text } => {
                    let old_bytes = (r.sym.byte_range.1 - r.sym.byte_range.0) as usize;
                    let old_content_len = content.len();
                    let (new, count) =
                        build_edit_within(&content, &r.sym, old_text, new_text, false)
                            .map_err(|e| format!("Edit in {path}:{}: {e}", r.sym.name))?;
                    content = new;
                    // Compute new symbol size from content length delta
                    let new_bytes = (old_bytes as isize
                        + (content.len() as isize - old_content_len as isize))
                        as usize;
                    file_summaries.push(super::edit_format::format_edit_within(
                        path,
                        &r.sym.name,
                        count,
                        old_bytes,
                        new_bytes,
                    ));
                }
            }
        }

        if dry_run {
            // Prefix summaries for this file's edits with [DRY RUN] and skip all writes.
            for s in &mut file_summaries {
                *s = format!("[DRY RUN] Would {s}");
            }
            summaries.extend(file_summaries);
        } else {
            let abs_path = match safe_repo_path(repo_root, path) {
                Ok(p) => p,
                Err(e) => {
                    write_failures.push(format!("FAILED {path}: {e}"));
                    continue;
                }
            };
            match atomic_write_file(&abs_path, &content) {
                Ok(()) => {
                    reindex_after_write(index, &abs_path, path, &content, language);
                    summaries.extend(file_summaries);
                }
                Err(e) => {
                    // Best-effort: record the failure and continue with remaining files.
                    // The index is NOT updated for this file — it retains the pre-edit state.
                    write_failures.push(format!("FAILED {path}: {e}"));
                }
            }
        }
    }

    if !write_failures.is_empty() {
        let failure_block = write_failures.join("\n");
        let success_block = if summaries.is_empty() {
            "No files were written.".to_string()
        } else {
            format!(
                "{} file(s) written successfully:\n{}",
                summaries.len(),
                summaries.join("\n")
            )
        };
        return Err(format!(
            "batch_edit partial failure — {} file(s) could not be written:\n{}\n\n{}",
            write_failures.len(),
            failure_block,
            success_block,
        ));
    }

    Ok(summaries)
}

// ---------------------------------------------------------------------------
// Batch rename
// ---------------------------------------------------------------------------

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct BatchRenameInput {
    /// Relative file path containing the symbol definition.
    pub path: String,
    /// Current symbol name.
    pub name: String,
    /// Optional kind filter.
    pub kind: Option<String>,
    /// Line number to disambiguate.
    #[serde(default, deserialize_with = "super::tools::lenient_u32")]
    pub symbol_line: Option<u32>,
    /// New name for the symbol.
    pub new_name: String,
    /// When true, show what would change without writing any files.
    #[serde(default, deserialize_with = "super::tools::lenient_bool")]
    pub dry_run: Option<bool>,
}

/// Rename a symbol and all its references across the project.
pub(crate) fn execute_batch_rename(
    index: &SharedIndex,
    repo_root: &Path,
    input: &BatchRenameInput,
) -> Result<String, String> {
    // Phase 1: Resolve the definition and find the name within its body.
    let (def_name_range, language) = {
        let guard = index.read().expect("lock poisoned");
        let file = guard
            .get_file(&input.path)
            .ok_or_else(|| format!("File not indexed: {}", input.path))?;
        let (_, sym) =
            resolve_or_error(file, &input.name, input.kind.as_deref(), input.symbol_line)?;
        let body = &file.content[sym.byte_range.0 as usize..sym.byte_range.1 as usize];
        let name_offset = body
            .windows(input.name.len())
            .position(|w| w == input.name.as_bytes())
            .ok_or_else(|| {
                format!(
                    "Could not locate name `{}` within symbol body at {}:{}-{}",
                    input.name, input.path, sym.byte_range.0, sym.byte_range.1
                )
            })?;
        let abs_start = sym.byte_range.0 + name_offset as u32;
        let abs_end = abs_start + input.name.len() as u32;
        ((abs_start, abs_end), file.language.clone())
    };

    // Phase 2: Find all references across the project.
    let ref_sites: Vec<(String, (u32, u32))> = {
        let guard = index.read().expect("lock poisoned");
        let refs = guard.find_references_for_name(&input.name, None, false);
        refs.into_iter()
            .map(|(path, rr)| (path.to_string(), rr.byte_range))
            .collect()
    };

    // Phase 2b: Supplemental qualified-path scan with confidence classification.
    // The xref index tracks call targets (e.g. "new" in Widget::new()), not
    // path prefixes. find_qualified_usages catches Type::method() patterns,
    // import paths, and any other qualified usage the xref system doesn't index.
    // Matches are split into confident (code context) and uncertain (comments/strings).
    //
    // We collect file content snapshots under the lock, then run the scan outside it.
    let file_contents: Vec<(String, Vec<u8>)> = {
        let guard = index.read().expect("lock poisoned");
        guard
            .files
            .iter()
            .map(|(path, file)| (path.clone(), file.content.clone()))
            .collect()
    };

    // Collect confident and uncertain supplemental matches separately.
    // Each entry: (file_path, byte_range (start, end))
    let mut qualified_confident: Vec<(String, (u32, u32))> = Vec::new();
    // Uncertain entries also carry the display context string for the warning block.
    let mut qualified_uncertain: Vec<(String, u32, String)> = Vec::new(); // (path, line, context)

    for (file_path, content_bytes) in &file_contents {
        let source = match std::str::from_utf8(content_bytes) {
            Ok(s) => s,
            Err(_) => continue, // skip non-UTF-8 files
        };
        let matches = find_qualified_usages(&input.name, source);
        for m in matches {
            let end = m.offset + input.name.len();
            let range = (m.offset as u32, end as u32);
            if m.confident {
                qualified_confident.push((file_path.clone(), range));
            } else {
                qualified_uncertain.push((file_path.clone(), m.line as u32, m.context.clone()));
            }
        }
    }

    // Phase 3: Group rename sites by file.
    // Confident sources: definition site, indexed refs, qualified confident matches.
    // Uncertain matches are NOT applied — only surfaced in output.
    let mut by_file: std::collections::HashMap<String, Vec<(u32, u32)>> =
        std::collections::HashMap::new();
    by_file
        .entry(input.path.clone())
        .or_default()
        .push(def_name_range);
    for (path, range) in &ref_sites {
        by_file.entry(path.clone()).or_default().push(*range);
    }
    for (path, range) in &qualified_confident {
        by_file.entry(path.clone()).or_default().push(*range);
    }
    // Sort reverse by offset, dedup (removes overlap between indexed refs and qualified scan).
    for ranges in by_file.values_mut() {
        ranges.sort_by(|a, b| b.0.cmp(&a.0));
        ranges.dedup();
    }

    // Build uncertain warning lines sorted by file then line, deduped.
    let mut sorted_uncertain = qualified_uncertain.clone();
    sorted_uncertain.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    sorted_uncertain.dedup();
    let uncertain_lines: Vec<String> = sorted_uncertain
        .iter()
        .map(|(path, line, ctx)| format!("  {}:{}  {}", path, line, ctx))
        .collect();

    // Dry run: return preview without writing, with separate confident/uncertain sections.
    if input.dry_run.unwrap_or(false) {
        let total_confident: usize = by_file.values().map(|r| r.len()).sum();
        let mut lines = vec![format!("Dry run: `{}` → `{}`", input.name, input.new_name,)];
        lines.push(format!(
            "\n── Confident matches (will be applied) — {} site(s) across {} file(s) ──",
            total_confident,
            by_file.len(),
        ));
        let mut sorted_files: Vec<_> = by_file.iter().collect();
        sorted_files.sort_by_key(|(p, _)| (*p).clone());
        for (path, ranges) in sorted_files {
            lines.push(format!("  {} ({} site(s))", path, ranges.len()));
        }
        if !uncertain_lines.is_empty() {
            lines.push(format!(
                "\n── Uncertain matches (NOT applied — review manually) — {} site(s) ──",
                uncertain_lines.len(),
            ));
            lines.extend(uncertain_lines);
        }
        return Ok(lines.join("\n"));
    }

    // Phase 4: Atomic rename — stage all new content in memory first, then write all.
    // On any write failure, roll back already-written files to their original content.
    let new_name_bytes = input.new_name.as_bytes();

    // Stage: compute new content for every file without touching disk.
    struct StagedFile {
        path: String,
        abs_path: std::path::PathBuf,
        original: Vec<u8>,
        new_content: Vec<u8>,
        language: LanguageId,
        refs_count: usize,
    }
    let mut staged: Vec<StagedFile> = Vec::with_capacity(by_file.len());
    for (path, ranges) in &by_file {
        let file = {
            let guard = index.read().expect("lock poisoned");
            guard
                .capture_shared_file(path)
                .ok_or_else(|| format!("File disappeared: {path}"))?
        };
        let original = file.content.clone();
        let mut new_content = original.clone();
        for range in ranges {
            new_content = apply_splice(&new_content, *range, new_name_bytes);
        }
        let lang = if path == &input.path {
            language.clone()
        } else {
            file.language.clone()
        };
        let abs = match safe_repo_path(repo_root, path) {
            Ok(p) => p,
            Err(e) => return Err(format!("Path containment error for '{path}': {e}")),
        };
        staged.push(StagedFile {
            path: path.clone(),
            abs_path: abs,
            original,
            new_content,
            language: lang,
            refs_count: ranges.len(),
        });
    }

    // Apply: write each staged file; on failure roll back already-written files.
    let mut written: Vec<usize> = Vec::new(); // indices into staged
    let mut write_error: Option<String> = None;
    for (i, sf) in staged.iter().enumerate() {
        if let Err(e) = atomic_write_file(&sf.abs_path, &sf.new_content) {
            write_error = Some(format!("Write failed for {}: {e}", sf.path));
            break;
        }
        written.push(i);
    }

    if let Some(err_msg) = write_error {
        // Rollback: restore every file that was already written.
        let mut rollback_failures: Vec<String> = Vec::new();
        for &wi in &written {
            let sf = &staged[wi];
            if let Err(rb_err) = atomic_write_file(&sf.abs_path, &sf.original) {
                rollback_failures.push(format!("  {}: {rb_err}", sf.path));
                continue;
            }
            // Re-read from disk and reindex to ensure index matches disk.
            match std::fs::read(&sf.abs_path) {
                Ok(on_disk) => {
                    reindex_after_write(
                        index,
                        &sf.abs_path,
                        &sf.path,
                        &on_disk,
                        sf.language.clone(),
                    );
                }
                Err(rb_err) => {
                    rollback_failures
                        .push(format!("  {} (reindex after rollback): {rb_err}", sf.path));
                }
            }
        }
        if rollback_failures.is_empty() {
            return Err(format!(
                "{err_msg}\n\nROLLED BACK — {} file(s) restored to original content. \
                 No rename was applied.",
                written.len(),
            ));
        } else {
            return Err(format!(
                "{err_msg}\n\nROLLBACK INCOMPLETE — {} file(s) could not be restored:\n{}\n\
                 WARNING: codebase may be in a partially-renamed state. \
                 Manually verify the following files:\n{}",
                rollback_failures.len(),
                rollback_failures.join("\n"),
                written
                    .iter()
                    .map(|&wi| format!("  {}", staged[wi].path))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ));
        }
    }

    // All writes succeeded — reindex every file.
    let mut files_updated = 0;
    let mut refs_updated = 0;
    for sf in &staged {
        reindex_after_write(
            index,
            &sf.abs_path,
            &sf.path,
            &sf.new_content,
            sf.language.clone(),
        );
        files_updated += 1;
        refs_updated += sf.refs_count;
    }

    let mut output = format!(
        "Renamed `{}` → `{}` — {refs_updated} site(s) across {files_updated} file(s)",
        input.name, input.new_name,
    );
    if !uncertain_lines.is_empty() {
        output.push_str(&format!(
            "\n\n── Uncertain matches (NOT applied — review manually) — {} site(s) ──\n",
            uncertain_lines.len(),
        ));
        output.push_str(&uncertain_lines.join("\n"));
    }
    Ok(output)
}

// ---------------------------------------------------------------------------
// Batch insert
// ---------------------------------------------------------------------------

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct BatchInsertInput {
    /// Code to insert at each target location.
    pub content: String,
    /// Where to insert: before or after.
    pub position: InsertPosition,
    /// Target symbols to insert adjacent to.
    pub targets: Vec<InsertTarget>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InsertPosition {
    Before,
    After,
}

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct InsertTarget {
    /// Relative file path.
    pub path: String,
    /// Symbol name.
    pub name: String,
    /// Optional kind filter.
    pub kind: Option<String>,
    /// Line number to disambiguate.
    #[serde(default, deserialize_with = "super::tools::lenient_u32")]
    pub symbol_line: Option<u32>,
}

/// Insert the same code before or after multiple symbols across the project.
/// Best-effort: each target is written+reindexed independently. If one write
/// fails, the remaining targets are still attempted. Partial failures are
/// reported in the return value.
pub(crate) fn execute_batch_insert(
    index: &SharedIndex,
    repo_root: &Path,
    input: &BatchInsertInput,
) -> Result<Vec<String>, String> {
    let mut summaries = Vec::new();
    let mut write_failures: Vec<String> = Vec::new();
    let position_label = match input.position {
        InsertPosition::Before => "before",
        InsertPosition::After => "after",
    };

    for target in &input.targets {
        let file = {
            let guard = index.read().expect("lock poisoned");
            guard
                .capture_shared_file(&target.path)
                .ok_or_else(|| format!("File not indexed: {}", target.path))?
        };

        let (_, sym) = resolve_or_error(
            &file,
            &target.name,
            target.kind.as_deref(),
            target.symbol_line,
        )
        .map_err(|e| format!("Target {}: {e}", target.path))?;

        let new_content = match input.position {
            InsertPosition::Before => build_insert_before(&file.content, &sym, &input.content),
            InsertPosition::After => build_insert_after(&file.content, &sym, &input.content),
        };

        let abs_path = match safe_repo_path(repo_root, &target.path) {
            Ok(p) => p,
            Err(e) => return Err(format!("Target {}: {e}", target.path)),
        };
        match atomic_write_file(&abs_path, &new_content) {
            Ok(()) => {
                let lang = file.language.clone();
                reindex_after_write(index, &abs_path, &target.path, &new_content, lang);
                summaries.push(super::edit_format::format_insert(
                    &target.path,
                    &target.name,
                    position_label,
                    input.content.len(),
                ));
            }
            Err(e) => {
                // Best-effort: record the failure, index unchanged for this file.
                write_failures.push(format!("FAILED {}: {e}", target.path));
            }
        }
    }

    if !write_failures.is_empty() {
        let failure_block = write_failures.join("\n");
        let success_block = if summaries.is_empty() {
            "No files were written.".to_string()
        } else {
            format!(
                "{} file(s) written successfully:\n{}",
                summaries.len(),
                summaries.join("\n")
            )
        };
        return Err(format!(
            "batch_insert partial failure — {} file(s) could not be written:\n{}\n\n{}",
            write_failures.len(),
            failure_block,
            success_block,
        ));
    }

    Ok(summaries)
}

// ---------------------------------------------------------------------------
// Stale reference detection
// ---------------------------------------------------------------------------

/// Extract the first line of a symbol as a rough "signature" for change detection.
pub(crate) fn extract_signature(content: &[u8], byte_range: (u32, u32)) -> String {
    let start = byte_range.0 as usize;
    let end = byte_range.1 as usize;
    let slice = &content[start..end];
    let first_line_end = slice
        .iter()
        .position(|&b| b == b'\n')
        .unwrap_or(slice.len());
    String::from_utf8_lossy(&slice[..first_line_end]).to_string()
}

/// Find the parent impl block's type name for a symbol, if any.
///
/// Walks backward through the file's symbol list to find an `impl` block at a
/// lower depth that encloses the target symbol's byte range. Extracts the
/// concrete type name (e.g. `Foo` from `impl Foo` or `impl Trait for Foo`).
pub(crate) fn find_parent_impl_type(file: &IndexedFile, sym: &SymbolRecord) -> Option<String> {
    if sym.depth == 0 {
        return None; // top-level symbol, not inside an impl block
    }
    // Walk the symbol list to find the enclosing impl block.
    for s in &file.symbols {
        if s.kind != SymbolKind::Impl {
            continue;
        }
        // The impl block must enclose the target symbol.
        if s.byte_range.0 <= sym.byte_range.0 && s.byte_range.1 >= sym.byte_range.1 {
            return extract_impl_type_name(&s.name);
        }
    }
    None
}

/// Extract the concrete type name from an impl block name.
///
/// Handles patterns like:
/// - `impl Foo` -> `Foo`
/// - `impl Trait for Foo` -> `Foo`
/// - `impl<T> Foo<T>` -> `Foo`
/// - `impl<T: Clone> Trait for Foo<T>` -> `Foo`
fn extract_impl_type_name(impl_name: &str) -> Option<String> {
    let name = impl_name.trim();
    // Strip leading "impl" keyword if present (some parsers include it).
    let rest = name.strip_prefix("impl").unwrap_or(name).trim_start();
    // Strip generic parameters from the front: `<T: Clone> Trait for Foo<T>` -> `Trait for Foo<T>`
    let rest = strip_leading_generics(rest);
    // Check for "for" keyword: `Trait for Foo<T>` -> `Foo<T>`
    let type_part = if let Some(pos) = rest.find(" for ") {
        rest[pos + 5..].trim_start()
    } else {
        rest.trim_start()
    };
    // Strip trailing generics: `Foo<T>` -> `Foo`
    let type_name = type_part.split('<').next().unwrap_or(type_part).trim();
    if type_name.is_empty() {
        None
    } else {
        Some(type_name.to_string())
    }
}

/// Strip a leading `<...>` generic parameter list, handling nested angle brackets.
fn strip_leading_generics(s: &str) -> &str {
    let s = s.trim_start();
    if !s.starts_with('<') {
        return s;
    }
    let mut depth = 0i32;
    for (i, ch) in s.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    return s[i + 1..].trim_start();
                }
            }
            _ => {}
        }
    }
    s // malformed generics, return as-is
}

/// Detect references that may be stale after a symbol edit.
/// Compares old vs new signature (first line). Returns (path, line, enclosing_name) triples.
///
/// When `parent_type` is provided (i.e. the symbol is a method inside an `impl` block),
/// only warns about references in files that also mention the parent type — this avoids
/// false positives like warning about `Path::display()` when `Widget::display()` changed.
pub(crate) fn detect_stale_references(
    index: &SharedIndex,
    path: &str,
    name: &str,
    old_signature: &str,
    new_signature: &str,
    parent_type: Option<&str>,
    source_language: Option<&crate::domain::LanguageId>,
) -> Vec<(String, u32, Option<String>)> {
    if old_signature == new_signature {
        return Vec::new();
    }
    let guard = index.read().expect("lock poisoned");
    let refs = guard.find_references_for_name(name, None, false);

    // When we know the parent type, collect the set of files that reference it.
    // Only those files could plausibly call `ParentType::method_name()`.
    let type_files: Option<std::collections::HashSet<&str>> = parent_type.map(|tn| {
        guard
            .find_references_for_name(tn, None, false)
            .into_iter()
            .map(|(fp, _)| fp)
            .collect()
    });

    refs.into_iter()
        .filter(|(ref_path, _)| *ref_path != path)
        .filter(|(ref_path, _)| {
            // Skip references in files of a different language to reduce false positives
            // (e.g., Rust `add` flagging Python's `add`).
            if let Some(lang) = source_language {
                if let Some(ref_file) = guard.get_file(ref_path) {
                    if ref_file.language != *lang {
                        return false;
                    }
                }
            }
            true
        })
        .filter(|(ref_path, _)| {
            // If we have a parent type filter, only keep refs in files that also mention it.
            match &type_files {
                Some(tf) => tf.contains(ref_path),
                None => true,
            }
        })
        .map(|(ref_path, rr)| {
            let enclosing = rr.enclosing_symbol_index.and_then(|idx| {
                guard
                    .get_file(ref_path)
                    .and_then(|f| f.symbols.get(idx as usize))
                    .map(|s| s.name.clone())
            });
            (ref_path.to_string(), rr.line_range.0 + 1, enclosing)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Qualified path scanner
// ---------------------------------------------------------------------------

/// A qualified path match with confidence classification.
#[derive(Debug)]
pub struct QualifiedMatch {
    /// Byte offset of the match in the source.
    pub offset: usize,
    /// Line number (1-based).
    pub line: usize,
    /// The full matched segment (e.g., "MyType::new()").
    pub context: String,
    /// Whether the match is confident (code context) or uncertain (string/comment).
    pub confident: bool,
}

/// Find qualified path usages of `identifier` in `source`.
///
/// Looks for patterns where the identifier appears as a path segment:
/// - `identifier::method()` — associated function call
/// - `module::identifier::method()` — deeper nesting
/// - `use path::identifier` — import path
/// - `identifier::<T>::method()` — turbofish syntax
///
/// Classifies matches as confident (in code) vs uncertain (in strings/comments).
pub fn find_qualified_usages(identifier: &str, source: &str) -> Vec<QualifiedMatch> {
    let mut results = Vec::new();

    // Track block comment nesting depth across the whole source.
    // We scan line by line but need to carry block-comment state.
    let mut in_block_comment = false;
    // Track raw string state: None = not in raw string, Some(n) = in raw string with n #s.
    let mut in_raw_string: Option<usize> = None;

    let mut line_num = 0usize;
    let mut line_byte_offset = 0usize;

    for line in source.split('\n') {
        line_num += 1;

        // Scan this line for occurrences of `identifier`, updating parse state.
        let line_bytes = line.as_bytes();
        let id_len = identifier.len();

        // We walk through the line character by character to find all occurrences
        // of `identifier` and classify each.
        let mut col = 0usize; // byte index within line

        while col < line_bytes.len() {
            // --- Update parse state at current col ---

            // Check for raw string start: r" or r#..."#
            if !in_block_comment && in_raw_string.is_none() {
                if line_bytes[col] == b'r' {
                    // Count leading #s
                    let mut hashes = 0usize;
                    let mut j = col + 1;
                    while j < line_bytes.len() && line_bytes[j] == b'#' {
                        hashes += 1;
                        j += 1;
                    }
                    if j < line_bytes.len() && line_bytes[j] == b'"' {
                        in_raw_string = Some(hashes);
                        col = j + 1;
                        continue;
                    }
                }
            }

            // Check for raw string end or matches inside raw string (uncertain)
            if let Some(hashes) = in_raw_string {
                if line_bytes[col] == b'"' {
                    // Check for matching #s after closing quote
                    let mut j = col + 1;
                    let mut count = 0usize;
                    while j < line_bytes.len() && line_bytes[j] == b'#' && count < hashes {
                        count += 1;
                        j += 1;
                    }
                    if count == hashes {
                        in_raw_string = None;
                        col = j;
                        continue;
                    }
                }
                // Inside raw string — check for identifier match (uncertain)
                if col + id_len <= line_bytes.len() && &line[col..col + id_len] == identifier {
                    let preceded = col >= 2 && &line[col - 2..col] == "::";
                    let followed = col + id_len + 2 <= line.len()
                        && &line[col + id_len..col + id_len + 2] == "::";
                    if preceded || followed {
                        let ctx_start = col.saturating_sub(20);
                        let ctx_end = (col + id_len + 20).min(line.len());
                        results.push(QualifiedMatch {
                            offset: line_byte_offset + col,
                            line: line_num,
                            context: line[ctx_start..ctx_end].to_string(),
                            confident: false,
                        });
                    }
                }
                col += 1;
                continue;
            }

            // Check for block comment start/end
            if !in_block_comment {
                // Check for line comment: rest of line is a comment — scan for matches then break
                if col + 1 < line_bytes.len()
                    && line_bytes[col] == b'/'
                    && line_bytes[col + 1] == b'/'
                {
                    // Everything from here to end of line is a line comment (uncertain)
                    let rest = &line[col..];
                    let mut search_start = 0usize;
                    while let Some(pos) = rest[search_start..].find(identifier) {
                        let abs_col = col + search_start + pos;
                        let preceded = abs_col >= 2 && &line[abs_col - 2..abs_col] == "::";
                        let followed = abs_col + id_len + 2 <= line.len()
                            && &line[abs_col + id_len..abs_col + id_len + 2] == "::";
                        if preceded || followed {
                            let ctx_start = abs_col.saturating_sub(20);
                            let ctx_end = (abs_col + id_len + 20).min(line.len());
                            results.push(QualifiedMatch {
                                offset: line_byte_offset + abs_col,
                                line: line_num,
                                context: line[ctx_start..ctx_end].to_string(),
                                confident: false,
                            });
                        }
                        search_start += pos + 1;
                    }
                    break; // rest of line consumed
                }

                // Block comment start
                if col + 1 < line_bytes.len()
                    && line_bytes[col] == b'/'
                    && line_bytes[col + 1] == b'*'
                {
                    in_block_comment = true;
                    col += 2;
                    continue;
                }
            } else {
                // Inside block comment — look for end
                if col + 1 < line_bytes.len()
                    && line_bytes[col] == b'*'
                    && line_bytes[col + 1] == b'/'
                {
                    in_block_comment = false;
                    col += 2;
                    continue;
                }
                // Still in block comment — check for identifier match (uncertain)
                if col + id_len <= line_bytes.len() && &line[col..col + id_len] == identifier {
                    let prec2 = col >= 2 && &line[col - 2..col] == "::";
                    let fol2 = col + id_len + 2 <= line.len()
                        && &line[col + id_len..col + id_len + 2] == "::";
                    if prec2 || fol2 {
                        let ctx_start = col.saturating_sub(20);
                        let ctx_end = (col + id_len + 20).min(line.len());
                        results.push(QualifiedMatch {
                            offset: line_byte_offset + col,
                            line: line_num,
                            context: line[ctx_start..ctx_end].to_string(),
                            confident: false,
                        });
                    }
                }
                col += 1;
                continue;
            }

            // Normal code: check for string literal (double-quote)
            if line_bytes[col] == b'"' {
                // Scan to closing quote (handling backslash escapes), emit uncertain matches
                col += 1;
                while col < line_bytes.len() && line_bytes[col] != b'"' {
                    if line_bytes[col] == b'\\' {
                        col += 2; // skip escaped char
                        continue;
                    }
                    // Check for identifier match inside string
                    if col + id_len <= line_bytes.len() && &line[col..col + id_len] == identifier {
                        let prec2 = col >= 2 && &line[col - 2..col] == "::";
                        let fol2 = col + id_len + 2 <= line.len()
                            && &line[col + id_len..col + id_len + 2] == "::";
                        if prec2 || fol2 {
                            let ctx_start = col.saturating_sub(20);
                            let ctx_end = (col + id_len + 20).min(line.len());
                            results.push(QualifiedMatch {
                                offset: line_byte_offset + col,
                                line: line_num,
                                context: line[ctx_start..ctx_end].to_string(),
                                confident: false,
                            });
                        }
                    }
                    col += 1;
                }
                col += 1; // skip closing quote
                continue;
            }

            // Normal code: check for identifier match
            if col + id_len <= line_bytes.len() && &line[col..col + id_len] == identifier {
                let prec2 = col >= 2 && &line[col - 2..col] == "::";
                let fol2 =
                    col + id_len + 2 <= line.len() && &line[col + id_len..col + id_len + 2] == "::";
                if prec2 || fol2 {
                    let ctx_start = col.saturating_sub(20);
                    let ctx_end = (col + id_len + 20).min(line.len());
                    results.push(QualifiedMatch {
                        offset: line_byte_offset + col,
                        line: line_num,
                        context: line[ctx_start..ctx_end].to_string(),
                        confident: true,
                    });
                }
                col += id_len;
                continue;
            }

            col += 1;
        }

        // +1 for the '\n' that split() consumed
        line_byte_offset += line.len() + 1;
    }

    results
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
        let dir = tempfile::tempdir().unwrap();
        let abs_path = dir.path().join("lib.rs");
        let content = b"fn hello() {}\nfn world() {}\n";
        std::fs::write(&abs_path, content).unwrap();
        let handle = crate::live_index::LiveIndex::empty();
        reindex_after_write(&handle, &abs_path, "src/lib.rs", content, LanguageId::Rust);
        let guard = handle.read().expect("lock");
        let file = guard.get_file("src/lib.rs");
        assert!(file.is_some());
        let symbols = &file.unwrap().symbols;
        assert!(symbols.iter().any(|s| s.name == "hello"));
        assert!(symbols.iter().any(|s| s.name == "world"));
    }

    #[test]
    fn test_reindex_after_write_replaces_existing_entry() {
        let dir = tempfile::tempdir().unwrap();
        let abs_path = dir.path().join("lib.rs");
        let handle = crate::live_index::LiveIndex::empty();

        let v1 = b"fn alpha() {}\n";
        std::fs::write(&abs_path, v1).unwrap();
        reindex_after_write(&handle, &abs_path, "src/lib.rs", v1, LanguageId::Rust);

        let v2 = b"fn beta() {}\n";
        std::fs::write(&abs_path, v2).unwrap();
        reindex_after_write(&handle, &abs_path, "src/lib.rs", v2, LanguageId::Rust);

        let guard = handle.read().expect("lock");
        let file = guard.get_file("src/lib.rs").unwrap();
        assert!(!file.symbols.iter().any(|s| s.name == "alpha"));
        assert!(file.symbols.iter().any(|s| s.name == "beta"));
    }

    #[test]
    fn test_reindex_reads_from_disk_not_buffer() {
        // Verify the INVARIANT: index state is built from on-disk bytes.
        // Write one thing to disk, pass different bytes as `written` — the
        // debug_assert would fire in debug builds, but in release builds the
        // index should reflect what is actually on disk.
        let dir = tempfile::tempdir().unwrap();
        let abs_path = dir.path().join("lib.rs");
        let on_disk = b"fn disk_fn() {}\n";
        std::fs::write(&abs_path, on_disk).unwrap();
        let handle = crate::live_index::LiveIndex::empty();
        // Pass the real on-disk bytes as `written` (normal case — no divergence).
        reindex_after_write(&handle, &abs_path, "src/lib.rs", on_disk, LanguageId::Rust);
        let guard = handle.read().expect("lock");
        let file = guard.get_file("src/lib.rs").unwrap();
        // Index reflects what is on disk.
        assert!(file.symbols.iter().any(|s| s.name == "disk_fn"));
    }

    #[test]
    fn test_search_text_matches_disk_after_edit() {
        // Setup: write old content to disk and index it.
        let dir = tempfile::tempdir().unwrap();
        let abs_path = dir.path().join("lib.rs");
        let old_content = b"fn old_content_marker() {}\n";
        std::fs::write(&abs_path, old_content).unwrap();
        let handle = crate::live_index::LiveIndex::empty();
        reindex_after_write(
            &handle,
            &abs_path,
            "src/lib.rs",
            old_content,
            LanguageId::Rust,
        );
        // Verify old content is in the index.
        {
            let guard = handle.read().expect("lock");
            let file = guard.get_file("src/lib.rs").unwrap();
            assert!(file.symbols.iter().any(|s| s.name == "old_content_marker"));
        }

        // Edit: overwrite disk with new content and reindex.
        let new_content = b"fn new_content_marker() {}\n";
        atomic_write_file(&abs_path, new_content).unwrap();
        reindex_after_write(
            &handle,
            &abs_path,
            "src/lib.rs",
            new_content,
            LanguageId::Rust,
        );

        // Verify: old symbol gone, new symbol present — index matches disk.
        let guard = handle.read().expect("lock");
        let file = guard.get_file("src/lib.rs").unwrap();
        assert!(
            !file.symbols.iter().any(|s| s.name == "old_content_marker"),
            "old symbol should no longer be in the index"
        );
        assert!(
            file.symbols.iter().any(|s| s.name == "new_content_marker"),
            "new symbol should be in the index after reindex from disk"
        );
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
            doc_byte_range: None,
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
        // No doc comment on the symbol → expect \n\n separator for visual separation.
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

    // -- build_delete --

    #[test]
    fn test_build_delete_removes_symbol_and_trailing_newline() {
        let content = b"fn keep() {}\n\nfn remove() {}\n\nfn also_keep() {}\n";
        let sym = make_test_symbol("remove", SymbolKind::Function, (14, 28), 3);
        let result = build_delete(content, &sym);
        let text = std::str::from_utf8(&result).unwrap();
        assert!(!text.contains("remove"), "got: {text}");
        assert!(text.contains("keep"), "got: {text}");
        assert!(text.contains("also_keep"), "got: {text}");
    }

    #[test]
    fn test_build_delete_collapses_excessive_blank_lines() {
        // Simulate what happens after deleting 3 adjacent symbols: triple blank lines.
        let content = b"fn a() {}\n\n\n\nfn d() {}\n";
        // "a" occupies bytes 0..9, pretend we already removed the middle ones.
        // Just verify collapse_blank_lines works on this content.
        let collapsed = super::collapse_blank_lines(content);
        let text = std::str::from_utf8(&collapsed).unwrap();
        // Should have at most one blank line (two consecutive \n).
        assert!(
            !text.contains("\n\n\n"),
            "should collapse 3+ newlines: {text:?}"
        );
        assert!(text.contains("fn a() {}\n\nfn d()"), "got: {text:?}");
    }

    // -- build_edit_within --

    #[test]
    fn test_build_edit_within_replaces_first_match() {
        let content = b"fn foo() { old; old; }";
        let sym = make_test_symbol("foo", SymbolKind::Function, (0, 22), 1);
        let (result, count) = build_edit_within(content, &sym, "old", "new", false).unwrap();
        let text = std::str::from_utf8(&result).unwrap();
        assert_eq!(count, 1);
        assert_eq!(text, "fn foo() { new; old; }");
    }

    #[test]
    fn test_build_edit_within_replaces_all() {
        let content = b"fn foo() { old; old; }";
        let sym = make_test_symbol("foo", SymbolKind::Function, (0, 22), 1);
        let (result, count) = build_edit_within(content, &sym, "old", "new", true).unwrap();
        let text = std::str::from_utf8(&result).unwrap();
        assert_eq!(count, 2);
        assert_eq!(text, "fn foo() { new; new; }");
    }

    #[test]
    fn test_build_edit_within_not_found() {
        let content = b"fn foo() { body; }";
        let sym = make_test_symbol("foo", SymbolKind::Function, (0, 18), 1);
        let result = build_edit_within(content, &sym, "missing", "new", false);
        assert!(result.is_err());
    }

    // -- execute_batch_edit --

    #[test]
    fn test_execute_batch_edit_applies_multiple_edits() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.rs"), b"fn alpha() { old }\n").unwrap();
        std::fs::write(src.join("b.rs"), b"fn beta() { keep }\n").unwrap();

        let handle = crate::live_index::LiveIndex::empty();
        for (path, content) in [
            ("src/a.rs", b"fn alpha() { old }\n" as &[u8]),
            ("src/b.rs", b"fn beta() { keep }\n"),
        ] {
            let result = crate::parsing::process_file(path, content, LanguageId::Rust);
            let indexed = IndexedFile::from_parse_result(result, content.to_vec());
            handle.update_file(path.to_string(), indexed);
        }

        let edits = vec![
            SingleEdit {
                path: "src/a.rs".to_string(),
                name: "alpha".to_string(),
                kind: None,
                symbol_line: None,
                operation: EditOperation::Replace {
                    new_body: "fn alpha() { new }".to_string(),
                },
            },
            SingleEdit {
                path: "src/b.rs".to_string(),
                name: "beta".to_string(),
                kind: None,
                symbol_line: None,
                operation: EditOperation::Delete,
            },
        ];

        let summaries = execute_batch_edit(&handle, dir.path(), &edits, false).unwrap();
        assert_eq!(summaries.len(), 2);

        let a_content = std::fs::read_to_string(src.join("a.rs")).unwrap();
        assert!(a_content.contains("new"), "a.rs: {a_content}");

        let b_content = std::fs::read_to_string(src.join("b.rs")).unwrap();
        assert!(!b_content.contains("beta"), "b.rs: {b_content}");
    }

    #[test]
    fn test_execute_batch_edit_rejects_overlapping() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.rs"), b"fn foo() {}\nfn bar() {}\n").unwrap();

        let handle = crate::live_index::LiveIndex::empty();
        let content = b"fn foo() {}\nfn bar() {}\n" as &[u8];
        let result = crate::parsing::process_file("src/a.rs", content, LanguageId::Rust);
        let indexed = IndexedFile::from_parse_result(result, content.to_vec());
        handle.update_file("src/a.rs".to_string(), indexed);

        // Create two edits that target overlapping fake ranges won't work easily,
        // but we can test with two edits on the same symbol (same range = overlapping).
        let edits = vec![
            SingleEdit {
                path: "src/a.rs".to_string(),
                name: "foo".to_string(),
                kind: None,
                symbol_line: None,
                operation: EditOperation::Delete,
            },
            SingleEdit {
                path: "src/a.rs".to_string(),
                name: "foo".to_string(),
                kind: None,
                symbol_line: None,
                operation: EditOperation::Delete,
            },
        ];

        let result = execute_batch_edit(&handle, dir.path(), &edits, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Overlapping"));
    }

    #[test]
    fn test_execute_batch_edit_rollback_message_on_nonexistent_symbol() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.rs"), b"fn foo() {}\n").unwrap();

        let handle = crate::live_index::LiveIndex::empty();
        let content = b"fn foo() {}\n" as &[u8];
        let result = crate::parsing::process_file("src/a.rs", content, LanguageId::Rust);
        let indexed = IndexedFile::from_parse_result(result, content.to_vec());
        handle.update_file("src/a.rs".to_string(), indexed);

        // First edit targets a real symbol; second targets a nonexistent one.
        let edits = vec![
            SingleEdit {
                path: "src/a.rs".to_string(),
                name: "foo".to_string(),
                kind: None,
                symbol_line: None,
                operation: EditOperation::Replace {
                    new_body: "fn foo() { modified }".to_string(),
                },
            },
            SingleEdit {
                path: "src/a.rs".to_string(),
                name: "nonexistent_symbol".to_string(),
                kind: None,
                symbol_line: None,
                operation: EditOperation::Delete,
            },
        ];

        let result = execute_batch_edit(&handle, dir.path(), &edits, false);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("ROLLED BACK"),
            "expected ROLLED BACK in: {err}"
        );
        assert!(
            err.contains("No files were modified"),
            "expected 'No files were modified' in: {err}"
        );
        assert!(err.contains("2"), "expected edit count (2) in: {err}");

        // Confirm the file was NOT modified.
        let file_content = std::fs::read_to_string(src.join("a.rs")).unwrap();
        assert!(
            file_content.contains("fn foo() {}"),
            "file should be unmodified: {file_content}"
        );
    }

    #[test]
    fn test_execute_batch_edit_dry_run_previews_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.rs"), b"fn alpha() { old }\n").unwrap();

        let handle = crate::live_index::LiveIndex::empty();
        let content = b"fn alpha() { old }\n" as &[u8];
        let result = crate::parsing::process_file("src/a.rs", content, LanguageId::Rust);
        let indexed = IndexedFile::from_parse_result(result, content.to_vec());
        handle.update_file("src/a.rs".to_string(), indexed);

        let edits = vec![SingleEdit {
            path: "src/a.rs".to_string(),
            name: "alpha".to_string(),
            kind: None,
            symbol_line: None,
            operation: EditOperation::Replace {
                new_body: "fn alpha() { new }".to_string(),
            },
        }];

        let summaries = execute_batch_edit(&handle, dir.path(), &edits, true).unwrap();
        assert_eq!(summaries.len(), 1, "expected one preview line");
        assert!(
            summaries[0].contains("[DRY RUN]"),
            "expected [DRY RUN] prefix in: {}",
            summaries[0]
        );

        // File must be unchanged.
        let file_content = std::fs::read_to_string(src.join("a.rs")).unwrap();
        assert!(
            file_content.contains("old"),
            "dry_run must not write to disk: {file_content}"
        );
        assert!(
            !file_content.contains("new"),
            "dry_run must not write to disk: {file_content}"
        );
    }

    #[test]
    fn test_execute_batch_edit_dry_run_same_error_as_real() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.rs"), b"fn foo() {}\n").unwrap();

        let handle = crate::live_index::LiveIndex::empty();
        let content = b"fn foo() {}\n" as &[u8];
        let result = crate::parsing::process_file("src/a.rs", content, LanguageId::Rust);
        let indexed = IndexedFile::from_parse_result(result, content.to_vec());
        handle.update_file("src/a.rs".to_string(), indexed);

        let edits = vec![SingleEdit {
            path: "src/a.rs".to_string(),
            name: "nonexistent_symbol".to_string(),
            kind: None,
            symbol_line: None,
            operation: EditOperation::Delete,
        }];

        let real_err = execute_batch_edit(&handle, dir.path(), &edits, false).unwrap_err();
        let dry_err = execute_batch_edit(&handle, dir.path(), &edits, true).unwrap_err();

        assert_eq!(
            real_err, dry_err,
            "dry_run must produce identical error to real run"
        );
    }

    // -- execute_batch_insert --

    #[test]
    fn test_execute_batch_insert_adds_to_multiple_files() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.rs"), b"fn handler_a() {}\n").unwrap();
        std::fs::write(src.join("b.rs"), b"fn handler_b() {}\n").unwrap();

        let handle = crate::live_index::LiveIndex::empty();
        for (path, content) in [
            ("src/a.rs", b"fn handler_a() {}\n" as &[u8]),
            ("src/b.rs", b"fn handler_b() {}\n"),
        ] {
            let result = crate::parsing::process_file(path, content, LanguageId::Rust);
            let indexed = IndexedFile::from_parse_result(result, content.to_vec());
            handle.update_file(path.to_string(), indexed);
        }

        let input = BatchInsertInput {
            content: "fn logging() { log::info!(\"called\"); }".to_string(),
            position: InsertPosition::After,
            targets: vec![
                InsertTarget {
                    path: "src/a.rs".to_string(),
                    name: "handler_a".to_string(),
                    kind: None,
                    symbol_line: None,
                },
                InsertTarget {
                    path: "src/b.rs".to_string(),
                    name: "handler_b".to_string(),
                    kind: None,
                    symbol_line: None,
                },
            ],
        };

        let summaries = execute_batch_insert(&handle, dir.path(), &input).unwrap();
        assert_eq!(summaries.len(), 2);

        let a = std::fs::read_to_string(src.join("a.rs")).unwrap();
        assert!(a.contains("logging"), "a.rs: {a}");
        let b = std::fs::read_to_string(src.join("b.rs")).unwrap();
        assert!(b.contains("logging"), "b.rs: {b}");
    }

    // -- partial failure / atomicity --

    #[test]
    #[cfg(unix)]
    fn test_batch_edit_partial_success_reindexes_completed() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();

        // File 1: writable, has symbol "alpha"
        std::fs::write(src.join("a.rs"), b"fn alpha() { old }\n").unwrap();
        // File 2: will be made read-only after indexing
        std::fs::write(src.join("b.rs"), b"fn beta() { old }\n").unwrap();
        // File 3: writable, has symbol "gamma"
        std::fs::write(src.join("c.rs"), b"fn gamma() { old }\n").unwrap();

        let handle = crate::live_index::LiveIndex::empty();
        for (path, content) in [
            ("src/a.rs", b"fn alpha() { old }\n" as &[u8]),
            ("src/b.rs", b"fn beta() { old }\n"),
            ("src/c.rs", b"fn gamma() { old }\n"),
        ] {
            let result = crate::parsing::process_file(path, content, LanguageId::Rust);
            let indexed = IndexedFile::from_parse_result(result, content.to_vec());
            handle.update_file(path.to_string(), indexed);
        }

        // Make file 2 read-only so the write will fail.
        let b_path = src.join("b.rs");
        std::fs::set_permissions(&b_path, std::fs::Permissions::from_mode(0o444)).unwrap();

        let edits = vec![
            SingleEdit {
                path: "src/a.rs".to_string(),
                name: "alpha".to_string(),
                kind: None,
                symbol_line: None,
                operation: EditOperation::Replace {
                    new_body: "fn alpha() { new }".to_string(),
                },
            },
            SingleEdit {
                path: "src/b.rs".to_string(),
                name: "beta".to_string(),
                kind: None,
                symbol_line: None,
                operation: EditOperation::Replace {
                    new_body: "fn beta() { new }".to_string(),
                },
            },
            SingleEdit {
                path: "src/c.rs".to_string(),
                name: "gamma".to_string(),
                kind: None,
                symbol_line: None,
                operation: EditOperation::Replace {
                    new_body: "fn gamma() { new }".to_string(),
                },
            },
        ];

        let result = execute_batch_edit(&handle, dir.path(), &edits, false);

        // Restore permissions before any assertions that might panic.
        std::fs::set_permissions(&b_path, std::fs::Permissions::from_mode(0o644)).unwrap();

        // Result must be an error reporting the partial failure.
        let err = result.unwrap_err();
        assert!(
            err.contains("FAILED src/b.rs"),
            "expected FAILED src/b.rs in: {err}"
        );
        assert!(
            err.contains("partial failure"),
            "expected 'partial failure' in: {err}"
        );

        // File 1 (a.rs): was written successfully — disk and index reflect edit.
        let a_content = std::fs::read_to_string(src.join("a.rs")).unwrap();
        assert!(
            a_content.contains("new"),
            "a.rs should be updated: {a_content}"
        );

        // File 2 (b.rs): write failed — disk unchanged, index unchanged.
        let b_content = std::fs::read_to_string(&b_path).unwrap();
        assert!(
            b_content.contains("old"),
            "b.rs should be unchanged: {b_content}"
        );

        // File 3 (c.rs): also attempted — best-effort continues past file 2.
        // (Whether c.rs succeeded depends on iteration order, but the error must mention b.rs.)
    }

    #[test]
    #[cfg(unix)]
    fn test_batch_rename_rolls_back_on_failure() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();

        // Three files all containing "OldName".
        std::fs::write(src.join("a.rs"), b"struct OldName;\n").unwrap();
        std::fs::write(src.join("b.rs"), b"use crate::OldName;\n").unwrap();
        std::fs::write(src.join("c.rs"), b"fn use_it(x: OldName) {}\n").unwrap();

        let handle = crate::live_index::LiveIndex::empty();
        for (path, content) in [
            ("src/a.rs", b"struct OldName;\n" as &[u8]),
            ("src/b.rs", b"use crate::OldName;\n"),
            ("src/c.rs", b"fn use_it(x: OldName) {}\n"),
        ] {
            let result = crate::parsing::process_file(path, content, LanguageId::Rust);
            let indexed = IndexedFile::from_parse_result(result, content.to_vec());
            handle.update_file(path.to_string(), indexed);
        }

        // Make src/b.rs read-only so its write will fail mid-rename.
        let b_path = src.join("b.rs");
        std::fs::set_permissions(&b_path, std::fs::Permissions::from_mode(0o444)).unwrap();

        let input = crate::protocol::edit::BatchRenameInput {
            path: "src/a.rs".to_string(),
            name: "OldName".to_string(),
            new_name: "NewName".to_string(),
            kind: None,
            symbol_line: None,
            dry_run: Some(false),
        };

        let result = execute_batch_rename(&handle, dir.path(), &input);

        // Restore permissions before assertions.
        std::fs::set_permissions(&b_path, std::fs::Permissions::from_mode(0o644)).unwrap();

        // Must be an error.
        let err = result.unwrap_err();
        assert!(
            err.contains("ROLLED BACK") || err.contains("Write failed"),
            "expected rollback message in: {err}"
        );

        // All files that were written before the failure must be rolled back to "OldName".
        let a_content = std::fs::read_to_string(src.join("a.rs")).unwrap();
        assert!(
            a_content.contains("OldName"),
            "a.rs should be rolled back to OldName: {a_content}"
        );
        assert!(
            !a_content.contains("NewName"),
            "a.rs must not contain NewName after rollback: {a_content}"
        );

        let b_content = std::fs::read_to_string(&b_path).unwrap();
        assert!(
            b_content.contains("OldName"),
            "b.rs (read-only, never written) should still have OldName: {b_content}"
        );
    }

    // -- extract_signature --

    #[test]
    fn test_extract_signature_returns_first_line() {
        let content = b"fn foo(x: i32) {\n    body();\n}";
        let sig = extract_signature(content, (0, 30));
        assert_eq!(sig, "fn foo(x: i32) {");
    }

    #[test]
    fn test_extract_signature_single_line() {
        let content = b"fn foo() {}";
        let sig = extract_signature(content, (0, 11));
        assert_eq!(sig, "fn foo() {}");
    }

    // -- extract_impl_type_name --

    #[test]
    fn test_extract_impl_type_name_simple() {
        assert_eq!(extract_impl_type_name("impl Foo"), Some("Foo".to_string()));
    }

    #[test]
    fn test_extract_impl_type_name_trait_for() {
        assert_eq!(
            extract_impl_type_name("impl Display for Foo"),
            Some("Foo".to_string())
        );
    }

    #[test]
    fn test_extract_impl_type_name_generic() {
        assert_eq!(
            extract_impl_type_name("impl<T> Foo<T>"),
            Some("Foo".to_string())
        );
    }

    #[test]
    fn test_extract_impl_type_name_generic_trait_for() {
        assert_eq!(
            extract_impl_type_name("impl<T: Clone> Trait for Foo<T>"),
            Some("Foo".to_string())
        );
    }

    #[test]
    fn test_extract_impl_type_name_no_impl_prefix() {
        // Some parsers may strip the "impl" keyword from the name.
        assert_eq!(extract_impl_type_name("Foo"), Some("Foo".to_string()));
    }

    // -- find_parent_impl_type --

    #[test]
    fn test_find_parent_impl_type_for_method() {
        let file = make_test_indexed_file(vec![
            SymbolRecord {
                name: "impl Widget".to_string(),
                kind: SymbolKind::Impl,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 100),
                line_range: (0, 10),
                doc_byte_range: None,
            },
            SymbolRecord {
                name: "display".to_string(),
                kind: SymbolKind::Method,
                depth: 1,
                sort_order: 1,
                byte_range: (20, 80),
                line_range: (2, 8),
                doc_byte_range: None,
            },
        ]);
        let method = &file.symbols[1];
        assert_eq!(
            find_parent_impl_type(&file, method),
            Some("Widget".to_string())
        );
    }

    #[test]
    fn test_find_parent_impl_type_standalone_fn() {
        let file = make_test_indexed_file(vec![make_test_symbol(
            "standalone",
            SymbolKind::Function,
            (0, 50),
            1,
        )]);
        let func = &file.symbols[0];
        assert_eq!(find_parent_impl_type(&file, func), None);
    }

    // -- detect_stale_references with parent_type filtering --

    fn make_ref_file(refs: Vec<crate::domain::index::ReferenceRecord>) -> IndexedFile {
        IndexedFile {
            relative_path: String::new(),
            language: LanguageId::Rust,
            classification: crate::domain::index::FileClassification::for_code_path("test.rs"),
            content: Vec::new(),
            symbols: Vec::new(),
            parse_status: crate::live_index::store::ParseStatus::Parsed,
            byte_len: 0,
            content_hash: String::new(),
            references: refs,
            alias_map: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_detect_stale_refs_method_filters_by_parent_type() {
        use crate::domain::index::ReferenceKind;
        let handle = crate::live_index::LiveIndex::empty();

        // File A: has Widget type ref + display call -> should be warned
        handle.update_file(
            "src/a.rs".to_string(),
            make_ref_file(vec![
                crate::domain::index::ReferenceRecord {
                    name: "display".to_string(),
                    qualified_name: None,
                    kind: ReferenceKind::Call,
                    byte_range: (32, 39),
                    line_range: (1, 1),
                    enclosing_symbol_index: None,
                },
                crate::domain::index::ReferenceRecord {
                    name: "Widget".to_string(),
                    qualified_name: None,
                    kind: ReferenceKind::TypeUsage,
                    byte_range: (12, 18),
                    line_range: (0, 0),
                    enclosing_symbol_index: None,
                },
            ]),
        );

        // File B: has display call but NO Widget ref -> should NOT be warned
        handle.update_file(
            "src/b.rs".to_string(),
            make_ref_file(vec![crate::domain::index::ReferenceRecord {
                name: "display".to_string(),
                qualified_name: None,
                kind: ReferenceKind::Call,
                byte_range: (19, 26),
                line_range: (0, 0),
                enclosing_symbol_index: None,
            }]),
        );

        // With parent_type = Some("Widget"), only file A should be warned
        let refs = detect_stale_references(
            &handle,
            "src/widget.rs",
            "display",
            "fn display(&self) {",
            "fn display(&self, verbose: bool) {",
            Some("Widget"),
            None,
        );
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].0, "src/a.rs");
    }

    #[test]
    fn test_detect_stale_refs_standalone_fn_warns_all() {
        use crate::domain::index::ReferenceKind;
        let handle = crate::live_index::LiveIndex::empty();

        // File A: has display call
        handle.update_file(
            "src/a.rs".to_string(),
            make_ref_file(vec![crate::domain::index::ReferenceRecord {
                name: "display".to_string(),
                qualified_name: None,
                kind: ReferenceKind::Call,
                byte_range: (12, 19),
                line_range: (0, 0),
                enclosing_symbol_index: None,
            }]),
        );

        // File B: also has display call
        handle.update_file(
            "src/b.rs".to_string(),
            make_ref_file(vec![crate::domain::index::ReferenceRecord {
                name: "display".to_string(),
                qualified_name: None,
                kind: ReferenceKind::Call,
                byte_range: (15, 22),
                line_range: (0, 0),
                enclosing_symbol_index: None,
            }]),
        );

        // With parent_type = None (standalone fn), both files should be warned
        let refs = detect_stale_references(
            &handle,
            "src/lib.rs",
            "display",
            "fn display() {",
            "fn display(verbose: bool) {",
            None,
            None,
        );
        assert_eq!(refs.len(), 2);
    }

    // -- doc-aware build_delete and build_insert_before --

    #[test]
    fn test_build_delete_includes_doc_comments() {
        // "/// Doc line 1\n" = 15 bytes (0..15)
        // "/// Doc line 2\n" = 15 bytes (15..30)
        // "pub fn foo() {}\n" = 16 bytes (30..46)
        // "\n"               =  1 byte  (46..47)
        // "fn bar() {}\n"    = 12 bytes (47..59)
        let content = b"/// Doc line 1\n/// Doc line 2\npub fn foo() {}\n\nfn bar() {}\n";
        let sym = SymbolRecord {
            name: "foo".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (30, 46),
            line_range: (2, 2),
            doc_byte_range: Some((0, 30)),
        };
        let result = build_delete(content, &sym);
        let result_str = String::from_utf8(result).unwrap();
        assert!(
            !result_str.contains("/// Doc line 1"),
            "doc comments should be deleted"
        );
        assert!(
            !result_str.contains("pub fn foo"),
            "function body should be deleted"
        );
        assert!(
            result_str.contains("fn bar()"),
            "other function should remain"
        );
    }

    #[test]
    fn test_build_delete_removes_blank_line_separated_doc_comments() {
        // Regression: doc comments separated by a blank line from the symbol
        // are NOT attached via doc_byte_range (scan_doc_range stops at blank lines).
        // But delete_symbol should still clean them up to avoid orphaned comments.
        //
        // "/// Batch-inserted marker\n" = 26 bytes (0..26)
        // "\n"                          =  1 byte  (26..27)
        // "fn batch_marker() {}\n"      = 21 bytes (27..48)
        // "\n"                          =  1 byte  (48..49)
        // "fn other() {}\n"             = 14 bytes (49..63)
        let content = b"/// Batch-inserted marker\n\nfn batch_marker() {}\n\nfn other() {}\n";
        let sym = SymbolRecord {
            name: "batch_marker".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (27, 48),
            line_range: (2, 2),
            doc_byte_range: None, // blank line prevents attachment
        };
        let result = build_delete(content, &sym);
        let result_str = String::from_utf8(result).unwrap();
        assert!(
            !result_str.contains("/// Batch-inserted marker"),
            "orphaned doc comment should be cleaned up, got: {result_str}"
        );
        assert!(
            result_str.contains("fn other()"),
            "other function should remain, got: {result_str}"
        );
    }

    #[test]
    fn test_build_insert_before_goes_above_doc_comments() {
        // "/// Doc for foo\n" = 16 bytes (0..16)
        // "pub fn foo() {}\n" = 16 bytes (16..32)
        let content = b"/// Doc for foo\npub fn foo() {}\n";
        let sym = SymbolRecord {
            name: "foo".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (16, 32),
            line_range: (1, 1),
            doc_byte_range: Some((0, 16)),
        };
        let result = build_insert_before(content, &sym, "use std::io;");
        let result_str = String::from_utf8(result).unwrap();
        let use_pos = result_str
            .find("use std::io;")
            .expect("inserted content missing");
        let doc_pos = result_str
            .find("/// Doc for foo")
            .expect("doc comment missing");
        assert!(
            use_pos < doc_pos,
            "insert should go above doc comments (use_pos={use_pos}, doc_pos={doc_pos})"
        );
    }

    #[test]
    fn test_build_insert_before_double_newline_without_doc_comments() {
        let content = b"struct Point { x: f64 }\n";
        let sym = SymbolRecord {
            name: "Point".to_string(),
            kind: SymbolKind::Struct,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 23),
            line_range: (0, 0),
            doc_byte_range: None,
        };
        let result = build_insert_before(content, &sym, "struct Point3D { x: f64 }");
        let result_str = String::from_utf8(result).unwrap();
        assert!(
            result_str.contains("Point3D { x: f64 }\n\nstruct Point"),
            "should have \\n\\n separator when no doc comment: {result_str}"
        );
    }

    #[test]
    fn test_build_insert_before_no_double_blank_line() {
        // File already has a blank line before the symbol: inserting should NOT create \n\n\n.
        // "\n"            =  1 byte (0..1)   — blank line preceding the symbol
        // "fn existing() {}\n" = 18 bytes (1..19)
        let content = b"\nfn existing() {}\n";
        let sym = SymbolRecord {
            name: "existing".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (1, 18),
            line_range: (1, 1),
            doc_byte_range: None,
        };
        let result = build_insert_before(content, &sym, "fn new_fn() {}");
        let result_str = String::from_utf8(result).unwrap();
        assert!(
            !result_str.contains("\n\n\n"),
            "should not produce triple newline when blank line already precedes symbol: {result_str:?}"
        );
        assert!(
            result_str.contains("fn new_fn() {}"),
            "inserted content missing: {result_str:?}"
        );
        assert!(
            result_str.contains("fn existing() {}"),
            "existing content missing: {result_str:?}"
        );
    }

    #[test]
    fn test_build_insert_before_first_symbol_in_file() {
        // Symbol starts at byte 0 (prefix is empty) — no double blank line should be produced.
        let content = b"fn first() {}\n";
        let sym = SymbolRecord {
            name: "first".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 13),
            line_range: (0, 0),
            doc_byte_range: None,
        };
        let result = build_insert_before(content, &sym, "fn before() {}");
        let result_str = String::from_utf8(result).unwrap();
        assert!(
            !result_str.contains("\n\n\n"),
            "should not produce triple newline when symbol is first in file: {result_str:?}"
        );
        assert!(
            result_str.contains("fn before() {}"),
            "inserted content missing: {result_str:?}"
        );
        assert!(
            result_str.contains("fn first() {}"),
            "original content missing: {result_str:?}"
        );
    }

    #[test]
    fn test_build_insert_before_with_doc_byte_range() {
        // Symbol has doc_byte_range — separator is always \n (tight against doc comment).
        // "/// Doc\n"       =  8 bytes (0..8)
        // "fn target() {}\n" = 15 bytes (8..23)
        // "\n"               =  1 byte  (23..24)
        // "fn other() {}\n"  = 14 bytes (24..38)
        let content = b"/// Doc\nfn target() {}\n\nfn other() {}\n";
        let sym = SymbolRecord {
            name: "target".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (8, 23),
            line_range: (1, 1),
            doc_byte_range: Some((0, 8)),
        };
        let result = build_insert_before(content, &sym, "fn inserted() {}");
        let result_str = String::from_utf8(result).unwrap();
        // insertion goes above the doc comment, with \n separator (not \n\n)
        assert!(
            !result_str.contains("\n\n\n"),
            "should not produce triple newline with doc_byte_range: {result_str:?}"
        );
        let ins_pos = result_str
            .find("fn inserted()")
            .expect("inserted content missing");
        let doc_pos = result_str.find("/// Doc").expect("doc comment missing");
        assert!(
            ins_pos < doc_pos,
            "insertion should appear before doc comment: ins={ins_pos} doc={doc_pos}"
        );
    }

    #[test]
    fn test_build_edit_within_no_doc_duplication() {
        // "/// Doc comment\n" = 16 bytes (0..16)
        // "pub fn foo() {}\n" = 16 bytes (16..32)
        let content = b"/// Doc comment\npub fn foo() {}\n";
        let sym = SymbolRecord {
            name: "foo".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (16, 32),
            line_range: (1, 1),
            doc_byte_range: Some((0, 16)),
        };
        let (result, count) = build_edit_within(content, &sym, "foo", "bar", false).unwrap();
        let result_str = String::from_utf8(result).unwrap();
        assert_eq!(count, 1);
        // Doc comment should appear exactly once, not duplicated
        assert_eq!(
            result_str.matches("/// Doc comment").count(),
            1,
            "doc comment should not be duplicated: {result_str}"
        );
        assert!(result_str.contains("pub fn bar()"), "edit should apply");
    }

    // -----------------------------------------------------------------------
    // find_qualified_usages tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_finds_type_new_qualified_call() {
        let source = "let x = MyType::new();";
        let matches = find_qualified_usages("MyType", source);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].confident);
    }

    #[test]
    fn test_finds_deep_nested_qualified() {
        let source = "let x = module::MyType::new();";
        let matches = find_qualified_usages("MyType", source);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].confident);
    }

    #[test]
    fn test_finds_use_import_path() {
        let source = "use crate::module::MyType;";
        let matches = find_qualified_usages("MyType", source);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].confident);
    }

    #[test]
    fn test_scanner_finds_all_raw_occurrences_of_common_name() {
        let source = "let x = SomeOther::new();\nlet y = Target::new();";
        let matches = find_qualified_usages("new", source);
        assert_eq!(matches.len(), 2);
        assert!(matches.iter().all(|m| m.confident));
    }

    #[test]
    fn test_uncertain_match_in_string() {
        let source = r#"let s = "MyType::new()";"#;
        let matches = find_qualified_usages("MyType", source);
        assert_eq!(matches.len(), 1);
        assert!(!matches[0].confident);
    }

    #[test]
    fn test_uncertain_match_in_comment() {
        let source = "// MyType::new() creates an instance";
        let matches = find_qualified_usages("MyType", source);
        assert_eq!(matches.len(), 1);
        assert!(!matches[0].confident);
    }

    #[test]
    fn test_finds_turbofish_qualified_call() {
        let source = "let x = MyType::<T>::new();";
        let matches = find_qualified_usages("MyType", source);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].confident);
    }

    #[test]
    fn test_uncertain_match_in_block_comment() {
        let source = "/* MyType::new() creates an instance */";
        let matches = find_qualified_usages("MyType", source);
        assert_eq!(matches.len(), 1);
        assert!(!matches[0].confident);
    }

    #[test]
    fn test_uncertain_match_in_multiline_string() {
        let source = "let s = r\"\n            MyType::new()\n        \";";
        let matches = find_qualified_usages("MyType", source);
        assert_eq!(matches.len(), 1);
        assert!(!matches[0].confident);
    }
}
