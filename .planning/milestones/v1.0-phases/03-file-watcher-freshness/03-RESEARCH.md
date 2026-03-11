# Phase 3: File Watcher + Freshness - Research

**Researched:** 2026-03-10
**Domain:** Rust file watching with notify + notify-debouncer-full, tokio task integration, Windows path normalization
**Confidence:** HIGH

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Debounce strategy:**
- Adaptive debounce: start at 200ms, extend to 500ms on burst detection
- Burst threshold: if >3 events for the same file within the 200ms window, extend that file's next debounce to 500ms
- Resets after a quiet period (e.g., 5s with no events for that file)
- Use `notify-debouncer-full` as the debouncing layer on top of `notify`
- content_hash always checked before re-parsing — if file bytes hash to same value, skip tree-sitter parse entirely (eliminates false re-indexes from metadata-only events or editor no-op writes)
- Rename events treated as delete + create (no special rename tracking). notify-debouncer-full already consolidates From+To events.

**Watcher lifecycle:**
- Watcher auto-starts immediately after initial index load completes
- Controlled by same TOKENIZOR_AUTO_INDEX env var — if auto-index is off, watcher is off too
- Calling index_folder also starts (or restarts) the watcher
- Watches repo root recursively (RecursiveMode::Recursive) — handles new directories automatically
- Only processes events for files with supported language extensions (6 languages)

**Failure behavior:**
- Auto-restart with 1s backoff on watcher errors (OS limit hit, watch path deleted, internal panic)
- After 3 consecutive restart failures, enter degraded mode — index is stale but queries still work
- health tool reports watcher state (active/degraded/off)
- index_folder resets watcher from degraded mode — attempts fresh watcher start. If fails again, back to degraded.
- File deletion during re-index (ENOENT): remove entry from LiveIndex silently, log warning. No crash, no error propagation to queries.

**Health stats extension:**
- health tool adds: watcher_state (active/degraded/off), events_processed count, last_event_at timestamp, debounce_window_ms
- Gives the model visibility into freshness status

**Change tracking:**
- Watcher updates LiveIndex's loaded_at_system timestamp on each file update
- what_changed(since) picks up watcher-triggered changes naturally via existing timestamp comparison

**Logging:**
- INFO level: watcher started/stopped/restarted, degraded mode entered/exited
- DEBUG level: individual file re-index events (path + duration + hash-skip/reparse)
- WARN level: watcher failures, restart attempts, file read errors
- Quiet at default info level — no noise during active editing sessions

### Claude's Discretion
- Exact notify-debouncer-full configuration and channel type (std::sync::mpsc vs crossbeam vs tokio channel)
- Internal structure of the watcher module (single file vs mod.rs + submodules)
- How the watcher task communicates with SharedIndex (channel vs direct Arc access)
- Windows path normalization implementation details (ReadDirectoryChangesW C:\ paths → forward-slash relative paths)
- Exact backoff strategy for watcher restart (linear vs exponential)

### Deferred Ideas (OUT OF SCOPE)
None — discussion stayed within phase scope.
</user_constraints>

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| FRSH-01 | File watcher (notify crate) detects file changes within 200ms (debounced) | notify-debouncer-full 0.7 with 200ms timeout; FileIdMap cache for reliable event consolidation |
| FRSH-02 | Single-file incremental reparse completes in <50ms | `parsing::process_file()` already exists; single-file re-parse is just calling it on new bytes; content_hash skip avoids redundant parses |
| FRSH-03 | LiveIndex always reflects current disk state — queries never serve stale data | Write lock on SharedIndex for each update; `update_file`/`remove_file`/`add_file` new methods on LiveIndex |
| FRSH-04 | File creation detected and indexed automatically | notify EventKind::Create — add_file path in watcher callback |
| FRSH-05 | File deletion detected and removed from LiveIndex automatically | notify EventKind::Remove — remove_file path in watcher callback; ENOENT handled silently |
| FRSH-06 | Real-time synchronization — index syncs in milliseconds on any file change, always current | Debounce 200ms baseline + content_hash skip for near-instant effective freshness |
| RELY-03 | File deletion during edit handled gracefully (no panic/crash) | ENOENT guard in watcher callback: if fs::read fails with NotFound, call remove_file, log warn, no propagation |
</phase_requirements>

