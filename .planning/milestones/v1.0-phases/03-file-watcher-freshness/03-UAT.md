---
status: complete
phase: 03-file-watcher-freshness
source: 03-01-SUMMARY.md, 03-02-SUMMARY.md, 03-03-SUMMARY.md
started: 2026-03-10T18:10:00Z
updated: 2026-03-10T18:15:00Z
---

## Current Test
<!-- OVERWRITE each test - shows where we are -->

[testing complete]

## Tests

### 1. Cold Start Smoke Test
expected: Kill any running tokenizor MCP server. Start fresh with `cargo run`. Server boots without errors, no panics or warnings. A health tool call returns a valid response showing watcher state as "Off" (no folder indexed yet).
result: pass

### 2. Health Tool Shows Watcher Active After Indexing
expected: After calling `index_folder` on a code directory, the `health` tool reports watcher state as "Active" with events_processed count, last_event timestamp, and debounce_window_ms displayed.
result: pass

### 3. Auto-detect File Modification
expected: Modify a source file in the watched/indexed folder (e.g. add a comment or change a function). Without calling index_folder again, the watcher automatically re-indexes the file. Querying for symbols in that file reflects the updated content.
result: pass

### 4. Auto-detect New File Creation
expected: Create a new source file (e.g. a .rs or .py file) in the watched folder. The watcher detects and indexes it automatically. The new file's symbols appear in search results without manual re-indexing.
result: pass

### 5. Auto-detect File Deletion
expected: Delete a source file from the watched folder. The watcher removes it from the index automatically. Searching for symbols that were only in that file returns no results.
result: pass

### 6. Hash-skip on Unchanged Content
expected: Save/touch a file without changing its content. The watcher detects the filesystem event but skips re-indexing because the content hash is unchanged. Health tool shows events_processed incremented but the file's index data is untouched.
result: pass

### 7. index_folder Restarts Watcher at New Root
expected: Call `index_folder` on a different directory than the one currently being watched. The watcher restarts watching the new root. Health tool shows the watcher is Active. Changes to files in the OLD directory should no longer trigger re-indexing.
result: pass

## Summary

total: 7
passed: 7
issues: 0
pending: 0
skipped: 0

## Gaps

[none yet]
