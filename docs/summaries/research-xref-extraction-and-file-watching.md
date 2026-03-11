# Research: Cross-Reference Extraction from Tree-Sitter ASTs + File Watching

**Date**: 2026-03-10
**Status**: Research complete, no code changes made

---

## Part 1: Tree-Sitter Cross-Reference Extraction

### Current State of Tokenizor

The codebase currently extracts **definitions only** (functions, structs, classes, etc.) via manual tree walking in `src/parsing/languages/*.rs`. Each language module uses `walk_node()` to match node types like `function_item`, `class_definition`, etc., and records them as `SymbolRecord` with name, kind, depth, byte_range, line_range.

There is **no cross-reference extraction** yet -- no call sites, no import tracking, no type usage tracking.

### Node Types Per Language for Cross-References

#### Rust (`tree-sitter-rust`)

| Concept | Node Type | Key Fields |
|---------|-----------|------------|
| Function call | `call_expression` | `function` (identifier/scoped_identifier/field_expression), `arguments` |
| Method call | `call_expression` with `field_expression` in function position | `function.field` (field_identifier) |
| Generic function call | `generic_function` | `function`, `type_arguments` |
| Import | `use_declaration` | `argument` (identifier/scoped_identifier/use_list/use_wildcard/use_as_clause/scoped_use_list) |
| Macro invocation | `macro_invocation` | `macro` (identifier/scoped_identifier) |
| Type reference | `type_identifier` | (leaf node, no fields) |
| Scoped path | `scoped_identifier` | `path`, `name` |
| Field access | `field_expression` | `value`, `field` |
| Generic type | `generic_type` | `type`, `type_arguments` |
| Struct expression | `struct_expression` | `name`, `body` |

**Extracting callee name from `call_expression`:**
```
call_expression
  function: identifier            -> name is the identifier text ("foo")
  function: scoped_identifier     -> name is path::name ("Vec::new")
  function: field_expression      -> name is value.field ("self.process")
  function: generic_function      -> recurse into function field
```

**Query example (from highlights.scm):**
```scheme
(call_expression function: (identifier) @function)
(call_expression function: (field_expression field: (field_identifier) @function.method))
(call_expression function: (scoped_identifier "::" name: (identifier) @function))
```

#### Python (`tree-sitter-python`)

| Concept | Node Type | Key Fields |
|---------|-----------|------------|
| Function call | `call` | `function`, `arguments` |
| Method call | `call` with `attribute` in function position | `function.attribute` (identifier) |
| Import | `import_statement` | `name` (aliased_import/dotted_name) |
| Import from | `import_from_statement` | `module_name`, `name` |
| Attribute access | `attribute` | `object`, `attribute` |
| Type hint | `type` node (child of typed_parameter, etc.) | child expression |
| Decorator | `decorator` | child expression |
| Class definition | `class_definition` | `name`, `superclasses` |

**Query example (from tags.scm):**
```scheme
(call
  function: [
    (identifier) @name
    (attribute attribute: (identifier) @name)
  ]) @reference.call
```

#### JavaScript (`tree-sitter-javascript`)

| Concept | Node Type | Key Fields |
|---------|-----------|------------|
| Function call | `call_expression` | `function`, `arguments`, `optional_chain` |
| Method call | `call_expression` with `member_expression` | `function.property` (property_identifier) |
| Constructor | `new_expression` | `constructor`, `arguments` |
| Import | `import_statement` | `source` (string) |
| Import specifier | `import_specifier` | `name`, `alias` |
| Member access | `member_expression` | `object`, `property`, `optional_chain` |

#### TypeScript (`tree-sitter-typescript`)

Inherits all JavaScript node types, plus:

| Concept | Node Type | Key Fields |
|---------|-----------|------------|
| Type annotation | `type_annotation` | child `type` |
| Type identifier | `type_identifier` | (leaf node) |
| Generic type | `generic_type` | `name`, `type_arguments` |
| Interface | `interface_declaration` | `name`, `type_parameters`, `body` |
| Type alias | `type_alias_declaration` | `name`, `type_parameters`, `value` |
| As expression | `as_expression` | child expression + type |
| Satisfies | `satisfies_expression` | child expression + type |
| Call with type args | `call_expression` | adds `type_arguments` field |

