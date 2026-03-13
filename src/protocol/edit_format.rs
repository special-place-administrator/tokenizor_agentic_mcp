/// Format the result of a replace_symbol_body operation.
pub(crate) fn format_replace(
    path: &str,
    name: &str,
    kind: &str,
    old_bytes: usize,
    new_bytes: usize,
) -> String {
    format!("{path} — replaced {kind} `{name}` ({old_bytes} → {new_bytes} bytes)")
}

/// Format the result of an insert operation.
pub(crate) fn format_insert(
    path: &str,
    name: &str,
    position: &str,
    inserted_bytes: usize,
) -> String {
    format!("{path} — inserted {position} `{name}` ({inserted_bytes} bytes)")
}

/// Format the result of a delete operation.
pub(crate) fn format_delete(path: &str, name: &str, kind: &str, deleted_bytes: usize) -> String {
    format!("{path} — deleted {kind} `{name}` ({deleted_bytes} bytes)")
}

/// Format the result of an edit-within-symbol operation.
pub(crate) fn format_edit_within(
    path: &str,
    name: &str,
    replacements: usize,
    old_bytes: usize,
    new_bytes: usize,
) -> String {
    format!(
        "{path} — edited within `{name}` ({replacements} replacement(s), {old_bytes} → {new_bytes} bytes)"
    )
}

/// Format stale reference warnings after a signature-changing edit.
pub(crate) fn format_stale_warnings(
    _path: &str,
    name: &str,
    refs: &[(String, u32, Option<String>)],
) -> String {
    if refs.is_empty() {
        return String::new();
    }
    let mut out = format!(
        "\n⚠ Signature of `{name}` may have changed — {} reference(s) to check:\n",
        refs.len()
    );
    for (ref_path, line, enclosing) in refs {
        out.push_str(&format!("  {ref_path}:{line}"));
        if let Some(enc) = enclosing {
            out.push_str(&format!(" (in {enc})"));
        }
        out.push('\n');
    }
    out
}

/// Format a batch edit summary.
pub(crate) fn format_batch_summary(results: &[String], file_count: usize) -> String {
    let mut out = format!("{} edit(s) across {} file(s):\n", results.len(), file_count);
    for r in results {
        out.push_str("  ");
        out.push_str(r);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_replace() {
        let result = format_replace("src/lib.rs", "process", "fn", 342, 287);
        assert!(result.contains("src/lib.rs"));
        assert!(result.contains("process"));
        assert!(result.contains("342"));
        assert!(result.contains("287"));
    }

    #[test]
    fn test_format_insert() {
        let result = format_insert("src/lib.rs", "handler", "after", 120);
        assert!(result.contains("src/lib.rs"));
        assert!(result.contains("after"));
        assert!(result.contains("handler"));
        assert!(result.contains("120"));
    }

    #[test]
    fn test_format_delete() {
        let result = format_delete("src/lib.rs", "old_fn", "fn", 200);
        assert!(result.contains("src/lib.rs"));
        assert!(result.contains("old_fn"));
        assert!(result.contains("200"));
    }

    #[test]
    fn test_format_edit_within() {
        let result = format_edit_within("src/lib.rs", "process", 2, 500, 480);
        assert!(result.contains("src/lib.rs"));
        assert!(result.contains("process"));
        assert!(result.contains("2"));
    }

    #[test]
    fn test_format_stale_warnings_empty() {
        let result = format_stale_warnings("src/lib.rs", "foo", &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_format_stale_warnings_with_refs() {
        let refs = vec![
            ("src/main.rs".to_string(), 45, Some("fn main".to_string())),
            ("src/handler.rs".to_string(), 23, None),
        ];
        let result = format_stale_warnings("src/lib.rs", "process", &refs);
        assert!(result.contains("src/main.rs:45"));
        assert!(result.contains("fn main"));
        assert!(result.contains("src/handler.rs:23"));
        assert!(result.contains("2 reference(s)"));
    }

    #[test]
    fn test_format_batch_summary() {
        let results = vec![
            "src/a.rs — replaced `foo`".to_string(),
            "src/b.rs — deleted `bar`".to_string(),
        ];
        let result = format_batch_summary(&results, 2);
        assert!(result.contains("2 edit(s)"));
        assert!(result.contains("2 file(s)"));
    }
}
