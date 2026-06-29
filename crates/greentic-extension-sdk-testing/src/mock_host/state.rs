//! Composer that bundles all mocks into a single object integration tests
//! can pass to extension fixtures.

use super::{MockBroker, MockHttpClient, MockLogger, MockSecretsBackend, MockTranslator};

/// All five mocks bundled. Cheap to clone — every field is an `Arc`-wrapped
/// inner store, so clones share state with the original.
///
/// ```
/// use greentic_extension_sdk_testing::mock_host::{MockHostState, CannedResponse};
/// let host = MockHostState::default();
/// host.logger.log("info", "boot");
/// host.translator.set("en", "ok", "OK");
/// host.secrets.set("api", "abc");
/// host.http.expect(
///     "GET",
///     "https://example.com/x",
///     CannedResponse { status: 200, body: vec![] },
/// );
/// assert_eq!(host.logger.records().len(), 1);
/// assert_eq!(host.translator.translate("en", "ok"), "OK");
/// assert_eq!(host.secrets.get("api").unwrap(), "abc");
/// assert_eq!(host.http.fetch("GET", "https://example.com/x", &[], None).status, 200);
/// ```
#[derive(Debug, Clone, Default)]
pub struct MockHostState {
    pub logger: MockLogger,
    pub translator: MockTranslator,
    pub secrets: MockSecretsBackend,
    pub http: MockHttpClient,
    pub broker: MockBroker,
}