---

## Summary

Phase 3 adds a file watcher to the LiveIndex so that queries always reflect disk state after any edit, create, or delete. The locked decisions define the complete behavior: `notify-debouncer-full` provides event consolidation with per-file debouncing, a SHA-256 content hash check gates tree-sitter reparsing, and a background tokio task owns the watcher and mutates `SharedIndex` via write lock.

The core architecture is a watcher task spawned after initial load in `main.rs`. It owns a `Debouncer` and holds a clone of the `SharedIndex` (Arc). For each debounced event batch, it filters to supported extensions, reads the file, hashes content, compares against `IndexedFile.content_hash`, and only reparses when the hash differs. New LiveIndex methods (`update_file`, `add_file`, `remove_file`) are needed — these acquire a write lock and mutate `self.files` directly, then update `loaded_at_system` so `what_changed` works naturally.

The adaptive debounce (200ms → 500ms on burst) is NOT built into `notify-debouncer-full` itself — that library uses a fixed timeout. The adaptive layer must be implemented in the watcher task's event processing loop using per-file state tracking (`HashMap<PathBuf, BurstTracker>`). The content_hash skip is the primary optimization: editors typically write 3-6 raw OS events per save, and most of them produce identical bytes — only the first hash change triggers a tree-sitter parse.

**Primary recommendation:** Implement `src/watcher/mod.rs` as a standalone module exporting a `WatcherHandle` struct. The handle owns the `Debouncer` (drops it on stop), tracks watcher state, and exposes a restart API called by `index_folder`. The watcher task uses `std::sync::mpsc::channel` to receive events from `notify-debouncer-full` (the most straightforward integration with the sync-first codebase). Tokio's `spawn_blocking` is not needed — notify runs its own internal thread.

---

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `notify` | 8.x (^8.0) | Raw file system event detection | Official Rust file watching crate; maintained by notify-rs; powers `notify-debouncer-full` |
| `notify-debouncer-full` | 0.7.x (^0.7) | Event consolidation, rename stitching, file ID cache | Sits atop `notify`; eliminates duplicate events, correctly handles rename-as-delete+create, maintains `FileIdMap` for OS-level file identity tracking |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `std::sync::mpsc` | stdlib | Channel from watcher callback to processing loop | Already used in codebase; notify's event handler runs on its internal thread so std channel is correct here |
| `std::collections::HashMap` | stdlib | Per-file burst tracking state | Keyed by PathBuf; tracks event count and last-seen timestamp for adaptive debounce |
| `tokio::task::spawn` (async task) | already in Cargo.toml | Watcher supervision loop | Top-level loop for restart-with-backoff logic; can await a sleep between restart attempts |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `notify-debouncer-full` | `notify-debouncer-mini` | mini is simpler but has no FileIdMap (weaker rename handling) and no path-update for pre-rename events |
| `std::sync::mpsc` channel | `tokio::sync::mpsc` channel | tokio channel requires `blocking_send` from notify's sync callback thread; std mpsc is simpler here |
| `std::sync::mpsc` channel | Direct `Arc<RwLock>` writes in callback | Direct writes from notify callback thread work but require more care; channel keeps callback non-blocking |

**Installation:**
```bash
# Add to Cargo.toml [dependencies]:
notify = "8"
notify-debouncer-full = "0.7"
```

---

## Architecture Patterns

### Recommended Project Structure

