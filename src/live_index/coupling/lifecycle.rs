//! Phase 3.1 — coupling store lifecycle.
//!
//! Opens the per-workspace coupling store at session start, cold-builds on
//! first session, applies HEAD-delta on subsequent sessions, and refreshes
//! on the watcher's reconcile tick so mid-session HEAD moves stay reflected.
//!
//! Env-gated on `SYMFORGE_COUPLING=1`. No user-visible behaviour — the store
//! exists only so Phase 3.3 can consume it. Contract in
//! `docs/plans/cochange-coupling-3.1-design.md`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tracing::{debug, trace};

use crate::live_index::store::SharedIndex;
use super::{CouplingStore, WalkerConfig, apply_head_delta, cold_build};

pub const COUPLING_FLAG_ENV: &str = "SYMFORGE_COUPLING";

fn flag_on() -> bool {
    std::env::var(COUPLING_FLAG_ENV).as_deref() == Ok("1")
}

fn is_git_repo(root: &Path) -> bool {
    git2::Repository::discover(root).is_ok()
}

/// Per-workspace in-flight guard. One `AtomicBool` per project root.
/// Lazily populated — a workspace that never gets coupling-initialised
/// never allocates a guard.
fn guard_for(project_root: &Path) -> Arc<AtomicBool> {
    static GUARDS: OnceLock<Mutex<HashMap<PathBuf, Arc<AtomicBool>>>> = OnceLock::new();
    let map = GUARDS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut g = map.lock().expect("coupling guard map poisoned");
    Arc::clone(
        g.entry(project_root.to_path_buf())
            .or_insert_with(|| Arc::new(AtomicBool::new(false))),
    )
}

/// RAII release for a per-workspace guard.
struct GuardRelease(Arc<AtomicBool>);
impl Drop for GuardRelease {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn try_acquire(guard: Arc<AtomicBool>) -> Option<GuardRelease> {
    match guard.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst) {
        Ok(_) => Some(GuardRelease(guard)),
        Err(_) => None,
    }
}

/// Boot-path entry point. Called once at `LiveIndex::load` per workspace.
///
/// With flag unset or no git repo at `project_root`: full no-op — no thread,
/// no DB file, no guard acquisition.
///
/// Otherwise spawns a named background thread that runs `run_init`. The
/// per-workspace guard is acquired before spawning and released by RAII in
/// the spawned thread. On `std::thread::Builder::spawn` failure the guard
/// is released synchronously so the workspace does not wedge.
pub fn init_coupling_store(project_root: &Path) {
    if !flag_on() {
        return;
    }
    if !is_git_repo(project_root) {
        return;
    }

    let guard = guard_for(project_root);
    if guard
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    let guard_for_thread = Arc::clone(&guard);

    let db_path = project_root.join(crate::paths::SYMFORGE_COUPLING_DB_PATH);
    let repo_root = project_root.to_path_buf();

    let spawn_result = std::thread::Builder::new()
        .name("coupling-init".into())
        .spawn(move || {
            let _release = GuardRelease(guard_for_thread);
            debug!("coupling init: starting");
            match run_init(&db_path, &repo_root) {
                Ok(()) => debug!("coupling init: ok"),
                Err(e) => debug!("coupling init: failed: {e}"),
            }
        });

    if let Err(e) = spawn_result {
        guard.store(false, Ordering::SeqCst);
        debug!("coupling init: spawn failed: {e}");
    }
}

/// Reconcile-tick entry point. Called from the watcher's 30 s reconcile
/// branch via `tokio::task::spawn_blocking`. Runs entirely on the calling
/// thread — no further spawn. Silently no-ops on flag-off, non-git project,
/// or contested guard.
pub fn refresh_on_reconcile_tick(project_root: &Path, expected_gen: u64, shared: &SharedIndex) {
    let current_gen = shared.current_project_generation();
    if current_gen != expected_gen {
        shared.note_rejected_stale_mutation();
        trace!(
            "coupling: pre-flight gen-check rejected; expected={expected_gen} current={current_gen}; not refreshing"
        );
        return;
    }

    if !flag_on() {
        return;
    }
    if !is_git_repo(project_root) {
        return;
    }

    let guard = guard_for(project_root);
    let Some(_release) = try_acquire(guard) else {
        return;
    };

    let db_path = project_root.join(crate::paths::SYMFORGE_COUPLING_DB_PATH);
    debug!("coupling tick: starting");
    match run_init(&db_path, project_root) {
        Ok(()) => debug!("coupling tick: ok"),
        Err(e) => debug!("coupling tick: failed: {e}"),
    }
}

