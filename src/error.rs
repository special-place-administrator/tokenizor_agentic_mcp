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
    #[error("invalid operation: {0}")]
    InvalidOperation(String),
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

    pub fn is_systemic(&self) -> bool {
        match self {
            Self::Io { .. } => true,
            Self::Storage(_) => true,
            Self::Integrity(_) => true,
            Self::ControlPlane(_) => true,
            Self::Serialization(_) => true,
            Self::Config(_) => false,
            Self::InvalidArgument(_) => false,
            Self::InvalidOperation(_) => false,
            Self::NotFound(_) => false,
        }
    }
}

impl From<serde_json::Error> for TokenizorError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serialization(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_systemic_returns_true_for_io_errors() {
        let err = TokenizorError::io("/tmp/test", io::Error::new(io::ErrorKind::NotFound, "gone"));
        assert!(err.is_systemic());
    }

    #[test]
    fn test_is_systemic_returns_true_for_storage_errors() {
        assert!(TokenizorError::Storage("disk full".into()).is_systemic());
    }

    #[test]
    fn test_is_systemic_returns_true_for_integrity_errors() {
        assert!(TokenizorError::Integrity("hash mismatch".into()).is_systemic());
    }

    #[test]
    fn test_is_systemic_returns_true_for_control_plane_errors() {
        assert!(TokenizorError::ControlPlane("connection lost".into()).is_systemic());
    }

    #[test]
    fn test_is_systemic_returns_true_for_serialization_errors() {
        assert!(TokenizorError::Serialization("invalid json".into()).is_systemic());
    }

    #[test]
    fn test_is_systemic_returns_false_for_config_errors() {
        assert!(!TokenizorError::Config("bad config".into()).is_systemic());
    }

    #[test]
    fn test_is_systemic_returns_false_for_invalid_argument_errors() {
        assert!(!TokenizorError::InvalidArgument("bad arg".into()).is_systemic());
    }

    #[test]
    fn test_is_systemic_returns_false_for_not_found_errors() {
        assert!(!TokenizorError::NotFound("missing".into()).is_systemic());
    }
}