```
src/
├── watcher/
│   └── mod.rs          # WatcherHandle, WatcherState, watcher_task fn, burst tracker
├── live_index/
│   ├── mod.rs          # pub use additions: add_file, update_file, remove_file
│   ├── store.rs        # + add_file, update_file, remove_file methods on LiveIndex
│   └── query.rs        # + HealthStats gets watcher_state, events_processed, last_event_at, debounce_window_ms
├── protocol/
│   ├── format.rs       # health_report extended to show watcher fields
│   └── tools.rs        # index_folder calls watcher restart
└── main.rs             # spawn watcher task after initial load
```

### Pattern 1: notify-debouncer-full with std::sync::mpsc

**What:** Create a debouncer that sends events through a std channel. The watcher task receives batches and processes them.

**When to use:** Primary pattern for this phase.

```rust
// Source: notify-debouncer-full 0.7 docs + oneuptime.com/blog/post/2026-01-25-file-watcher-debouncing-rust
use notify_debouncer_full::{new_debouncer, DebouncedEvent, DebounceEventResult};
use std::sync::mpsc;
use std::time::Duration;

let (tx, rx) = mpsc::channel::<DebounceEventResult>();

let mut debouncer = new_debouncer(
    Duration::from_millis(200),  // baseline debounce window
    None,                         // tick_rate: None → auto (1/4 of timeout = 50ms)
    move |result: DebounceEventResult| {
        let _ = tx.send(result);
    },
)?;

debouncer.watcher().watch(repo_root, RecursiveMode::Recursive)?;

// In the processing loop:
while let Ok(result) = rx.recv() {
    match result {
        Ok(events) => process_events(events, &shared_index),
        Err(errors) => handle_watcher_errors(errors),
    }
}
```

### Pattern 2: Adaptive Debounce (Per-File Burst Tracking)

**What:** `notify-debouncer-full` uses a fixed timeout. The adaptive layer (200ms → 500ms on burst) must be implemented in the event-processing loop using per-file state.

**When to use:** Applied after events arrive from the channel, before deciding whether to re-parse.

```rust
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

struct BurstTracker {
    event_count: u32,
    window_start: Instant,
    last_event_at: Instant,
    extended: bool,   // true when burst threshold triggered 500ms mode
}

// Debounce window selection
fn effective_debounce(tracker: &BurstTracker) -> Duration {
    const BASE: Duration = Duration::from_millis(200);
    const BURST: Duration = Duration::from_millis(500);
    const QUIET: Duration = Duration::from_secs(5);

    if tracker.last_event_at.elapsed() > QUIET {
        // Reset — back to base
        return BASE;
    }
    if tracker.extended {
        BURST
    } else {
        BASE
    }
}

// On each incoming event path, update tracker:
fn update_burst(tracker: &mut BurstTracker, now: Instant) {
    const BURST_THRESHOLD: u32 = 3;
    const WINDOW: Duration = Duration::from_millis(200);

    if now.duration_since(tracker.window_start) > WINDOW {
        // New window
        tracker.event_count = 1;
        tracker.window_start = now;
        tracker.extended = false;
    } else {
        tracker.event_count += 1;
        if tracker.event_count > BURST_THRESHOLD {
            tracker.extended = true;
        }
    }
    tracker.last_event_at = now;
}
```

### Pattern 3: Content Hash Skip Before Re-parse

**What:** Read file bytes, compute SHA-256, compare against `IndexedFile.content_hash`. Only call `parsing::process_file` when hash differs.

**When to use:** Every watcher callback before re-parsing. Eliminates false re-indexes from editor metadata-only writes.

