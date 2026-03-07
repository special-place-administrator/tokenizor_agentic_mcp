use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{domain::ComponentHealth, error::Result};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredBlob {
    pub blob_id: String,
    pub byte_len: u64,
    pub was_created: bool,
}

pub trait BlobStore: Send + Sync {
    fn backend_name(&self) -> &'static str;
    fn root_dir(&self) -> &Path;
    fn initialize(&self) -> Result<ComponentHealth>;
    fn health_check(&self) -> Result<ComponentHealth>;
    fn store_bytes(&self, bytes: &[u8]) -> Result<StoredBlob>;
    fn read_bytes(&self, blob_id: &str) -> Result<Vec<u8>>;
}
