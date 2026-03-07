use std::collections::BTreeMap;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::config::{
    ControlPlaneBackend, ControlPlaneConfig, SUPPORTED_SPACETIMEDB_SCHEMA_VERSION,
    SpacetimeDbConfig,
};
use crate::domain::{
    Checkpoint, ComponentHealth, HealthIssueCategory, IdempotencyRecord, IndexRun, Repository,
};
use crate::error::{Result, TokenizorError};

pub trait ControlPlane: Send + Sync {
    fn backend_name(&self) -> &'static str;
    fn health_check(&self) -> Result<ComponentHealth>;
    fn deployment_checks(&self) -> Result<Vec<ComponentHealth>>;
    fn upsert_repository(&self, repository: Repository) -> Result<()>;
    fn create_index_run(&self, run: IndexRun) -> Result<()>;
    fn write_checkpoint(&self, checkpoint: Checkpoint) -> Result<()>;
    fn put_idempotency_record(&self, record: IdempotencyRecord) -> Result<()>;
}

pub fn build_control_plane(config: &ControlPlaneConfig) -> Result<Arc<dyn ControlPlane>> {
    match config.backend {
        ControlPlaneBackend::InMemory => Ok(Arc::new(InMemoryControlPlane::default())),
        ControlPlaneBackend::SpacetimeDb => Ok(Arc::new(SpacetimeControlPlane::new(
            config.spacetimedb.clone(),
        ))),
    }
}

#[derive(Default)]
struct InMemoryState {
    repositories: BTreeMap<String, Repository>,
    runs: BTreeMap<String, IndexRun>,
    checkpoints: Vec<Checkpoint>,
    idempotency_records: BTreeMap<String, IdempotencyRecord>,
}

#[derive(Default)]
pub struct InMemoryControlPlane {
    state: Mutex<InMemoryState>,
}

impl ControlPlane for InMemoryControlPlane {
    fn backend_name(&self) -> &'static str {
        "in_memory"
    }

    fn health_check(&self) -> Result<ComponentHealth> {
        Ok(ComponentHealth::ok(
            "control_plane",
            HealthIssueCategory::Configuration,
            "using the in-memory control plane configured for tests or disposable local sessions",
        ))
    }

    fn deployment_checks(&self) -> Result<Vec<ComponentHealth>> {
        Ok(vec![ComponentHealth::warning(
            "control_plane_backend",
            HealthIssueCategory::Configuration,
            "the configured control plane backend is in-memory, so run metadata will not be durable and repository/workspace registration remains in the local bootstrap registry",
            "Set TOKENIZOR_CONTROL_PLANE_BACKEND=spacetimedb when you need authoritative durable control-plane state.",
        )])
    }

    fn upsert_repository(&self, repository: Repository) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| {
            TokenizorError::ControlPlane("in-memory control plane lock poisoned".into())
        })?;
        state
            .repositories
            .insert(repository.repo_id.clone(), repository);
        Ok(())
    }

    fn create_index_run(&self, run: IndexRun) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| {
            TokenizorError::ControlPlane("in-memory control plane lock poisoned".into())
        })?;
        state.runs.insert(run.run_id.clone(), run);
        Ok(())
    }

    fn write_checkpoint(&self, checkpoint: Checkpoint) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| {
            TokenizorError::ControlPlane("in-memory control plane lock poisoned".into())
        })?;
        state.checkpoints.push(checkpoint);
        Ok(())
    }

    fn put_idempotency_record(&self, record: IdempotencyRecord) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| {
            TokenizorError::ControlPlane("in-memory control plane lock poisoned".into())
        })?;
        state.idempotency_records.insert(
            format!("{}::{}", record.operation, record.idempotency_key),
            record,
        );
        Ok(())
    }
}

trait SpacetimeRuntimeProbe: Send + Sync {
    fn cli_available(&self, cli_path: &str) -> Result<bool>;
    fn endpoint_reachable(&self, endpoint: &str, timeout: Duration) -> Result<bool>;
    fn path_exists(&self, path: &Path) -> bool;
}

#[derive(Default)]
struct SystemSpacetimeRuntimeProbe;

