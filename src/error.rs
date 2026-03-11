use std::{io, path::PathBuf};

use thiserror::Error;

pub type Result<T> = std::result::Result<T, TokenizorError>;

#[derive(Debug, Error)]
pub enum TokenizorError {
    #[error("i/o error at `{path}`: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("parse error: {0}")]
    Parse(String),
    #[error("discovery error: {0}")]
    Discovery(String),
    #[error("circuit breaker: {0}")]
    CircuitBreaker(String),
    #[error("invalid configuration: {0}")]
    Config(String),
}

impl TokenizorError {
    pub fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

impl From<io::Error> for TokenizorError {
    fn from(source: io::Error) -> Self {
        Self::Io {
            path: PathBuf::from("<unknown>"),
            source,
        }
    }
}