#### Go (`tree-sitter-go`)

| Concept | Node Type | Key Fields |
|---------|-----------|------------|
| Function call | `call_expression` | `function`, `arguments`, `type_arguments` |
| Method call | `call_expression` with `selector_expression` | via selector_expression `field` |
| Import | `import_declaration` | children: import_spec or import_spec_list |
| Import path | `import_spec` | `path`, `name` (alias) |
| Selector | `selector_expression` | `operand`, `field` |
| Method decl | `method_declaration` | `receiver`, `name`, `parameters` |
| Qualified type | `qualified_type` | `package`, `name` |
| Composite literal | `composite_literal` | `type`, `body` |

#### Java (`tree-sitter-java`)

| Concept | Node Type | Key Fields |
|---------|-----------|------------|
| Method call | `method_invocation` | `name`, `object`, `arguments`, `type_arguments` |
| Constructor | `object_creation_expression` | `type`, `arguments`, `type_arguments` |
| Import | `import_declaration` | children: identifier/scoped_identifier + asterisk |
| Field access | `field_access` | `object`, `field` |
| Type reference | `type_identifier` | (leaf node) |
| Generic type | `generic_type` | children: type_identifier + type_arguments |
| Scoped name | `scoped_identifier` | `scope`, `name` |

### Tree-Sitter Query Syntax Reference

**Basic pattern:** `(node_type child_pattern...)`

**Field access:** `field_name: (child_type)` -- e.g., `function: (identifier) @name`

**Captures:** `@capture_name` after a node pattern

**Wildcards:** `(_)` matches any named node; `_` matches any node

**Quantifiers:** `+` (one or more), `*` (zero or more), `?` (optional)

**Alternations:** `[pattern1 pattern2]` -- match one of several patterns

**Anchors:** `.` constrains to first/last child or immediate sibling

**Negated fields:** `!field_name` -- match when field is absent

**Predicates:**
- `(#eq? @capture "string")` -- exact text match
- `(#not-eq? @capture "string")` -- negated
- `(#match? @capture "regex")` -- regex match
- `(#any-of? @capture "a" "b" "c")` -- match any of several strings
- `(#set! key "value")` -- attach metadata
- `(#is? property)` / `(#is-not? property)` -- property assertions

**Rust API (tree-sitter 0.24):**
```rust
let query = Query::new(&language, query_source)?;
let mut cursor = QueryCursor::new();
// Option A: iterate matches (grouped by pattern)
for match in cursor.matches(&query, root_node, source.as_bytes()) {
    for capture in match.captures {
        let name = query.capture_names()[capture.index as usize];
        let text = capture.node.utf8_text(source.as_bytes())?;
    }
}
// Option B: iterate captures (flat stream)
for (match, capture_index) in cursor.captures(&query, root_node, source.as_bytes()) {
    // ...
}
```

### Limitations -- What Tree-Sitter Cannot Do

**1. Purely syntactic -- no semantic resolution.**
Tree-sitter gives you the AST node and its text. When you see `new()`, you get the string `"new"`. You do NOT know which struct's `new()` it is. For `Vec::new()` you get `"Vec::new"` as text from the `scoped_identifier`, but you don't know it resolves to `std::vec::Vec`.

**2. Method calls on objects are not resolvable.**
`self.foo()` gives you `field_expression` with field `"foo"` and value `"self"`. You know the method name is `foo`, but you do NOT know the type of `self` and therefore cannot resolve which `foo` implementation is called.

**3. No type inference.**
```rust
let x = get_something();
x.process();  // tree-sitter knows method name is "process" but not the type of x
```

**4. No cross-file resolution.**
Tree-sitter parses one file at a time. `use crate::foo::Bar` tells you the import path text, but tree-sitter cannot follow it to the actual definition.

