use crate::domain::{SymbolKind, SymbolRecord};
use super::{ConfigExtractor, EditCapability, ExtractionOutcome, ExtractionResult, join_key_path, MAX_DEPTH};

pub struct TomlExtractor;

impl ConfigExtractor for TomlExtractor {
    fn extract(&self, content: &[u8]) -> ExtractionResult {
        let content_str = match std::str::from_utf8(content) {
            Ok(s) => s,
            Err(e) => {
                return ExtractionResult {
                    symbols: vec![],
                    outcome: ExtractionOutcome::Failed(e.to_string()),
                };
            }
        };

        if content_str.trim().is_empty() {
            return ExtractionResult {
                symbols: vec![],
                outcome: ExtractionOutcome::Ok,
            };
        }

        // Try strict parse first; fall back to line-scanning on parse error.
        // Note: DocumentMut rejects some constructs (e.g. [a] with a.b="v" then [a.b])
        // that real-world TOML files use. Line scanning handles these gracefully.
        match content_str.parse::<toml_edit::DocumentMut>() {
            Ok(doc) => {
                let mut symbols = Vec::new();
                let mut sort_order: u32 = 0;
                walk_table(doc.as_table(), "", 0, content, &mut symbols, &mut sort_order);
                ExtractionResult { symbols, outcome: ExtractionOutcome::Ok }
            }
            Err(_) => {
                // Fall back to line-based scanning so we still extract useful keys.
                // Only return Failed for truly unparseable content (binary, wrong encoding, etc.)
                // — but we already handled UTF-8 above. A TOML "duplicate key" error still
                // produces meaningful output from line scanning.
                let symbols = line_scan(content);
                if symbols.is_empty() {
                    // If line scan also produced nothing, report failure so callers know
                    // parsing was degraded. But we check for the specific case of truly
                    // malformed TOML (unclosed bracket, etc.) vs. spec-edge duplicate keys.
                    let parse_err = content_str.parse::<toml_edit::DocumentMut>().unwrap_err();
                    let msg = parse_err.message();
                    // For truly broken TOML (not just duplicate-key edge cases), report failure
                    if msg.contains("invalid") || msg.contains("expected") || msg.contains("unterminated") {
                        return ExtractionResult {
                            symbols: vec![],
                            outcome: ExtractionOutcome::Failed(msg.to_string()),
                        };
                    }
                }
                ExtractionResult { symbols, outcome: ExtractionOutcome::Ok }
            }
        }
    }

    fn edit_capability(&self) -> EditCapability {
        EditCapability::StructuralEditSafe
    }
}

// ---------------------------------------------------------------------------
// toml_edit document walker (used when parse succeeds)
// ---------------------------------------------------------------------------

fn walk_table(
    table: &toml_edit::Table,
    parent_path: &str,
    depth: u32,
    raw: &[u8],
    symbols: &mut Vec<SymbolRecord>,
    sort_order: &mut u32,
) {
    if depth >= MAX_DEPTH {
        return;
    }
    for (key, item) in table.iter() {
        let key_path = join_key_path(parent_path, key);
        walk_item(item, key, &key_path, depth, raw, symbols, sort_order);
    }
}

