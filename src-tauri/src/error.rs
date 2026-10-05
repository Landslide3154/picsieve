use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("数据库错误: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("文件读写失败 {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("图片解码失败 {path}: {source}")]
    Image {
        path: PathBuf,
        #[source]
        source: image::ImageError,
    },

    #[error("路径不是目录: {0}")]
    NotADirectory(PathBuf),

    #[error("磁盘空间不足：需要 {need} 字节，可用 {available} 字节")]
    InsufficientSpace { need: u64, available: u64 },

    #[error("数据库结构异常: {0}")]
    Schema(String),

    #[error("{0}")]
    Other(String),
}

impl AppError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        AppError::Io {
            path: path.into(),
            source,
        }
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
