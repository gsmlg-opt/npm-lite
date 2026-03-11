use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("S3 upload failed for key '{key}': {source}")]
    UploadFailed {
        key: String,
        #[source]
        source: aws_sdk_s3::Error,
    },

    #[error("S3 download failed for key '{key}': {source}")]
    DownloadFailed {
        key: String,
        #[source]
        source: aws_sdk_s3::Error,
    },

    #[error("S3 delete failed for key '{key}': {source}")]
    DeleteFailed {
        key: String,
        #[source]
        source: aws_sdk_s3::Error,
    },

    #[error("S3 list objects failed for prefix '{prefix:?}': {source}")]
    ListFailed {
        prefix: Option<String>,
        #[source]
        source: aws_sdk_s3::Error,
    },

    #[error("S3 head object failed for key '{key}': {source}")]
    HeadFailed {
        key: String,
        #[source]
        source: aws_sdk_s3::Error,
    },

    #[error("Failed to read response body for key '{key}': {message}")]
    BodyReadFailed { key: String, message: String },

    #[error("AWS configuration error: {0}")]
    ConfigError(String),

    #[error("Invalid timestamp returned by S3 for key '{key}'")]
    InvalidTimestamp { key: String },
}

pub type Result<T, E = StorageError> = std::result::Result<T, E>;
