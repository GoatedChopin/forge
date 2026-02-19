mod auth;
pub mod jwks;
mod mcp;
mod multipart;
mod request;
mod response;
mod rpc;
mod server;
mod sse;
mod tracing;

pub use auth::{AuthConfig, AuthMiddleware, build_auth_context_from_claims};
pub use jwks::{JwksClient, JwksError};
pub use mcp::{McpState, mcp_get_handler, mcp_post_handler};
pub use multipart::rpc_multipart_handler;
pub use request::RpcRequest;
pub use response::{RpcError, RpcResponse};
pub use rpc::RpcHandler;
pub use server::{GatewayConfig, GatewayServer};
pub use sse::{
    SseConfig, SsePayload, SseQuery, SseState, sse_handler, sse_job_subscribe_handler,
    sse_subscribe_handler, sse_unsubscribe_handler, sse_workflow_subscribe_handler,
};
pub use tracing::TracingMiddleware;
