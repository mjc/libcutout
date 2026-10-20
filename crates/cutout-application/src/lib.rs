#![cfg_attr(test, allow(clippy::disallowed_macros))]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![forbid(unsafe_code)]

//! Portable application operations shared by Rust interfaces and native bindings.
//!
//! Application services own cross-call decisions and return typed effects for their
//! caller to execute. Platform handles and UI state stay with the caller. The
//! mobile binding crate adapts this API without becoming another policy owner.

pub mod alarms;
pub mod camera;
pub mod capture;
pub mod capture_writer;
pub mod charging;
pub mod database;
pub mod lighting;
pub mod location;
pub mod music;
pub mod ride_activity;
pub mod session;
