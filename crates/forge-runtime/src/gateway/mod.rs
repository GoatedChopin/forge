mod auth;
pub mod jwks;
mod mcp;
mod multipart;
pub mod oauth;
mod request;
mod response;
mod rpc;
mod server;
mod sse;
mod tls;
mod tracing;

pub use auth::{AuthConfig, AuthMiddleware, HmacTokenIssuer, build_auth_context_from_claims};
pub use jwks::{JwksClient, JwksError};
pub use mcp::{McpState, mcp_get_handler, mcp_post_handler};
pub use multipart::rpc_multipart_handler;
pub use oauth::OAuthState;
pub use request::{BatchRpcRequest, BatchRpcResponse, RpcRequest};
pub use response::{RpcError, RpcResponse};
pub use rpc::RpcHandler;
pub use server::{GatewayConfig, GatewayServer};
pub use sse::{
    SseConfig, SsePayload, SseQuery, SseState, sse_handler, sse_job_subscribe_handler,
    sse_subscribe_handler, sse_unsubscribe_handler, sse_workflow_subscribe_handler,
};
pub use tls::{TlsListenConfig, TlsListener, bind as bind_tls, load_rustls_config};
pub use tracing::TracingMiddleware;

pub(crate) fn extract_client_ip(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
}

pub(crate) fn extract_header(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.is_empty())
        .map(String::from)
}
