# Testing Patterns

**Analysis Date:** 2026-03-14

## Test Framework

**Runner:**
- Rust built-in test framework (no external test runner)
- Run via `cargo test --all-targets`
- CI enforces single-threaded mode: `cargo test --all-targets -- --test-threads=1`

**Assertion Library:**
- Standard `assert_eq!`, `assert!`, `assert_ne!` macros
- `serde_json::from_str()` for JSON parsing validation
- Value comparison with `==`

**Run Commands:**
```bash
cargo test --all-targets              # Run all tests
cargo test --all-targets -- --test-threads=1   # Match CI single-threaded mode
cargo test --lib                      # Unit tests only
cargo test --test '*'                 # Integration tests only
```

## Test File Organization

**Location:**
- Co-located with source code using `#[cfg(test)]` modules
- Tests live in the same file as the code they test
- No separate `tests/` directory at project root

**Naming:**
- Test modules: `mod tests { }`
- Test functions: `test_verb_condition` pattern
  - Examples: `test_fail_open_json_is_valid()`, `test_relative_path_strips_unix_cwd_prefix()`, `test_endpoint_for_read_stdin_routes_to_outline()`

**Structure:**
```
src/
├── cli/
│   ├── hook.rs          # Contains #[cfg(test)] mod tests with ~80+ tests
│   ├── init.rs          # Contains #[cfg(test)] mod tests with ~10 tests
│   └── mod.rs           # Contains #[cfg(test)] mod tests with ~5 tests
├── hash.rs              # Contains #[cfg(test)] mod tests with 2 SHA256 tests
└── [other modules]      # Tests co-located as needed
```

## Test Structure

**Suite Organization:**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    // Test groups organized by function being tested
    // --- fail_open_json ---

    #[test]
    fn test_fail_open_json_is_valid() {
        let json = fail_open_json("PostToolUse");
        let v: Value = serde_json::from_str(&json)
            .expect("fail_open_json must produce valid JSON");

        let output = &v["hookSpecificOutput"];
        assert_eq!(output["hookEventName"], "PostToolUse");
        assert_eq!(output["additionalContext"], "");
    }

    // --- relative_path ---

    #[test]
    fn test_relative_path_strips_unix_cwd_prefix() {
        let rel = relative_path("/home/user/project/src/foo.rs", "/home/user/project");
        assert_eq!(rel, "src/foo.rs");
    }
}
```

**Patterns:**
- Import all public symbols from the module: `use super::*;`
- Import testing utilities: `use serde_json::Value;`
- Comments group related tests: `// --- function_name ---`
- Tests validate single behavior per test function
- Use `expect()` for setup assertions; use `assert_*!()` for behavior assertions

## Testing Patterns

**JSON Parsing Validation:**
```rust
#[test]
fn test_success_json_escapes_special_chars() {
    let context = r#"{"key":"value"}"#;
    let json = success_json("PostToolUse", context);
    let v: Value = serde_json::from_str(&json)
        .expect("success_json with embedded quotes must be valid JSON");

    let ctx = v["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .expect("additionalContext must be a string");
    assert_eq!(ctx, context);
}
```

**Deserialization Testing:**
```rust
#[test]
fn test_parse_stdin_deserializes_read_payload() {
    let json = r#"{"tool_name":"Read","tool_input":{"file_path":"/abs/src/foo.rs"},"cwd":"/abs"}"#;
    let result: HookInput = serde_json::from_str(json).unwrap_or_default();

    assert_eq!(result.tool_name.as_deref(), Some("Read"));
    assert_eq!(
        result.tool_input.as_ref().and_then(|ti| ti.file_path.as_deref()),
        Some("/abs/src/foo.rs")
    );
}
```

**Fail-Open Testing:**
- Tests verify that invalid inputs produce valid fallback JSON
- Ensures graceful degradation: `unwrap_or_default()`
- Example:
  ```rust
  #[test]
  fn test_parse_stdin_returns_default_on_invalid_json() {
      let result: HookInput = serde_json::from_str("not valid json").unwrap_or_default();
      assert!(result.tool_name.is_none());
  }
  ```