```rust
// Source: existing src/hash.rs + src/live_index/store.rs pattern
use crate::hash::digest_hex;
use crate::parsing;
use crate::live_index::store::IndexedFile;

fn maybe_reindex(
    relative_path: &str,
    abs_path: &Path,
    shared: &SharedIndex,
    language: LanguageId,
) {
    let bytes = match std::fs::read(abs_path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // File was deleted between event and now — treat as remove
            let mut idx = shared.write().expect("lock poisoned");
            idx.remove_file(relative_path);
            tracing::warn!("ENOENT during re-index of {relative_path}: treated as delete");
            return;
        }
        Err(e) => {
            tracing::warn!("failed to read {relative_path}: {e}");
            return;
        }
    };

    let new_hash = digest_hex(&bytes);

    {
        let idx = shared.read().expect("lock poisoned");
        if let Some(existing) = idx.get_file(relative_path) {
            if existing.content_hash == new_hash {
                tracing::debug!("hash-skip for {relative_path}: no content change");
                return;  // No parse needed
            }
        }
    }  // Drop read lock

    // Hash changed — reparse
    let result = parsing::process_file(relative_path, &bytes, language);
    let indexed = IndexedFile::from_parse_result(result, bytes);

    let mut idx = shared.write().expect("lock poisoned");
    idx.update_file(relative_path.to_string(), indexed);
    tracing::debug!("re-indexed {relative_path}");
}
```

### Pattern 4: WatcherState and WatcherHandle

**What:** Encapsulate watcher lifecycle with an enum for visibility to health tool.

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WatcherState {
    Active,
    Degraded,  // 3 consecutive restart failures; queries still work
    Off,       // TOKENIZOR_AUTO_INDEX=false
}

