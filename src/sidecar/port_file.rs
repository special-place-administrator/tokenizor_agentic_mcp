//! Port and PID file management for the HTTP sidecar.
//!
//! All files live under `.tokenizor/` in the current working directory.
//! The hook binary reads `sidecar.port` to locate the running sidecar.

use std::io::{self, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::Duration;

const DIR_NAME: &str = ".tokenizor";
const PORT_FILE: &str = "sidecar.port";
const PID_FILE: &str = "sidecar.pid";

/// Ensure `.tokenizor/` exists in the current working directory.
/// Creates the directory if it doesn't exist. Returns its path.
pub fn ensure_tokenizor_dir() -> io::Result<PathBuf> {
    let dir = PathBuf::from(DIR_NAME);
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

/// Write the sidecar port to `.tokenizor/sidecar.port`.
///
/// The file contains ONLY the port number as ASCII digits, no trailing newline.
/// This is the convention the hook binary relies on.
pub fn write_port_file(port: u16) -> io::Result<()> {
    let dir = ensure_tokenizor_dir()?;
    let path = dir.join(PORT_FILE);
    let mut file = std::fs::File::create(&path)?;
    write!(file, "{port}")?;
    Ok(())
}

/// Write the sidecar PID to `.tokenizor/sidecar.pid`.
///
/// The file contains ONLY the PID as ASCII digits, no trailing newline.
pub fn write_pid_file(pid: u32) -> io::Result<()> {
    let dir = ensure_tokenizor_dir()?;
    let path = dir.join(PID_FILE);
    let mut file = std::fs::File::create(&path)?;
    write!(file, "{pid}")?;
    Ok(())
}

/// Read and parse the port from `.tokenizor/sidecar.port`.
///
/// Returns an error if the file doesn't exist or contains invalid data.
pub fn read_port() -> io::Result<u16> {
    let path = PathBuf::from(DIR_NAME).join(PORT_FILE);
    let contents = std::fs::read_to_string(&path)?;
    contents
        .trim()
        .parse::<u16>()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Remove both port and PID files. Ignores all errors.
///
/// Called during sidecar shutdown — it is safe to call even if files don't exist.
pub fn cleanup_files() {
    let dir = PathBuf::from(DIR_NAME);
    let _ = std::fs::remove_file(dir.join(PORT_FILE));
    let _ = std::fs::remove_file(dir.join(PID_FILE));
}

/// Check whether the port/PID files are stale (i.e., the old sidecar is no longer running).
///
/// If no port file exists, there is nothing stale — returns `false`.
/// If a port file exists, attempts a blocking TCP connect to `{bind_host}:{port}` with a
/// 200 ms timeout. If the connection succeeds the sidecar is alive and returns `false`.
/// If the connection is refused or times out, the files are stale: calls `cleanup_files()`
/// and returns `true`.
pub fn check_stale(bind_host: &str) -> bool {
    let port = match read_port() {
        Ok(p) => p,
        Err(_) => return false, // No port file — nothing to clean up.
    };

    let addr = format!("{bind_host}:{port}");
    match TcpStream::connect_timeout(&addr.parse().unwrap_or_else(|_| "127.0.0.1:0".parse().unwrap()), Duration::from_millis(200)) {
        Ok(_) => false, // Connection succeeded — sidecar is still alive.
        Err(_) => {
            // Connection refused or timed out — files are stale.
            cleanup_files();
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tempfile::TempDir;

    /// Serialize all cwd-manipulating tests so they don't interfere with each other.
    /// cwd is process-global — parallel manipulation causes flaky failures.
    static CWD_LOCK: Mutex<()> = Mutex::new(());

    /// Run a test with cwd set to a temp directory so file operations are isolated.
    /// Holds `CWD_LOCK` for the duration, restores cwd on exit (even on panic).
    fn with_temp_dir<F: FnOnce() + std::panic::UnwindSafe>(f: F) {
        let _guard = CWD_LOCK.lock().unwrap();
        let tmp = TempDir::new().unwrap();
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();
        let result = std::panic::catch_unwind(f);
        std::env::set_current_dir(&original).unwrap();
        drop(tmp);
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_write_read_port_roundtrip() {
        with_temp_dir(|| {
            write_port_file(12345).expect("write_port_file should succeed");
            let port = read_port().expect("read_port should succeed after write");
            assert_eq!(port, 12345, "port roundtrip must preserve value");
        });
    }

    #[test]
    fn test_write_port_file_no_trailing_newline() {
        with_temp_dir(|| {
            write_port_file(8080).expect("write_port_file should succeed");
            // Read while still inside the temp cwd so the relative path resolves correctly.
            let port_path = PathBuf::from(DIR_NAME).join(PORT_FILE);
            let bytes = std::fs::read(&port_path).unwrap();
            assert_eq!(bytes, b"8080", "port file must contain ONLY the digits, no newline");
        });
    }

    #[test]
    fn test_cleanup_removes_files() {
        with_temp_dir(|| {
            write_port_file(9000).expect("write should succeed");
            write_pid_file(12345).expect("write should succeed");

            let dir = PathBuf::from(DIR_NAME);
            assert!(dir.join(PORT_FILE).exists(), "port file should exist before cleanup");
            assert!(dir.join(PID_FILE).exists(), "pid file should exist before cleanup");

            cleanup_files();

            assert!(!dir.join(PORT_FILE).exists(), "port file should be gone after cleanup");
            assert!(!dir.join(PID_FILE).exists(), "pid file should be gone after cleanup");
        });
    }

    #[test]
    fn test_cleanup_is_noop_when_no_files() {
        with_temp_dir(|| {
            // Should not panic even if files don't exist.
            cleanup_files();
        });
    }

    #[test]
    fn test_read_port_missing_returns_error() {
        with_temp_dir(|| {
            let result = read_port();
            assert!(result.is_err(), "read_port should return error when file is missing");
        });
    }

    #[test]
    fn test_ensure_tokenizor_dir_creates_directory() {
        with_temp_dir(|| {
            let dir = ensure_tokenizor_dir().expect("ensure_tokenizor_dir should succeed");
            assert!(dir.exists(), ".tokenizor directory should exist after ensure_tokenizor_dir");
            assert!(dir.is_dir(), "path should be a directory");
        });
    }

    #[test]
    fn test_ensure_tokenizor_dir_idempotent() {
        with_temp_dir(|| {
            ensure_tokenizor_dir().expect("first call should succeed");
            ensure_tokenizor_dir().expect("second call should also succeed (idempotent)");
        });
    }

    #[test]
    fn test_check_stale_returns_false_when_no_port_file() {
        with_temp_dir(|| {
            let is_stale = check_stale("127.0.0.1");
            assert!(!is_stale, "no port file means nothing is stale");
        });
    }

    #[test]
    fn test_check_stale_cleans_up_when_port_is_closed() {
        with_temp_dir(|| {
            // Write a port that is very unlikely to have anything listening.
            write_port_file(19999).expect("write should succeed");
            write_pid_file(99999).expect("write should succeed");

            let is_stale = check_stale("127.0.0.1");
            assert!(is_stale, "port 19999 should be detected as stale (nothing listening)");

            // Cleanup should have been called.
            let dir = PathBuf::from(DIR_NAME);
            assert!(!dir.join(PORT_FILE).exists(), "port file cleaned up after stale detection");
        });
    }
}
