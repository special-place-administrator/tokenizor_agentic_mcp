use std::collections::HashMap;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum LanguageId {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    Java,
    C,
    Cpp,
    CSharp,
    Ruby,
    Php,
    Swift,
    Kotlin,
    Dart,
    Perl,
    Elixir,
}

impl LanguageId {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "rs" => Some(Self::Rust),
            "py" => Some(Self::Python),
            "js" | "jsx" => Some(Self::JavaScript),
            "ts" | "tsx" => Some(Self::TypeScript),
            "go" => Some(Self::Go),
            "java" => Some(Self::Java),
            "c" | "h" => Some(Self::C),
            "cpp" | "cxx" | "cc" | "hpp" | "hxx" | "hh" => Some(Self::Cpp),
            "cs" => Some(Self::CSharp),
            "rb" => Some(Self::Ruby),
            "php" => Some(Self::Php),
            "swift" => Some(Self::Swift),
            "dart" => Some(Self::Dart),
            "kt" | "kts" => Some(Self::Kotlin),
            "pl" | "pm" => Some(Self::Perl),
            "ex" | "exs" => Some(Self::Elixir),
            _ => None,
        }
    }

    pub fn extensions(&self) -> &[&str] {
        match self {
            Self::Rust => &["rs"],
            Self::Python => &["py"],
            Self::JavaScript => &["js", "jsx"],
            Self::TypeScript => &["ts", "tsx"],
            Self::Go => &["go"],
            Self::Java => &["java"],
            Self::C => &["c", "h"],
            Self::Cpp => &["cpp", "cxx", "cc", "hpp", "hxx", "hh"],
            Self::CSharp => &["cs"],
            Self::Ruby => &["rb"],
            Self::Php => &["php"],
            Self::Swift => &["swift"],
            Self::Kotlin => &["kt", "kts"],
            Self::Dart => &["dart"],
            Self::Perl => &["pl", "pm"],
            Self::Elixir => &["ex", "exs"],
        }
    }

    pub fn support_tier(&self) -> SupportTier {
        match self {
            Self::Rust | Self::Python | Self::JavaScript | Self::TypeScript | Self::Go => {
                SupportTier::QualityFocus
            }
            Self::Java
            | Self::C
            | Self::Cpp
            | Self::CSharp
            | Self::Ruby
            | Self::Php
            | Self::Swift
            | Self::Kotlin
            | Self::Dart
            | Self::Perl
            | Self::Elixir => SupportTier::Broader,
        }
    }
}

