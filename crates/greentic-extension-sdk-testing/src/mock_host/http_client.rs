//! In-memory `http::fetch` mock that records calls and returns canned responses.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A request the test extension made via the host http import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

/// A canned response.
#[derive(Debug, Clone)]
pub struct CannedResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

/// Records all outgoing calls and returns a canned response per `(method, url)`
/// key. Default response is `404 not found` for any unmatched call.
///
/// Optionally enforces the extension's declared network permissions
/// (`runtime.permissions.network`): call [`restrict_to_hosts`](Self::restrict_to_hosts)
/// and any `fetch` to an undeclared host returns a synthetic `403` (the call is
/// still recorded, so the test can assert the attempt). A permission entry
/// matches the request host exactly, or as a `*.suffix` wildcard.
///
/// ```
/// use greentic_extension_sdk_testing::mock_host::{MockHttpClient, CannedResponse};
/// let http = MockHttpClient::new();
/// http.expect("GET", "https://example.com/ping", CannedResponse {
///     status: 200,
///     body: b"pong".to_vec(),
/// });
/// let resp = http.fetch("GET", "https://example.com/ping", &[], None);
/// assert_eq!(resp.status, 200);
/// assert_eq!(resp.body, b"pong");
/// assert_eq!(http.calls().len(), 1);
/// // unmatched call returns 404
/// let miss = http.fetch("GET", "https://nope/", &[], None);
/// assert_eq!(miss.status, 404);
///
/// // Enforce declared network permissions.
/// http.restrict_to_hosts(&["example.com".to_string()]);
/// assert_eq!(http.fetch("GET", "https://example.com/ping", &[], None).status, 200);
/// assert_eq!(http.fetch("GET", "https://evil.test/x", &[], None).status, 403);
/// ```
#[derive(Debug, Clone, Default)]
pub struct MockHttpClient {
    canned: Arc<Mutex<HashMap<(String, String), CannedResponse>>>,
    calls: Arc<Mutex<Vec<CapturedRequest>>>,
    /// `Some(allowlist)` enables host enforcement; `None` allows any host.
    allowed_hosts: Arc<Mutex<Option<Vec<String>>>>,
}

impl MockHttpClient {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn expect(&self, method: &str, url: &str, response: CannedResponse) {
        self.canned
            .lock()
            .expect("mock http poisoned")
            .insert((method.to_string(), url.to_string()), response);
    }

    /// Enable network-permission enforcement against the declared host patterns
    /// (typically `describe.runtime.permissions.network`). After this, a `fetch`
    /// to any host not matched by `hosts` returns `403`.
    pub fn restrict_to_hosts(&self, hosts: &[String]) {
        *self.allowed_hosts.lock().expect("mock http poisoned") = Some(hosts.to_vec());
    }

    #[must_use]
    pub fn fetch(
        &self,
        method: &str,
        url: &str,
        headers: &[(&str, &str)],
        body: Option<Vec<u8>>,
    ) -> CannedResponse {
        let captured = CapturedRequest {
            method: method.to_string(),
            url: url.to_string(),
            headers: headers
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
            body,
        };
        self.calls
            .lock()
            .expect("mock http poisoned")
            .push(captured);

        // Enforce declared network permissions before serving any response.
        if let Some(allowed) = self
            .allowed_hosts
            .lock()
            .expect("mock http poisoned")
            .as_ref()
        {
            let host = host_of(url);
            if !allowed.iter().any(|pattern| host_matches(pattern, host)) {
                return CannedResponse {
                    status: 403,
                    body: format!("mock: network permission denied for host {host:?}").into_bytes(),
                };
            }
        }

        self.canned
            .lock()
            .expect("mock http poisoned")
            .get(&(method.to_string(), url.to_string()))
            .cloned()
            .unwrap_or(CannedResponse {
                status: 404,
                body: b"mock: no canned response".to_vec(),
            })
    }

    #[must_use]
    pub fn calls(&self) -> Vec<CapturedRequest> {
        self.calls.lock().expect("mock http poisoned").clone()
    }
}

/// Extract the host from a URL without pulling in a URL-parsing dependency:
/// strip the scheme, then take everything up to the first `/`, `?`, or `:`
/// (port). Good enough for permission matching in tests.
fn host_of(url: &str) -> &str {
    let after_scheme = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host_port = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(after_scheme);
    // Drop userinfo (`user@host`) then the port (`host:443`).
    let host = host_port.rsplit_once('@').map_or(host_port, |(_, h)| h);
    host.split_once(':').map_or(host, |(h, _)| h)
}

/// Match a declared permission `pattern` against a request `host`: exact match,
/// or a `*.suffix` wildcard (which also matches the bare apex `suffix`).
fn host_matches(pattern: &str, host: &str) -> bool {
    if let Some(suffix) = pattern.strip_prefix("*.") {
        host == suffix || host.ends_with(&format!(".{suffix}"))
    } else {
        pattern == host
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_of_strips_scheme_userinfo_port_and_path() {
        assert_eq!(host_of("https://example.com/a/b?q=1"), "example.com");
        assert_eq!(
            host_of("http://user@api.example.com:8443/x"),
            "api.example.com"
        );
        assert_eq!(host_of("example.com"), "example.com");
    }

    #[test]
    fn host_matches_exact_and_wildcard() {
        assert!(host_matches("example.com", "example.com"));
        assert!(!host_matches("example.com", "api.example.com"));
        assert!(host_matches("*.example.com", "api.example.com"));
        assert!(host_matches("*.example.com", "example.com")); // apex
        assert!(!host_matches("*.example.com", "example.org"));
    }

    #[test]
    fn enforcement_denies_undeclared_host_but_records_the_attempt() {
        let http = MockHttpClient::new();
        http.expect(
            "GET",
            "https://evil.test/x",
            CannedResponse {
                status: 200,
                body: b"should-not-serve".to_vec(),
            },
        );
        http.restrict_to_hosts(&["example.com".to_string()]);

        let resp = http.fetch("GET", "https://evil.test/x", &[], None);
        assert_eq!(
            resp.status, 403,
            "undeclared host must be denied even if canned"
        );
        assert_eq!(http.calls().len(), 1, "denied attempt is still recorded");
    }
}
