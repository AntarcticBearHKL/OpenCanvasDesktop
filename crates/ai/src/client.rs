//! Shared HTTP client and cooperative cancellation.
//!
//! The web sets no global request timeout (AbortSignal is the only cancel path);
//! this port keeps a 30 s connect timeout and applies a 300 s per-request timeout
//! to non-streaming calls, while the SSE stream runs without a total timeout and
//! is bounded by [`Cancel`] instead. The API key travels as a Bearer header sent
//! straight to the user's configured endpoint.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::error::{map_reqwest_error, AiError};

/// Re-exported so downstream crates can name the client type without a direct
/// `reqwest` dependency.
pub use reqwest::Client;

/// Per-request timeout for non-streaming calls.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);
/// Connect timeout applied to the whole client.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// How often [`Cancel::cancelled`] polls the flag.
const CANCEL_POLL: Duration = Duration::from_millis(50);

/// A cooperative, cloneable cancellation flag. The app flips it from the UI
/// thread when the user stops a generation; long-running loops (retry, video
/// poll) and in-flight requests observe it.
#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_canceled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    /// Resolves once the flag is set (or immediately when already set).
    pub async fn cancelled(&self) {
        while !self.is_canceled() {
            tokio::time::sleep(CANCEL_POLL).await;
        }
    }
}

/// Builds the shared client (rustls TLS, tuned connect timeout).
pub fn build_client() -> Result<Client, AiError> {
    Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .user_agent(concat!("open-canvas-desktop/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| AiError::Network(error.to_string()))
}

/// `Authorization: Bearer <apiKey>` (`image.ts:314`).
pub fn bearer(api_key: &str) -> String {
    format!("Bearer {api_key}")
}

/// Sends a request, racing it against cancellation: the returned future resolves
/// to [`AiError::Canceled`] the moment the flag flips.
pub async fn send_cancelable(
    builder: reqwest::RequestBuilder,
    cancel: &Cancel,
) -> Result<reqwest::Response, AiError> {
    tokio::select! {
        biased;
        () = cancel.cancelled() => Err(AiError::Canceled),
        result = builder.send() => result.map_err(|error| map_reqwest_error(&error)),
    }
}
