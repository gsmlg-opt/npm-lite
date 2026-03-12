use aws_config::BehaviorVersion;
use aws_sdk_s3::Client;
use aws_sdk_s3::primitives::ByteStream;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures::Stream;
use tracing::{debug, instrument};

use crate::{
    S3Storage,
    error::{Result, StorageError},
};

/// Metadata about an object stored in S3.
#[derive(Debug, Clone)]
pub struct S3Object {
    pub key: String,
    pub last_modified: DateTime<Utc>,
}

/// Metadata returned by a HeadObject request.
#[derive(Debug, Clone)]
pub struct ObjectMetadata {
    pub size: i64,
    pub last_modified: DateTime<Utc>,
}

impl S3Storage {
    /// Create a new `S3Storage` from an existing AWS SDK client and bucket name.
    pub fn new(client: Client, bucket: String) -> Self {
        Self { client, bucket }
    }

    /// Create an `S3Storage` by loading AWS configuration from the environment
    /// (environment variables, `~/.aws/credentials`, instance metadata, etc.).
    pub async fn from_env(bucket: String) -> Result<Self> {
        let config = aws_config::defaults(BehaviorVersion::latest()).load().await;

        let client = Client::new(&config);
        Ok(Self::new(client, bucket))
    }

    /// Upload `data` to `key` in the configured bucket.
    #[instrument(skip(self, data), fields(bucket = %self.bucket, key = %key))]
    pub async fn upload(&self, key: &str, data: Bytes, content_type: &str) -> Result<()> {
        debug!(bytes = data.len(), "uploading object");

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .content_type(content_type)
            .body(ByteStream::from(data))
            .send()
            .await
            .map_err(|e| StorageError::UploadFailed {
                key: key.to_owned(),
                source: e.into(),
            })?;

        debug!("upload complete");
        Ok(())
    }

    /// Download an object and return a streaming body suitable for piping
    /// through Axum's response body.
    #[instrument(skip(self), fields(bucket = %self.bucket, key = %key))]
    pub async fn download_stream(
        &self,
        key: &str,
    ) -> Result<std::pin::Pin<Box<dyn Stream<Item = Result<Bytes>> + Send>>> {
        debug!("starting streaming download");

        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| StorageError::DownloadFailed {
                key: key.to_owned(),
                source: e.into(),
            })?;

        let key_owned = key.to_owned();
        let mut body = resp.body.into_async_read();
        let stream = async_stream::stream! {
            use tokio::io::AsyncReadExt;
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                match body.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => yield Ok(Bytes::copy_from_slice(&buf[..n])),
                    Err(e) => {
                        yield Err(StorageError::BodyReadFailed {
                            key: key_owned.clone(),
                            message: e.to_string(),
                        });
                        break;
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }

    /// Delete the object at `key` from the configured bucket.
    #[instrument(skip(self), fields(bucket = %self.bucket, key = %key))]
    pub async fn delete(&self, key: &str) -> Result<()> {
        debug!("deleting object");

        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| StorageError::DeleteFailed {
                key: key.to_owned(),
                source: e.into(),
            })?;

        debug!("delete complete");
        Ok(())
    }

    /// List all objects whose keys share `prefix` (or all objects when `None`).
    ///
    /// Handles S3 pagination automatically.
    #[instrument(skip(self), fields(bucket = %self.bucket, prefix = ?prefix))]
    pub async fn list_objects(&self, prefix: Option<&str>) -> Result<Vec<S3Object>> {
        debug!("listing objects");

        let mut objects = Vec::new();
        let mut continuation_token: Option<String> = None;

        loop {
            let mut req = self.client.list_objects_v2().bucket(&self.bucket);

            if let Some(p) = prefix {
                req = req.prefix(p);
            }
            if let Some(token) = continuation_token.take() {
                req = req.continuation_token(token);
            }

            let resp = req.send().await.map_err(|e| StorageError::ListFailed {
                prefix: prefix.map(str::to_owned),
                source: e.into(),
            })?;

            for obj in resp.contents() {
                let key = obj.key().unwrap_or_default().to_owned();
                let last_modified = obj
                    .last_modified()
                    .and_then(|t| {
                        let secs = t.secs();
                        let nanos = t.subsec_nanos();
                        DateTime::from_timestamp(secs, nanos)
                    })
                    .ok_or_else(|| StorageError::InvalidTimestamp { key: key.clone() })?;

                objects.push(S3Object { key, last_modified });
            }

            if resp.is_truncated().unwrap_or(false) {
                continuation_token = resp.next_continuation_token().map(str::to_owned);
            } else {
                break;
            }
        }

        debug!(count = objects.len(), "list complete");
        Ok(objects)
    }

    /// Return metadata for `key`, or `None` if the object does not exist.
    #[instrument(skip(self), fields(bucket = %self.bucket, key = %key))]
    pub async fn head_object(&self, key: &str) -> Result<Option<ObjectMetadata>> {
        debug!("heading object");

        let resp = match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                // A 404 / NotFound means the object simply doesn't exist.
                use aws_sdk_s3::operation::head_object::HeadObjectError;
                if let Some(svc_err) = e.as_service_error()
                    && matches!(svc_err, HeadObjectError::NotFound(_))
                {
                    return Ok(None);
                }
                return Err(StorageError::HeadFailed {
                    key: key.to_owned(),
                    source: e.into(),
                });
            }
        };

        let size = resp.content_length().unwrap_or(0);
        let last_modified = resp
            .last_modified()
            .and_then(|t| {
                let secs = t.secs();
                let nanos = t.subsec_nanos();
                DateTime::from_timestamp(secs, nanos)
            })
            .ok_or_else(|| StorageError::InvalidTimestamp {
                key: key.to_owned(),
            })?;

        Ok(Some(ObjectMetadata {
            size,
            last_modified,
        }))
    }
}
