use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Database(#[from] diesel::result::Error),

    #[error("connection error: {0}")]
    Connection(#[from] diesel::ConnectionError),

    #[error("migration error: {0}")]
    Migration(String),

    #[error("invalid data: {0}")]
    InvalidData(String),

    #[error("SDK error: {0}")]
    Sdk(String),

    #[error("sync error: {0}")]
    Sync(String),
}

impl From<deadcat_sdk::Error> for StoreError {
    fn from(e: deadcat_sdk::Error) -> Self {
        StoreError::Sdk(e.to_string())
    }
}