**Platform-Specific Testing:**
```rust
#[test]
#[cfg(windows)]
fn test_relative_path_normalizes_backslashes() {
    let rel = relative_path(
        "C:\\Users\\dev\\project\\src\\foo.rs",
        "C:\\Users\\dev\\project",
    );
    // On Windows, path may use backslashes; test normalization to forward slashes
    assert!(
        !rel.contains('\\'),
        "backslashes must be normalized to forward slashes; got: {rel}"
    );
}
```

## Test Helper Functions

**Internal Test Utilities:**
- Located within test module: `#[cfg(test)]`
- Example (`src/cli/hook.rs`):
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      // Helper to construct test input
      fn make_input(
          tool_name: &str,
          file_path: Option<&str>,
          pattern: Option<&str>,
          cwd: &str,
      ) -> HookInput {
          HookInput {
              tool_name: Some(tool_name.to_string()),
              tool_input: Some(HookToolInput {
                  file_path: file_path.map(|s| s.to_string()),
                  pattern: pattern.map(|s| s.to_string()),
                  path: None,
              }),
              cwd: Some(cwd.to_string()),
              prompt: None,
          }
      }
  }
  ```

## Coverage

**Requirements:**
- No explicit coverage target enforced in CI
- Focus on critical paths: error handling, JSON encoding/decoding, path normalization

**Test Count Examples:**
- `src/cli/hook.rs`: ~80+ tests covering all endpoint routing, JSON encoding, path handling
- `src/cli/init.rs`: ~10 tests
- `src/hash.rs`: 2 tests validating SHA256 implementation against known vectors

## Test Types

**Unit Tests:**
- Scope: Single function behavior
- Approach: Direct function calls with known inputs
- Isolation: Tests are independent; can run in any order
- Example: `test_relative_path_strips_unix_cwd_prefix()` tests path normalization

**Integration Tests:**
- Not present in current structure
- Would test interactions between modules (e.g., hook call → sidecar response)
- Could be added in `tests/` directory at project root if needed

**E2E Tests:**
- Not present; codebase is a library/daemon used by Claude Code editor
- Real E2E would require Claude Code editor instance

## Anti-Patterns to Avoid

**From Documentation:**
- Never use stdin mocking in unit tests (per `src/cli/hook.rs` comments)
  - Instead: Provide test JSON strings directly to parsing function
- Never test async behavior with `#[tokio::test]` without reason
  - Use `tokio::runtime::Runtime` in non-async test if async setup needed
- Never hold locks across test assertions
  - Extract owned values before asserting

## Common Test Scenarios

**Option/Result Handling:**
```rust
#[test]
fn test_parse_stdin_returns_default_on_empty() {
    let result: HookInput = serde_json::from_str("").unwrap_or_default();
    assert!(result.tool_name.is_none());
    assert!(result.tool_input.is_none());
    assert!(result.cwd.is_none());
}
```

**String Assertion with Context:**
```rust
#[test]
fn test_relative_path_unchanged_when_no_prefix_match() {
    let rel = relative_path("/unrelated/path.rs", "/home/user/project");
    assert_eq!(rel, "/unrelated/path.rs");
}

#[test]
fn test_url_encode_preserves_safe_characters() {
    let encoded = url_encode("src/foo-bar_baz.rs");
    assert_eq!(encoded, "src/foo-bar_baz.rs");
    // Forward slash, hyphen, underscore, dot all preserved
}
```

**Substring Matching:**
```rust
#[test]
fn test_endpoint_for_read_stdin_routes_to_outline() {
    let input = make_input("Read", Some("/abs/src/foo.rs"), None, "/abs");
    let (path, query) = endpoint_for(Some(&HookSubcommand::Read), &input);

    assert_eq!(path, "/outline");
    assert!(
        query.contains("src/foo.rs"),
        "query must include relative path; got: {query}"
    );
}
```

---

*Testing analysis: 2026-03-14*
