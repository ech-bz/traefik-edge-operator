use crate::error::{OperatorError, Result};
use serde::Deserialize;
use std::time::Duration;

#[derive(Clone, Debug, Deserialize)]
pub struct OperatorConfig {
    pub lease_namespace: String,
    pub lease_name: String,
    pub holder_id: String,
    pub lease_duration_seconds: u64,
    pub lease_grace_seconds: u64,
    pub health_port: u16,

    pub traefik_image: String,
    pub reconcile_interval_seconds: u64,
    pub error_backoff_seconds: u64,
    pub wait_seconds: u64,
}

impl OperatorConfig {
    pub fn from_env() -> Result<Self> {
        let config: Self = envy::prefixed("OPERATOR_").from_env()?;
        if config.lease_grace_seconds == 0
            || config.lease_grace_seconds >= config.lease_duration_seconds
        {
            return Err(OperatorError::Config(
                "lease grace must be positive and shorter than lease duration".into(),
            ));
        }
        Ok(config)
    }

    pub fn reconcile_interval(&self) -> Duration {
        Duration::from_secs(self.reconcile_interval_seconds)
    }

    pub fn error_backoff(&self) -> Duration {
        Duration::from_secs(self.error_backoff_seconds)
    }

    pub fn wait_backoff(&self) -> Duration {
        Duration::from_secs(self.wait_seconds)
    }
}
