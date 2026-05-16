//! Extension lifecycle state — persistent enable/disable per extension.

#![forbid(unsafe_code)]

mod atomic;
mod error;
mod state;

pub use error::StateError;
pub use state::ExtensionState;