impl SpacetimeRuntimeProbe for SystemSpacetimeRuntimeProbe {
    fn cli_available(&self, cli_path: &str) -> Result<bool> {
        match Command::new(cli_path)
            .arg("--help")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
        {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(TokenizorError::ControlPlane(format!(
                "failed to invoke `{cli_path}`: {error}"
            ))),
        }
    }

    fn endpoint_reachable(&self, endpoint: &str, timeout: Duration) -> Result<bool> {
        let authority = authority_from_endpoint(endpoint)?;
        let addresses = authority.to_socket_addrs().map_err(|error| {
            TokenizorError::Config(format!(
                "invalid SpacetimeDB endpoint `{endpoint}`: {error}"
            ))
        })?;

        for address in addresses {
            if TcpStream::connect_timeout(&address, timeout).is_ok() {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn path_exists(&self, path: &Path) -> bool {
        path.exists()
    }
}

pub struct SpacetimeControlPlane {
    config: SpacetimeDbConfig,
    runtime_probe: Arc<dyn SpacetimeRuntimeProbe>,
}

impl SpacetimeControlPlane {
    pub fn new(config: SpacetimeDbConfig) -> Self {
        Self {
            config,
            runtime_probe: Arc::new(SystemSpacetimeRuntimeProbe),
        }
    }

    fn pending_write_error(&self) -> TokenizorError {
        TokenizorError::ControlPlane(
            "SpacetimeDB persistence is not wired in this slice yet; only deployment and health checks are active"
                .into(),
        )
    }

    fn cli_check(&self) -> ComponentHealth {
        if self.config.cli_path.trim().is_empty() {
            return ComponentHealth::error(
                "spacetimedb_cli",
                HealthIssueCategory::Configuration,
                "SpacetimeDB CLI path is empty",
                "Set TOKENIZOR_SPACETIMEDB_CLI to the SpacetimeDB CLI binary name or absolute path.",
            );
        }

        match self.runtime_probe.cli_available(&self.config.cli_path) {
            Ok(true) => ComponentHealth::ok(
                "spacetimedb_cli",
                HealthIssueCategory::Dependency,
                format!(
                    "`{}` is available for operator commands",
                    self.config.cli_path
                ),
            ),
            Ok(false) => ComponentHealth::error(
                "spacetimedb_cli",
                HealthIssueCategory::Dependency,
                format!(
                    "`{}` is not installed or not available on PATH",
                    self.config.cli_path
                ),
                "Install the SpacetimeDB CLI and ensure the configured path resolves before running doctor or init.",
            ),
            Err(error) => ComponentHealth::error(
                "spacetimedb_cli",
                HealthIssueCategory::Dependency,
                error.to_string(),
                "Verify the configured CLI path points to an executable SpacetimeDB binary.",
            ),
        }
    }

    fn endpoint_check(&self, component_name: &str) -> ComponentHealth {
        if self.config.endpoint.trim().is_empty() {
            return ComponentHealth::error(
                component_name,
                HealthIssueCategory::Configuration,
                "SpacetimeDB endpoint is empty",
                "Set TOKENIZOR_SPACETIMEDB_ENDPOINT to the local SpacetimeDB HTTP endpoint, such as http://127.0.0.1:3007.",
            );
        }

        match self
            .runtime_probe
            .endpoint_reachable(&self.config.endpoint, Duration::from_millis(500))
        {
            Ok(true) => ComponentHealth::ok(
                component_name,
                HealthIssueCategory::Dependency,
                format!("SpacetimeDB endpoint {} is reachable", self.config.endpoint),
            ),
            Ok(false) => ComponentHealth::error(
                component_name,
                HealthIssueCategory::Dependency,
                format!(
                    "SpacetimeDB endpoint {} is not reachable",
                    self.config.endpoint
                ),
                "Start the local SpacetimeDB runtime or correct TOKENIZOR_SPACETIMEDB_ENDPOINT before retrying.",
            ),
            Err(TokenizorError::Config(detail)) => ComponentHealth::error(
                component_name,
                HealthIssueCategory::Configuration,
                detail,
                "Fix TOKENIZOR_SPACETIMEDB_ENDPOINT so it contains a valid host and optional port.",
            ),
            Err(error) => ComponentHealth::error(
                component_name,
                HealthIssueCategory::Dependency,
                error.to_string(),
                "Verify the local SpacetimeDB runtime is reachable and the endpoint is resolvable from this machine.",
            ),
        }
    }

    fn database_check(&self) -> ComponentHealth {
        if self.config.database.trim().is_empty() {
            ComponentHealth::error(
                "spacetimedb_database",
                HealthIssueCategory::Configuration,
                "SpacetimeDB database name is empty",
                "Set TOKENIZOR_SPACETIMEDB_DATABASE to the target database name before running doctor or init.",
            )
        } else {
            ComponentHealth::ok(
                "spacetimedb_database",
                HealthIssueCategory::Configuration,
                format!("database `{}` is configured", self.config.database),
            )
        }
    }

    fn module_path_check(&self) -> ComponentHealth {
        if self.config.module_path.as_os_str().is_empty() {
            return ComponentHealth::error(
                "spacetimedb_module_path",
                HealthIssueCategory::Configuration,
                "SpacetimeDB module path is empty",
                "Set TOKENIZOR_SPACETIMEDB_MODULE_PATH to the local module directory before running bootstrap flows.",
            );
        }

        if self.runtime_probe.path_exists(&self.config.module_path) {
            ComponentHealth::ok(
                "spacetimedb_module_path",
                HealthIssueCategory::Bootstrap,
                format!(
                    "module path {} is present",
                    self.config.module_path.display()
                ),
            )
        } else {
            ComponentHealth::error(
                "spacetimedb_module_path",
                HealthIssueCategory::Bootstrap,
                format!(
                    "module path {} does not exist",
                    self.config.module_path.display()
                ),
                "Build or place the SpacetimeDB module at the configured path, or update TOKENIZOR_SPACETIMEDB_MODULE_PATH.",
            )
        }
    }

    fn schema_compatibility_check(&self) -> ComponentHealth {
        if self.config.schema_version != SUPPORTED_SPACETIMEDB_SCHEMA_VERSION {
            return ComponentHealth::error(
                "spacetimedb_schema_compatibility",
                HealthIssueCategory::Compatibility,
                format!(
                    "configured schema version {} does not match Tokenizor's supported schema version {}",
                    self.config.schema_version, SUPPORTED_SPACETIMEDB_SCHEMA_VERSION
                ),
                format!(
                    "Set TOKENIZOR_SPACETIMEDB_SCHEMA_VERSION={} or upgrade Tokenizor to a build that supports schema version {}.",
                    SUPPORTED_SPACETIMEDB_SCHEMA_VERSION, self.config.schema_version
                ),
            );
        }

        ComponentHealth::warning(
            "spacetimedb_schema_compatibility",
            HealthIssueCategory::Compatibility,
            format!(
                "configured schema version {} matches Tokenizor's current expectation, but doctor cannot yet verify the published module schema",
                self.config.schema_version
            ),
            "Treat this as an operator warning only; if startup still fails later, re-run doctor after the compatibility probe is expanded.",
        )
    }
}

impl ControlPlane for SpacetimeControlPlane {
    fn backend_name(&self) -> &'static str {
        "spacetimedb"
    }

    fn health_check(&self) -> Result<ComponentHealth> {
        Ok(self.endpoint_check("control_plane"))
    }

    fn deployment_checks(&self) -> Result<Vec<ComponentHealth>> {
        Ok(vec![
            self.database_check(),
            self.schema_compatibility_check(),
            self.cli_check(),
            self.endpoint_check("spacetimedb_endpoint"),
            self.module_path_check(),
        ])
    }

    fn upsert_repository(&self, _repository: Repository) -> Result<()> {
        Err(self.pending_write_error())
    }

    fn create_index_run(&self, _run: IndexRun) -> Result<()> {
        Err(self.pending_write_error())
    }

    fn write_checkpoint(&self, _checkpoint: Checkpoint) -> Result<()> {
        Err(self.pending_write_error())
    }

    fn put_idempotency_record(&self, _record: IdempotencyRecord) -> Result<()> {
        Err(self.pending_write_error())
    }
}

fn authority_from_endpoint(endpoint: &str) -> Result<String> {
    let without_scheme = endpoint.split("://").nth(1).unwrap_or(endpoint);
    let authority = without_scheme.split('/').next().unwrap_or_default().trim();

    if authority.is_empty() {
        return Err(TokenizorError::Config(format!(
            "SpacetimeDB endpoint `{endpoint}` is missing a host"
        )));
    }

    if authority.contains(':') {
        Ok(authority.to_string())
    } else if endpoint.starts_with("https://") {
        Ok(format!("{authority}:443"))
    } else {
        Ok(format!("{authority}:80"))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ControlPlane, SpacetimeControlPlane, SpacetimeRuntimeProbe, authority_from_endpoint,
    };
    use crate::config::{SUPPORTED_SPACETIMEDB_SCHEMA_VERSION, SpacetimeDbConfig};
    use crate::domain::{ComponentHealth, HealthIssueCategory, HealthSeverity, HealthStatus};
    use crate::error::{Result, TokenizorError};
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::Duration;

    struct FakeProbe {
        cli_available: bool,
        cli_error: Option<String>,
        endpoint_reachable: bool,
        endpoint_config_error: Option<String>,
        endpoint_probe_error: Option<String>,
        existing_paths: HashSet<PathBuf>,
    }

    impl Default for FakeProbe {
        fn default() -> Self {
            let mut existing_paths = HashSet::new();
            existing_paths.insert(PathBuf::from("spacetime/tokenizor"));

            Self {
                cli_available: true,
                cli_error: None,
                endpoint_reachable: true,
                endpoint_config_error: None,
                endpoint_probe_error: None,
                existing_paths,
            }
        }
    }

    impl SpacetimeRuntimeProbe for FakeProbe {
        fn cli_available(&self, _cli_path: &str) -> Result<bool> {
            if let Some(message) = &self.cli_error {
                Err(TokenizorError::ControlPlane(message.clone()))
            } else {
                Ok(self.cli_available)
            }
        }

        fn endpoint_reachable(&self, _endpoint: &str, _timeout: Duration) -> Result<bool> {
            if let Some(message) = &self.endpoint_config_error {
                Err(TokenizorError::Config(message.clone()))
            } else if let Some(message) = &self.endpoint_probe_error {
                Err(TokenizorError::ControlPlane(message.clone()))
            } else {
                Ok(self.endpoint_reachable)
            }
        }

        fn path_exists(&self, path: &Path) -> bool {
            self.existing_paths.contains(path)
        }
    }

    fn base_config() -> SpacetimeDbConfig {
        SpacetimeDbConfig {
            cli_path: "spacetimedb".to_string(),
            endpoint: "http://127.0.0.1:3007".to_string(),
            database: "tokenizor".to_string(),
            module_path: PathBuf::from("spacetime/tokenizor"),
            schema_version: SUPPORTED_SPACETIMEDB_SCHEMA_VERSION,
        }
    }

    fn control_plane_with_probe(
        config: SpacetimeDbConfig,
        probe: FakeProbe,
    ) -> SpacetimeControlPlane {
        SpacetimeControlPlane {
            config,
            runtime_probe: Arc::new(probe),
        }
    }

    fn find_check<'a>(checks: &'a [ComponentHealth], name: &str) -> &'a ComponentHealth {
        checks
            .iter()
            .find(|check| check.name == name)
            .expect("expected check to be present")
    }

