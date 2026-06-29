//! In-memory mock implementations of Greentic Designer host functions.
//!
//! Use these to integration-test extensions without spinning up a real
//! runtime. Compose them into a [`MockHostState`] for a single object that
//! carries every mock at once. Each mock is independently usable when you
//! only need one piece (e.g. an LLM extension test that only mocks `http`).
//!
//! Every mock is internally `Arc<Mutex<...>>`, so they're cheap to clone
//! and safe to share between threads — assertions made through a clone see
//! the same captured state as the original.

pub mod broker;
pub mod http_client;
pub mod logger;
pub mod secrets;
pub mod state;
pub mod translator;

pub use self::broker::MockBroker;
pub use self::http_client::{CannedResponse, CapturedRequest, MockHttpClient};
pub use self::logger::{LogRecord, MockLogger};
pub use self::secrets::{MockSecretError, MockSecretsBackend};
pub use self::state::MockHostState;
pub use self::translator::MockTranslator;
