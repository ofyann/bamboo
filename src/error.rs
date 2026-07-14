use thiserror::Error;

#[derive(Error, Debug)]
pub enum BambooError {
    #[error("解析镜像引用失败: {0}")]
    ImageParse(String),

    #[error("认证失败: {0}")]
    Auth(String),

    #[error("Registry 操作失败: {0}")]
    Registry(String),

    #[error("同步失败: {0}")]
    Sync(String),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, BambooError>;
