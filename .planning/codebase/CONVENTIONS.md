# Coding Conventions

**Analysis Date:** 2026-03-14

## Naming Patterns

**Files:**
- Snake case for module files: `git_temporal.rs`, `live_index.rs`, `edit_format.rs`
- Entry point files: `main.rs`, `lib.rs`, `mod.rs`
- Test modules in same file: `#[cfg(test)] mod tests { }`

**Functions:**
- Snake case: `process_file()`, `find_enclosing_symbol()`, `parse_stdin_input()`, `sync_http_get()`
- Helper functions marked `pub(crate)` for internal access only
- Public API functions marked `pub`

**Variables:**
- Snake case throughout: `file_count`, `symbol_count`, `parsed_count`, `partial_parse_count`
- Loop indices: `index`, not single letters
- Constants: `UPPER_SNAKE_CASE`: `INITIAL_STATE`, `K`, `PORT_FILE`, `HTTP_TIMEOUT`

**Types:**
- PascalCase for enums: `LanguageId`, `FileClass`, `FileClassification`, `TokenizorError`, `StartupIndexLogView`
- PascalCase for structs: `SymbolRecord`, `ReferenceRecord`, `HookInput`, `TokenizorServer`
- Generic type bounds use standard naming: `T`, `D: Deserializer<'de>`

**Module Organization:**
- Group related functions within modules
- Barrel exports via `pub use` in `mod.rs`: `src/live_index/mod.rs` re-exports from submodules
- Private modules: `edit`, `edit_format` (marked `pub(crate)`)
- Public modules: `format`, `tools`, `resources`, `explore`

## Code Style

**Formatting:**
- `cargo fmt` enforced in CI (`.github/workflows/ci.yml`: `cargo fmt --all --check`)
- Default Rust formatting rules apply
- No custom `.rustfmt.toml` — uses Cargo defaults

**Linting:**
- No clippy config detected; defaults apply
- Focus on correctness over style warnings

**Line Length:**
- No explicit limit configured (Rust default: ~99 characters)

**Imports:**
- Group by category:
  1. Standard library: `use std::...`
  2. External crates: `use serde::...`, `use rmcp::...`, `use tree_sitter::...`
  3. Internal modules: `use crate::{...}` with wildcard expansions
- Order imports alphabetically within groups
- Example (`src/protocol/tools.rs`):
  ```rust
  use std::collections::HashMap;
  use std::path::PathBuf;
  use std::sync::{Arc, RwLock};

  use axum::http::StatusCode;
  use rmcp::handler::server::wrapper::Parameters;
  use serde::{Deserialize, Deserializer, Serialize};

  use crate::domain::LanguageId;
  use crate::live_index::{IndexedFile, SearchFilesView};
  ```

## Error Handling

**Strategy:**
- Custom error type `TokenizorError` with `#[derive(thiserror::Error)]` macro
- Result alias: `pub type Result<T> = std::result::Result<T, TokenizorError>;`
- Location: `src/error.rs`

**Patterns:**
- `thiserror` for error definition with `#[error(...)]` attributes
- `anyhow::Result<()>` for main/top-level functions
- `TokenizorError::io(path, source)` constructor for I/O context
- `impl From<io::Error>` for transparent error conversions

**Example** (`src/error.rs`):
```rust
#[derive(Debug, Error)]
pub enum TokenizorError {
    #[error("i/o error at `{path}`: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("parse error: {0}")]
    Parse(String),
}

impl TokenizorError {
    pub fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io { path: path.into(), source }
    }
}
```

## Comments & Documentation

**Module Documentation:**
- Each module file starts with `//!` doc comment explaining purpose
- Example (`src/cli/hook.rs`):
  ```rust
  //! Hook binary logic — reads `.tokenizor/sidecar.port`, calls sidecar over sync HTTP,
  //! and outputs a single JSON line to stdout.
  //!
  //! Design constraints (HOOK-10):
  //! - The ONLY thing written to stdout is the final JSON line.
  ```

**Function Documentation:**
- `///` doc comments for public functions
- Explains parameters, return values, and error cases
- Include examples or behavior notes for complex functions
- Example (`src/cli/hook.rs`):
  ```rust
  /// Converts an absolute path to a relative path by stripping the `cwd` prefix.
  ///
  /// Uses `std::path::Path::strip_prefix` for correct platform-aware stripping,
  /// then normalises backslashes to forward slashes for the sidecar query.
  /// Returns `absolute` unchanged if it does not start with `cwd`.
  pub(crate) fn relative_path(absolute: &str, cwd: &str) -> String {
  ```

