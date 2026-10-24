#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

//! Cutout's music broker, charging directory, ride database, and website, served by one process.
//!
//! Public registration budgets group canonical IPv4 addresses and IPv6 /64
//! prefixes. IPv4-mapped IPv6 addresses use the corresponding IPv4 budget.
//! Only hashes of those source keys are stored; global budgets remain separate.
//! Upgrading from full-address IPv6 keys starts fresh prefix budgets because
//! old hashes cannot be regrouped; existing global budget counts are retained.

mod content_filter;
mod http;
mod model;
mod moderation;
mod runtime;
mod signup;
mod signup_sync;
mod site;
mod store;
pub mod vhosts;

pub use http::{TrustedProxy, router};
pub use runtime::run_from_arguments;
pub use store::Store;

/// Classified outcomes exposed by the directory API.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum Error {
    /// A diagnostic capture is malformed.
    #[error("invalid capture")]
    CaptureInvalid,
    /// The capture representation is unsupported.
    #[error("unsupported capture format")]
    CaptureUnsupported,
    /// Submitted bytes disagree with their approved digest or length.
    #[error("capture integrity check failed")]
    CaptureCorrupt,
    /// Music metadata is prohibited in diagnostic submissions.
    #[error("capture contains prohibited music metadata")]
    CaptureMusic,
    /// A capture exceeds a hard resource limit.
    #[error("capture resource limit exceeded")]
    CaptureTooLarge,
    /// Private retained-byte capacity is exhausted.
    #[error("private capture storage quota exhausted")]
    CaptureStorageFull,
    /// The request violates the input contract.
    #[error("invalid request")]
    Invalid,
    /// A valid contributor credential is required.
    #[error("authentication required")]
    Unauthorized,
    /// This contributor cannot perform the operation.
    #[error("operation forbidden")]
    Forbidden,
    /// No visible record exists for this identifier.
    #[error("record not found")]
    NotFound,
    /// A revision or operation identifier conflicts with current state.
    #[error("revision or operation conflict")]
    Conflict,
    /// The caller's durable request budget is exhausted.
    #[error("request budget exhausted")]
    Throttled,
    /// The service cannot currently complete the request.
    #[error("service unavailable")]
    Unavailable,
}