    #[test]
    fn derives_default_http_port() {
        assert_eq!(
            authority_from_endpoint("http://127.0.0.1").expect("authority should parse"),
            "127.0.0.1:80"
        );
    }

    #[test]
    fn preserves_explicit_port() {
        assert_eq!(
            authority_from_endpoint("http://127.0.0.1:3007/v1").expect("authority should parse"),
            "127.0.0.1:3007"
        );
    }

    #[test]
    fn reports_missing_cli_as_dependency_error() {
        let probe = FakeProbe {
            cli_available: false,
            ..FakeProbe::default()
        };
        let control_plane = control_plane_with_probe(base_config(), probe);

        let checks = control_plane
            .deployment_checks()
            .expect("deployment checks should succeed");
        let cli = find_check(&checks, "spacetimedb_cli");

        assert_eq!(cli.status, HealthStatus::Unavailable);
        assert_eq!(cli.category, HealthIssueCategory::Dependency);
        assert_eq!(cli.severity, HealthSeverity::Error);
        assert!(
            cli.remediation
                .as_deref()
                .expect("remediation should be present")
                .contains("Install the SpacetimeDB CLI")
        );
    }

    #[test]
    fn reports_unreachable_endpoint_as_dependency_error() {
        let probe = FakeProbe {
            endpoint_reachable: false,
            ..FakeProbe::default()
        };
        let control_plane = control_plane_with_probe(base_config(), probe);

        let checks = control_plane
            .deployment_checks()
            .expect("deployment checks should succeed");
        let endpoint = find_check(&checks, "spacetimedb_endpoint");

        assert_eq!(endpoint.status, HealthStatus::Unavailable);
        assert_eq!(endpoint.category, HealthIssueCategory::Dependency);
        assert_eq!(endpoint.severity, HealthSeverity::Error);
        assert!(
            endpoint
                .remediation
                .as_deref()
                .expect("remediation should be present")
                .contains("Start the local SpacetimeDB runtime")
        );
    }

