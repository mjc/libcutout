//! RGB-accessory mobile FFI surface.
//!
//! `accessory` owns persisted RGB lighting state. `melk` is the current
//! controller-specific protocol implementation.

mod accessory;
mod melk;
mod store;

#[cfg(test)]
mod tests;

pub use accessory::*;
pub use melk::*;
pub use store::*;