impl fmt::Display for LanguageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Rust => "Rust",
            Self::Python => "Python",
            Self::JavaScript => "JavaScript",
            Self::TypeScript => "TypeScript",
            Self::Go => "Go",
            Self::Java => "Java",
            Self::C => "C",
            Self::Cpp => "C++",
            Self::CSharp => "C#",
            Self::Ruby => "Ruby",
            Self::Php => "PHP",
            Self::Swift => "Swift",
            Self::Kotlin => "Kotlin",
            Self::Dart => "Dart",
            Self::Perl => "Perl",
            Self::Elixir => "Elixir",
        };
        write!(f, "{name}")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SupportTier {
    QualityFocus,
    Broader,
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileProcessingResult {
    pub relative_path: String,
    pub language: LanguageId,
    pub outcome: FileOutcome,
    pub symbols: Vec<SymbolRecord>,
    pub byte_len: u64,
    pub content_hash: String,
    /// Cross-references extracted by `parsing::xref::extract_references`.
    /// Empty until Task 2 wires xref extraction into the parse pipeline.
    pub references: Vec<ReferenceRecord>,
    /// Import alias map for this file: alias -> original name (e.g. "Map" -> "HashMap").
    pub alias_map: HashMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileOutcome {
    Processed,
    PartialParse { warning: String },
    Failed { error: String },
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SymbolRecord {
    pub name: String,
    pub kind: SymbolKind,
    pub depth: u32,
    pub sort_order: u32,
    pub byte_range: (u32, u32),
    pub line_range: (u32, u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Struct,
    Enum,
    Interface,
    Module,
    Constant,
    Variable,
    Type,
    Trait,
    Impl,
    Other,
}

impl fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let prefix = match self {
            Self::Function => "fn",
            Self::Method => "fn",
            Self::Class => "class",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Interface => "interface",
            Self::Module => "mod",
            Self::Constant => "const",
            Self::Variable => "let",
            Self::Type => "type",
            Self::Trait => "trait",
            Self::Impl => "impl",
            Self::Other => "other",
        };
        write!(f, "{prefix}")
    }
}

/// A single cross-reference (call site, import, type usage, or macro use) extracted
/// from a source file. Part of the Phase 4 cross-reference pipeline.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReferenceRecord {
    /// Simple name at the reference site (e.g. "new", "process", "HashMap").
    pub name: String,
    /// Best-effort qualified name when available (e.g. "Vec::new", "fmt.Println").
    pub qualified_name: Option<String>,
    /// What kind of reference this is.
    pub kind: ReferenceKind,
    /// Byte range in the source file (start, end).
    pub byte_range: (u32, u32),
    /// Line range in the source file (start, end — zero-indexed).
    pub line_range: (u32, u32),
    /// Index into the file's symbol list for the innermost containing definition.
    /// `None` means the reference is at module/top level.
    pub enclosing_symbol_index: Option<u32>,
}

/// Discriminates the semantic role of a cross-reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ReferenceKind {
    /// A function or method call site.
    Call,
    /// An import/use/require statement.
    Import,
    /// A type annotation, generic parameter, or other type usage.
    TypeUsage,
    /// A macro invocation.
    MacroUse,
}

impl fmt::Display for ReferenceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Call => "call",
            Self::Import => "import",
            Self::TypeUsage => "type_usage",
            Self::MacroUse => "macro_use",
        };
        write!(f, "{s}")
    }
}

