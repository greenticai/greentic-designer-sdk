//! Extension lifecycle state — persistent enable/disable per extension.

mod atomic;
mod error;
mod state;

pub use error::StateError;
pub use state::ExtensionState;
