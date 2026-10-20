#![cfg_attr(test, allow(clippy::disallowed_macros))]
#![forbid(unsafe_code)]

//! Portable application operations shared by Rust interfaces and native bindings.
//!
//! Application services own cross-call decisions and return typed effects for their
//! caller to execute. Platform handles and UI state stay with the caller. The
//! mobile binding crate adapts this API without becoming another policy owner.

pub mod charging;
