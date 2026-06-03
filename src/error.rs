use thiserror::Error;

#[derive(Debug, Error)]
pub enum OperatorError {
    #[error("{kind} {name} has no namespace")]
    MissingNamespace { kind: &'static str, name: String },
    #[error("config: {0}")]
    Config(String),
    #[error("finalizer: {0}")]
    Finalizer(String),
    #[error("controller fatal: {0}")]
    ControllerFatal(String),
    #[error(transparent)]
    Kube(#[from] kube::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
    #[error(transparent)]
    LeaderElection(#[from] kube_lease_manager::LeaseManagerError),
    #[error(transparent)]
    Envy(#[from] envy::Error),
}

pub type Result<T> = std::result::Result<T, OperatorError>;