    #[test]
    fn reports_empty_database_as_configuration_error() {
        let mut config = base_config();
        config.database.clear();
        let control_plane = control_plane_with_probe(config, FakeProbe::default());

        let checks = control_plane
            .deployment_checks()
            .expect("deployment checks should succeed");
        let database = find_check(&checks, "spacetimedb_database");

        assert_eq!(database.status, HealthStatus::Unavailable);
        assert_eq!(database.category, HealthIssueCategory::Configuration);
        assert_eq!(database.severity, HealthSeverity::Error);
    }

    #[test]
    fn reports_missing_module_path_as_bootstrap_error() {
        let control_plane = control_plane_with_probe(
            base_config(),
            FakeProbe {
                existing_paths: HashSet::new(),
                ..FakeProbe::default()
            },
        );

        let checks = control_plane
            .deployment_checks()
            .expect("deployment checks should succeed");
        let module_path = find_check(&checks, "spacetimedb_module_path");

        assert_eq!(module_path.status, HealthStatus::Unavailable);
        assert_eq!(module_path.category, HealthIssueCategory::Bootstrap);
        assert_eq!(module_path.severity, HealthSeverity::Error);
    }

    #[test]
    fn reports_schema_version_mismatch_as_compatibility_error() {
        let mut config = base_config();
        config.schema_version = SUPPORTED_SPACETIMEDB_SCHEMA_VERSION + 1;
        let control_plane = control_plane_with_probe(config, FakeProbe::default());

        let checks = control_plane
            .deployment_checks()
            .expect("deployment checks should succeed");
        let compatibility = find_check(&checks, "spacetimedb_schema_compatibility");

        assert_eq!(compatibility.status, HealthStatus::Unavailable);
        assert_eq!(compatibility.category, HealthIssueCategory::Compatibility);
        assert_eq!(compatibility.severity, HealthSeverity::Error);
    }