**5. Aliased imports create ambiguity.**
`use std::collections::HashMap as Map` -- later uses of `Map` won't match `HashMap` by name alone.

**6. Trait method dispatch is invisible.**
`item.len()` could be `Vec::len`, `String::len`, `&str::len`, a custom `Len` trait impl, etc. Tree-sitter only sees `"len"`.

### Practical Approach: Building a Call-Site Index

**Recommended strategy: Simple name matching with path context.**

The approach that balances effort vs. value for a code indexing tool:

#### A. What to Extract Per File

For each file, extract two lists:

1. **Definitions** (already done): function/method/class/struct names with their byte ranges
2. **References** (new): call sites, imports, type usages -- each with:
   - `name`: the simple name string (e.g., `"process"`, `"Vec"`, `"HashMap"`)
   - `qualified_name`: best-effort qualified name (e.g., `"self.process"`, `"Vec::new"`, `"std::collections::HashMap"`)
   - `ref_kind`: Call | Import | TypeUsage | FieldAccess
   - `byte_range` / `line_range`: location
   - `enclosing_symbol`: which definition this reference appears inside (for scope context)

#### B. Resolution Strategy

**Phase 1 (purely syntactic, do this first):**
- Match references to definitions by simple name within the same file
- Match references to definitions by simple name across the index (fuzzy: multiple candidates are fine)
- Record import paths verbatim for later resolution

**Phase 2 (import-aware, optional enhancement):**
- Build an import map per file: `{alias -> qualified_path}`
- When you see `Map::new()`, look up `Map` in the import map to get `std::collections::HashMap`
- Match qualified paths to definitions across files

**Phase 3 (scope-aware, advanced):**
- Track which `impl` block a method is defined in
- Use `impl Type` context to disambiguate method calls when the receiver type is known
- This is where the ROI drops sharply -- consider LSP integration instead

#### C. Query-Based vs. Walk-Based Extraction

Two implementation approaches:

**Option 1: Tree-sitter queries (recommended).**
Write `.scm` query files per language, use `QueryCursor` to extract matches. This is:
- Declarative and easy to maintain
- Matches the official `tags.scm` pattern used by GitHub/tree-sitter
- Can be loaded from files, making them editable without recompilation
- The `tags.scm` files from tree-sitter repos are a ready-made starting point

**Option 2: Manual tree walking (current approach).**
Extend the existing `walk_node()` functions to also record call expressions. This is:
- More flexible for complex extraction logic
- Already the pattern used in the codebase
- Harder to maintain across 6 languages

**Recommendation:** Use queries for reference extraction. The official `tags.scm` files already define patterns for `@reference.call` and `@definition.*` captures. You can start with those and extend.

#### D. Suggested Reference Record

```rust
pub struct ReferenceRecord {
    /// The simple name at the call site (e.g., "new", "process", "HashMap")
    pub name: String,
    /// Best-effort qualified name (e.g., "Vec::new", "self.process")
    pub qualified_name: Option<String>,
    /// What kind of reference this is
    pub kind: ReferenceKind,
    /// Byte range in the source file
    pub byte_range: (u32, u32),
    /// Line range
    pub line_range: (u32, u32),
    /// Index into the file's symbol list for the enclosing definition (if any)
    pub enclosing_symbol_index: Option<u32>,
}

pub enum ReferenceKind {
    Call,        // function/method call
    Import,      // use/import statement
    TypeUsage,   // type annotation, generic parameter, etc.
    MacroUse,    // macro invocation
}
```

#### E. Cross-Language Query Templates

**Rust references query:**
```scheme
;; Function calls
(call_expression function: (identifier) @ref.call)
(call_expression function: (scoped_identifier name: (identifier) @ref.call))
(call_expression function: (field_expression field: (field_identifier) @ref.method_call))

;; Macro invocations
(macro_invocation macro: (identifier) @ref.macro)
(macro_invocation macro: (scoped_identifier name: (identifier) @ref.macro))

;; Imports
(use_declaration argument: (identifier) @ref.import)
(use_declaration argument: (scoped_identifier) @ref.import)

;; Type references
(type_identifier) @ref.type
(generic_type type: (type_identifier) @ref.type)
```

