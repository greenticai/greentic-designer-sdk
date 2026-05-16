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
/// ```
#[derive(Debug, Clone, Default)]
pub struct MockHttpClient {
    canned: Arc<Mutex<HashMap<(String, String), CannedResponse>>>,
    calls: Arc<Mutex<Vec<CapturedRequest>>>,
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