pub struct WatcherHandle {
    /// None when state is Off
    _debouncer: Option<Debouncer<RecommendedWatcher, RecommendedCache>>,
    pub state: WatcherState,
    pub events_processed: u64,
    pub last_event_at: Option<SystemTime>,
}
```

### Pattern 5: Watcher Restart with Backoff

**What:** Supervision loop that restarts the watcher after failures. 1s backoff between attempts; degraded mode after 3 failures.

**When to use:** In the watcher task body. Note: `spawn_blocking` is NOT needed — notify runs its own threads. Use `tokio::spawn` with `tokio::time::sleep` for async backoff.

```rust
// Conceptual structure for main.rs watcher spawning:
tokio::spawn(async move {
    let mut consecutive_failures = 0u32;

    loop {
        match start_watcher(&repo_root, Arc::clone(&shared_index)) {
            Ok(handle) => {
                // Watcher running — block this task on the event channel
                consecutive_failures = 0;
                handle.run_until_error().await;  // returns when watcher fails
            }
            Err(e) => {
                consecutive_failures += 1;
                tracing::warn!("watcher restart failed ({consecutive_failures}/3): {e}");
                if consecutive_failures >= 3 {
                    tracing::error!("entering degraded mode — watcher failed 3 times");
                    // Update WatcherState in shared watcher state (Arc<Mutex<WatcherState>>)
                    break;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
});
```

### Pattern 6: New LiveIndex Methods

**What:** Three new mutation methods on `LiveIndex` for incremental single-file updates.

```rust
// Add to src/live_index/store.rs impl LiveIndex:

/// Insert or update a single file in the index.
/// Updates loaded_at_system so what_changed() picks up the change.
pub fn update_file(&mut self, path: String, file: IndexedFile) {
    self.files.insert(path, file);
    self.loaded_at_system = std::time::SystemTime::now();
}

/// Alias for update_file — semantically used for newly created files.
pub fn add_file(&mut self, path: String, file: IndexedFile) {
    self.update_file(path, file);
}

/// Remove a file from the index (on delete or ENOENT).
/// Silently does nothing if the path is not present.
pub fn remove_file(&mut self, path: &str) {
    if self.files.remove(path).is_some() {
        self.loaded_at_system = std::time::SystemTime::now();
    }
}
```

### Pattern 7: HealthStats Extension

**What:** Add watcher fields to `HealthStats` and extend `health_report` format.

```rust
// In src/live_index/query.rs HealthStats:
pub struct HealthStats {
    // ... existing fields ...
    pub watcher_state: WatcherState,       // Active/Degraded/Off
    pub events_processed: u64,
    pub last_event_at: Option<SystemTime>,
    pub debounce_window_ms: u64,           // current effective window (200 or 500)
}
```

**Note:** `WatcherState` is stored in shared state (e.g., `Arc<Mutex<WatcherInfo>>`) separate from `LiveIndex`. The health tool reads both.

### Pattern 8: Windows Path Normalization

**What:** Events from `notify` on Windows carry paths with `\\?\C:\...` prefix (extended-length path format from `ReadDirectoryChangesW`). The LiveIndex keys on forward-slash relative paths. Normalization is required.

**When to use:** At the top of the event handler, before any path lookup.

```rust
// Source: github.com/notify-rs/notify/issues/95 — documented behavior
fn normalize_event_path(abs_path: &Path, repo_root: &Path) -> Option<String> {
    // 1. Convert to string to strip \\?\ prefix (Windows extended-length format)
    let path_str = abs_path.to_string_lossy();
    let stripped = path_str.trim_start_matches(r"\\?\");
    let clean_path = Path::new(stripped);

    // 2. Strip repo root prefix to get relative path
    let relative = clean_path.strip_prefix(repo_root).ok()
        .or_else(|| {
            // Try stripping from the stripped version (handles canonicalization mismatch)
            let root_str = repo_root.to_string_lossy();
            let clean_root_str = root_str.trim_start_matches(r"\\?\");
            let clean_root = Path::new(clean_root_str);
            clean_path.strip_prefix(clean_root).ok()
        })?;

    // 3. Normalize backslashes to forward slashes
    Some(relative.to_string_lossy().replace('\\', "/"))
}
```

### Anti-Patterns to Avoid

- **Holding write lock during tree-sitter parse:** The write lock should only be held during the `files.insert()` call, not during `parsing::process_file()`. Parse first, lock second.
- **Counting watcher events toward circuit breaker:** The circuit breaker (RELY-01) is only for bulk initial load failures. Per-file watcher errors are handled individually with warn-and-continue.
- **Using `spawn_blocking` for notify:** notify creates its own OS thread internally. No `spawn_blocking` needed. Just `tokio::spawn` the supervision loop.
- **Not filtering event kinds:** `notify-debouncer-full` can emit `EventKind::Access` events (read-only). Only process `EventKind::Create`, `EventKind::Modify`, and `EventKind::Remove`.
- **Not filtering by extension:** The watcher watches the full repo root but should only process events for the 6 supported languages. Filter via `LanguageId::from_extension()`.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Raw OS event consolidation | Custom de-duplication loop | `notify-debouncer-full` | Handles rename stitching, duplicate suppression, pre-rename path updates across platforms |
| File ID tracking | Custom inode/GUID cache | `FileIdMap` (built into notify-debouncer-full) | Platform-specific: inode on Linux/macOS, file GUID on Windows |
| Raw FS events | `ReadDirectoryChangesW` / `inotify` directly | `notify` crate `recommended_watcher()` | Abstracts cross-platform differences behind a single API |
| No-op write detection | Timestamp comparison | SHA-256 via existing `src/hash.rs::digest_hex` | Timestamps are unreliable (some editors touch mtime without changing content) |

**Key insight:** The adaptive debounce IS hand-rolled because `notify-debouncer-full` only supports fixed timeouts. The per-file burst tracker is the one custom component justified by locked decisions.

---

## Common Pitfalls

### Pitfall 1: Windows \\?\ Path Prefix
**What goes wrong:** Event paths on Windows carry the extended-length path prefix `\\?\C:\...`. `Path::strip_prefix(repo_root)` fails silently because the repo root was not canonicalized to include this prefix.
**Why it happens:** `notify` internally calls `fs::canonicalize` on watched paths, which on Windows returns UNC-style paths.
**How to avoid:** String-level stripping of `\\?\` before any `strip_prefix` call. Do NOT use `fs::canonicalize` on the repo_root itself (produces the same UNC prefix, but string comparison is fragile). See Pattern 8 above.
**Warning signs:** Watcher active, events received, but `normalize_event_path` always returns `None` → no re-indexing happens.

### Pitfall 2: Write Lock Held During Parse
**What goes wrong:** Holding `SharedIndex` write lock while calling `parsing::process_file` blocks all concurrent read queries for the full parse duration (potentially 10-50ms per file).
**Why it happens:** Simpler to write the callback as "lock → read → parse → insert → unlock".
**How to avoid:** Pattern: read bytes → release any lock → parse → acquire write lock → insert. The write lock should be held for milliseconds, not tens of milliseconds.
**Warning signs:** Query latency spikes after file saves; `health` tool shows long lock wait times.

### Pitfall 3: Watcher Callback Thread + Tokio Runtime
**What goes wrong:** Calling async code or using `tokio::sync::mpsc::Sender::send` (async) inside the notify callback, which runs on a sync OS thread, not inside the tokio runtime.
**Why it happens:** Mixing async and sync contexts.
**How to avoid:** Use `std::sync::mpsc::Sender` inside the notify callback. The processing loop (which calls async-aware code) runs in a `tokio::spawn` task and receives from the `rx` end.
**Warning signs:** Compile error "cannot use `.await` in a closure that is not async" or runtime panic about calling async outside tokio context.

### Pitfall 4: ENOENT Not Guarded
**What goes wrong:** File deleted between the debouncer event and the watcher's `fs::read` call — the read returns `Err(NotFound)` and propagates as an error, potentially panicking or logging noise.
**Why it happens:** Racey by nature — file system events and actual file state can diverge.
**How to avoid:** Match on `e.kind() == std::io::ErrorKind::NotFound` explicitly in the `fs::read` error arm. Call `remove_file` on the index and log `WARN`. Do not propagate. This covers RELY-03.
**Warning signs:** Panic in watcher thread after rapid file deletion; error logs showing ENOENT without graceful handling.

### Pitfall 5: Adaptive Debounce Shared State Contention
**What goes wrong:** The burst tracker `HashMap<PathBuf, BurstTracker>` is owned by the watcher task and accessed per-event. If the channel buffer fills and multiple events arrive at once, the tracker state may be stale.
**Why it happens:** notify-debouncer-full batches events before sending; all events in a batch arrive at the same time.
**How to avoid:** Process events in the batch sequentially, updating the tracker per path. The effective debounce is computed at process time, not at event-receipt time.

### Pitfall 6: Watcher Drop Before Processing Completes
**What goes wrong:** `Debouncer` is dropped when `WatcherHandle` goes out of scope, stopping the watcher. If the handle is owned by the spawned task, it's fine. If it leaks to another scope, the task's channel `rx` will see a `RecvError` immediately.
**Why it happens:** `Debouncer` implements `Drop` with cleanup logic — dropping it terminates the watcher thread and closes the channel sender.
**How to avoid:** Keep the `Debouncer` alive inside the watcher task scope for the lifetime of the watching loop. Never move it out.

---

## Code Examples

### DebouncedEvent Kind Filtering

```rust
// Source: docs.rs/notify/latest/notify — EventKind enum
use notify::EventKind;
use notify_debouncer_full::DebouncedEvent;

fn is_relevant_event(event: &DebouncedEvent) -> bool {
    matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    )
}
```

### LanguageId Extension Filter (Reuse Existing)

```rust
// Reuse existing: src/discovery/mod.rs LanguageId::from_extension
use crate::domain::LanguageId;

fn supported_language(path: &Path) -> Option<LanguageId> {
    let ext = path.extension()?.to_str()?;
    LanguageId::from_extension(ext)
}
```

### Watcher + SharedIndex Integration (main.rs)

```rust
// In main.rs, after LiveIndex::load completes:
if should_auto_index {
    let watcher_index = Arc::clone(&index);
    let watcher_root = root.clone();
    tokio::spawn(async move {
        crate::watcher::run_watcher(watcher_root, watcher_index).await;
    });
}
```

### index_folder Tool Extension (for watcher restart)

```rust
// tools.rs index_folder handler must also trigger watcher restart.
// Options:
// A) Pass Arc<Mutex<WatcherHandle>> into TokenizorServer and call restart inside handler
// B) Send a "restart watcher at new root" message over a separate channel
// Recommendation: option A — simpler, TokenizorServer already holds SharedIndex
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `notify` 4.x DebouncedEvent enum | `notify` 6.x+ hierarchical EventKind (Create/Modify/Remove + subtypes) | ~2022 | Breaking change — 4.x patterns do NOT apply |
| `notify-debouncer-mini` (simpler, less features) | `notify-debouncer-full` (FileIdMap, rename stitching) | ~2022 | debouncer-full preferred for production use with rename support |
| `notify` with `crossbeam-channel` as default | std::sync::mpsc or user-supplied callback | 8.x | Crossbeam is optional feature, not default |

**Deprecated/outdated:**
- `notify::DebouncedEvent` enum (v4 era): Replaced by `notify_debouncer_full::DebouncedEvent` in current ecosystem. Do not confuse with the old `notify::DebouncedEvent` from notify 4.x docs still appearing in search results.
- `RecommendedWatcher::new_immediate`: Not a current API. Use `new_debouncer()` from debouncer crate.

---

## Open Questions

1. **WatcherState in HealthStats: shared ownership**
   - What we know: `HealthStats` is computed from `&LiveIndex` in `query.rs`. `WatcherState` is owned by the watcher task, not `LiveIndex`.
   - What's unclear: Best ownership pattern — `Arc<Mutex<WatcherInfo>>` shared between watcher task and health tool, vs storing watcher state inside `LiveIndex`.
   - Recommendation: Store `Option<Arc<Mutex<WatcherInfo>>>` in `TokenizorServer`. The `health` tool reads it alongside the index. Keeps `LiveIndex` clean of watcher concerns.

2. **index_folder watcher restart: how to pass WatcherHandle to tool handler**
   - What we know: `TokenizorServer` holds `SharedIndex`. `index_folder` calls `reload()`. After reload, watcher root may have changed.
   - What's unclear: How to pass a restart signal from the `index_folder` handler to the watcher task.
   - Recommendation: Store `Arc<Mutex<Option<WatcherHandle>>>` in `TokenizorServer`. `index_folder` takes the write lock, calls restart, updates the handle. Simplest approach.

3. **MSYS path format: absolute path comparison under MSYS bash**
   - What we know: Project runs on MSYS2 (Windows). notify events return `C:\...` paths. LiveIndex keys on `src/lib.rs` format. strip_prefix must handle this.
   - What's unclear: Whether the repo_root in MSYS context is a forward-slash path like `/c/AI_STUFF/...` or a Windows path like `C:\AI_STUFF\...`.
   - Recommendation: Use `discovery::find_git_root()` to get the root (which is a `PathBuf` from `std::env::current_dir()`). On MSYS, `current_dir()` returns the Windows-format path (`C:\...`), so strip_prefix comparisons against notify's `C:\...` paths should work after `\\?\` removal. Add a Windows-specific test to verify.

---

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust built-in + cargo test |
| Config file | none (uses Cargo.toml test integration) |
| Quick run command | `cargo test --lib 2>/dev/null` |
| Full suite command | `cargo test 2>/dev/null` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| FRSH-01 | Watcher detects file change within 200ms | integration | `cargo test test_watcher_detects_change_within_200ms` | Wave 0 |
| FRSH-02 | Single-file reparse completes in <50ms | unit | `cargo test test_single_file_reparse_under_50ms` | Wave 0 |
| FRSH-03 | LiveIndex reflects disk state after edit | integration | `cargo test test_watcher_updates_index_after_edit` | Wave 0 |
| FRSH-04 | File creation auto-indexed | integration | `cargo test test_watcher_indexes_new_file` | Wave 0 |
| FRSH-05 | File deletion auto-removed from index | integration | `cargo test test_watcher_removes_deleted_file` | Wave 0 |
| FRSH-06 | Symbol visible within 300ms after edit | integration | `cargo test test_symbol_freshness_after_rename` | Wave 0 |
| RELY-03 | Delete during re-index: no panic | unit | `cargo test test_enoent_handled_gracefully` | Wave 0 |
| content_hash skip | No reparse on metadata-only write | unit | `cargo test test_hash_skip_prevents_reparse` | Wave 0 |
| WatcherState | health tool shows watcher state | unit | `cargo test test_health_report_shows_watcher_state` | Wave 0 |
| Windows paths | normalize_event_path handles \\?\ | unit | `cargo test test_windows_path_normalization` | Wave 0 |
| Burst detection | Burst threshold extends debounce window | unit | `cargo test test_burst_tracker_extends_window` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test --lib`
- **Per wave merge:** `cargo test`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `tests/watcher_integration.rs` — covers FRSH-01, FRSH-03, FRSH-04, FRSH-05, FRSH-06 (needs real FS + timing)
- [ ] `src/watcher/mod.rs` unit tests — covers RELY-03, content_hash skip, burst tracker, Windows path normalization (pure unit, no timing dependency)
- [ ] Framework install: none needed (cargo test already present)

**Note on timing tests:** FRSH-01 and FRSH-06 require actual file system events with timing assertions. Use `std::thread::sleep(Duration::from_millis(350))` after file write to give watcher time to fire. Tests should be marked `#[ignore]` if flaky in CI (same pattern as existing `test_load_perf_1000_files`).

---

## Sources

### Primary (HIGH confidence)
- `docs.rs/notify/latest/notify/` — notify 8.2.0 API: `recommended_watcher`, `RecursiveMode`, `EventKind`
- `docs.rs/notify-debouncer-full/latest/notify_debouncer_full/` — new_debouncer function signature, DebounceEventResult, DebouncedEvent, feature flags, FileIdMap
- `docs.rs/notify-debouncer-full/0.7.0/notify_debouncer_full/fn.new_debouncer.html` — exact function signature: `new_debouncer(timeout, tick_rate, event_handler)`
- Existing codebase: `src/hash.rs` (digest_hex), `src/live_index/store.rs` (IndexedFile, SharedIndex, RwLock pattern), `src/parsing/` (process_file)

### Secondary (MEDIUM confidence)
- `github.com/notify-rs/notify/issues/95` — Windows `\\?\` path prefix documented issue; workaround: string-level `trim_start_matches("\\\\?\\")` before strip_prefix
- `oneuptime.com/blog/post/2026-01-25-file-watcher-debouncing-rust/view` — Complete code examples for notify 8.x watcher patterns including std::sync::mpsc channel integration
- `tokio.rs/tokio/topics/shutdown` — CancellationToken for graceful shutdown; `tokio::time::sleep` for backoff loops

### Tertiary (LOW confidence)
- `github.com/rust-lang/rust-analyzer/pull/17227` — "Hash file contents to verify whether file actually changed" pattern; confirms hash-gating approach is production practice

---

## Metadata

**Confidence breakdown:**
- Standard stack (notify 8.x + notify-debouncer-full 0.7): HIGH — confirmed via docs.rs
- Architecture patterns (watcher task, hash skip, LiveIndex mutations): HIGH — directly derived from existing codebase patterns + verified API
- Windows path normalization: MEDIUM — github issue confirms the problem and workaround; exact MSYS behavior needs a Windows test to verify
- Adaptive burst tracker: MEDIUM — design is straightforward but no prior art in this codebase; behavior under load is speculative

**Research date:** 2026-03-10
**Valid until:** 2026-06-10 (stable crates; notify 8.x and debouncer-full 0.7 are mature)
