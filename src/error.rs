use thiserror::Error;

#[derive(Error, Debug)]
pub enum BambooError {
    #[error("failed to parse image reference: {0}")]
    ImageParse(String),

    #[error("authentication error: {0}")]
    Auth(String),

    #[error("registry error: {0}")]
    Registry(String),

    #[error("sync error: {0}")]
    Sync(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, BambooError>;
