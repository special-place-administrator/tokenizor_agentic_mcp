use std::{io, path::PathBuf};

use thiserror::Error;

pub type Result<T> = std::result::Result<T, TokenizorError>;

#[derive(Debug, Error)]
pub enum TokenizorError {
    #[error("invalid configuration: {0}")]
    Config(String),
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("entity not found: {0}")]
    NotFound(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("integrity check failed: {0}")]
    Integrity(String),
    #[error("control plane error: {0}")]
    ControlPlane(String),
    #[error("i/o error at `{path}`: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("serialization error: {0}")]
    Serialization(String),
}

impl TokenizorError {
    pub fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

impl From<serde_json::Error> for TokenizorError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serialization(error.to_string())
    }
}