/// Returns the index of the innermost `SymbolRecord` whose `line_range` contains
/// `ref_line`, or `None` if the reference is at module level.
///
/// "Innermost" is defined as the symbol with the latest `line_range.0` (start line)
/// that still contains `ref_line`. This handles nested function definitions correctly.
pub fn find_enclosing_symbol(symbols: &[SymbolRecord], ref_line: u32) -> Option<u32> {
    let mut best: Option<(u32, u32)> = None; // (start_line, index)
    for (idx, sym) in symbols.iter().enumerate() {
        let (start, end) = sym.line_range;
        if ref_line >= start && ref_line <= end {
            match best {
                None => best = Some((start, idx as u32)),
                Some((best_start, _)) if start > best_start => best = Some((start, idx as u32)),
                _ => {}
            }
        }
    }
    best.map(|(_, idx)| idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_kind_display_function() {
        assert_eq!(SymbolKind::Function.to_string(), "fn");
    }

    #[test]
    fn test_symbol_kind_display_method() {
        assert_eq!(SymbolKind::Method.to_string(), "fn");
    }

    #[test]
    fn test_symbol_kind_display_class() {
        assert_eq!(SymbolKind::Class.to_string(), "class");
    }

    #[test]
    fn test_symbol_kind_display_struct() {
        assert_eq!(SymbolKind::Struct.to_string(), "struct");
    }

    #[test]
    fn test_symbol_kind_display_enum() {
        assert_eq!(SymbolKind::Enum.to_string(), "enum");
    }

    #[test]
    fn test_symbol_kind_display_interface() {
        assert_eq!(SymbolKind::Interface.to_string(), "interface");
    }

    #[test]
    fn test_symbol_kind_display_module() {
        assert_eq!(SymbolKind::Module.to_string(), "mod");
    }

    #[test]
    fn test_symbol_kind_display_constant() {
        assert_eq!(SymbolKind::Constant.to_string(), "const");
    }

    #[test]
    fn test_symbol_kind_display_variable() {
        assert_eq!(SymbolKind::Variable.to_string(), "let");
    }

    #[test]
    fn test_symbol_kind_display_type() {
        assert_eq!(SymbolKind::Type.to_string(), "type");
    }

    #[test]
    fn test_symbol_kind_display_trait() {
        assert_eq!(SymbolKind::Trait.to_string(), "trait");
    }

    #[test]
    fn test_symbol_kind_display_impl() {
        assert_eq!(SymbolKind::Impl.to_string(), "impl");
    }

    #[test]
    fn test_symbol_kind_display_other() {
        assert_eq!(SymbolKind::Other.to_string(), "other");
    }

    // --- ReferenceRecord and ReferenceKind ---

    #[test]
    fn test_reference_kind_all_variants_constructible() {
        let _call = ReferenceKind::Call;
        let _import = ReferenceKind::Import;
        let _type_usage = ReferenceKind::TypeUsage;
        let _macro_use = ReferenceKind::MacroUse;
    }

    #[test]
    fn test_reference_kind_display_call() {
        assert_eq!(ReferenceKind::Call.to_string(), "call");
    }

    #[test]
    fn test_reference_kind_display_import() {
        assert_eq!(ReferenceKind::Import.to_string(), "import");
    }

    #[test]
    fn test_reference_kind_display_type_usage() {
        assert_eq!(ReferenceKind::TypeUsage.to_string(), "type_usage");
    }

    #[test]
    fn test_reference_kind_display_macro_use() {
        assert_eq!(ReferenceKind::MacroUse.to_string(), "macro_use");
    }

    #[test]
    fn test_reference_record_construction_with_all_fields() {
        let r = ReferenceRecord {
            name: "foo".to_string(),
            qualified_name: Some("Bar::foo".to_string()),
            kind: ReferenceKind::Call,
            byte_range: (10, 20),
            line_range: (1, 1),
            enclosing_symbol_index: Some(0),
        };
        assert_eq!(r.name, "foo");
        assert_eq!(r.qualified_name.as_deref(), Some("Bar::foo"));
        assert_eq!(r.kind, ReferenceKind::Call);
        assert_eq!(r.byte_range, (10, 20));
        assert_eq!(r.line_range, (1, 1));
        assert_eq!(r.enclosing_symbol_index, Some(0));
    }

    #[test]
    fn test_reference_record_without_optional_fields() {
        let r = ReferenceRecord {
            name: "baz".to_string(),
            qualified_name: None,
            kind: ReferenceKind::Import,
            byte_range: (0, 5),
            line_range: (0, 0),
            enclosing_symbol_index: None,
        };
        assert!(r.qualified_name.is_none());
        assert!(r.enclosing_symbol_index.is_none());
    }

    #[test]
    fn test_file_processing_result_backward_compat_with_empty_refs() {
        use std::collections::HashMap;
        let result = FileProcessingResult {
            relative_path: "test.rs".to_string(),
            language: LanguageId::Rust,
            outcome: FileOutcome::Processed,
            symbols: vec![],
            byte_len: 0,
            content_hash: "abc".to_string(),
            references: vec![],
            alias_map: HashMap::new(),
        };
        assert!(result.references.is_empty());
        assert!(result.alias_map.is_empty());
    }

    #[test]
    fn test_find_enclosing_symbol_innermost_for_nested() {
        // outer: line 0..10, inner: line 3..6
        let symbols = vec![
            SymbolRecord {
                name: "outer".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 100),
                line_range: (0, 10),
            },
            SymbolRecord {
                name: "inner".to_string(),
                kind: SymbolKind::Function,
                depth: 1,
                sort_order: 1,
                byte_range: (30, 60),
                line_range: (3, 6),
            },
        ];
        // Reference at line 4 is inside both — should return inner (index 1)
        let idx = find_enclosing_symbol(&symbols, 4);
        assert_eq!(idx, Some(1), "should return innermost enclosing symbol");
    }

    #[test]
    fn test_find_enclosing_symbol_none_at_module_level() {
        let symbols = vec![SymbolRecord {
            name: "foo".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (50, 100),
            line_range: (5, 10),
        }];
        // Reference at line 0 is not inside any symbol
        let idx = find_enclosing_symbol(&symbols, 0);
        assert_eq!(idx, None, "should return None when not inside any symbol");
    }
}