**Python references query:**
```scheme
;; Function/method calls
(call function: (identifier) @ref.call)
(call function: (attribute attribute: (identifier) @ref.method_call))

;; Imports
(import_statement name: (dotted_name) @ref.import)
(import_from_statement module_name: (dotted_name) @ref.import)
(import_from_statement name: (dotted_name) @ref.import)

;; Type hints (in annotations)
(type (identifier) @ref.type)
```

**JavaScript/TypeScript references query:**
```scheme
;; Function/method calls
(call_expression function: (identifier) @ref.call)
(call_expression function: (member_expression property: (property_identifier) @ref.method_call))

;; Constructor calls
(new_expression constructor: (identifier) @ref.call)

;; Imports
(import_statement source: (string) @ref.import)
(import_specifier name: (identifier) @ref.import)

;; TypeScript type references
(type_identifier) @ref.type
(generic_type name: (type_identifier) @ref.type)
```

**Go references query:**
```scheme
;; Function/method calls
(call_expression function: (identifier) @ref.call)
(call_expression function: (selector_expression field: (field_identifier) @ref.method_call))

;; Imports
(import_spec path: (_) @ref.import)

;; Type references
(type_identifier) @ref.type
(qualified_type name: (type_identifier) @ref.type)
```

**Java references query:**
```scheme
;; Method calls
(method_invocation name: (identifier) @ref.call)

;; Constructor calls
(object_creation_expression type: (type_identifier) @ref.call)

;; Imports
(import_declaration (scoped_identifier) @ref.import)

;; Type references
(type_identifier) @ref.type
(generic_type (type_identifier) @ref.type)
```

---

## Part 2: Rust `notify` Crate for File Watching

### Overview

- **Crate**: `notify` v8.2.0 (MSRV 1.85)
- **License**: CC0 / MIT / Apache-2.0
- **Purpose**: Cross-platform filesystem event notification

### Platform Backends

| Platform | Backend | API |
|----------|---------|-----|
| Linux/Android | inotify | `INotifyWatcher` |
| macOS | FSEvents (default) or kqueue | `FsEventWatcher` / `KqueueWatcher` |
| Windows | ReadDirectoryChangesW | (impl name varies) |
| FreeBSD/NetBSD/OpenBSD | kqueue | `KqueueWatcher` |
| All platforms | Polling fallback | `PollWatcher` |

### Core API

```rust
use notify::{Watcher, RecommendedWatcher, RecursiveMode, Config, Event, EventKind};
use std::sync::mpsc;

// Create channel-based watcher
let (tx, rx) = mpsc::channel();
let mut watcher = notify::recommended_watcher(tx)?;

// Watch a directory recursively
watcher.watch(Path::new("./src"), RecursiveMode::Recursive)?;

// Process events
for event in rx {
    match event {
        Ok(event) => handle_event(event),
        Err(e) => eprintln!("watch error: {e}"),
    }
}
```

### Watcher Trait Methods

- `watch(&mut self, path: &Path, recursive_mode: RecursiveMode) -> Result<()>`
- `unwatch(&mut self, path: &Path) -> Result<()>`
- `configure(&mut self, option: Config) -> Result<bool>`
- `kind() -> WatcherKind`
- `paths_mut(&mut self) -> Box<dyn PathsMut>` -- batch add/remove paths

### Event Types

```rust
pub enum EventKind {
    Any,                        // Unknown/catch-all
    Access(AccessKind),         // File opened/closed/executed
    Create(CreateKind),         // File/Folder/Any/Other
    Modify(ModifyKind),         // Data/Metadata/Name(rename)/Any/Other
    Remove(RemoveKind),         // File/Folder/Any/Other
    Other,                      // Meta-events about the watch itself
}

pub enum ModifyKind {
    Any,
    Data(DataChange),           // Content changed
    Metadata(MetadataKind),     // Permissions, timestamps, etc.
    Name(RenameMode),           // Rename: From, To, Both, Any, Other
    Other,
}

pub enum CreateKind { Any, File, Folder, Other }
```