**Inline Comments:**
- Single-line comments for complex logic blocks
- Marked with logical IDs for reference: `HOOK-10`, `INFR-02`, `AD-6`
- Example (`src/protocol/tools.rs`):
  ```rust
  /// Each handler follows the pattern:
  /// 1. Acquire read lock (or write lock for `index_folder`)
  /// 2. Check loading guard (except `health` which always responds)
  /// 3. Extract needed data into owned values
  /// 4. Drop lock
  /// 5. Call `format::` function
  /// 6. Return `String`
  ```

**When to Comment:**
- Design constraints and invariants (e.g., "Never hold RwLockReadGuard across await points")
- Anti-patterns being avoided (e.g., "Never return JSON — always plain text String")
- Platform-specific behavior (e.g., Windows backslash normalization)
- Reference IDs for requirements (HOOK-, INFR-, AD-)

## Panic Handling

**Use `panic::catch_unwind`:**
- Wrap potentially panicking operations (tree-sitter parser)
- Return `FileOutcome::Failed` with error message on panic
- Example (`src/parsing/mod.rs`):
  ```rust
  let parse_result = panic::catch_unwind(|| parse_source(&source, &language));
  match parse_result {
      Ok(Ok((symbols, has_error, references, alias_map))) => { ... },
      Ok(Err(err)) => { ... },
      Err(_panic) => FileProcessingResult {
          outcome: FileOutcome::Failed {
              error: "tree-sitter parser panicked during parsing".to_string(),
          },
          ...
      },
  }
  ```

## Async/Await Patterns

**Rules:**
- Never hold RwLock guard across await points; extract into owned values first
- Example (`src/protocol/mod.rs`):
  ```rust
  // Step 1 — extract data while holding lock
  let client = daemon_lock.read().await;
  match client.call_tool_value(tool_name, value.clone()).await {
      Ok(result) => return Some(result),
      ...
  }
  // Step 2 — drop lock before long async operation
  ```

## Struct & Enum Design

**Derive Macros:**
- Standard derives: `Clone, Debug, PartialEq, Eq, Hash`
- Serialization: `serde::Serialize, serde::Deserialize`
- JSON Schema: `schemars::JsonSchema`
- Example (`src/domain/index.rs`):
  ```rust
  #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord,
           serde::Serialize, serde::Deserialize)]
  pub enum LanguageId { ... }
  ```

**Serde Custom Deserializers:**
- Used for lenient parsing (accept JSON numbers OR string numbers)
- Marked with `#[serde(deserialize_with = "...")]`
- Example (`src/protocol/tools.rs`):
  ```rust
  pub(crate) fn lenient_u32<'de, D: Deserializer<'de>>(
      deserializer: D,
  ) -> Result<Option<u32>, D::Error> {
      #[derive(Deserialize)]
      #[serde(untagged)]
      enum NumOrStr { Num(u32), Str(String), Null }
      match NumOrStr::deserialize(deserializer)? {
          NumOrStr::Num(n) => Ok(Some(n)),
          NumOrStr::Str(s) => s.parse::<u32>().map(Some).map_err(...),
          ...
      }
  }
  ```

## Feature Gates

**Defined:**
- `v1`: Feature gate for v1 domain types; kept for backward-compatibility during migration
- Located in `Cargo.toml` features section
- Purpose: Keep `retrieval_conformance.rs` compiling without v1 types during Phase 2

## Common Patterns

**Path Handling:**
- Normalize backslashes to forward slashes on Windows
- Use `std::path::Path::strip_prefix` for platform-aware operations
- Example (`src/cli/hook.rs`):
  ```rust
  pub(crate) fn relative_path(absolute: &str, cwd: &str) -> String {
      match abs.strip_prefix(base) {
          Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
          Err(_) => absolute.to_string(),
      }
  }
  ```

**JSON String Escaping:**
- Minimal escaping for backslash, quotes, newlines, carriage returns, tabs
- Example (`src/cli/hook.rs`):
  ```rust
  fn json_escape(s: &str) -> String {
      let mut out = String::with_capacity(s.len());
      for c in s.chars() {
          match c {
              '"' => out.push_str("\\\""),
              '\\' => out.push_str("\\\\"),
              '\n' => out.push_str("\\n"),
              ...
          }
      }
      out
  }
  ```

**URL Encoding:**
- Only encode unsafe characters in query strings (space, `&`, `=`, `+`, `%`, non-ASCII)
- Preserve safe characters (`-`, `_`, `.`, `~`, `/`, `:`)
- Example (`src/cli/hook.rs`):
  ```rust
  fn url_encode(s: &str) -> String {
      for b in s.bytes() {
          match b {
              b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
              | b'-' | b'_' | b'.' | b'~' | b'/' | b':' => out.push(b as char),
              b => out.push_str(&format!("%{b:02X}")),
          }
      }
      out
  }
  ```

---

*Convention analysis: 2026-03-14*
