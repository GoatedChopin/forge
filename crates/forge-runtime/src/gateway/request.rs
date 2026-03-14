use serde::{Deserialize, Serialize};

/// RPC request body for function calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    /// Function name to invoke.
    pub function: String,
    /// Function arguments as JSON.
    #[serde(default)]
    pub args: serde_json::Value,
}

impl RpcRequest {
    /// Create a new RPC request.
    pub fn new(function: impl Into<String>, args: serde_json::Value) -> Self {
        Self {
            function: function.into(),
            args,
        }
    }
}

/// Batch RPC request containing multiple function calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchRpcRequest {
    /// List of requests to execute.
    pub requests: Vec<RpcRequest>,
}

/// Batch RPC response containing results for each request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchRpcResponse {
    /// Individual results for each request.
    pub results: Vec<super::response::RpcResponse>,
}

impl axum::response::IntoResponse for BatchRpcResponse {
    fn into_response(self) -> axum::response::Response {
        (axum::http::StatusCode::OK, axum::Json(self)).into_response()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_rpc_request_deserialization() {
        let json = r#"{"function": "getUser", "args": {"id": "123"}}"#;
        let req: RpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.function, "getUser");
        assert_eq!(req.args["id"], "123");
    }

    #[test]
    fn test_rpc_request_default_args() {
        let json = r#"{"function": "healthCheck"}"#;
        let req: RpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.function, "healthCheck");
        assert!(req.args.is_null());
    }

    #[test]
    fn test_batch_request() {
        let json = r#"{"requests": [{"function": "a"}, {"function": "b"}]}"#;
        let batch: BatchRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(batch.requests.len(), 2);
    }

    #[test]
    fn test_batch_response_serialization() {
        use super::super::response::RpcResponse;

        let resp = BatchRpcResponse {
            results: vec![
                RpcResponse::success(serde_json::json!({"id": 1})),
                RpcResponse::error(super::super::response::RpcError::not_found("not found")),
            ],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"results\""));

        let parsed: BatchRpcResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.results.len(), 2);
        assert!(parsed.results[0].success);
        assert!(!parsed.results[1].success);
    }
}