### Config Options

- `with_poll_interval(Duration)` -- default 30s for PollWatcher
- `with_manual_polling()` -- disable auto-poll, call `PollWatcher::poll()` manually
- `with_compare_contents(bool)` -- hash files to detect real changes (expensive)
- `with_follow_symlinks(bool)` -- follow symlinks during recursive watch (default: true)

### RecursiveMode

- `Recursive` -- watch all subdirectories, including newly created ones
- `NonRecursive` -- watch only the specified directory

### Debouncing: `notify-debouncer-full` v0.7.0

The base `notify` crate does NOT include debouncing. Use the companion crate:

```rust
use notify_debouncer_full::new_debouncer;
use std::time::Duration;

let mut debouncer = new_debouncer(Duration::from_secs(2), None, |result| {
    match result {
        Ok(events) => { /* DebouncedEvent list */ },
        Err(errors) => { /* Error list */ },
    }
})?;

debouncer.watch("./src", RecursiveMode::Recursive)?;
```

**Features of `notify-debouncer-full`:**
- Consolidates rename `From`+`To` events into single events
- Updates paths in queued events when renames happen before emission
- Collapses multiple `Remove` events for directory deletion (inotify quirk)
- Deduplicates `Create` + immediate `Modify` events
- Optional file ID tracking for cross-platform rename matching
- Feature flags: `serde`, `crossbeam-channel`, `flume`, `macos_fsevent`, `macos_kqueue`

### Known Limitations and Gotchas

1. **Network filesystems**: Unreliable -- events may be missed or delayed
2. **Docker on M1 Mac**: Compatibility issues reported
3. **Linux inotify limits**: Each watched directory consumes an inotify descriptor. Default limit is ~8192. Large repos may hit this. Workaround: increase `/proc/sys/fs/inotify/max_user_watches` or use `PollWatcher`
4. **Windows/MSYS**: Uses `ReadDirectoryChangesW` API. No specific MSYS issues documented, but this is the native Windows API so it should work correctly under MSYS/MinGW since it's the same kernel
5. **PollWatcher fallback**: Available on all platforms. Higher latency (default 30s interval) but no OS-level limits. Good for containers, network mounts, or when native watchers fail
6. **Rename tracking**: Platform-dependent. Some platforms emit separate `From`/`To` events, others emit a single `Both` event. `notify-debouncer-full` normalizes this

### Recommendation for Tokenizor

For a code indexing tool that needs to know when source files change:

1. **Use `notify` with `notify-debouncer-full`** -- 2-second debounce covers the burst of events during save/compile
2. **Watch the repo root recursively** -- `RecursiveMode::Recursive` handles new directories automatically
3. **Filter events**: Only care about `Create(File)`, `Modify(Data)`, `Remove(File)`, and `Modify(Name)` on files matching supported extensions
4. **Debounce to file-level**: Multiple rapid edits to the same file should collapse into one re-index event
5. **Fallback**: Offer `PollWatcher` mode for users on network mounts or Docker volumes

---

## Key Decisions for Implementation

1. **Query-based extraction is preferred** over extending walk_node(). The tags.scm patterns from official repos are battle-tested.

2. **Simple name matching is the right starting point.** Fully-qualified resolution is a diminishing-returns rabbit hole. Store the qualified text when available (e.g., `Vec::new`), but match on simple names.

3. **The `enclosing_symbol` field is high value.** Knowing that a call to `process()` happens inside `fn handle_request()` enables "who calls what" queries without semantic analysis.

4. **notify + notify-debouncer-full** is the standard Rust solution for file watching. The API is mature (v8.2) and cross-platform.

5. **No new dependencies needed for xref extraction** -- the existing `tree-sitter` 0.24 already has `Query` and `QueryCursor`. Just need to write the query strings.
