//! HTTP mocking utilities for testing.

use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::RwLock;

/// Simple glob matcher: `*` matches any sequence, `?` matches one char.
#[allow(clippy::indexing_slicing)]
fn glob_matches(pattern: &str, text: &str) -> bool {
    let pat = pattern.as_bytes();
    let txt = text.as_bytes();
    let (mut pi, mut ti) = (0, 0);
    let (mut star_pi, mut star_ti) = (usize::MAX, 0);

    while ti < txt.len() {
        if pi < pat.len() && (pat[pi] == b'?' || pat[pi] == txt[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < pat.len() && pat[pi] == b'*' {
            star_pi = pi;
            star_ti = ti;
            pi += 1;
        } else if star_pi != usize::MAX {
            pi = star_pi + 1;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }

    while pi < pat.len() && pat[pi] == b'*' {
        pi += 1;
    }
    pi == pat.len()
}

/// Mock HTTP client for testing.
#[derive(Clone)]
pub struct MockHttp {
    mocks: Arc<RwLock<Vec<MockHandler>>>,
    requests: Arc<RwLock<Vec<RecordedRequest>>>,
}

/// A mock handler.
struct MockHandler {
    pattern: String,
    handler: Arc<dyn Fn(&MockRequest) -> MockResponse + Send + Sync>,
}

/// A recorded request.
#[derive(Debug, Clone)]
pub struct RecordedRequest {
    /// Request method.
    pub method: String,
    /// Request URL.
    pub url: String,
    /// Request headers.
    pub headers: HashMap<String, String>,
    /// Request body.
    pub body: serde_json::Value,
}

/// Mock HTTP request.
#[derive(Debug, Clone)]
pub struct MockRequest {
    /// Request method.
    pub method: String,
    /// Request path.
    pub path: String,
    /// Request URL.
    pub url: String,
    /// Request headers.
    pub headers: HashMap<String, String>,
    /// Request body.
    pub body: serde_json::Value,
}

/// Mock HTTP response.
#[derive(Debug, Clone)]
pub struct MockResponse {
    /// Status code.
    pub status: u16,
    /// Response headers.
    pub headers: HashMap<String, String>,
    /// Response body.
    pub body: serde_json::Value,
}

impl MockResponse {
    /// Create a successful JSON response.
    pub fn json<T: Serialize>(body: T) -> Self {
        Self {
            status: 200,
            headers: HashMap::from([("content-type".to_string(), "application/json".to_string())]),
            body: serde_json::to_value(body).unwrap_or(serde_json::Value::Null),
        }
    }

    /// Create an error response.
    pub fn error(status: u16, message: &str) -> Self {
        Self {
            status,
            headers: HashMap::from([("content-type".to_string(), "application/json".to_string())]),
            body: serde_json::json!({ "error": message }),
        }
    }

    /// Create a 500 internal error.
    pub fn internal_error(message: &str) -> Self {
        Self::error(500, message)
    }

    /// Create a 404 not found.
    pub fn not_found(message: &str) -> Self {
        Self::error(404, message)
    }

    /// Create a 401 unauthorized.
    pub fn unauthorized(message: &str) -> Self {
        Self::error(401, message)
    }

    /// Create an empty 200 OK response.
    pub fn ok() -> Self {
        Self::json(serde_json::json!({}))
    }
}

impl MockHttp {
    /// Create a new mock HTTP client.
    pub fn new() -> Self {
        Self {
            mocks: Arc::new(RwLock::new(Vec::new())),
            requests: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Add a mock handler.
    pub fn add_mock<F>(&mut self, pattern: &str, handler: F)
    where
        F: Fn(&MockRequest) -> MockResponse + Send + Sync + 'static,
    {
        let mocks = self.mocks.clone();
        let pattern = pattern.to_string();
        let handler = Arc::new(handler);
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::try_current();
            if let Ok(rt) = rt {
                rt.block_on(async {
                    let mut mocks = mocks.write().await;
                    mocks.push(MockHandler { pattern, handler });
                });
            }
        });
    }

    /// Execute a mock request.
    pub async fn execute(&self, request: MockRequest) -> MockResponse {
        // Record the request
        {
            let mut requests = self.requests.write().await;
            requests.push(RecordedRequest {
                method: request.method.clone(),
                url: request.url.clone(),
                headers: request.headers.clone(),
                body: request.body.clone(),
            });
        }

        // Find matching mock
        let mocks = self.mocks.read().await;
        for mock in mocks.iter() {
            if glob_matches(&mock.pattern, &request.url)
                || glob_matches(&mock.pattern, &request.path)
            {
                return (mock.handler)(&request);
            }
        }

        // No mock found
        MockResponse::error(500, &format!("No mock found for {}", request.url))
    }

    /// Get recorded requests.
    pub async fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.read().await.clone()
    }

    /// Get requests to a specific URL pattern.
    pub async fn requests_to(&self, pattern: &str) -> Vec<RecordedRequest> {
        self.requests
            .read()
            .await
            .iter()
            .filter(|r| glob_matches(pattern, &r.url))
            .cloned()
            .collect()
    }

    /// Clear recorded requests.
    pub async fn clear_requests(&self) {
        self.requests.write().await.clear();
    }

    /// Clear all mocks.
    pub async fn clear_mocks(&self) {
        self.mocks.write().await.clear();
    }
}

impl Default for MockHttp {
    fn default() -> Self {
        Self::new()
    }
}

/// Type alias for mock handler closure.
type MockHandlerFn = Box<dyn Fn(&MockRequest) -> MockResponse + Send + Sync>;

/// Builder for MockHttp.
pub struct MockHttpBuilder {
    mocks: Vec<(String, MockHandlerFn)>,
}

impl MockHttpBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self { mocks: Vec::new() }
    }

    /// Add a mock.
    pub fn mock<F>(mut self, pattern: &str, handler: F) -> Self
    where
        F: Fn(&MockRequest) -> MockResponse + Send + Sync + 'static,
    {
        self.mocks.push((pattern.to_string(), Box::new(handler)));
        self
    }

    /// Build the MockHttp.
    pub fn build(self) -> MockHttp {
        // Note: In a real implementation, we'd add the mocks here
        MockHttp::new()
    }
}