    #[test]
    fn reports_schema_verification_gap_as_non_blocking_warning() {
        let control_plane = control_plane_with_probe(base_config(), FakeProbe::default());

        let checks = control_plane
            .deployment_checks()
            .expect("deployment checks should succeed");
        let compatibility = find_check(&checks, "spacetimedb_schema_compatibility");

        assert_eq!(compatibility.status, HealthStatus::Degraded);
        assert_eq!(compatibility.category, HealthIssueCategory::Compatibility);
        assert_eq!(compatibility.severity, HealthSeverity::Warning);
        assert!(
            compatibility
                .remediation
                .as_deref()
                .expect("remediation should be present")
                .contains("operator warning")
        );
    }

    #[test]
    fn reports_configuration_derived_findings_before_runtime_probe_findings() {
        let control_plane = control_plane_with_probe(
            base_config(),
            FakeProbe {
                cli_available: false,
                endpoint_reachable: false,
                existing_paths: HashSet::new(),
                ..FakeProbe::default()
            },
        );

        let checks = control_plane
            .deployment_checks()
            .expect("deployment checks should succeed");
        let names = checks
            .iter()
            .map(|check| check.name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "spacetimedb_database",
                "spacetimedb_schema_compatibility",
                "spacetimedb_cli",
                "spacetimedb_endpoint",
                "spacetimedb_module_path",
            ]
        );
    }
}
