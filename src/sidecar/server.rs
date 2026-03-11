//! Sidecar server spawner.
//!
//! Binds to an OS-assigned ephemeral port, writes port/PID files,
//! and spawns an axum serve task with graceful shutdown support.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use tokio::net::TcpListener;
use tracing::info;

use super::{SidecarHandle, SidecarState, TokenStats, port_file, router};
use crate::live_index::store::SharedIndex;

/// Spawn the HTTP sidecar.
///
/// 1. Reads `TOKENIZOR_SIDECAR_BIND` env var (default `"127.0.0.1"`).
/// 2. Calls `port_file::check_stale(bind_host)` to clean up any stale files.
/// 3. Binds `TcpListener::bind("{bind_host}:0")` (OS assigns the port).
/// 4. Writes port and PID files via `port_file`.
/// 5. Creates `SidecarState` with `TokenStats` and empty symbol cache.
/// 6. Builds the axum router via `router::build_router`.
/// 7. Spawns `axum::serve` with graceful shutdown wired to a oneshot channel.
/// 8. After the server completes, calls `port_file::cleanup_files()`.
/// 9. Returns `SidecarHandle { port, shutdown_tx }`.
pub async fn spawn_sidecar(index: SharedIndex, bind_host: &str) -> anyhow::Result<SidecarHandle> {
    // Allow overriding bind host via env var.
    let resolved_host =
        std::env::var("TOKENIZOR_SIDECAR_BIND").unwrap_or_else(|_| bind_host.to_string());

    // Clean up stale files from a previous crashed sidecar.
    port_file::check_stale(&resolved_host);
    // Ensure local sidecar mode does not inherit a daemon session routing file.
    port_file::cleanup_session_file();

    // Bind to an OS-assigned ephemeral port.
    let addr = format!("{resolved_host}:0");
    let listener = TcpListener::bind(&addr).await?;
    let port = listener.local_addr()?.port();

    // Write port and PID files so hook scripts can locate the sidecar.
    port_file::write_port_file(port)?;
    port_file::write_pid_file(std::process::id())?;

    info!("sidecar listening on {resolved_host}:{port}");

    // Construct SidecarState with fresh TokenStats and empty symbol cache.
    // Keep a clone of the Arc<TokenStats> to return in SidecarHandle so the MCP server
    // can read token savings directly without an HTTP round-trip.
    let token_stats = TokenStats::new();
    let state = SidecarState {
        index,
        token_stats: Arc::clone(&token_stats),
        repo_root: std::env::current_dir().ok(),
        symbol_cache: Arc::new(RwLock::new(HashMap::new())),
    };

    // Build the router with SidecarState.
    let app = router::build_router(state);

    // Create graceful shutdown channel.
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    // Spawn the server task.
    tokio::spawn(async move {
        let shutdown_signal = async move {
            let _ = shutdown_rx.await;
        };

        if let Err(e) = axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal)
            .await
        {
            tracing::error!("sidecar server error: {e}");
        }

        // Clean up port/PID files after shutdown.
        port_file::cleanup_files();
        tracing::info!("sidecar shut down, port/PID files cleaned up");
    });

    Ok(SidecarHandle {
        port,
        shutdown_tx,
        token_stats,
    })
}
