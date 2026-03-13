//! Shared local daemon for project-aware and session-aware backend state.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::SystemTime;

use anyhow::Context;
use axum::extract::{Path as AxumPath, Query, State};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use rmcp::handler::server::wrapper::Parameters;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;

use crate::live_index::{self, SharedIndex};
use crate::protocol::TokenizorServer;
use crate::protocol::edit::{
    BatchEditInput, BatchInsertInput, BatchRenameInput, DeleteSymbolInput, EditWithinSymbolInput,
    InsertSymbolInput, ReplaceSymbolBodyInput,
};
use crate::protocol::tools::{
    AnalyzeFileImpactInput, DiffSymbolsInput, ExploreInput, FindDependentsInput,
    FindImplementationsInput, FindReferencesInput, GetCoChangesInput, GetContextBundleInput,
    GetFileContentInput, GetFileContextInput, GetFileOutlineInput, GetFileTreeInput,
    GetRepoMapInput, GetSymbolContextInput, GetSymbolInput, GetSymbolsInput, IndexFolderInput,
    InspectMatchInput, ResolvePathInput, SearchFilesInput, SearchSymbolsInput, SearchTextInput,
    TraceSymbolInput, WhatChangedInput,
};
use crate::sidecar::{SidecarState, SymbolSnapshot, TokenStats};
use crate::watcher::{self, WatcherInfo};

const DAEMON_DIR_NAME: &str = ".tokenizor";
const DAEMON_PORT_FILE: &str = "daemon.port";
const DAEMON_PID_FILE: &str = "daemon.pid";
const DAEMON_START_LOCK_FILE: &str = "daemon.starting";

pub type SharedDaemonState = Arc<DaemonState>;

pub struct DaemonHandle {
    pub port: u16,
    pub shutdown_tx: tokio::sync::oneshot::Sender<()>,
    pub state: SharedDaemonState,
}

#[derive(Clone)]
pub struct DaemonSessionClient {
    http_client: reqwest::Client,
    base_url: String,
    project_id: String,
    session_id: String,
    project_name: String,
    /// Stored so reconnection can re-open a session at the same project root.
    project_root: Option<PathBuf>,
}

pub struct DaemonState {
    next_session_id: AtomicU64,
    projects: RwLock<HashMap<String, ProjectInstance>>,
    sessions: RwLock<HashMap<String, SessionRecord>>,
    identity: DaemonIdentity,
}

struct ProjectInstance {
    project_id: String,
    canonical_root: PathBuf,
    project_name: String,
    index: SharedIndex,
    watcher_info: Arc<Mutex<WatcherInfo>>,
    watcher_task: Option<tokio::task::JoinHandle<()>>,
    token_stats: Arc<TokenStats>,
    symbol_cache: Arc<RwLock<HashMap<String, Vec<SymbolSnapshot>>>>,
    session_ids: HashSet<String>,
    opened_at: SystemTime,
}