fn walk_item(
    item: &toml_edit::Item,
    raw_key: &str,
    key_path: &str,
    depth: u32,
    raw: &[u8],
    symbols: &mut Vec<SymbolRecord>,
    sort_order: &mut u32,
) {
    match item {
        toml_edit::Item::None => {}

        toml_edit::Item::Value(value) => {
            let (start, end) = find_key_value_bytes(raw, raw_key);
            symbols.push(make_symbol(key_path, depth, start, end, *sort_order));
            *sort_order += 1;

            if depth + 1 < MAX_DEPTH {
                if let Some(inline_table) = value.as_inline_table() {
                    for (k, v) in inline_table.iter() {
                        let child_path = join_key_path(key_path, k);
                        walk_item(
                            &toml_edit::Item::Value(v.clone()),
                            k,
                            &child_path,
                            depth + 1,
                            raw,
                            symbols,
                            sort_order,
                        );
                    }
                }
            }
        }

        toml_edit::Item::Table(table) => {
            let (start, end) = find_table_header_bytes(raw, key_path);
            symbols.push(make_symbol(key_path, depth, start, end, *sort_order));
            *sort_order += 1;
            if depth + 1 < MAX_DEPTH {
                walk_table(table, key_path, depth + 1, raw, symbols, sort_order);
            }
        }

        toml_edit::Item::ArrayOfTables(array) => {
            for (i, table) in array.iter().enumerate() {
                let indexed_path = format!("{}[{}]", key_path, i);
                let (start, end) = find_array_table_header_bytes(raw, key_path, i);
                symbols.push(make_symbol(&indexed_path, depth, start, end, *sort_order));
                *sort_order += 1;
                if depth + 1 < MAX_DEPTH {
                    walk_table(table, &indexed_path, depth + 1, raw, symbols, sort_order);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Line-based fallback scanner (used when toml_edit rejects the file)
// ---------------------------------------------------------------------------

/// Scan TOML line by line, extracting section headers and key = value lines.
/// Does not recurse into inline tables. Suitable for files that toml_edit rejects.
fn line_scan(raw: &[u8]) -> Vec<SymbolRecord> {
    let mut symbols = Vec::new();
    let mut sort_order: u32 = 0;
    let mut current_section: String = String::new();
    let mut depth_offset: u32 = 0;
    let len = raw.len();
    let mut i = 0;

    while i < len {
        let line_start = i;
        let line_end = raw[i..].iter().position(|&b| b == b'\n')
            .map(|p| i + p + 1)
            .unwrap_or(len);

        let line_bytes = &raw[line_start..line_end];
        let trimmed = trim_leading_whitespace(line_bytes);

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with(b"#") {
            i = line_end;
            continue;
        }

        if trimmed.starts_with(b"[[") {
            // Array of tables: [[section]]
            if let Some(section) = extract_bracket_content(trimmed, true) {
                current_section = section.clone();
                depth_offset = section.matches('.').count() as u32;
                symbols.push(make_symbol(&section, depth_offset, line_start, line_end, sort_order));
                sort_order += 1;
            }
        } else if trimmed.starts_with(b"[") {
            // Table header: [section]
            if let Some(section) = extract_bracket_content(trimmed, false) {
                current_section = section.clone();
                depth_offset = section.matches('.').count() as u32;
                symbols.push(make_symbol(&section, depth_offset, line_start, line_end, sort_order));
                sort_order += 1;
            }
        } else if let Some(key) = extract_key_from_line(trimmed) {
            // key = value
            let key_path = if current_section.is_empty() {
                key.clone()
            } else {
                join_key_path(&current_section, &key)
            };
            let d = depth_offset + 1;
            if d < MAX_DEPTH {
                symbols.push(make_symbol(&key_path, d, line_start, line_end, sort_order));
                sort_order += 1;
            }
        }

        i = line_end;
    }

    symbols
}

/// Extract section name from `[section]` or `[[section]]` line.
fn extract_bracket_content(line: &[u8], double: bool) -> Option<String> {
    let open: &[u8] = if double { b"[[" } else { b"[" };
    let close: &[u8] = if double { b"]]" } else { b"]" };

    if !line.starts_with(open) {
        return None;
    }
    let inner_start = open.len();
    let close_pos = line[inner_start..].windows(close.len()).position(|w| w == close)?;
    let inner = &line[inner_start..inner_start + close_pos];
    let s = std::str::from_utf8(inner).ok()?.trim();
    if s.is_empty() { None } else { Some(s.to_string()) }
}

/// Extract key name from `key = value` line. Returns bare key name.
fn extract_key_from_line(line: &[u8]) -> Option<String> {
    // Find `=` that isn't inside quotes
    let mut in_string = false;
    let mut escape = false;
    for (i, &b) in line.iter().enumerate() {
        if escape { escape = false; continue; }
        if b == b'\\' && in_string { escape = true; continue; }
        if b == b'"' { in_string = !in_string; continue; }
        if !in_string && b == b'=' {
            let key_bytes = trim_trailing_whitespace(&line[..i]);
            let key = std::str::from_utf8(key_bytes).ok()?.trim();
            // Strip surrounding quotes if any
            let key = key.trim_matches('"').trim_matches('\'');
            if key.is_empty() { return None; }
            return Some(key.to_string());
        }
    }
    None
}

fn trim_trailing_whitespace(s: &[u8]) -> &[u8] {
    let end = s.iter().rposition(|&b| b != b' ' && b != b'\t').map(|p| p + 1).unwrap_or(0);
    &s[..end]
}

// ---------------------------------------------------------------------------
// Raw byte span finders (used by toml_edit walker)
// ---------------------------------------------------------------------------

fn find_key_value_bytes(raw: &[u8], key: &str) -> (usize, usize) {
    let key_bytes = key.as_bytes();
    let len = raw.len();
    let mut i = 0;
    while i < len {
        let line_start = i;
        let line_end = raw[i..].iter().position(|&b| b == b'\n')
            .map(|p| i + p + 1)
            .unwrap_or(len);
        let line = &raw[line_start..line_end];
        let trimmed = trim_leading_whitespace(line);
        if !trimmed.starts_with(b"#") && !trimmed.starts_with(b"[") {
            if line_starts_with_key(trimmed, key_bytes) {
                return (line_start, line_end.min(len));
            }
        }
        i = line_end;
    }
    (0, 0)
}

fn find_table_header_bytes(raw: &[u8], key_path: &str) -> (usize, usize) {
    find_header_pattern(raw, &format!("[{}]", key_path))
}

fn find_array_table_header_bytes(raw: &[u8], key_path: &str, index: usize) -> (usize, usize) {
    let pattern = format!("[[{}]]", key_path);
    let pattern_bytes = pattern.as_bytes();
    let len = raw.len();
    let mut i = 0;
    let mut count = 0;
    while i < len {
        let line_start = i;
        let line_end = raw[i..].iter().position(|&b| b == b'\n')
            .map(|p| i + p + 1)
            .unwrap_or(len);
        let line = trim_leading_whitespace(&raw[line_start..line_end]);
        if line.starts_with(pattern_bytes) {
            if count == index {
                return (line_start, line_end.min(len));
            }
            count += 1;
        }
        i = line_end;
    }
    (0, 0)
}

fn find_header_pattern(raw: &[u8], pattern: &str) -> (usize, usize) {
    let pattern_bytes = pattern.as_bytes();
    let len = raw.len();
    let mut i = 0;
    while i < len {
        let line_start = i;
        let line_end = raw[i..].iter().position(|&b| b == b'\n')
            .map(|p| i + p + 1)
            .unwrap_or(len);
        let line = trim_leading_whitespace(&raw[line_start..line_end]);
        if line.starts_with(pattern_bytes) {
            return (line_start, line_end.min(len));
        }
        i = line_end;
    }
    (0, 0)
}

fn trim_leading_whitespace(s: &[u8]) -> &[u8] {
    let pos = s.iter().position(|&b| b != b' ' && b != b'\t').unwrap_or(s.len());
    &s[pos..]
}

fn line_starts_with_key(line: &[u8], key: &[u8]) -> bool {
    if line.len() < key.len() { return false; }
    if !line.starts_with(key) { return false; }
    let after = &line[key.len()..];
    after.first().map(|&b| b == b' ' || b == b'\t' || b == b'=').unwrap_or(false)
}

fn make_symbol(name: &str, depth: u32, byte_start: usize, byte_end: usize, sort_order: u32) -> SymbolRecord {
    SymbolRecord {
        name: name.to_string(),
        kind: SymbolKind::Key,
        depth,
        sort_order,
        byte_range: (byte_start as u32, byte_end as u32),
        line_range: (0, 0),
        doc_byte_range: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_top_level_keys() {
        let content = b"name = \"test\"\nversion = \"1.0\"\n";
        let result = TomlExtractor.extract(content);
        assert!(result.symbols.iter().any(|s| s.name == "name" && s.kind == SymbolKind::Key));
        assert!(result.symbols.iter().any(|s| s.name == "version"));
    }

    #[test]
    fn test_table_keys() {
        let content = b"[package]\nname = \"test\"\nversion = \"1.0\"\n";
        let result = TomlExtractor.extract(content);
        assert!(result.symbols.iter().any(|s| s.name == "package"), "missing package");
        assert!(result.symbols.iter().any(|s| s.name == "package.name"), "missing package.name");
        assert!(result.symbols.iter().any(|s| s.name == "package.version"), "missing package.version");
    }

    #[test]
    fn test_nested_tables() {
        let content = b"[dependencies]\nserde = \"1.0\"\n\n[dependencies.serde]\nfeatures = [\"derive\"]\n";
        let result = TomlExtractor.extract(content);
        assert!(result.symbols.iter().any(|s| s.name == "dependencies.serde"),
            "missing dependencies.serde; symbols={:?}", result.symbols.iter().map(|s| &s.name).collect::<Vec<_>>());
    }

    #[test]
    fn test_inline_table() {
        let content = b"[package]\nmetadata = { key = \"value\" }\n";
        let result = TomlExtractor.extract(content);
        assert!(result.symbols.iter().any(|s| s.name == "package.metadata"), "missing package.metadata");
        assert!(result.symbols.iter().any(|s| s.name == "package.metadata.key"), "missing package.metadata.key");
    }

    #[test]
    fn test_empty_file() {
        assert!(TomlExtractor.extract(b"").symbols.is_empty());
    }

    #[test]
    fn test_malformed_toml() {
        let result = TomlExtractor.extract(b"[invalid\nno closing");
        assert!(result.symbols.is_empty());
        assert!(matches!(result.outcome, ExtractionOutcome::Failed(_)));
    }

    #[test]
    fn test_edit_capability() {
        assert_eq!(TomlExtractor.edit_capability(), EditCapability::StructuralEditSafe);
    }
}
