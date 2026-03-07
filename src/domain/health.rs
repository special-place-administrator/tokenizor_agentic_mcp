use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Ok,
    Degraded,
    Unavailable,
}

impl HealthStatus {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ok)
    }

    fn severity(&self) -> u8 {
        match self {
            Self::Ok => 0,
            Self::Degraded => 1,
            Self::Unavailable => 2,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HealthIssueCategory {
    Bootstrap,
    Dependency,
    Configuration,
    Compatibility,
    Storage,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HealthSeverity {
    Info,
    Warning,
    Error,
}

impl HealthSeverity {
    pub fn blocks_readiness(&self) -> bool {
        matches!(self, Self::Error)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComponentHealth {
    pub name: String,
    pub category: HealthIssueCategory,
    pub status: HealthStatus,
    pub severity: HealthSeverity,
    pub detail: String,
    pub remediation: Option<String>,
    pub observed_at_unix_ms: u64,
}

impl ComponentHealth {
    pub fn ok(
        name: impl Into<String>,
        category: HealthIssueCategory,
        detail: impl Into<String>,
    ) -> Self {
        Self::new(
            name,
            category,
            HealthStatus::Ok,
            HealthSeverity::Info,
            detail,
            None::<String>,
        )
    }

    pub fn warning(
        name: impl Into<String>,
        category: HealthIssueCategory,
        detail: impl Into<String>,
        remediation: impl Into<String>,
    ) -> Self {
        Self::new(
            name,
            category,
            HealthStatus::Degraded,
            HealthSeverity::Warning,
            detail,
            Some(remediation.into()),
        )
    }

    pub fn error(
        name: impl Into<String>,
        category: HealthIssueCategory,
        detail: impl Into<String>,
        remediation: impl Into<String>,
    ) -> Self {
        Self::new(
            name,
            category,
            HealthStatus::Unavailable,
            HealthSeverity::Error,
            detail,
            Some(remediation.into()),
        )
    }

    pub fn blocks_readiness(&self) -> bool {
        !self.status.is_ready() && self.severity.blocks_readiness()
    }

    fn new(
        name: impl Into<String>,
        category: HealthIssueCategory,
        status: HealthStatus,
        severity: HealthSeverity,
        detail: impl Into<String>,
        remediation: Option<String>,
    ) -> Self {
        Self {
            name: name.into(),
            category,
            status,
            severity,
            detail: detail.into(),
            remediation,
            observed_at_unix_ms: unix_timestamp_ms(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceIdentity {
    pub name: String,
    pub version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HealthReport {
    pub checked_at_unix_ms: u64,
    pub service: ServiceIdentity,
    pub overall_status: HealthStatus,
    pub components: Vec<ComponentHealth>,
}

impl HealthReport {
    pub fn new(service: ServiceIdentity, components: Vec<ComponentHealth>) -> Self {
        Self {
            checked_at_unix_ms: unix_timestamp_ms(),
            overall_status: aggregate_status(&components),
            service,
            components,
        }
    }

    pub fn summary(&self) -> String {
        let failing = self
            .components
            .iter()
            .filter(|component| !component.status.is_ready())
            .map(|component| {
                format!(
                    "{}={:?}/{:?}",
                    component.name, component.status, component.severity
                )
            })
            .collect::<Vec<_>>();

        if failing.is_empty() {
            "all components ready".to_string()
        } else {
            failing.join(", ")
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeploymentReport {
    pub checked_at_unix_ms: u64,
    pub overall_status: HealthStatus,
    pub ready_for_run: bool,
    pub control_plane_backend: String,
    pub blob_root: PathBuf,
    pub checks: Vec<ComponentHealth>,
}

impl DeploymentReport {
    pub fn new(
        control_plane_backend: impl Into<String>,
        blob_root: PathBuf,
        checks: Vec<ComponentHealth>,
    ) -> Self {
        let ready_for_run = checks.iter().all(|check| !check.blocks_readiness());

        Self {
            checked_at_unix_ms: unix_timestamp_ms(),
            overall_status: aggregate_status(&checks),
            ready_for_run,
            control_plane_backend: control_plane_backend.into(),
            blob_root,
            checks,
        }
    }

    pub fn is_ready(&self) -> bool {
        self.ready_for_run
    }

    pub fn blocking_checks(&self) -> impl Iterator<Item = &ComponentHealth> {
        self.checks.iter().filter(|check| check.blocks_readiness())
    }

    pub fn blocking_summary(&self) -> String {
        let blocking = self
            .blocking_checks()
            .map(|check| match &check.remediation {
                Some(remediation) => format!(
                    "{} [{:?}/{:?}]: {} Remediation: {}",
                    check.name, check.category, check.severity, check.detail, remediation
                ),
                None => format!(
                    "{} [{:?}/{:?}]: {}",
                    check.name, check.category, check.severity, check.detail
                ),
            })
            .collect::<Vec<_>>();

        if blocking.is_empty() {
            "all deployment prerequisites are satisfied".to_string()
        } else {
            blocking.join("; ")
        }
    }
}

pub fn aggregate_status(checks: &[ComponentHealth]) -> HealthStatus {
    checks.iter().fold(HealthStatus::Ok, |current, check| {
        if check.status.severity() > current.severity() {
            check.status.clone()
        } else {
            current
        }
    })
}

pub fn unix_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        ComponentHealth, DeploymentReport, HealthIssueCategory, HealthSeverity, HealthStatus,
    };

    #[test]
    fn warnings_do_not_block_readiness() {
        let report = DeploymentReport::new(
            "spacetimedb",
            PathBuf::from(".tokenizor"),
            vec![ComponentHealth::warning(
                "spacetimedb_schema_compatibility",
                HealthIssueCategory::Compatibility,
                "schema compatibility is not fully verified yet",
                "Run `tokenizor_agentic_mcp doctor` after the compatibility probe is implemented.",
            )],
        );

        assert_eq!(report.overall_status, HealthStatus::Degraded);
        assert!(report.is_ready());
        assert_eq!(report.blocking_checks().count(), 0);
    }

    #[test]
    fn errors_block_readiness_and_preserve_remediation() {
        let report = DeploymentReport::new(
            "spacetimedb",
            PathBuf::from(".tokenizor"),
            vec![ComponentHealth::error(
                "spacetimedb_cli",
                HealthIssueCategory::Dependency,
                "`spacetimedb` is not installed",
                "Install the SpacetimeDB CLI and ensure it is on PATH.",
            )],
        );

        let blocking = report.blocking_checks().collect::<Vec<_>>();

        assert!(!report.is_ready());
        assert_eq!(blocking.len(), 1);
        assert_eq!(blocking[0].severity, HealthSeverity::Error);
        assert_eq!(
            blocking[0].remediation.as_deref(),
            Some("Install the SpacetimeDB CLI and ensure it is on PATH.")
        );
        assert!(
            report
                .blocking_summary()
                .contains("Remediation: Install the SpacetimeDB CLI")
        );
    }

    #[test]
    fn serializes_machine_readable_metadata() {
        let check = ComponentHealth::error(
            "blob_store",
            HealthIssueCategory::Storage,
            "local CAS layout is missing required directories",
            "Run `tokenizor_agentic_mcp init` to create the CAS layout.",
        );

        let value = serde_json::to_value(&check).expect("component health should serialize");

        assert_eq!(value["category"], "storage");
        assert_eq!(value["status"], "unavailable");
        assert_eq!(value["severity"], "error");
        assert_eq!(
            value["remediation"],
            "Run `tokenizor_agentic_mcp init` to create the CAS layout."
        );
    }
}
