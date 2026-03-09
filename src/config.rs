use std::{env, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Result, TokenizorError};

const DEFAULT_BLOB_ROOT: &str = ".tokenizor";
const DEFAULT_SPACETIMEDB_CLI: &str = "spacetimedb";
const DEFAULT_SPACETIMEDB_ENDPOINT: &str = "http://127.0.0.1:3007";
const DEFAULT_SPACETIMEDB_DATABASE: &str = "tokenizor";
const DEFAULT_SPACETIMEDB_MODULE_PATH: &str = "spacetime/tokenizor";
pub const SUPPORTED_SPACETIMEDB_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerConfig {
    pub runtime: RuntimeConfig,
    pub blob_store: BlobStoreConfig,
    pub control_plane: ControlPlaneConfig,
}

impl ServerConfig {
    pub fn from_env() -> Result<Self> {
        let mut config = Self::default();

        if let Ok(value) = env::var("TOKENIZOR_BLOB_ROOT") {
            config.blob_store.root_dir = PathBuf::from(value);
        }

        if let Ok(value) = env::var("TOKENIZOR_CONTROL_PLANE_BACKEND") {
            config.control_plane.backend = ControlPlaneBackend::parse(&value)?;
        }

        if let Ok(value) = env::var("TOKENIZOR_SPACETIMEDB_CLI") {
            config.control_plane.spacetimedb.cli_path = value;
        }

        if let Ok(value) = env::var("TOKENIZOR_SPACETIMEDB_ENDPOINT") {
            config.control_plane.spacetimedb.endpoint = value;
        }

        if let Ok(value) = env::var("TOKENIZOR_SPACETIMEDB_DATABASE") {
            config.control_plane.spacetimedb.database = value;
        }

        if let Ok(value) = env::var("TOKENIZOR_SPACETIMEDB_MODULE_PATH") {
            config.control_plane.spacetimedb.module_path = PathBuf::from(value);
        }

        if let Ok(value) = env::var("TOKENIZOR_SPACETIMEDB_SCHEMA_VERSION") {
            config.control_plane.spacetimedb.schema_version = value.parse().map_err(|error| {
                TokenizorError::Config(format!(
                    "TOKENIZOR_SPACETIMEDB_SCHEMA_VERSION must be an integer: {error}"
                ))
            })?;
        }

        if let Ok(value) = env::var("TOKENIZOR_REQUIRE_READY_CONTROL_PLANE") {
            config.runtime.require_ready_control_plane =
                parse_bool("TOKENIZOR_REQUIRE_READY_CONTROL_PLANE", &value)?;
        }

        Ok(config)
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            runtime: RuntimeConfig {
                require_ready_control_plane: true,
            },
            blob_store: BlobStoreConfig {
                root_dir: PathBuf::from(DEFAULT_BLOB_ROOT),
            },
            control_plane: ControlPlaneConfig {
                backend: ControlPlaneBackend::LocalRegistry,
                spacetimedb: SpacetimeDbConfig {
                    cli_path: DEFAULT_SPACETIMEDB_CLI.to_string(),
                    endpoint: DEFAULT_SPACETIMEDB_ENDPOINT.to_string(),
                    database: DEFAULT_SPACETIMEDB_DATABASE.to_string(),
                    module_path: PathBuf::from(DEFAULT_SPACETIMEDB_MODULE_PATH),
                    schema_version: SUPPORTED_SPACETIMEDB_SCHEMA_VERSION,
                },
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub require_ready_control_plane: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlobStoreConfig {
    pub root_dir: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlPlaneConfig {
    pub backend: ControlPlaneBackend,
    pub spacetimedb: SpacetimeDbConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ControlPlaneBackend {
    InMemory,
    LocalRegistry,
    SpacetimeDb,
}

impl ControlPlaneBackend {
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "in_memory" | "in-memory" => Ok(Self::InMemory),
            "local_registry" | "local-registry" | "registry" => Ok(Self::LocalRegistry),
            "spacetimedb" => Ok(Self::SpacetimeDb),
            other => Err(TokenizorError::Config(format!(
                "unsupported control plane backend `{other}`"
            ))),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InMemory => "in_memory",
            Self::LocalRegistry => "local_registry",
            Self::SpacetimeDb => "spacetimedb",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpacetimeDbConfig {
    pub cli_path: String,
    pub endpoint: String,
    pub database: String,
    pub module_path: PathBuf,
    pub schema_version: u32,
}

fn parse_bool(name: &str, value: &str) -> Result<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        other => Err(TokenizorError::Config(format!(
            "{name} must be one of true/false/1/0/yes/no/on/off, received `{other}`"
        ))),
    }
}