#[derive(Clone)]
struct SessionRecord {
    session_id: String,
    project_id: String,
    client_name: String,
    pid: Option<u32>,
    opened_at: SystemTime,
    last_seen_at: SystemTime,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenProjectRequest {
    pub project_root: String,
    pub client_name: String,
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct OpenProjectResponse {
    pub project_id: String,
    pub session_id: String,
    pub project_name: String,
    pub canonical_root: String,
    pub session_count: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CloseSessionResponse {
    pub session_id: String,
    pub project_id: String,
    pub remaining_sessions: usize,
    pub project_removed: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct HeartbeatResponse {
    pub session_id: String,
    pub known_session: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ProjectSummary {
    pub project_id: String,
    pub project_name: String,
    pub canonical_root: String,
    pub session_count: usize,
    pub opened_at_unix_secs: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SessionSummary {
    pub session_id: String,
    pub project_id: String,
    pub client_name: String,
    pub pid: Option<u32>,
    pub opened_at_unix_secs: u64,
    pub last_seen_at_unix_secs: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ProjectHealth {
    pub project_id: String,
    pub project_name: String,
    pub canonical_root: String,
    pub session_count: usize,
    pub file_count: usize,
    pub symbol_count: usize,
    pub index_state: String,
    pub opened_at_unix_secs: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct DaemonHealth {
    pub project_count: usize,
    pub session_count: usize,
    pub daemon_version: String,
    pub executable_path: String,
}

#[derive(Clone)]
struct SessionRuntime {
    project_name: String,
    canonical_root: PathBuf,
    index: SharedIndex,
    watcher_info: Arc<Mutex<WatcherInfo>>,
    token_stats: Arc<TokenStats>,
    symbol_cache: Arc<RwLock<HashMap<String, Vec<SymbolSnapshot>>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DaemonIdentity {
    version: String,
    executable_path: String,
}

impl DaemonState {
    pub fn new() -> Self {
        Self {
            next_session_id: AtomicU64::new(1),
            projects: RwLock::new(HashMap::new()),
            sessions: RwLock::new(HashMap::new()),
            identity: current_daemon_identity(),
        }
    }

    pub fn open_project_session(
        &self,
        request: OpenProjectRequest,
    ) -> anyhow::Result<OpenProjectResponse> {
        let canonical_root = canonical_project_root(Path::new(&request.project_root))?;
        let project_id = project_key(&canonical_root);

        {
            // Hold the write lock for the entire check-and-insert to prevent
            // TOCTOU: two concurrent callers for the same project_id must not
            // both call ProjectInstance::load (which spawns watcher tasks and
            // starts git-temporal analysis), leaving an orphaned task behind.
            let mut projects = self.projects.write().expect("lock poisoned");
            if !projects.contains_key(&project_id) {
                let project = ProjectInstance::load(&canonical_root)?;
                projects.insert(project_id.clone(), project);
            }
        }

        let session_id = format!(
            "session-{}",
            self.next_session_id.fetch_add(1, Ordering::Relaxed)
        );
        let now = SystemTime::now();

        let (project_name, canonical_root_text, session_count) = {
            let mut projects = self.projects.write().expect("lock poisoned");
            let project = projects
                .get_mut(&project_id)
                .expect("project must exist after creation");
            project.session_ids.insert(session_id.clone());
            (
                project.project_name.clone(),
                normalized_path_string(&project.canonical_root),
                project.session_ids.len(),
            )
        };

        let session = SessionRecord {
            session_id: session_id.clone(),
            project_id: project_id.clone(),
            client_name: request.client_name,
            pid: request.pid,
            opened_at: now,
            last_seen_at: now,
        };
        self.sessions
            .write()
            .expect("lock poisoned")
            .insert(session_id.clone(), session);

        Ok(OpenProjectResponse {
            project_id,
            session_id,
            project_name,
            canonical_root: canonical_root_text,
            session_count,
        })
    }

    pub fn heartbeat(&self, session_id: &str) -> HeartbeatResponse {
        let known_session = self
            .sessions
            .write()
            .expect("lock poisoned")
            .get_mut(session_id)
            .map(|session| {
                session.last_seen_at = SystemTime::now();
                true
            })
            .unwrap_or(false);

        HeartbeatResponse {
            session_id: session_id.to_string(),
            known_session,
        }
    }

    pub fn close_session(&self, session_id: &str) -> Option<CloseSessionResponse> {
        // Lock ordering: projects before sessions (matches open_project_session).
        // We need the project_id from the session first, so peek with a read lock,
        // then acquire projects.write(), then sessions.write() to remove.
        let project_id = {
            let sessions = self.sessions.read().expect("lock poisoned");
            sessions.get(session_id)?.project_id.clone()
        };

        let mut project_removed = false;
        let remaining_sessions = {
            let mut projects = self.projects.write().expect("lock poisoned");
            match projects.get_mut(&project_id) {
                Some(project) => {
                    project.session_ids.remove(session_id);
                    let remaining = project.session_ids.len();
                    if remaining == 0 {
                        if let Some(removed) = projects.remove(&project_id) {
                            let mut watcher_task = removed.watcher_task;
                            abort_watcher_task(&mut watcher_task);
                        }
                        project_removed = true;
                    }
                    remaining
                }
                None => 0,
            }
        };

        // Now remove the session (projects lock fully released).
        let session = self
            .sessions
            .write()
            .expect("lock poisoned")
            .remove(session_id)?;

        Some(CloseSessionResponse {
            session_id: session.session_id,
            project_id: session.project_id,
            remaining_sessions,
            project_removed,
        })
    }

    pub fn list_projects(&self) -> Vec<ProjectSummary> {
        let projects = self.projects.read().expect("lock poisoned");
        let mut summaries: Vec<ProjectSummary> = projects
            .values()
            .map(|project| ProjectSummary {
                project_id: project.project_id.clone(),
                project_name: project.project_name.clone(),
                canonical_root: normalized_path_string(&project.canonical_root),
                session_count: project.session_ids.len(),
                opened_at_unix_secs: unix_seconds(project.opened_at),
            })
            .collect();
        summaries.sort_by(|a, b| a.canonical_root.cmp(&b.canonical_root));
        summaries
    }

    pub fn project_health(&self, project_id: &str) -> Option<ProjectHealth> {
        let projects = self.projects.read().expect("lock poisoned");
        let project = projects.get(project_id)?;
        let published = project.index.published_state();

        Some(ProjectHealth {
            project_id: project.project_id.clone(),
            project_name: project.project_name.clone(),
            canonical_root: normalized_path_string(&project.canonical_root),
            session_count: project.session_ids.len(),
            file_count: published.file_count,
            symbol_count: published.symbol_count,
            index_state: published.status_label().to_string(),
            opened_at_unix_secs: unix_seconds(project.opened_at),
        })
    }

    pub fn list_sessions(&self, project_id: &str) -> Option<Vec<SessionSummary>> {
        let session_ids: Vec<String> = {
            let projects = self.projects.read().expect("lock poisoned");
            let project = projects.get(project_id)?;
            project.session_ids.iter().cloned().collect()
        };

        let sessions = self.sessions.read().expect("lock poisoned");
        let mut summaries: Vec<SessionSummary> = session_ids
            .iter()
            .filter_map(|session_id| sessions.get(session_id))
            .map(|session| SessionSummary {
                session_id: session.session_id.clone(),
                project_id: session.project_id.clone(),
                client_name: session.client_name.clone(),
                pid: session.pid,
                opened_at_unix_secs: unix_seconds(session.opened_at),
                last_seen_at_unix_secs: unix_seconds(session.last_seen_at),
            })
            .collect();
        summaries.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        Some(summaries)
    }

    fn index_folder_for_session(
        &self,
        session_id: &str,
        input: IndexFolderInput,
    ) -> anyhow::Result<String> {
        let target_root = canonical_project_root(Path::new(&input.path))?;
        let target_project_id = project_key(&target_root);

        let current_project_id = {
            let sessions = self.sessions.read().expect("lock poisoned");
            sessions
                .get(session_id)
                .map(|session| session.project_id.clone())
                .ok_or_else(|| anyhow::anyhow!("unknown session '{session_id}'"))?
        };

        let needs_reassign = current_project_id != target_project_id;

        // All project-map mutations happen inside this block so the write guard
        // is fully released before we touch sessions.write() below — preventing
        // the lock-order inversion with close_session (sessions → projects).
        let (file_count, symbol_count) = {
            let mut projects = self.projects.write().expect("lock poisoned");

            if needs_reassign {
                if !projects.contains_key(&target_project_id) {
                    let project = ProjectInstance::load(&target_root)?;
                    projects.insert(target_project_id.clone(), project);
                }

                if let Some(current_project) = projects.get_mut(&current_project_id) {
                    current_project.session_ids.remove(session_id);
                }

                if let Some(target_project) = projects.get_mut(&target_project_id) {
                    target_project.session_ids.insert(session_id.to_string());
                }

                let should_remove_old = projects
                    .get(&current_project_id)
                    .map(|project| project.session_ids.is_empty())
                    .unwrap_or(false);
                if should_remove_old && let Some(removed) = projects.remove(&current_project_id) {
                    let mut watcher_task = removed.watcher_task;
                    abort_watcher_task(&mut watcher_task);
                }
            }

            let target_project = projects
                .get_mut(&target_project_id)
                .ok_or_else(|| anyhow::anyhow!("missing target project after reload"))?;
            target_project.reload(&target_root)?
        }; // projects write lock released here

        // Update the session's project association *after* the projects lock is
        // released to maintain lock order (projects before sessions everywhere).
        if needs_reassign {
            if let Some(session) = self
                .sessions
                .write()
                .expect("lock poisoned")
                .get_mut(session_id)
            {
                session.project_id = target_project_id;
                session.last_seen_at = SystemTime::now();
            }
        }

        Ok(format!(
            "Indexed {} files, {} symbols.",
            file_count, symbol_count
        ))
    }

    fn session_runtime(&self, session_id: &str) -> Option<SessionRuntime> {
        let project_id = {
            let sessions = self.sessions.read().expect("lock poisoned");
            sessions.get(session_id)?.project_id.clone()
        };

        let projects = self.projects.read().expect("lock poisoned");
        let project = projects.get(&project_id)?;
        Some(SessionRuntime {
            project_name: project.project_name.clone(),
            canonical_root: project.canonical_root.clone(),
            index: Arc::clone(&project.index),
            watcher_info: Arc::clone(&project.watcher_info),
            token_stats: Arc::clone(&project.token_stats),
            symbol_cache: Arc::clone(&project.symbol_cache),
        })
    }

    pub fn health(&self) -> DaemonHealth {
        DaemonHealth {
            project_count: self.projects.read().expect("lock poisoned").len(),
            session_count: self.sessions.read().expect("lock poisoned").len(),
            daemon_version: self.identity.version.clone(),
            executable_path: self.identity.executable_path.clone(),
        }
    }
}

impl Default for DaemonState {
    fn default() -> Self {
        Self::new()
    }
}

impl DaemonSessionClient {
    fn new(base_url: String, project_id: String, session_id: String, project_name: String) -> Self {
        Self {
            http_client: reqwest::Client::new(),
            base_url,
            project_id,
            session_id,
            project_name,
            project_root: None,
        }
    }

    fn with_project_root(mut self, root: PathBuf) -> Self {
        self.project_root = Some(root);
        self
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(
        base_url: String,
        project_id: String,
        session_id: String,
        project_name: String,
    ) -> Self {
        Self::new(base_url, project_id, session_id, project_name)
    }

    pub fn project_name(&self) -> &str {
        &self.project_name
    }

    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn project_root(&self) -> Option<&Path> {
        self.project_root.as_deref()
    }

    pub fn port(&self) -> Option<u16> {
        self.base_url
            .rsplit(':')
            .next()
            .and_then(|value| value.parse::<u16>().ok())
    }

    /// Attempt to reconnect to the daemon after a connection failure.
    ///
    /// Calls `ensure_daemon_running` (which will spawn a new daemon if needed),
    /// opens a fresh session, and returns the new client. The caller should
    /// replace their stored client with the returned one.
    pub async fn reconnect(&self) -> anyhow::Result<DaemonSessionClient> {
        let project_root = self
            .project_root
            .as_deref()
            .context("cannot reconnect: no project root stored")?;
        tracing::info!(
            "attempting daemon reconnection for project {}",
            self.project_name
        );
        let new_client =
            connect_or_spawn_session(project_root, "mcp-stdio", Some(std::process::id())).await?;
        Ok(new_client.with_project_root(project_root.to_path_buf()))
    }

    pub async fn call_tool_value(
        &self,
        tool_name: &str,
        params: serde_json::Value,
    ) -> anyhow::Result<String> {
        let response = self
            .http_client
            .post(format!(
                "{}/v1/sessions/{}/tools/{}",
                self.base_url, self.session_id, tool_name
            ))
            .json(&params)
            .send()
            .await
            .with_context(|| format!("calling daemon tool '{tool_name}'"))?
            .error_for_status()
            .with_context(|| format!("daemon rejected tool '{tool_name}'"))?;

        response
            .text()
            .await
            .with_context(|| format!("reading daemon tool response for '{tool_name}'"))
    }

    pub async fn heartbeat(&self) -> anyhow::Result<HeartbeatResponse> {
        self.http_client
            .post(format!(
                "{}/v1/sessions/{}/heartbeat",
                self.base_url, self.session_id
            ))
            .send()
            .await
            .context("sending daemon heartbeat")?
            .error_for_status()
            .context("daemon heartbeat status")?
            .json::<HeartbeatResponse>()
            .await
            .context("daemon heartbeat body")
    }

    pub async fn close(&self) -> anyhow::Result<CloseSessionResponse> {
        self.http_client
            .delete(format!("{}/v1/sessions/{}", self.base_url, self.session_id))
            .send()
            .await
            .context("closing daemon session")?
            .error_for_status()
            .context("daemon close status")?
            .json::<CloseSessionResponse>()
            .await
            .context("daemon close body")
    }
}

struct DaemonStartLock {
    path: PathBuf,
}

impl Drop for DaemonStartLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub async fn connect_or_spawn_session(
    project_root: &Path,
    client_name: &str,
    pid: Option<u32>,
) -> anyhow::Result<DaemonSessionClient> {
    let port = ensure_daemon_running().await?;
    let base_url = format!("http://127.0.0.1:{port}");
    let opened = reqwest::Client::new()
        .post(format!("{base_url}/v1/sessions/open"))
        .json(&OpenProjectRequest {
            project_root: project_root.display().to_string(),
            client_name: client_name.to_string(),
            pid,
        })
        .send()
        .await
        .context("opening daemon session")?
        .error_for_status()
        .context("daemon session open status")?
        .json::<OpenProjectResponse>()
        .await
        .context("daemon session open body")?;

    Ok(DaemonSessionClient::new(
        base_url,
        opened.project_id,
        opened.session_id,
        opened.project_name,
    )
    .with_project_root(project_root.to_path_buf()))
}

async fn ensure_daemon_running() -> anyhow::Result<u16> {
    let identity = current_daemon_identity();
    if let Some(port) = daemon_port_if_compatible(&identity).await? {
        return Ok(port);
    }

    if let Some(_lock) = try_acquire_start_lock()? {
        if let Some(port) = daemon_port_if_compatible(&identity).await? {
            return Ok(port);
        }
        stop_incompatible_recorded_daemon(&identity).await?;
        spawn_daemon_process()?;
        wait_for_daemon_ready(&identity).await
    } else {
        wait_for_daemon_ready(&identity).await
    }
}

async fn daemon_port_if_compatible(identity: &DaemonIdentity) -> anyhow::Result<Option<u16>> {
    let port = match read_daemon_port_file() {
        Ok(port) => port,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("reading daemon port file"),
    };

    match daemon_health(port).await {
        Some(health) if daemon_health_matches(&health, identity) => Ok(Some(port)),
        Some(health) => {
            tracing::warn!(
                recorded_port = port,
                recorded_version = %health.daemon_version,
                expected_version = %identity.version,
                recorded_executable = %health.executable_path,
                expected_executable = %identity.executable_path,
                "recorded tokenizor daemon is incompatible with the current executable"
            );
            Ok(None)
        }
        None => Ok(None),
    }
}

async fn wait_for_daemon_ready(identity: &DaemonIdentity) -> anyhow::Result<u16> {
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(5);
    loop {
        if let Some(port) = daemon_port_if_compatible(identity).await? {
            return Ok(port);
        }

        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!("timed out waiting for tokenizor daemon to become ready");
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}

async fn daemon_health(port: u16) -> Option<DaemonHealth> {
    reqwest::Client::new()
        .get(format!("http://127.0.0.1:{port}/health"))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json::<DaemonHealth>()
        .await
        .ok()
}

async fn daemon_health_ok(port: u16) -> bool {
    daemon_health(port).await.is_some()
}

async fn stop_incompatible_recorded_daemon(identity: &DaemonIdentity) -> anyhow::Result<()> {
    let port = match read_daemon_port_file() {
        Ok(port) => port,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error).context("reading daemon port file"),
    };

    let Some(health) = daemon_health(port).await else {
        cleanup_daemon_files();
        return Ok(());
    };

    if daemon_health_matches(&health, identity) {
        return Ok(());
    }

    if let Ok(pid) = read_daemon_pid_file() {
        if let Err(error) = terminate_process(pid) {
            tracing::warn!(
                pid,
                "failed to terminate incompatible tokenizor daemon automatically: {error}"
            );
        }
        wait_for_daemon_unhealthy(port).await;
    }

    cleanup_daemon_files();
    Ok(())
}

async fn wait_for_daemon_unhealthy(port: u16) {
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        if !daemon_health_ok(port).await {
            break;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}

fn try_acquire_start_lock() -> anyhow::Result<Option<DaemonStartLock>> {
    let path = daemon_dir()?.join(DAEMON_START_LOCK_FILE);
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(_) => Ok(Some(DaemonStartLock { path })),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(None),
        Err(error) => Err(error).context("creating daemon start lock"),
    }
}

fn spawn_daemon_process() -> anyhow::Result<()> {
    let current_exe = std::env::current_exe().context("locating current tokenizor executable")?;
    let mut command = std::process::Command::new(current_exe);
    command
        .arg("daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW);
    }

    command
        .spawn()
        .context("spawning detached tokenizor daemon")?;
    Ok(())
}

fn current_daemon_identity() -> DaemonIdentity {
    let executable_path = std::env::current_exe()
        .ok()
        .map(|path| normalized_path_string(&path))
        .unwrap_or_else(|| "unknown".to_string());
    DaemonIdentity {
        version: env!("CARGO_PKG_VERSION").to_string(),
        executable_path,
    }
}

fn daemon_health_matches(health: &DaemonHealth, identity: &DaemonIdentity) -> bool {
    if health.daemon_version != identity.version {
        return false;
    }

    if health.executable_path == "unknown" || identity.executable_path == "unknown" {
        return true;
    }

    stable_path_identity(&health.executable_path) == stable_path_identity(&identity.executable_path)
}

fn stable_path_identity(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    if cfg!(windows) {
        normalized.to_lowercase()
    } else {
        normalized
    }
}

impl ProjectInstance {
    fn load(canonical_root: &Path) -> anyhow::Result<Self> {
        let project_name = canonical_root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("project")
            .to_string();

        let index = live_index::LiveIndex::load(canonical_root).with_context(|| {
            format!(
                "failed to load project index for {}",
                canonical_root.display()
            )
        })?;
        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        let watcher_task = start_project_watcher(
            canonical_root.to_path_buf(),
            Arc::clone(&index),
            Arc::clone(&watcher_info),
        );

        // Kick off background git temporal analysis (non-blocking).
        live_index::git_temporal::spawn_git_temporal_computation(
            Arc::clone(&index),
            canonical_root.to_path_buf(),
        );

        Ok(Self {
            project_id: project_key(canonical_root),
            canonical_root: canonical_root.to_path_buf(),
            project_name,
            index,
            watcher_info,
            watcher_task,
            token_stats: TokenStats::new(),
            symbol_cache: Arc::new(RwLock::new(HashMap::new())),
            session_ids: HashSet::new(),
            opened_at: SystemTime::now(),
        })
    }

    fn reload(&mut self, canonical_root: &Path) -> anyhow::Result<(usize, usize)> {
        self.index.reload(canonical_root)?;
        let published = self.index.published_state();
        let file_count = published.file_count;
        let symbol_count = published.symbol_count;

        abort_watcher_task(&mut self.watcher_task);
        self.watcher_task = start_project_watcher(
            canonical_root.to_path_buf(),
            Arc::clone(&self.index),
            Arc::clone(&self.watcher_info),
        );
        self.canonical_root = canonical_root.to_path_buf();
        self.project_name = canonical_root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("project")
            .to_string();
        self.project_id = project_key(canonical_root);

        // Refresh git temporal data after reload.
        live_index::git_temporal::spawn_git_temporal_computation(
            Arc::clone(&self.index),
            canonical_root.to_path_buf(),
        );

        Ok((file_count, symbol_count))
    }
}

fn start_project_watcher(
    repo_root: PathBuf,
    index: SharedIndex,
    watcher_info: Arc<Mutex<WatcherInfo>>,
) -> Option<tokio::task::JoinHandle<()>> {
    tokio::runtime::Handle::try_current()
        .ok()
        .map(|handle| handle.spawn(watcher::run_watcher(repo_root, index, watcher_info)))
}

fn abort_watcher_task(task: &mut Option<tokio::task::JoinHandle<()>>) {
    if let Some(task) = task.take() {
        task.abort();
    }
}

pub fn build_router(state: SharedDaemonState) -> Router {
    Router::new()
        .route("/health", get(daemon_health_handler))
        .route("/v1/projects", get(list_projects_handler))
        .route(
            "/v1/projects/{project_id}/health",
            get(project_health_handler),
        )
        .route(
            "/v1/projects/{project_id}/sessions",
            get(list_sessions_handler),
        )
        .route("/v1/sessions/open", post(open_project_session_handler))
        .route(
            "/v1/sessions/{session_id}/tools/{tool_name}",
            post(call_tool_handler),
        )
        .route(
            "/v1/sessions/{session_id}/sidecar/health",
            get(sidecar_health_handler),
        )
        .route(
            "/v1/sessions/{session_id}/sidecar/outline",
            get(sidecar_outline_handler),
        )
        .route(
            "/v1/sessions/{session_id}/sidecar/impact",
            get(sidecar_impact_handler),
        )
        .route(
            "/v1/sessions/{session_id}/sidecar/symbol-context",
            get(sidecar_symbol_context_handler),
        )
        .route(
            "/v1/sessions/{session_id}/sidecar/repo-map",
            get(sidecar_repo_map_handler),
        )
        .route(
            "/v1/sessions/{session_id}/sidecar/prompt-context",
            get(sidecar_prompt_context_handler),
        )
        .route(
            "/v1/sessions/{session_id}/sidecar/stats",
            get(sidecar_stats_handler),
        )
        .route(
            "/v1/sessions/{session_id}/heartbeat",
            post(heartbeat_handler),
        )
        .route("/v1/sessions/{session_id}", delete(close_session_handler))
        .with_state(state)
}

pub async fn spawn_daemon(bind_host: &str) -> anyhow::Result<DaemonHandle> {
    let resolved_host =
        std::env::var("TOKENIZOR_DAEMON_BIND").unwrap_or_else(|_| bind_host.to_string());
    cleanup_daemon_files();

    let listener = TcpListener::bind(format!("{resolved_host}:0")).await?;
    let port = listener.local_addr()?.port();
    write_daemon_port_file(port)?;
    write_daemon_pid_file(std::process::id())?;

    let state = Arc::new(DaemonState::new());
    let app = build_router(Arc::clone(&state));
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    tokio::spawn(async move {
        let shutdown_signal = async move {
            let _ = shutdown_rx.await;
        };

        if let Err(error) = axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal)
            .await
        {
            tracing::error!("daemon server error: {error}");
        }

        cleanup_daemon_files();
    });

    Ok(DaemonHandle {
        port,
        shutdown_tx,
        state,
    })
}

pub async fn run_daemon_until_shutdown(bind_host: &str) -> anyhow::Result<()> {
    let handle = spawn_daemon(bind_host).await?;
    tracing::info!(port = handle.port, "shared daemon started");
    tokio::signal::ctrl_c().await?;
    let _ = handle.shutdown_tx.send(());
    Ok(())
}

async fn daemon_health_handler(State(state): State<SharedDaemonState>) -> Json<DaemonHealth> {
    Json(state.health())
}

async fn list_projects_handler(
    State(state): State<SharedDaemonState>,
) -> Json<Vec<ProjectSummary>> {
    Json(state.list_projects())
}

async fn project_health_handler(
    State(state): State<SharedDaemonState>,
    AxumPath(project_id): AxumPath<String>,
) -> Result<Json<ProjectHealth>, axum::http::StatusCode> {
    state
        .project_health(&project_id)
        .map(Json)
        .ok_or(axum::http::StatusCode::NOT_FOUND)
}

async fn list_sessions_handler(
    State(state): State<SharedDaemonState>,
    AxumPath(project_id): AxumPath<String>,
) -> Result<Json<Vec<SessionSummary>>, axum::http::StatusCode> {
    state
        .list_sessions(&project_id)
        .map(Json)
        .ok_or(axum::http::StatusCode::NOT_FOUND)
}

async fn open_project_session_handler(
    State(state): State<SharedDaemonState>,
    Json(request): Json<OpenProjectRequest>,
) -> Result<Json<OpenProjectResponse>, (axum::http::StatusCode, String)> {
    let state_for_load = Arc::clone(&state);
    let response =
        tokio::task::spawn_blocking(move || state_for_load.open_project_session(request))
            .await
            .map_err(internal_error)?
            .map_err(bad_request)?;
    Ok(Json(response))
}

async fn call_tool_handler(
    State(state): State<SharedDaemonState>,
    AxumPath((session_id, tool_name)): AxumPath<(String, String)>,
    Json(params): Json<serde_json::Value>,
) -> Result<String, (axum::http::StatusCode, String)> {
    if tool_name == "index_folder" {
        let input = decode_params::<IndexFolderInput>(params).map_err(bad_request)?;
        return state
            .index_folder_for_session(&session_id, input)
            .map_err(bad_request);
    }

    let runtime = state.session_runtime(&session_id).ok_or_else(|| {
        (
            axum::http::StatusCode::NOT_FOUND,
            format!("unknown session '{session_id}'"),
        )
    })?;

    execute_tool_call(runtime, &tool_name, params)
        .await
        .map_err(bad_request)
}

async fn sidecar_health_handler(
    State(state): State<SharedDaemonState>,
    AxumPath(session_id): AxumPath<String>,
) -> Result<Json<crate::sidecar::handlers::HealthResponse>, axum::http::StatusCode> {
    let runtime = state
        .session_runtime(&session_id)
        .ok_or(axum::http::StatusCode::NOT_FOUND)?;
    crate::sidecar::handlers::health_handler(State(sidecar_state_for_runtime(&runtime))).await
}

async fn sidecar_outline_handler(
    State(state): State<SharedDaemonState>,
    AxumPath(session_id): AxumPath<String>,
    Query(params): Query<crate::sidecar::handlers::OutlineParams>,
) -> Result<String, axum::http::StatusCode> {
    let runtime = state
        .session_runtime(&session_id)
        .ok_or(axum::http::StatusCode::NOT_FOUND)?;
    crate::sidecar::handlers::outline_handler(
        State(sidecar_state_for_runtime(&runtime)),
        Query(params),
    )
    .await
}

async fn sidecar_impact_handler(
    State(state): State<SharedDaemonState>,
    AxumPath(session_id): AxumPath<String>,
    Query(params): Query<crate::sidecar::handlers::ImpactParams>,
) -> Result<String, axum::http::StatusCode> {
    let runtime = state
        .session_runtime(&session_id)
        .ok_or(axum::http::StatusCode::NOT_FOUND)?;
    crate::sidecar::handlers::impact_handler(
        State(sidecar_state_for_runtime(&runtime)),
        Query(params),
    )
    .await
}

async fn sidecar_symbol_context_handler(
    State(state): State<SharedDaemonState>,
    AxumPath(session_id): AxumPath<String>,
    Query(params): Query<crate::sidecar::handlers::SymbolContextParams>,
) -> Result<String, axum::http::StatusCode> {
    let runtime = state
        .session_runtime(&session_id)
        .ok_or(axum::http::StatusCode::NOT_FOUND)?;
    crate::sidecar::handlers::symbol_context_handler(
        State(sidecar_state_for_runtime(&runtime)),
        Query(params),
    )
    .await
}

async fn sidecar_repo_map_handler(
    State(state): State<SharedDaemonState>,
    AxumPath(session_id): AxumPath<String>,
) -> Result<String, axum::http::StatusCode> {
    let runtime = state
        .session_runtime(&session_id)
        .ok_or(axum::http::StatusCode::NOT_FOUND)?;
    crate::sidecar::handlers::repo_map_handler(State(sidecar_state_for_runtime(&runtime))).await
}

async fn sidecar_prompt_context_handler(
    State(state): State<SharedDaemonState>,
    AxumPath(session_id): AxumPath<String>,
    Query(params): Query<crate::sidecar::handlers::PromptContextParams>,
) -> Result<String, axum::http::StatusCode> {
    let runtime = state
        .session_runtime(&session_id)
        .ok_or(axum::http::StatusCode::NOT_FOUND)?;
    crate::sidecar::handlers::prompt_context_handler(
        State(sidecar_state_for_runtime(&runtime)),
        Query(params),
    )
    .await
}

async fn sidecar_stats_handler(
    State(state): State<SharedDaemonState>,
    AxumPath(session_id): AxumPath<String>,
) -> Result<Json<crate::sidecar::StatsSnapshot>, axum::http::StatusCode> {
    let runtime = state
        .session_runtime(&session_id)
        .ok_or(axum::http::StatusCode::NOT_FOUND)?;
    Ok(crate::sidecar::handlers::stats_handler(State(sidecar_state_for_runtime(&runtime))).await)
}

async fn heartbeat_handler(
    State(state): State<SharedDaemonState>,
    AxumPath(session_id): AxumPath<String>,
) -> Json<HeartbeatResponse> {
    Json(state.heartbeat(&session_id))
}

async fn close_session_handler(
    State(state): State<SharedDaemonState>,
    AxumPath(session_id): AxumPath<String>,
) -> Result<Json<CloseSessionResponse>, axum::http::StatusCode> {
    state
        .close_session(&session_id)
        .map(Json)
        .ok_or(axum::http::StatusCode::NOT_FOUND)
}

async fn execute_tool_call(
    runtime: SessionRuntime,
    tool_name: &str,
    params: serde_json::Value,
) -> anyhow::Result<String> {
    let server = TokenizorServer::new(
        Arc::clone(&runtime.index),
        runtime.project_name,
        Arc::clone(&runtime.watcher_info),
        Some(runtime.canonical_root),
        Some(Arc::clone(&runtime.token_stats)),
    );

    match tool_name {
        // Backward-compat alias: get_file_outline → get_file_context with sections=['outline']
        "get_file_outline" => {
            let outline_input = decode_params::<GetFileOutlineInput>(params)?;
            let ctx_input = GetFileContextInput {
                path: outline_input.path,
                max_tokens: None,
                sections: Some(vec!["outline".to_string()]),
            };
            Ok(server.get_file_context(Parameters(ctx_input)).await)
        }
        "get_symbol" => Ok(server
            .get_symbol(Parameters(decode_params::<GetSymbolInput>(params)?))
            .await),
        // Backward-compat alias: get_symbols → get_symbol with targets[]
        "get_symbols" => {
            let batch_input = decode_params::<GetSymbolsInput>(params)?;
            let merged = GetSymbolInput {
                path: String::new(),
                name: String::new(),
                kind: None,
                targets: Some(batch_input.targets),
            };
            Ok(server.get_symbol(Parameters(merged)).await)
        }
        // Backward-compat alias: get_repo_outline → get_repo_map with detail='full'
        "get_repo_outline" => {
            let merged = GetRepoMapInput {
                detail: Some("full".to_string()),
                path: None,
                depth: None,
            };
            Ok(server.get_repo_map(Parameters(merged)).await)
        }
        "get_repo_map" => Ok(server
            .get_repo_map(Parameters(decode_params::<GetRepoMapInput>(params)?))
            .await),
        "get_file_context" => Ok(server
            .get_file_context(Parameters(decode_params::<GetFileContextInput>(params)?))
            .await),
        "get_symbol_context" => Ok(server
            .get_symbol_context(Parameters(decode_params::<GetSymbolContextInput>(params)?))
            .await),
        "analyze_file_impact" => Ok(server
            .analyze_file_impact(Parameters(decode_params::<AnalyzeFileImpactInput>(params)?))
            .await),
        "search_symbols" => Ok(server
            .search_symbols(Parameters(decode_params::<SearchSymbolsInput>(params)?))
            .await),
        "search_text" => Ok(server
            .search_text(Parameters(decode_params::<SearchTextInput>(params)?))
            .await),
        "trace_symbol" => Ok(server
            .trace_symbol(Parameters(decode_params::<TraceSymbolInput>(params)?))
            .await),
        "inspect_match" => Ok(server
            .inspect_match(Parameters(decode_params::<InspectMatchInput>(params)?))
            .await),
        "search_files" => Ok(server
            .search_files(Parameters(decode_params::<SearchFilesInput>(params)?))
            .await),
        // Backward-compat alias: resolve_path → search_files with resolve=true
        "resolve_path" => {
            let rp = decode_params::<ResolvePathInput>(params)?;
            let merged = SearchFilesInput {
                query: rp.hint,
                limit: None,
                current_file: None,
                changed_with: None,
                resolve: Some(true),
            };
            Ok(server.search_files(Parameters(merged)).await)
        }
        "health" => Ok(server.health().await),
        "index_folder" => Ok(server
            .index_folder(Parameters(decode_params::<IndexFolderInput>(params)?))
            .await),
        "what_changed" => Ok(server
            .what_changed(Parameters(decode_params::<WhatChangedInput>(params)?))
            .await),
        "get_file_content" => Ok(server
            .get_file_content(Parameters(decode_params::<GetFileContentInput>(params)?))
            .await),
        "find_references" => Ok(server
            .find_references(Parameters(decode_params::<FindReferencesInput>(params)?))
            .await),
        "find_dependents" => Ok(server
            .find_dependents(Parameters(decode_params::<FindDependentsInput>(params)?))
            .await),
        // Backward-compat alias: get_file_tree → get_repo_map with detail='tree'
        "get_file_tree" => {
            let tree_input = decode_params::<GetFileTreeInput>(params)?;
            let merged = GetRepoMapInput {
                detail: Some("tree".to_string()),
                path: tree_input.path,
                depth: tree_input.depth,
            };
            Ok(server.get_repo_map(Parameters(merged)).await)
        }
        // Backward-compat alias: get_context_bundle → get_symbol_context with bundle=true
        "get_context_bundle" => {
            let bundle_input = decode_params::<GetContextBundleInput>(params)?;
            let merged = GetSymbolContextInput {
                name: bundle_input.name,
                file: None,
                path: Some(bundle_input.path),
                symbol_kind: bundle_input.kind,
                symbol_line: bundle_input.symbol_line,
                verbosity: bundle_input.verbosity,
                bundle: Some(true),
            };
            Ok(server.get_symbol_context(Parameters(merged)).await)
        }
        "explore" => Ok(server
            .explore(Parameters(decode_params::<ExploreInput>(params)?))
            .await),
        // Backward-compat alias: get_co_changes → analyze_file_impact with include_co_changes=true
        "get_co_changes" => {
            let co_input = decode_params::<GetCoChangesInput>(params)?;
            let merged = AnalyzeFileImpactInput {
                path: co_input.path,
                new_file: None,
                include_co_changes: Some(true),
                co_changes_limit: co_input.limit,
            };
            Ok(server.analyze_file_impact(Parameters(merged)).await)
        }
        "diff_symbols" => Ok(server
            .diff_symbols(Parameters(decode_params::<DiffSymbolsInput>(params)?))
            .await),
        // Backward-compat alias: find_implementations → find_references with mode='implementations'
        "find_implementations" => {
            let impl_input = decode_params::<FindImplementationsInput>(params)?;
            let merged = FindReferencesInput {
                name: impl_input.name,
                kind: None,
                path: None,
                symbol_kind: None,
                symbol_line: None,
                limit: impl_input.limit,
                max_per_file: None,
                compact: None,
                mode: Some("implementations".to_string()),
                direction: impl_input.direction,
            };
            Ok(server.find_references(Parameters(merged)).await)
        }
        "replace_symbol_body" => Ok(server
            .replace_symbol_body(Parameters(decode_params::<ReplaceSymbolBodyInput>(params)?))
            .await),
        "insert_symbol" => Ok(server
            .insert_symbol(Parameters(decode_params::<InsertSymbolInput>(params)?))
            .await),
        // Backward-compat aliases for the merged insert_symbol tool
        "insert_before_symbol" => {
            let mut input = decode_params::<InsertSymbolInput>(params)?;
            input.position = Some("before".to_string());
            Ok(server.insert_symbol(Parameters(input)).await)
        }
        "insert_after_symbol" => {
            let mut input = decode_params::<InsertSymbolInput>(params)?;
            input.position = Some("after".to_string());
            Ok(server.insert_symbol(Parameters(input)).await)
        }
        "delete_symbol" => Ok(server
            .delete_symbol(Parameters(decode_params::<DeleteSymbolInput>(params)?))
            .await),
        "edit_within_symbol" => Ok(server
            .edit_within_symbol(Parameters(decode_params::<EditWithinSymbolInput>(params)?))
            .await),
        "batch_edit" => Ok(server
            .batch_edit(Parameters(decode_params::<BatchEditInput>(params)?))
            .await),
        "batch_rename" => Ok(server
            .batch_rename(Parameters(decode_params::<BatchRenameInput>(params)?))
            .await),
        "batch_insert" => Ok(server
            .batch_insert(Parameters(decode_params::<BatchInsertInput>(params)?))
            .await),
        other => anyhow::bail!("unknown tool '{other}'"),
    }
}

fn sidecar_state_for_runtime(runtime: &SessionRuntime) -> SidecarState {
    SidecarState {
        index: Arc::clone(&runtime.index),
        token_stats: Arc::clone(&runtime.token_stats),
        repo_root: Some(runtime.canonical_root.clone()),
        symbol_cache: Arc::clone(&runtime.symbol_cache),
    }
}

fn decode_params<T>(params: serde_json::Value) -> anyhow::Result<T>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(params).context("invalid tool parameters")
}

fn bad_request(error: anyhow::Error) -> (axum::http::StatusCode, String) {
    (axum::http::StatusCode::BAD_REQUEST, error.to_string())
}

fn internal_error(error: tokio::task::JoinError) -> (axum::http::StatusCode, String) {
    (
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        error.to_string(),
    )
}

fn canonical_project_root(root: &Path) -> anyhow::Result<PathBuf> {
    root.canonicalize()
        .with_context(|| format!("failed to canonicalize project root {}", root.display()))
}

fn project_key(root: &Path) -> String {
    let normalized = normalized_path_string(root);
    let stable_path = if cfg!(windows) {
        normalized.to_lowercase()
    } else {
        normalized
    };
    format!(
        "project-{}",
        crate::hash::digest_hex(stable_path.as_bytes())
    )
}

fn normalized_path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn daemon_dir() -> io::Result<PathBuf> {
    if let Some(explicit_home) = std::env::var_os("TOKENIZOR_HOME") {
        let dir = PathBuf::from(explicit_home);
        std::fs::create_dir_all(&dir)?;
        return Ok(dir);
    }

    let home = dirs::home_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "home directory not found"))?;
    let dir = home.join(DAEMON_DIR_NAME);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn write_daemon_port_file(port: u16) -> io::Result<()> {
    std::fs::write(daemon_dir()?.join(DAEMON_PORT_FILE), port.to_string())
}

fn write_daemon_pid_file(pid: u32) -> io::Result<()> {
    std::fs::write(daemon_dir()?.join(DAEMON_PID_FILE), pid.to_string())
}

fn read_daemon_pid_file() -> io::Result<u32> {
    let contents = std::fs::read_to_string(daemon_dir()?.join(DAEMON_PID_FILE))?;
    contents
        .trim()
        .parse::<u32>()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn read_daemon_port_file() -> io::Result<u16> {
    let contents = std::fs::read_to_string(daemon_dir()?.join(DAEMON_PORT_FILE))?;
    contents
        .trim()
        .parse::<u16>()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn cleanup_daemon_files() {
    if let Ok(dir) = daemon_dir() {
        let _ = std::fs::remove_file(dir.join(DAEMON_PORT_FILE));
        let _ = std::fs::remove_file(dir.join(DAEMON_PID_FILE));
    }
}

fn terminate_process(pid: u32) -> io::Result<()> {
    #[cfg(windows)]
    let status = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;

    #[cfg(not(windows))]
    let status = std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "process termination command exited with status {status}"
        )))
    }
}

fn unix_seconds(time: SystemTime) -> u64 {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use once_cell::sync::Lazy;
    use tempfile::TempDir;
    use tokio::sync::{Mutex, MutexGuard};

    static ENV_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    fn project_dir(name: &str) -> TempDir {
        let dir = TempDir::with_prefix(name).expect("temp dir");
        std::fs::create_dir_all(dir.path().join("src")).expect("src dir");
        dir
    }

    async fn env_lock() -> MutexGuard<'static, ()> {
        ENV_LOCK.lock().await
    }

    async fn wait_for_path_absent(path: &Path) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while path.exists() {
            if tokio::time::Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    async fn spawn_fake_health_server(
        health: DaemonHealth,
    ) -> (u16, tokio::sync::oneshot::Sender<()>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake daemon");
        let port = listener.local_addr().expect("listener addr").port();
        let app = Router::new().route(
            "/health",
            get({
                let health = health.clone();
                move || {
                    let health = health.clone();
                    async move { Json(health) }
                }
            }),
        );
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let shutdown = async move {
                let _ = shutdown_rx.await;
            };
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(shutdown)
                .await;
        });
        (port, shutdown_tx)
    }

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &std::path::Path) -> Self {
            let previous = std::env::var_os(key);
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(previous) => unsafe {
                    std::env::set_var(self.key, previous);
                },
                None => unsafe {
                    std::env::remove_var(self.key);
                },
            }
        }
    }