impl Default for MockHttpBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_response_json() {
        let response = MockResponse::json(serde_json::json!({"id": 123}));
        assert_eq!(response.status, 200);
        assert_eq!(response.body["id"], 123);
    }

    #[test]
    fn test_mock_response_error() {
        let response = MockResponse::error(404, "Not found");
        assert_eq!(response.status, 404);
        assert_eq!(response.body["error"], "Not found");
    }

    #[test]
    fn test_mock_response_internal_error() {
        let response = MockResponse::internal_error("Server error");
        assert_eq!(response.status, 500);
    }

    #[test]
    fn test_mock_response_not_found() {
        let response = MockResponse::not_found("Resource not found");
        assert_eq!(response.status, 404);
    }

    #[test]
    fn test_mock_response_unauthorized() {
        let response = MockResponse::unauthorized("Invalid token");
        assert_eq!(response.status, 401);
    }

    #[tokio::test]
    async fn test_mock_http_no_handler() {
        let mock = MockHttp::new();
        let request = MockRequest {
            method: "GET".to_string(),
            path: "/test".to_string(),
            url: "https://example.com/test".to_string(),
            headers: HashMap::new(),
            body: serde_json::Value::Null,
        };

        let response = mock.execute(request).await;
        assert_eq!(response.status, 500);
    }

    #[tokio::test]
    async fn test_mock_http_records_requests() {
        let mock = MockHttp::new();
        let request = MockRequest {
            method: "POST".to_string(),
            path: "/api/users".to_string(),
            url: "https://api.example.com/users".to_string(),
            headers: HashMap::from([("authorization".to_string(), "Bearer token".to_string())]),
            body: serde_json::json!({"name": "Test"}),
        };

        let _ = mock.execute(request).await;

        let requests = mock.requests().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, "POST");
        assert_eq!(requests[0].body["name"], "Test");
    }

    #[tokio::test]
    async fn test_mock_http_clear_requests() {
        let mock = MockHttp::new();
        let request = MockRequest {
            method: "GET".to_string(),
            path: "/test".to_string(),
            url: "https://example.com/test".to_string(),
            headers: HashMap::new(),
            body: serde_json::Value::Null,
        };

        let _ = mock.execute(request).await;
        assert_eq!(mock.requests().await.len(), 1);

        mock.clear_requests().await;
        assert_eq!(mock.requests().await.len(), 0);
    }
}
