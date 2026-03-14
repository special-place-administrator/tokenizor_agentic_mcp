use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::index::{LanguageId, SymbolKind, SymbolRecord};
use crate::live_index::SharedIndex;
use crate::live_index::query::{
    SymbolSelectorMatch, render_symbol_selector, resolve_symbol_selector,
};
use crate::live_index::store::IndexedFile;

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
    let sym_start = sym.effective_start() as usize;
    let line_start = file_content[..sym_start]
        .iter()
        .rposition(|&b| b == b'\n')
        .map(|p| p + 1)
        .unwrap_or(0) as u32;
    let indent = detect_indentation(file_content, sym.byte_range.0);
    let indented = apply_indentation(new_code, &indent);
    let mut insertion = indented;
    // Single newline: the content is placed immediately before the symbol's line.
    // Using \n\n would create an unwanted blank line between e.g. a doc comment and the symbol.
    insertion.extend_from_slice(b"\n");
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
        file_content[..s]
            .iter()
            .rposition(|&b| b == b'\n')
            .map(|p| p + 1)
            .unwrap_or(0) as u32
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
pub(crate) fn execute_batch_edit(
    index: &SharedIndex,
    repo_root: &Path,
    edits: &[SingleEdit],
) -> Result<Vec<String>, String> {
    struct ResolvedEdit {
        path: String,
        sym: SymbolRecord,
        operation: usize,
        language: LanguageId,
    }

    // Phase 1: Resolve all symbols.
    let mut resolved = Vec::with_capacity(edits.len());
    {
        let guard = index.read().expect("lock poisoned");
        for (i, edit) in edits.iter().enumerate() {
            let file = guard
                .get_file(&edit.path)
                .ok_or_else(|| format!("File not indexed: {}", edit.path))?;
            let (_, sym) =
                resolve_or_error(file, &edit.name, edit.kind.as_deref(), edit.symbol_line)
                    .map_err(|e| format!("Edit {}: {e}", i + 1))?;
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
                         Split into separate calls.",
                        resolved[indices[i]].sym.name,
                        a.0,
                        a.1,
                        resolved[indices[j]].sym.name,
                        b.0,
                        b.1,
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
    let mut summaries = Vec::new();

    for (path, indices) in &by_file {
        let file = {
            let guard = index.read().expect("lock poisoned");
            guard
                .capture_shared_file(path)
                .ok_or_else(|| format!("File disappeared: {path}"))?
        };

        let mut content = file.content.clone();
        let language = resolved[indices[0]].language.clone();

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
                    summaries.push(super::edit_format::format_replace(
                        path,
                        &r.sym.name,
                        &r.sym.kind.to_string(),
                        old_bytes,
                        new_body.len(),
                    ));
                }
                EditOperation::InsertBefore { content: code } => {
                    content = build_insert_before(&content, &r.sym, code);
                    summaries.push(super::edit_format::format_insert(
                        path,
                        &r.sym.name,
                        "before",
                        code.len(),
                    ));
                }
                EditOperation::InsertAfter { content: code } => {
                    content = build_insert_after(&content, &r.sym, code);
                    summaries.push(super::edit_format::format_insert(
                        path,
                        &r.sym.name,
                        "after",
                        code.len(),
                    ));
                }
                EditOperation::Delete => {
                    let deleted = (r.sym.byte_range.1 - r.sym.byte_range.0) as usize;
                    content = build_delete(&content, &r.sym);
                    summaries.push(super::edit_format::format_delete(
                        path,
                        &r.sym.name,
                        &r.sym.kind.to_string(),
                        deleted,
                    ));
                }
                EditOperation::EditWithin { old_text, new_text } => {
                    let old_bytes = (r.sym.byte_range.1 - r.sym.byte_range.0) as usize;
                    let (new, count) =
                        build_edit_within(&content, &r.sym, old_text, new_text, false)
                            .map_err(|e| format!("Edit in {path}:{}: {e}", r.sym.name))?;
                    content = new;
                    summaries.push(super::edit_format::format_edit_within(
                        path,
                        &r.sym.name,
                        count,
                        old_bytes,
                        old_bytes,
                    ));
                }
            }
        }

        let abs_path = repo_root.join(path);
        atomic_write_file(&abs_path, &content)
            .map_err(|e| format!("Write failed for {path}: {e}"))?;
        reindex_after_write(index, path, content, language);
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

    // Phase 3: Group all rename sites by file.
    let mut by_file: std::collections::HashMap<String, Vec<(u32, u32)>> =
        std::collections::HashMap::new();
    by_file
        .entry(input.path.clone())
        .or_default()
        .push(def_name_range);
    for (path, range) in &ref_sites {
        by_file.entry(path.clone()).or_default().push(*range);
    }
    // Sort reverse by offset, dedup.
    for ranges in by_file.values_mut() {
        ranges.sort_by(|a, b| b.0.cmp(&a.0));
        ranges.dedup();
    }

    // Phase 4: Apply renames, write, reindex.
    let new_name_bytes = input.new_name.as_bytes();
    let mut files_updated = 0;
    let mut refs_updated = 0;

    for (path, ranges) in &by_file {
        let file = {
            let guard = index.read().expect("lock poisoned");
            guard
                .capture_shared_file(path)
                .ok_or_else(|| format!("File disappeared: {path}"))?
        };

        let mut content = file.content.clone();
        for range in ranges {
            content = apply_splice(&content, *range, new_name_bytes);
            refs_updated += 1;
        }

        let abs_path = repo_root.join(path);
        atomic_write_file(&abs_path, &content)
            .map_err(|e| format!("Write failed for {path}: {e}"))?;

        let lang = if path == &input.path {
            language.clone()
        } else {
            file.language.clone()
        };
        reindex_after_write(index, path, content, lang);
        files_updated += 1;
    }

    Ok(format!(
        "Renamed `{}` → `{}` — {refs_updated} site(s) across {files_updated} file(s)",
        input.name, input.new_name,
    ))
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
pub(crate) fn execute_batch_insert(
    index: &SharedIndex,
    repo_root: &Path,
    input: &BatchInsertInput,
) -> Result<Vec<String>, String> {
    let mut summaries = Vec::new();
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

        let abs_path = repo_root.join(&target.path);
        atomic_write_file(&abs_path, &new_content)
            .map_err(|e| format!("Write failed for {}: {e}", target.path))?;

        let lang = file.language.clone();
        reindex_after_write(index, &target.path, new_content, lang);
        summaries.push(super::edit_format::format_insert(
            &target.path,
            &target.name,
            position_label,
            input.content.len(),
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
        assert!(
            text.starts_with("    fn new_fn() {}\n    fn existing"),
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

        let summaries = execute_batch_edit(&handle, dir.path(), &edits).unwrap();
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

        let result = execute_batch_edit(&handle, dir.path(), &edits);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Overlapping"));
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
}
