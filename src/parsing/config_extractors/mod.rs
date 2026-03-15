pub mod env;
pub mod json;
pub mod markdown;
pub mod toml_ext;
pub mod yaml;

use crate::domain::{LanguageId, SymbolRecord};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const MAX_DEPTH: u32 = 6;
pub const MAX_ARRAY_ITEMS: usize = 20;

// ---------------------------------------------------------------------------
// EditCapability
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditCapability {
    IndexOnly,
    TextEditSafe,
    StructuralEditSafe,
}

// ---------------------------------------------------------------------------
// ExtractionResult / ExtractionOutcome
// ---------------------------------------------------------------------------

pub struct ExtractionResult {
    pub symbols: Vec<SymbolRecord>,
    pub outcome: ExtractionOutcome,
}

pub enum ExtractionOutcome {
    Ok,
    Failed(String),
}

// ---------------------------------------------------------------------------
// ConfigExtractor trait
// ---------------------------------------------------------------------------

pub trait ConfigExtractor: Send + Sync {
    fn extract(&self, content: &[u8]) -> ExtractionResult;
    fn edit_capability(&self) -> EditCapability;
}

// ---------------------------------------------------------------------------
// Registry helpers
// ---------------------------------------------------------------------------

/// Returns true for config-style languages handled by this module.
pub fn is_config_language(language: &LanguageId) -> bool {
    matches!(
        language,
        LanguageId::Json
            | LanguageId::Toml
            | LanguageId::Yaml
            | LanguageId::Markdown
            | LanguageId::Env
    )
}

/// Returns a boxed extractor for the given language, or None for non-config languages.
pub fn extractor_for(language: &LanguageId) -> Option<Box<dyn ConfigExtractor>> {
    match language {
        LanguageId::Json => Some(Box::new(json::JsonExtractor)),
        LanguageId::Toml => Some(Box::new(toml_ext::TomlExtractor)),
        LanguageId::Yaml => Some(Box::new(yaml::YamlExtractor)),
        LanguageId::Markdown => Some(Box::new(markdown::MarkdownExtractor)),
        LanguageId::Env => Some(Box::new(env::EnvExtractor)),
        _ => None,
    }
}

/// Returns the edit capability for the given language by delegating to its extractor.
pub fn edit_capability_for(language: &LanguageId) -> Option<EditCapability> {
    extractor_for(language).map(|e| e.edit_capability())
}

/// Unified edit capability check for all languages (config + source).
/// Returns `None` for languages with no edit restrictions (mature tree-sitter languages).
pub fn edit_capability_for_language(language: &LanguageId) -> Option<EditCapability> {
    // Config languages — delegate to their extractor
    if let Some(cap) = edit_capability_for(language) {
        return Some(cap);
    }
    // Source languages with restricted editing
    match language {
        LanguageId::Html | LanguageId::Css | LanguageId::Scss => Some(EditCapability::TextEditSafe),
        // All other source languages → None (unrestricted)
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Key escaping helpers
// ---------------------------------------------------------------------------

/// Escapes a raw key segment:
/// - `~` → `~0`
/// - `.` → `~1`
/// - `[` → `~2`
/// - `]` → `~3`
pub fn escape_key_segment(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '~' => out.push_str("~0"),
            '.' => out.push_str("~1"),
            '[' => out.push_str("~2"),
            ']' => out.push_str("~3"),
            _ => out.push(ch),
        }
    }
    out
}

/// Joins a parent path and a child key segment with a dot, escaping the child.
pub fn join_key_path(parent: &str, child: &str) -> String {
    let escaped = escape_key_segment(child);
    if parent.is_empty() {
        escaped
    } else {
        format!("{}.{}", parent, escaped)
    }
}

/// Joins a parent path with an array index: `parent[index]`.
pub fn join_array_index(parent: &str, index: usize) -> String {
    format!("{}[{}]", parent, index)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_key_segment_plain() {
        assert_eq!(escape_key_segment("hello"), "hello");
    }

    #[test]
    fn test_escape_key_segment_tilde() {
        assert_eq!(escape_key_segment("a~b"), "a~0b");
    }

    #[test]
    fn test_escape_key_segment_dot() {
        assert_eq!(escape_key_segment("a.b"), "a~1b");
    }

    #[test]
    fn test_escape_key_segment_brackets() {
        assert_eq!(escape_key_segment("a[0]"), "a~20~3");
    }

    #[test]
    fn test_escape_key_segment_all_special() {
        assert_eq!(escape_key_segment("~.[]]"), "~0~1~2~3~3");
    }

    #[test]
    fn test_join_key_path_from_empty_parent() {
        assert_eq!(join_key_path("", "child"), "child");
    }

    #[test]
    fn test_join_key_path_with_parent() {
        assert_eq!(join_key_path("root", "child"), "root.child");
    }

    #[test]
    fn test_join_key_path_escapes_child() {
        assert_eq!(join_key_path("root", "a.b"), "root.a~1b");
    }

    #[test]
    fn test_join_key_path_escapes_child_tilde() {
        assert_eq!(join_key_path("parent", "x~y"), "parent.x~0y");
    }

    #[test]
    fn test_join_array_index() {
        assert_eq!(join_array_index("items", 3), "items[3]");
    }

    #[test]
    fn test_join_array_index_zero() {
        assert_eq!(join_array_index("arr", 0), "arr[0]");
    }

    #[test]
    fn test_is_config_language_json() {
        assert!(is_config_language(&LanguageId::Json));
    }

    #[test]
    fn test_is_config_language_toml() {
        assert!(is_config_language(&LanguageId::Toml));
    }

    #[test]
    fn test_is_config_language_yaml() {
        assert!(is_config_language(&LanguageId::Yaml));
    }

    #[test]
    fn test_is_config_language_markdown() {
        assert!(is_config_language(&LanguageId::Markdown));
    }

    #[test]
    fn test_is_config_language_env() {
        assert!(is_config_language(&LanguageId::Env));
    }

    #[test]
    fn test_is_config_language_rust_false() {
        assert!(!is_config_language(&LanguageId::Rust));
    }

    #[test]
    fn test_edit_capability_for_language_frontend() {
        use crate::domain::LanguageId;

        // Frontend languages should return TextEditSafe
        assert_eq!(
            edit_capability_for_language(&LanguageId::Html),
            Some(EditCapability::TextEditSafe)
        );
        assert_eq!(
            edit_capability_for_language(&LanguageId::Css),
            Some(EditCapability::TextEditSafe)
        );
        assert_eq!(
            edit_capability_for_language(&LanguageId::Scss),
            Some(EditCapability::TextEditSafe)
        );

        // Config languages delegate to their extractor
        // JSON delegates to its extractor — verify it returns Some (exact level varies)
        assert!(edit_capability_for_language(&LanguageId::Json).is_some());

        // Regular source languages return None (unrestricted)
        assert_eq!(edit_capability_for_language(&LanguageId::Rust), None);
        assert_eq!(edit_capability_for_language(&LanguageId::Python), None);
    }
}
