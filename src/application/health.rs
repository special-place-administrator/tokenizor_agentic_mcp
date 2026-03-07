use crate::domain::{HealthReport, ServiceIdentity};
use crate::error::Result;
use crate::storage::{BlobStore, ControlPlane};

pub struct HealthService<'a> {
    blob_store: &'a dyn BlobStore,
    control_plane: &'a dyn ControlPlane,
}

impl<'a> HealthService<'a> {
    pub fn new(blob_store: &'a dyn BlobStore, control_plane: &'a dyn ControlPlane) -> Self {
        Self {
            blob_store,
            control_plane,
        }
    }

    pub fn report(&self) -> Result<HealthReport> {
        let components = vec![
            self.control_plane.health_check()?,
            self.blob_store.health_check()?,
        ];

        Ok(HealthReport::new(
            ServiceIdentity {
                name: env!("CARGO_PKG_NAME").to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            components,
        ))
    }
}
