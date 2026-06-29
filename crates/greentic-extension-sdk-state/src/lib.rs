//! Extension lifecycle state — persistent enable/disable per extension.

#![forbid(unsafe_code)]

mod atomic;
mod error;
mod state;
pub mod update;

pub use error::StateError;
pub use state::{ExtensionPolicy, ExtensionState, FailedUpgrade, UpdateMode};
pub use update::{UpdateStatus, resolve};
