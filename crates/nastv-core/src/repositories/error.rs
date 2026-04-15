use nastv_domain::EntityId;

#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("{kind} with id {id} not found")]
    NotFound { kind: &'static str, id: EntityId },
    #[error("conflict: {0}")]
    Conflict(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

pub type Result<T> = std::result::Result<T, RepositoryError>;
