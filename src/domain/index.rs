use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
            Self::Java => SupportTier::Broader,
            Self::C
            | Self::Cpp
            | Self::CSharp
            | Self::Ruby
            | Self::Php
            | Self::Swift
            | Self::Dart
            | Self::Perl
            | Self::Elixir => SupportTier::Unsupported,
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileOutcome {
    Processed,
    PartialParse { warning: String },
    Failed { error: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolRecord {
    pub name: String,
    pub kind: SymbolKind,
    pub depth: u32,
    pub sort_order: u32,
    pub byte_range: (u32, u32),
    pub line_range: (u32, u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
}
