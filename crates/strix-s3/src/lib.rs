//! S3 API implementation for Strix.
//!
//! This crate implements the S3 protocol using the `s3s` crate,
//! bridging S3 requests to the storage backend.

mod auth;
mod error;
mod iam_auth;
mod presign;
mod service;
mod stream;

pub use auth::{AuthProvider, SimpleAuthProvider};
pub use iam_auth::IamAuth;
pub use presign::{PresignMethod, PresignOptions, PresignUrlGenerator};
pub use service::{RequestAuditContext, StrixS3Service};
pub use stream::S3BodyStream;
