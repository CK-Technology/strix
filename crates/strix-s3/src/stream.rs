//! Streaming utilities for S3 responses.
//!
//! This module provides adapters to convert storage layer streams
//! to s3s-compatible streaming bodies without buffering.

use bytes::Bytes;
use futures::Stream;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::Mutex;

use strix_core::ObjectBody;

type StdError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// A wrapper that converts an ObjectBody stream to an s3s-compatible body.
///
/// This wrapper:
/// - Maps `std::io::Error` to `Box<dyn std::error::Error + Send + Sync>`
/// - Provides `Sync` by using interior mutability
/// - Implements the streaming interface required by s3s
pub struct S3BodyStream {
    inner: Arc<Mutex<ObjectBody>>,
    content_length: u64,
}

impl S3BodyStream {
    /// Create a new streaming body from an ObjectBody.
    pub fn new(body: ObjectBody, content_length: u64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(body)),
            content_length,
        }
    }

    /// Convert to s3s Body type.
    pub fn into_s3s_body(self) -> s3s::Body {
        // Box and pin the stream for DynByteStream compatibility
        let boxed: Pin<
            Box<
                dyn s3s::stream::ByteStream<Item = Result<Bytes, StdError>> + Send + Sync + 'static,
            >,
        > = Box::pin(self);
        s3s::Body::from(boxed)
    }
}

impl Stream for S3BodyStream {
    type Item = Result<Bytes, StdError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let inner = self.inner.clone();

        // Try to acquire the lock
        let mut guard = match inner.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                // Lock is held, wake up later
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
        };

        // Poll the inner stream
        let pinned = Pin::new(&mut *guard);
        match pinned.poll_next(cx) {
            Poll::Ready(Some(Ok(bytes))) => Poll::Ready(Some(Ok(bytes))),
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(Box::new(e) as StdError))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl s3s::stream::ByteStream for S3BodyStream {
    fn remaining_length(&self) -> s3s::stream::RemainingLength {
        s3s::stream::RemainingLength::new(
            self.content_length as usize,
            Some(self.content_length as usize),
        )
    }
}

// Safety: S3BodyStream is Sync because the inner stream is protected by a Mutex
unsafe impl Sync for S3BodyStream {}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;

    #[tokio::test]
    async fn test_stream_conversion() {
        use futures::StreamExt;

        // Create a simple test stream
        let chunks = vec![
            Ok(Bytes::from("hello")),
            Ok(Bytes::from(" ")),
            Ok(Bytes::from("world")),
        ];
        let body: ObjectBody = Box::pin(stream::iter(chunks));

        // Convert to S3BodyStream
        let s3_stream = S3BodyStream::new(body, 11); // "hello world" = 11 bytes

        // Collect all bytes
        let collected: Vec<_> = s3_stream.collect().await;
        assert_eq!(collected.len(), 3);
        assert!(collected.iter().all(|r| r.is_ok()));
    }

    #[tokio::test]
    async fn test_remaining_length() {
        use s3s::stream::ByteStream;

        let chunks = vec![Ok(Bytes::from("test"))];
        let body: ObjectBody = Box::pin(stream::iter(chunks));

        let s3_stream = S3BodyStream::new(body, 1024);
        let _remaining = s3_stream.remaining_length();

        // RemainingLength should be created without panic
        // (fields are private, so we just verify it doesn't panic)
    }

    #[tokio::test]
    async fn test_error_conversion() {
        use futures::StreamExt;
        use std::io::Error;

        let chunks: Vec<std::io::Result<Bytes>> =
            vec![Ok(Bytes::from("data")), Err(Error::other("test error"))];
        let body: ObjectBody = Box::pin(stream::iter(chunks));

        let s3_stream = S3BodyStream::new(body, 100);
        let collected: Vec<_> = s3_stream.collect().await;

        assert_eq!(collected.len(), 2);
        assert!(collected[0].is_ok());
        assert!(collected[1].is_err());
    }
}
