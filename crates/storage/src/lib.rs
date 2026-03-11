//! `npm-storage` — S3 abstraction layer for npm-lite.
//!
//! # Overview
//!
//! This crate wraps the AWS SDK S3 client with a thin, domain-focused API used
//! by the rest of the npm-lite workspace.  The central type is [`S3Storage`],
//! which holds an authenticated [`aws_sdk_s3::Client`] and the target bucket
//! name.
//!
//! ## Feature areas
//!
//! | Module | Responsibility |
//! |--------|----------------|
//! | [`error`] | Typed error enum for all storage operations |
//! | [`operations`] | Core S3 operations (upload, download, delete, list, head) |
//! | [`gc`] | Orphan-blob garbage collection |

pub mod error;
pub mod gc;
pub mod operations;

pub use error::{Result, StorageError};
pub use gc::{GcReport, OrphanGc};
pub use operations::{ObjectMetadata, S3Object};

use aws_sdk_s3::Client;

/// Thin wrapper around an AWS S3 client scoped to a single bucket.
///
/// Construct via [`S3Storage::new`] when you already have a configured
/// [`Client`], or via [`S3Storage::from_env`] to load credentials and region
/// from the standard AWS environment (env vars, `~/.aws/credentials`, EC2
/// instance metadata, etc.).
pub struct S3Storage {
    client: Client,
    bucket: String,
}