    struct CwdGuard {
        previous: PathBuf,
    }

    impl CwdGuard {
        fn set(path: &Path) -> Self {
            let previous = std::env::current_dir().expect("current dir");
            std::env::set_current_dir(path).expect("set current dir");
            Self { previous }
        }
    }

    impl Drop for CwdGuard {
        fn drop(&mut self) {
            if std::env::set_current_dir(&self.previous).is_err() {
                std::env::set_current_dir(env!("CARGO_MANIFEST_DIR"))
                    .expect("manifest dir must be a valid cwd fallback");
            }
        }
    }

    #[test]
    fn test_open_same_root_reuses_project_instance() {
        let project = project_dir("tokenizor-daemon-a");
        let state = DaemonState::new();

        let first = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "claude".to_string(),
                pid: Some(100),
            })
            .expect("first session");
        let second = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().join(".").display().to_string(),
                client_name: "codex".to_string(),
                pid: Some(200),
            })
            .expect("second session");

        assert_eq!(first.project_id, second.project_id);
        assert_ne!(first.session_id, second.session_id);

        let projects = state.list_projects();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].session_count, 2);
    }

    #[test]
    fn test_open_distinct_roots_creates_distinct_projects() {
        let project_a = project_dir("tokenizor-daemon-b");
        let project_b = project_dir("tokenizor-daemon-c");
        let state = DaemonState::new();

        let first = state
            .open_project_session(OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "claude".to_string(),
                pid: None,
            })
            .expect("first project");
        let second = state
            .open_project_session(OpenProjectRequest {
                project_root: project_b.path().display().to_string(),
                client_name: "codex".to_string(),
                pid: None,
            })
            .expect("second project");

        assert_ne!(first.project_id, second.project_id);
        assert_eq!(state.list_projects().len(), 2);
        assert_eq!(state.health().session_count, 2);
    }

    #[test]
    fn test_close_session_removes_project_when_last_session_leaves() {
        let project = project_dir("tokenizor-daemon-d");
        let state = DaemonState::new();

        let first = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "claude".to_string(),
                pid: None,
            })
            .expect("first session");
        let second = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "codex".to_string(),
                pid: None,
            })
            .expect("second session");

        let close_first = state
            .close_session(&first.session_id)
            .expect("close first session");
        assert_eq!(close_first.remaining_sessions, 1);
        assert!(!close_first.project_removed);
        assert_eq!(state.list_projects().len(), 1);

        let close_second = state
            .close_session(&second.session_id)
            .expect("close second session");
        assert_eq!(close_second.remaining_sessions, 0);
        assert!(close_second.project_removed);
        assert!(state.list_projects().is_empty());
    }

    #[test]
    fn test_heartbeat_updates_known_session() {
        let project = project_dir("tokenizor-daemon-e");
        let state = DaemonState::new();

        let opened = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "claude".to_string(),
                pid: Some(123),
            })
            .expect("session");

        let known = state.heartbeat(&opened.session_id);
        let unknown = state.heartbeat("missing-session");

        assert!(known.known_session);
        assert!(!unknown.known_session);
    }

    #[test]
    fn test_project_health_and_sessions_expose_instance_metadata() {
        let project = project_dir("tokenizor-daemon-f");
        let state = DaemonState::new();

        let first = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "claude".to_string(),
                pid: Some(111),
            })
            .expect("first session");
        let second = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "codex".to_string(),
                pid: Some(222),
            })
            .expect("second session");

        let health = state
            .project_health(&first.project_id)
            .expect("project health should exist");
        assert_eq!(health.project_id, first.project_id);
        assert_eq!(health.session_count, 2);
        assert_eq!(health.index_state, "Ready");

        let sessions = state
            .list_sessions(&first.project_id)
            .expect("session list should exist");
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].project_id, first.project_id);
        assert!(
            sessions
                .iter()
                .any(|session| session.client_name == "claude")
        );
        assert!(
            sessions
                .iter()
                .any(|session| session.client_name == "codex")
        );
        assert!(sessions.iter().any(|session| session.pid == Some(111)));
        assert!(sessions.iter().any(|session| session.pid == Some(222)));
        assert_ne!(first.session_id, second.session_id);
    }

    #[tokio::test]
    async fn test_spawn_daemon_serves_project_and_session_endpoints() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("TOKENIZOR_HOME", daemon_home.path());
        let project = project_dir("tokenizor-daemon-http");

        let handle = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let client = reqwest::Client::new();
        let base_url = format!("http://127.0.0.1:{}", handle.port);

        let daemon_health = client
            .get(format!("{base_url}/health"))
            .send()
            .await
            .expect("health request")
            .error_for_status()
            .expect("health status")
            .json::<DaemonHealth>()
            .await
            .expect("health body");
        assert_eq!(daemon_health.project_count, 0);
        assert_eq!(daemon_health.session_count, 0);
        assert_eq!(daemon_health.daemon_version, env!("CARGO_PKG_VERSION"));
        assert!(!daemon_health.executable_path.is_empty());

        let opened = client
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "codex".to_string(),
                pid: Some(4242),
            })
            .send()
            .await
            .expect("open request")
            .error_for_status()
            .expect("open status")
            .json::<OpenProjectResponse>()
            .await
            .expect("open body");

        let project_health = client
            .get(format!(
                "{base_url}/v1/projects/{}/health",
                opened.project_id
            ))
            .send()
            .await
            .expect("project health request")
            .error_for_status()
            .expect("project health status")
            .json::<ProjectHealth>()
            .await
            .expect("project health body");
        assert_eq!(project_health.project_id, opened.project_id);
        assert_eq!(project_health.session_count, 1);
        assert_eq!(project_health.index_state, "Ready");

        let sessions = client
            .get(format!(
                "{base_url}/v1/projects/{}/sessions",
                opened.project_id
            ))
            .send()
            .await
            .expect("sessions request")
            .error_for_status()
            .expect("sessions status")
            .json::<Vec<SessionSummary>>()
            .await
            .expect("sessions body");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, opened.session_id);
        assert_eq!(sessions[0].client_name, "codex");
        assert_eq!(sessions[0].pid, Some(4242));

        let heartbeat = client
            .post(format!(
                "{base_url}/v1/sessions/{}/heartbeat",
                opened.session_id
            ))
            .send()
            .await
            .expect("heartbeat request")
            .error_for_status()
            .expect("heartbeat status")
            .json::<HeartbeatResponse>()
            .await
            .expect("heartbeat body");
        assert!(heartbeat.known_session);

        let closed = client
            .delete(format!("{base_url}/v1/sessions/{}", opened.session_id))
            .send()
            .await
            .expect("close request")
            .error_for_status()
            .expect("close status")
            .json::<CloseSessionResponse>()
            .await
            .expect("close body");
        assert!(closed.project_removed);
        assert_eq!(closed.remaining_sessions, 0);

        let final_health = client
            .get(format!("{base_url}/health"))
            .send()
            .await
            .expect("final health request")
            .error_for_status()
            .expect("final health status")
            .json::<DaemonHealth>()
            .await
            .expect("final health body");
        assert_eq!(final_health.project_count, 0);
        assert_eq!(final_health.session_count, 0);
        assert_eq!(final_health.daemon_version, env!("CARGO_PKG_VERSION"));
        assert!(!final_health.executable_path.is_empty());

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&daemon_home.path().join(DAEMON_PORT_FILE)).await;
        wait_for_path_absent(&daemon_home.path().join(DAEMON_PID_FILE)).await;
        assert!(
            !daemon_home.path().join(DAEMON_PORT_FILE).exists(),
            "daemon port file should be removed on shutdown"
        );
        assert!(
            !daemon_home.path().join(DAEMON_PID_FILE).exists(),
            "daemon pid file should be removed on shutdown"
        );
    }

    #[tokio::test]
    async fn test_daemon_executes_session_scoped_tool_calls() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("TOKENIZOR_HOME", daemon_home.path());
        let project = project_dir("tokenizor-daemon-tool");
        std::fs::write(project.path().join("src").join("main.rs"), "fn main() {}\n")
            .expect("write source");

        let handle = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let client = reqwest::Client::new();
        let base_url = format!("http://127.0.0.1:{}", handle.port);

        let opened = client
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "codex".to_string(),
                pid: Some(9001),
            })
            .send()
            .await
            .expect("open request")
            .error_for_status()
            .expect("open status")
            .json::<OpenProjectResponse>()
            .await
            .expect("open body");

        let response = client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/get_repo_outline",
                opened.session_id
            ))
            .json(&serde_json::json!({}))
            .send()
            .await
            .expect("tool request");

        assert!(
            response.status().is_success(),
            "tool endpoint should succeed, got {}",
            response.status()
        );

        let body = response.text().await.expect("tool body");
        assert!(
            body.contains("main.rs"),
            "repo outline should include the indexed file, got: {body}"
        );

        let search_files = client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/search_files",
                opened.session_id
            ))
            .json(&serde_json::json!({
                "query": "main.rs",
                "limit": 5
            }))
            .send()
            .await
            .expect("search_files request");

        assert!(
            search_files.status().is_success(),
            "search_files endpoint should succeed, got {}",
            search_files.status()
        );

        let search_files_body = search_files.text().await.expect("search_files body");
        assert!(
            search_files_body.contains("src/main.rs"),
            "search_files should return the indexed file, got: {search_files_body}"
        );

        let resolve_path = client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/resolve_path",
                opened.session_id
            ))
            .json(&serde_json::json!({
                "hint": "main.rs"
            }))
            .send()
            .await
            .expect("resolve_path request");

        assert!(
            resolve_path.status().is_success(),
            "resolve_path endpoint should succeed, got {}",
            resolve_path.status()
        );

        let resolve_path_body = resolve_path.text().await.expect("resolve_path body");
        assert!(
            resolve_path_body.contains("src/main.rs"),
            "resolve_path should return the indexed file, got: {resolve_path_body}"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&daemon_home.path().join(DAEMON_PORT_FILE)).await;
    }

    #[tokio::test]
    async fn test_daemon_port_if_compatible_accepts_matching_identity() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("TOKENIZOR_HOME", daemon_home.path());

        let health = DaemonState::new().health();
        let (port, shutdown_tx) = spawn_fake_health_server(health).await;
        std::fs::write(daemon_home.path().join(DAEMON_PORT_FILE), port.to_string())
            .expect("write daemon port");

        let identity = current_daemon_identity();
        let selected = daemon_port_if_compatible(&identity)
            .await
            .expect("compatible health lookup");

        assert_eq!(selected, Some(port));

        let _ = shutdown_tx.send(());
    }

    #[tokio::test]
    async fn test_daemon_port_if_compatible_rejects_version_mismatch() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("TOKENIZOR_HOME", daemon_home.path());

        let health = DaemonHealth {
            project_count: 0,
            session_count: 0,
            daemon_version: "0.0.0".to_string(),
            executable_path: current_daemon_identity().executable_path,
        };
        let (port, shutdown_tx) = spawn_fake_health_server(health).await;
        std::fs::write(daemon_home.path().join(DAEMON_PORT_FILE), port.to_string())
            .expect("write daemon port");

        let identity = current_daemon_identity();
        let selected = daemon_port_if_compatible(&identity)
            .await
            .expect("mismatch health lookup");

        assert_eq!(selected, None);

        let _ = shutdown_tx.send(());
    }

    #[tokio::test]
    async fn test_stop_incompatible_recorded_daemon_cleans_port_file_without_pid() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("TOKENIZOR_HOME", daemon_home.path());

        let health = DaemonHealth {
            project_count: 0,
            session_count: 0,
            daemon_version: "0.0.0".to_string(),
            executable_path: current_daemon_identity().executable_path,
        };
        let (port, shutdown_tx) = spawn_fake_health_server(health).await;
        std::fs::write(daemon_home.path().join(DAEMON_PORT_FILE), port.to_string())
            .expect("write daemon port");

        stop_incompatible_recorded_daemon(&current_daemon_identity())
            .await
            .expect("stop incompatible daemon");

        assert!(
            !daemon_home.path().join(DAEMON_PORT_FILE).exists(),
            "incompatible daemon port file should be cleared"
        );

        let _ = shutdown_tx.send(());
    }

    #[tokio::test]
    async fn test_daemon_serves_session_scoped_repo_map_hook_endpoint() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("TOKENIZOR_HOME", daemon_home.path());
        let project = project_dir("tokenizor-daemon-hook");
        std::fs::write(project.path().join("src").join("main.rs"), "fn main() {}\n")
            .expect("write source");

        let handle = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let client = reqwest::Client::new();
        let base_url = format!("http://127.0.0.1:{}", handle.port);

        let opened = client
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "claude".to_string(),
                pid: Some(77),
            })
            .send()
            .await
            .expect("open request")
            .error_for_status()
            .expect("open status")
            .json::<OpenProjectResponse>()
            .await
            .expect("open body");

        let response = client
            .get(format!(
                "{base_url}/v1/sessions/{}/sidecar/repo-map",
                opened.session_id
            ))
            .send()
            .await
            .expect("hook request");

        assert!(
            response.status().is_success(),
            "repo-map hook endpoint should succeed, got {}",
            response.status()
        );

        let body = response.text().await.expect("hook body");
        assert!(
            body.contains("Index: 1 files, 1 symbols"),
            "repo-map hook output should come from daemon project instance, got: {body}"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&daemon_home.path().join(DAEMON_PORT_FILE)).await;
    }

    #[tokio::test]
    async fn test_daemon_serves_session_scoped_prompt_context_hook_endpoint() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("TOKENIZOR_HOME", daemon_home.path());
        let project = project_dir("tokenizor-daemon-prompt-hook");
        std::fs::write(project.path().join("src").join("main.rs"), "fn main() {}\n")
            .expect("write source");

        let handle = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let client = reqwest::Client::new();
        let base_url = format!("http://127.0.0.1:{}", handle.port);

        let opened = client
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "claude".to_string(),
                pid: Some(88),
            })
            .send()
            .await
            .expect("open request")
            .error_for_status()
            .expect("open status")
            .json::<OpenProjectResponse>()
            .await
            .expect("open body");

        let response = client
            .get(format!(
                "{base_url}/v1/sessions/{}/sidecar/prompt-context",
                opened.session_id
            ))
            .query(&[("text", "please inspect src/main.rs")])
            .send()
            .await
            .expect("hook request");

        assert!(
            response.status().is_success(),
            "prompt-context hook endpoint should succeed, got {}",
            response.status()
        );

        let body = response.text().await.expect("hook body");
        assert!(
            body.contains("src/main.rs") && body.contains("main"),
            "prompt-context hook output should come from daemon project instance, got: {body}"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&daemon_home.path().join(DAEMON_PORT_FILE)).await;
    }

    #[tokio::test]
    async fn test_index_folder_rebinds_session_to_new_project_root() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("TOKENIZOR_HOME", daemon_home.path());
        let project_a = project_dir("tokenizor-daemon-a");
        let project_b = project_dir("tokenizor-daemon-b");
        std::fs::write(
            project_a.path().join("src").join("old.rs"),
            "fn old_fn() {}\n",
        )
        .expect("write source a");
        std::fs::write(
            project_b.path().join("src").join("new.rs"),
            "fn new_fn() {}\n",
        )
        .expect("write source b");

        let handle = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let client = reqwest::Client::new();
        let base_url = format!("http://127.0.0.1:{}", handle.port);

        let opened = client
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "codex".to_string(),
                pid: Some(55),
            })
            .send()
            .await
            .expect("open request")
            .error_for_status()
            .expect("open status")
            .json::<OpenProjectResponse>()
            .await
            .expect("open body");

        let reload = client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/index_folder",
                opened.session_id
            ))
            .json(&IndexFolderInput {
                path: project_b.path().display().to_string(),
            })
            .send()
            .await
            .expect("index request")
            .error_for_status()
            .expect("index status")
            .text()
            .await
            .expect("index body");
        assert!(
            reload.contains("Indexed"),
            "index_folder should report success, got: {reload}"
        );

        let sessions = client
            .get(format!(
                "{base_url}/v1/projects/{}/sessions",
                project_key(&canonical_project_root(project_b.path()).expect("canonical root"))
            ))
            .send()
            .await
            .expect("session list request")
            .error_for_status()
            .expect("session list status")
            .json::<Vec<SessionSummary>>()
            .await
            .expect("session list body");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, opened.session_id);

        let outline = client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/get_repo_outline",
                opened.session_id
            ))
            .json(&serde_json::json!({}))
            .send()
            .await
            .expect("outline request")
            .error_for_status()
            .expect("outline status")
            .text()
            .await
            .expect("outline body");
        assert!(
            outline.contains("new.rs"),
            "rebound session should see new root: {outline}"
        );
        assert!(
            !outline.contains("old.rs"),
            "rebound session should no longer point at old root: {outline}"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&daemon_home.path().join(DAEMON_PORT_FILE)).await;
    }

    #[tokio::test]
    async fn test_analyze_file_impact_uses_session_project_root_not_process_cwd() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("TOKENIZOR_HOME", daemon_home.path());
        let project = project_dir("tokenizor-daemon-impact-root");
        let outside = TempDir::new().expect("outside cwd");
        let source_path = project.path().join("src").join("lib.rs");
        std::fs::write(&source_path, "pub fn old_name() {}\n").expect("write initial source");

        let handle = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let client = reqwest::Client::new();
        let base_url = format!("http://127.0.0.1:{}", handle.port);

        let opened = client
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "codex".to_string(),
                pid: Some(4242),
            })
            .send()
            .await
            .expect("open request")
            .error_for_status()
            .expect("open status")
            .json::<OpenProjectResponse>()
            .await
            .expect("open body");

        std::fs::write(&source_path, "pub fn new_name() {}\n").expect("write updated source");

        let _cwd_guard = CwdGuard::set(outside.path());

        let impact = client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/analyze_file_impact",
                opened.session_id
            ))
            .json(&serde_json::json!({
                "path": "src/lib.rs"
            }))
            .send()
            .await
            .expect("impact request")
            .error_for_status()
            .expect("impact status")
            .text()
            .await
            .expect("impact body");

        assert!(
            impact.contains("new_name"),
            "impact analysis should read from the session project root, got: {impact}"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&daemon_home.path().join(DAEMON_PORT_FILE)).await;
    }
}