/// Synchronous unit of work shared by both entry points.
///
/// Branches on `store.cold_built_at()`:
///   - `None` → run `cold_build` (first session, or after a manual wipe).
///   - `Some(_)` → cheap HEAD-check; run `apply_head_delta` when the current
///     HEAD differs from `store.last_head()`. Missing current HEAD is
///     treated as `None` on both sides so the pre-check is symmetric.
///
/// Errors are returned as `String` for the caller to log-and-drop.
pub(crate) fn run_init(db_path: &Path, repo_root: &Path) -> Result<(), String> {
    let store = CouplingStore::open(db_path).map_err(|e| e.to_string())?;
    let cfg = WalkerConfig::system_now();

    let cold_built_at = store.cold_built_at().map_err(|e| e.to_string())?;
    if cold_built_at.is_none() {
        cold_build(&store, repo_root, &cfg).map_err(|e| e.to_string())?;
        return Ok(());
    }

    let current_head = crate::git::head_sha(repo_root).ok();
    let stored_head = store.last_head().map_err(|e| e.to_string())?;
    if current_head == stored_head {
        return Ok(());
    }

    apply_head_delta(&store, repo_root, &cfg).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;
    use tempfile::TempDir;

    // Serialises COUPLING_FLAG_ENV mutation across tests. Mirrors frecency's
    // FRECENCY_ENV_LOCK. Project test policy already enforces --test-threads=1.
    static COUPLING_ENV_LOCK: StdMutex<()> = StdMutex::new(());

    fn set_flag_on() {
        // SAFETY: callers hold COUPLING_ENV_LOCK; tests run single-threaded.
        unsafe { std::env::set_var(COUPLING_FLAG_ENV, "1") };
    }

    fn clear_flag() {
        // SAFETY: callers hold COUPLING_ENV_LOCK; tests run single-threaded.
        unsafe { std::env::remove_var(COUPLING_FLAG_ENV) };
    }

    fn init_repo_with_root_commit(root: &Path) -> String {
        let repo = git2::Repository::init(root).expect("init repo");
        let sig = git2::Signature::now("t", "t@x").expect("sig");
        let tree_id = {
            let mut idx = repo.index().expect("index");
            idx.write_tree().expect("write tree")
        };
        let tree = repo.find_tree(tree_id).expect("find tree");
        let oid = repo
            .commit(Some("HEAD"), &sig, &sig, "root", &tree, &[])
            .expect("root commit");
        oid.to_string()
    }

    fn add_commit(root: &Path, msg: &str, files: &[(&str, &str)]) -> String {
        for (rel, content) in files {
            std::fs::write(root.join(rel), content).expect("write file");
        }
        let repo = git2::Repository::open(root).expect("open repo");
        let mut idx = repo.index().expect("index");
        for (rel, _) in files {
            idx.add_path(Path::new(rel)).expect("add path");
        }
        idx.write().expect("write index");
        let tree_id = idx.write_tree().expect("write tree");
        let tree = repo.find_tree(tree_id).expect("find tree");
        let parent_commit = repo
            .head()
            .ok()
            .and_then(|h| h.peel_to_commit().ok());
        let parents_vec: Vec<&git2::Commit> = parent_commit.iter().collect();
        let sig = git2::Signature::now("t", "t@x").expect("sig");
        let oid = repo
            .commit(Some("HEAD"), &sig, &sig, msg, &tree, &parents_vec)
            .expect("commit");
        oid.to_string()
    }

    // ─── Wrapper-only tests ─────────────────────────────────────────────

    #[test]
    fn public_init_is_noop_when_flag_unset() {
        let _lock = COUPLING_ENV_LOCK.lock().unwrap();
        clear_flag();
        let tmp = TempDir::new().unwrap();
        init_repo_with_root_commit(tmp.path());

        init_coupling_store(tmp.path());

        let db_path = tmp.path().join(crate::paths::SYMFORGE_COUPLING_DB_PATH);
        assert!(
            !db_path.exists(),
            "no db should be created with SYMFORGE_COUPLING unset"
        );
    }

    #[test]
    fn public_init_is_noop_on_non_git_project() {
        let _lock = COUPLING_ENV_LOCK.lock().unwrap();
        set_flag_on();
        let tmp = TempDir::new().unwrap();
        // No git init — just a bare directory.

        init_coupling_store(tmp.path());

        let db_path = tmp.path().join(crate::paths::SYMFORGE_COUPLING_DB_PATH);
        assert!(
            !db_path.exists(),
            "no db should be created on non-git project"
        );
        clear_flag();
    }

    #[test]
    fn refresh_on_tick_is_noop_when_flag_unset() {
        let _lock = COUPLING_ENV_LOCK.lock().unwrap();
        clear_flag();
        let tmp = TempDir::new().unwrap();
        init_repo_with_root_commit(tmp.path());
        let shared = crate::live_index::LiveIndex::empty();
        let expected_gen = shared.current_project_generation();

        refresh_on_reconcile_tick(tmp.path(), expected_gen, &shared);

        let db_path = tmp.path().join(crate::paths::SYMFORGE_COUPLING_DB_PATH);
        assert!(
            !db_path.exists(),
            "no db touch on tick with flag unset"
        );
    }

    // ─── run_init tests ─────────────────────────────────────────────────

    #[test]
    fn run_init_cold_builds_on_first_session() {
        let tmp = TempDir::new().unwrap();
        init_repo_with_root_commit(tmp.path());
        let head = add_commit(tmp.path(), "pair", &[("a.txt", "a"), ("b.txt", "b")]);

        let db_path = tmp.path().join(crate::paths::SYMFORGE_COUPLING_DB_PATH);
        run_init(&db_path, tmp.path()).expect("run_init ok");

        let store = CouplingStore::open(&db_path).expect("open");
        assert!(
            store.cold_built_at().unwrap().is_some(),
            "cold_built_at must be set after cold-build"
        );
        assert_eq!(
            store.last_head().unwrap().as_deref(),
            Some(head.as_str()),
            "last_head must match HEAD"
        );
    }

    #[test]
    fn run_init_is_noop_on_repeated_head() {
        let tmp = TempDir::new().unwrap();
        init_repo_with_root_commit(tmp.path());
        add_commit(tmp.path(), "pair", &[("a.txt", "a"), ("b.txt", "b")]);
        let db_path = tmp.path().join(crate::paths::SYMFORGE_COUPLING_DB_PATH);

        run_init(&db_path, tmp.path()).expect("first");
        let (cbat_before, lrt_before) = {
            let s = CouplingStore::open(&db_path).unwrap();
            (s.cold_built_at().unwrap(), s.last_reference_ts().unwrap())
        };

        run_init(&db_path, tmp.path()).expect("second");
        let s = CouplingStore::open(&db_path).unwrap();
        assert_eq!(
            s.cold_built_at().unwrap(),
            cbat_before,
            "cold-build must not re-run on repeated HEAD"
        );
        assert_eq!(
            s.last_reference_ts().unwrap(),
            lrt_before,
            "delta must not run on repeated HEAD"
        );
    }

    #[test]
    fn run_init_applies_delta_on_head_move() {
        let tmp = TempDir::new().unwrap();
        init_repo_with_root_commit(tmp.path());
        let head_a = add_commit(tmp.path(), "a", &[("x.txt", "x"), ("y.txt", "y")]);
        let db_path = tmp.path().join(crate::paths::SYMFORGE_COUPLING_DB_PATH);

        run_init(&db_path, tmp.path()).expect("cold");
        let cbat_before = CouplingStore::open(&db_path)
            .unwrap()
            .cold_built_at()
            .unwrap();

        let head_b = add_commit(tmp.path(), "b", &[("z.txt", "z"), ("w.txt", "w")]);
        assert_ne!(head_a, head_b);
        run_init(&db_path, tmp.path()).expect("delta");

        let s = CouplingStore::open(&db_path).unwrap();
        assert_eq!(
            s.last_head().unwrap().as_deref(),
            Some(head_b.as_str()),
            "last_head must advance after delta"
        );
        assert_eq!(
            s.cold_built_at().unwrap(),
            cbat_before,
            "cold_built_at must not change on delta path"
        );
    }

    #[test]
    fn run_init_handles_empty_repo_no_head() {
        let tmp = TempDir::new().unwrap();
        git2::Repository::init(tmp.path()).expect("init bare repo");
        let db_path = tmp.path().join(crate::paths::SYMFORGE_COUPLING_DB_PATH);

        run_init(&db_path, tmp.path()).expect("run_init ok on no-HEAD repo");

        let s = CouplingStore::open(&db_path).unwrap();
        assert!(
            s.cold_built_at().unwrap().is_some(),
            "cold_built_at set even when no commits exist"
        );
        assert!(
            s.last_head().unwrap().is_none(),
            "no HEAD stored when repo has no commits"
        );
    }

    #[test]
    fn run_init_reports_err_on_unwriteable_dir() {
        let tmp = TempDir::new().unwrap();
        init_repo_with_root_commit(tmp.path());

        // Create `.symforge` as a FILE (not a directory) so
        // `CouplingStore::open` fails in its `create_dir_all(parent)` step.
        let symforge_path = tmp.path().join(crate::paths::SYMFORGE_DIR_NAME);
        std::fs::write(&symforge_path, b"blocker").expect("write blocker file");

        let db_path = tmp.path().join(crate::paths::SYMFORGE_COUPLING_DB_PATH);
        let result = run_init(&db_path, tmp.path());
        assert!(
            result.is_err(),
            "run_init must surface store-open failure as Err"
        );
    }

    // ─── Reconcile-tick tests ───────────────────────────────────────────

    #[test]
    fn refresh_on_tick_shortcircuits_on_unchanged_head() {
        let _lock = COUPLING_ENV_LOCK.lock().unwrap();
        set_flag_on();
        let tmp = TempDir::new().unwrap();
        init_repo_with_root_commit(tmp.path());
        add_commit(tmp.path(), "seed", &[("a.txt", "a"), ("b.txt", "b")]);
        let db_path = tmp.path().join(crate::paths::SYMFORGE_COUPLING_DB_PATH);

        run_init(&db_path, tmp.path()).expect("seed store");
        let lrt_before = CouplingStore::open(&db_path)
            .unwrap()
            .last_reference_ts()
            .unwrap();
        let shared = crate::live_index::LiveIndex::empty();
        let expected_gen = shared.current_project_generation();

        refresh_on_reconcile_tick(tmp.path(), expected_gen, &shared);

        let lrt_after = CouplingStore::open(&db_path)
            .unwrap()
            .last_reference_ts()
            .unwrap();
        assert_eq!(
            lrt_before, lrt_after,
            "tick with unchanged HEAD must skip delta"
        );
        clear_flag();
    }

    #[test]
    fn refresh_on_tick_applies_delta_when_head_moved() {
        let _lock = COUPLING_ENV_LOCK.lock().unwrap();
        set_flag_on();
        let tmp = TempDir::new().unwrap();
        init_repo_with_root_commit(tmp.path());
        let head_a = add_commit(tmp.path(), "a", &[("x.txt", "x"), ("y.txt", "y")]);
        let db_path = tmp.path().join(crate::paths::SYMFORGE_COUPLING_DB_PATH);

        run_init(&db_path, tmp.path()).expect("seed");
        let head_b = add_commit(tmp.path(), "b", &[("z.txt", "z"), ("w.txt", "w")]);
        assert_ne!(head_a, head_b);
        let shared = crate::live_index::LiveIndex::empty();
        let expected_gen = shared.current_project_generation();

        refresh_on_reconcile_tick(tmp.path(), expected_gen, &shared);

        let s = CouplingStore::open(&db_path).unwrap();
        assert_eq!(
            s.last_head().unwrap().as_deref(),
            Some(head_b.as_str()),
            "tick must advance last_head when HEAD moved"
        );
        clear_flag();
    }

    // ─── Guard tests ────────────────────────────────────────────────────

    #[test]
    fn guard_skips_tick_when_held() {
        let _lock = COUPLING_ENV_LOCK.lock().unwrap();
        set_flag_on();
        let tmp = TempDir::new().unwrap();
        init_repo_with_root_commit(tmp.path());
        let head_a = add_commit(tmp.path(), "a", &[("x.txt", "x"), ("y.txt", "y")]);
        let db_path = tmp.path().join(crate::paths::SYMFORGE_COUPLING_DB_PATH);

        // Seed at A.
        run_init(&db_path, tmp.path()).expect("seed");

        // Advance to B so an unblocked tick would update last_head.
        let head_b = add_commit(tmp.path(), "b", &[("z.txt", "z"), ("w.txt", "w")]);
        assert_ne!(head_a, head_b);

        // Take the workspace guard.
        let guard = guard_for(tmp.path());
        let _hold = try_acquire(guard).expect("test must win the guard");
        let shared = crate::live_index::LiveIndex::empty();
        let expected_gen = shared.current_project_generation();

        refresh_on_reconcile_tick(tmp.path(), expected_gen, &shared);

        let s = CouplingStore::open(&db_path).unwrap();
        assert_eq!(
            s.last_head().unwrap().as_deref(),
            Some(head_a.as_str()),
            "guard must prevent tick from running delta"
        );
        drop(_hold);

        // Sanity: once released, the tick advances.
        refresh_on_reconcile_tick(tmp.path(), expected_gen, &shared);
        let s = CouplingStore::open(&db_path).unwrap();
        assert_eq!(
            s.last_head().unwrap().as_deref(),
            Some(head_b.as_str()),
            "tick must advance last_head after guard release"
        );
        clear_flag();
    }

    #[test]
    fn guard_is_per_workspace() {
        let tmp_a = TempDir::new().unwrap();
        let tmp_b = TempDir::new().unwrap();

        let guard_a = guard_for(tmp_a.path());
        let guard_b = guard_for(tmp_b.path());

        let hold_a = try_acquire(Arc::clone(&guard_a));
        let hold_b = try_acquire(Arc::clone(&guard_b));
        assert!(hold_a.is_some(), "workspace A must acquire");
        assert!(
            hold_b.is_some(),
            "workspace B must acquire independently of A"
        );

        // Second acquisition of A must fail while the first is held.
        let hold_a2 = try_acquire(Arc::clone(&guard_a));
        assert!(hold_a2.is_none(), "second acquisition on A must fail");
    }
}
