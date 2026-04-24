//! Phase 4 investigation — publish/lookup atomicity at the `LiveIndex` layer.
//!
//! Original 2026-04-24 ultrareview symptom:
//!   `index_folder(...)` reports "Indexed N files", then an immediate
//!   `search_files("main.rs")` returns "No indexed source files matching 'main'"
//!   and `get_file_content("src/main.rs")` returns "File not found". A second
//!   `index_folder` restores consistency.
//!
//! Hypothesis: publish-atomicity bug — the LiveIndex's symbol table (used by
//! `get_repo_map`) becomes queryable before the path lookup maps
//! (`files_by_basename`, `files_by_dir_component`) and file map are visible to
//! readers.
//!
//! These tests drive the real `LiveIndex::load` and `SharedIndexHandle::reload`
//! code paths, then IMMEDIATELY exercise the three reader surfaces the bug
//! touches, in the same thread, and assert they agree.

use std::fs;
use std::path::Path;
use symforge::live_index::LiveIndex;
use tempfile::tempdir;

fn write_file(dir: &Path, name: &str, content: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

/// Build a small fixture with at least one file at a non-trivial nested path
/// so both `files_by_basename` and `files_by_dir_component` are populated.
fn write_fixture(dir: &Path) {
    write_file(dir, "src/main.rs", "fn main() {}\n");
    write_file(dir, "src/lib.rs", "pub fn lib() {}\n");
    write_file(dir, "tests/it.rs", "fn it() {}\n");
    write_file(dir, "alpha.rs", "fn alpha() {}\n");
}

/// Assert that the snapshot exposed by `SharedIndexHandle::read()` is
/// internally consistent across the three lookup surfaces:
///   1. the primary file map (`all_files` / `file_count` / `get_file`) —
///      used by `get_file_content`
///   2. the basename secondary index (`find_files_by_basename`) — used by
///      `search_files` (via `capture_search_files_view`)
///   3. the repo outline view (`capture_repo_outline_view`) — used by
///      `get_repo_map` in "full"/"tree" detail.
fn assert_snapshot_consistent(guard: &LiveIndex, context: &str) {
    let file_count = guard.file_count();
    let paths: Vec<String> = guard.all_files().map(|(p, _)| p.clone()).collect();
    assert_eq!(
        paths.len(),
        file_count,
        "[{context}] file_count disagrees with all_files len"
    );
    assert!(
        file_count > 0,
        "[{context}] expected non-empty index after publish"
    );

    // Every path must be reachable via basename lookup (the map that
    // `search_files` walks). If the map trails the file set, this fails.
    for path in &paths {
        let basename = Path::new(path.as_str())
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        let hits = guard.find_files_by_basename(&basename);
        assert!(
            hits.contains(&path.as_str()),
            "[{context}] path {path:?} missing from files_by_basename['{basename}']"
        );
    }

    // Every path must resolve via get_file (the map that `get_file_content`
    // walks).
    for path in &paths {
        assert!(
            guard.get_file(path.as_str()).is_some(),
            "[{context}] path {path:?} missing from get_file"
        );
    }

    // Repo-outline view (drives `get_repo_map`) must agree with file_count.
    let view = guard.capture_repo_outline_view();
    assert_eq!(
        view.total_files, file_count,
        "[{context}] repo_outline.total_files disagrees with file_count"
    );
    assert_eq!(
        view.files.len(),
        file_count,
        "[{context}] repo_outline.files.len disagrees with file_count"
    );
}

/// Drive `LiveIndex::load` and, without yielding the thread, run the three
/// reader surfaces back-to-back against the freshly-published snapshot. This
/// is the exact post-`index_folder` pattern that the reviewer reported
/// failing.
#[test]
fn publish_then_immediate_lookup_is_consistent_load() {
    let dir = tempdir().unwrap();
    write_fixture(dir.path());

    let shared = LiveIndex::load(dir.path()).unwrap();

    // IMMEDIATE three-surface read.
    let guard = shared.read();
    assert_snapshot_consistent(&guard, "fresh-load");

    // Targeted reproduction of the exact reviewer queries.
    assert!(
        guard.get_file("src/main.rs").is_some(),
        "get_file(src/main.rs) must resolve immediately after load"
    );
    let by_basename = guard.find_files_by_basename("main.rs");
    assert!(
        by_basename.contains(&"src/main.rs"),
        "search_files equivalent (basename=main.rs) must return src/main.rs \
         immediately after load; got {:?}",
        by_basename
    );
}

/// The same check against `SharedIndexHandle::reload` — the path `index_folder`
/// actually takes on subsequent calls. Different code path (`build_reload_data`
/// → `apply_reload_data` → `swap_and_publish`), same invariants.
#[test]
fn publish_then_immediate_lookup_is_consistent_reload() {
    let dir_a = tempdir().unwrap();
    write_file(dir_a.path(), "bootstrap.rs", "fn bootstrap() {}\n");

    let shared = LiveIndex::load(dir_a.path()).unwrap();

    let dir_b = tempdir().unwrap();
    write_fixture(dir_b.path());

    // Reload into a second directory — this is the path-lookup rebuild path.
    shared.reload(dir_b.path()).expect("reload must succeed");

    // IMMEDIATE three-surface read from the same thread.
    let guard = shared.read();
    assert_snapshot_consistent(&guard, "reload");

    // Bootstrap file must be gone; main.rs from dir_b must be reachable.
    assert!(
        guard.get_file("bootstrap.rs").is_none(),
        "dir_a's bootstrap.rs must not survive reload into dir_b"
    );
    assert!(
        guard.get_file("src/main.rs").is_some(),
        "get_file(src/main.rs) must resolve immediately after reload"
    );
    let by_basename = guard.find_files_by_basename("main.rs");
    assert!(
        by_basename.contains(&"src/main.rs"),
        "basename lookup must return src/main.rs immediately after reload; got {:?}",
        by_basename
    );
}

/// Sharpened root-switch regression guard. The original 2026-04-24 repro
/// went A → B across different roots with different file sets, then
/// observed `get_repo_map` seeing B's tree while `search_files` /
/// `get_file_content` returned "not found" for B's files. That shape
/// requires `files` and `files_by_basename` to disagree on the B generation.
/// This test asserts they don't:
///   - after reload(B), basenames unique to A must be purged from
///     `files_by_basename`
///   - after reload(B), basenames unique to B must resolve
///   - no A path survives the switch
#[test]
fn reload_across_different_roots_purges_prior_path_indices() {
    let dir_a = tempdir().unwrap();
    // A-only basenames: 'aardvark.rs', 'antelope.rs'
    write_file(dir_a.path(), "src/aardvark.rs", "fn aardvark() {}\n");
    write_file(dir_a.path(), "lib/antelope.rs", "fn antelope() {}\n");
    write_file(dir_a.path(), "shared.rs", "fn from_a() {}\n");

    let dir_b = tempdir().unwrap();
    // B-only basenames: 'baboon.rs', 'buffalo.rs'; also 'main.rs' at src/
    write_file(dir_b.path(), "src/main.rs", "fn main() {}\n");
    write_file(dir_b.path(), "src/baboon.rs", "fn baboon() {}\n");
    write_file(dir_b.path(), "lib/buffalo.rs", "fn buffalo() {}\n");
    write_file(dir_b.path(), "shared.rs", "fn from_b() {}\n");

    let shared = LiveIndex::load(dir_a.path()).unwrap();
    {
        let guard = shared.read();
        assert_snapshot_consistent(&guard, "root-switch A");
        assert!(
            !guard.find_files_by_basename("aardvark.rs").is_empty(),
            "A basename aardvark.rs should resolve before switch"
        );
        assert!(
            guard.find_files_by_basename("main.rs").is_empty(),
            "B-only basename main.rs should NOT resolve while on root A"
        );
    }

    // Root switch.
    shared.reload(dir_b.path()).unwrap();

    let guard = shared.read();
    assert_snapshot_consistent(&guard, "root-switch B");

    // A's paths are gone from the primary map.
    assert!(
        guard.get_file("src/aardvark.rs").is_none(),
        "A path src/aardvark.rs must not survive root switch"
    );
    assert!(
        guard.get_file("lib/antelope.rs").is_none(),
        "A path lib/antelope.rs must not survive root switch"
    );

    // A's basenames are gone from the basename index (the exact shape of
    // the 2026-04-24 repro — primary map fresh, secondary index stale).
    assert!(
        guard.find_files_by_basename("aardvark.rs").is_empty(),
        "files_by_basename must purge A's aardvark.rs on root switch; got {:?}",
        guard.find_files_by_basename("aardvark.rs")
    );
    assert!(
        guard.find_files_by_basename("antelope.rs").is_empty(),
        "files_by_basename must purge A's antelope.rs on root switch; got {:?}",
        guard.find_files_by_basename("antelope.rs")
    );

    // B's paths resolve through BOTH surfaces.
    assert!(
        guard.get_file("src/main.rs").is_some(),
        "B path src/main.rs must resolve via primary map after switch"
    );
    assert!(
        guard.find_files_by_basename("main.rs").contains(&"src/main.rs"),
        "B basename main.rs must resolve via files_by_basename after switch; got {:?}",
        guard.find_files_by_basename("main.rs")
    );
    assert!(
        guard.find_files_by_basename("baboon.rs").contains(&"src/baboon.rs"),
        "B basename baboon.rs must resolve after switch; got {:?}",
        guard.find_files_by_basename("baboon.rs")
    );
    assert!(
        guard.find_files_by_basename("buffalo.rs").contains(&"lib/buffalo.rs"),
        "B basename buffalo.rs must resolve after switch; got {:?}",
        guard.find_files_by_basename("buffalo.rs")
    );

    // 'shared.rs' exists in both — after switch it must resolve to B's copy.
    // find_files_by_basename should contain exactly the single 'shared.rs'
    // path and no phantom A entries.
    let shared_hits = guard.find_files_by_basename("shared.rs");
    assert_eq!(
        shared_hits, vec!["shared.rs"],
        "shared.rs basename must resolve to exactly one entry after switch"
    );
}

/// Run the load + reload cycle across 50 iterations with **rotating**
/// per-iteration filenames. If a reload leaks basenames from the prior
/// iteration's file set into `files_by_basename`, this test catches it:
/// each iteration asserts that the previous iteration's unique basename
/// is absent from the lookup map and the current iteration's unique
/// basename is present.
///
/// Same-fixture rotation (what the earlier version did) could have
/// passed even with stale lookup maps, because the leaked basename
/// would still have pointed at a path that happened to exist in the new
/// root. Rotation makes leaks observable.
#[test]
fn publish_atomicity_stress_50_iterations() {
    // First iteration: fresh load. Keep the handle and reload into fresh
    // roots for the remaining iterations — this is the exact shape of the
    // 2026-04-24 repro (root switches on a persistent `SharedIndexHandle`).
    let dir0 = tempdir().unwrap();
    write_fixture(dir0.path());
    let unique0 = "iter_000_marker.rs".to_string();
    write_file(dir0.path(), &unique0, "fn marker() {}\n");
    let handle = LiveIndex::load(dir0.path()).unwrap();
    {
        let guard = handle.read();
        assert_snapshot_consistent(&guard, "stress iter=0 (fresh)");
        assert!(
            guard.find_files_by_basename(&unique0).contains(&unique0.as_str()),
            "[iter=0] fresh load: {unique0} should resolve"
        );
    }
    let mut prior_unique = unique0;

    for iteration in 1..50 {
        let unique = format!("iter_{iteration:03}_marker.rs");

        let dir = tempdir().unwrap();
        write_fixture(dir.path());
        write_file(dir.path(), &unique, "fn marker() {}\n");

        handle.reload(dir.path()).unwrap();
        let guard = handle.read();
        assert_snapshot_consistent(&guard, &format!("stress iter={iteration} (reload)"));

        // Current iteration's marker resolves.
        let hits = guard.find_files_by_basename(&unique);
        assert!(
            hits.contains(&unique.as_str()),
            "[iter={iteration}] reload: {unique} should resolve; got {hits:?}"
        );

        // Prior iteration's marker basename must NOT leak into the fresh
        // generation's basename index.
        let leaked = guard.find_files_by_basename(&prior_unique);
        assert!(
            leaked.is_empty(),
            "[iter={iteration}] stale basename '{prior_unique}' leaked after reload; got {leaked:?}"
        );

        drop(guard);
        prior_unique = unique;
    }
}

