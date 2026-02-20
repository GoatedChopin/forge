use forge::prelude::*;

mod functions;
mod schema;

#[cfg(feature = "embedded-frontend")]
mod embedded {
    use axum::{
        body::Body,
        http::{Request, StatusCode, header},
        response::{IntoResponse, Response},
    };
    use rust_embed::Embed;
    use std::future::Future;
    use std::pin::Pin;

    #[derive(Embed)]
    #[folder = "frontend/build"]
    pub struct Assets;

    async fn serve_frontend_inner(req: Request<Body>) -> Response {
        let path = req.uri().path().trim_start_matches('/');
        let path = if path.is_empty() { "index.html" } else { path };

        match Assets::get(path) {
            Some(content) => {
                let mime = mime_guess::from_path(path).first_or_octet_stream();
                ([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response()
            }
            None => {
                // SPA fallback - serve index.html for client-side routing
                match Assets::get("index.html") {
                    Some(content) => {
                        ([(header::CONTENT_TYPE, "text/html")], content.data).into_response()
                    }
                    None => (StatusCode::NOT_FOUND, "not found").into_response(),
                }
            }
        }
    }

    pub fn serve_frontend(req: Request<Body>) -> Pin<Box<dyn Future<Output = Response> + Send>> {
        Box::pin(serve_frontend_inner(req))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let config = ForgeConfig::from_file("forge.toml")?;
    let mut builder = Forge::builder();

    builder
        .function_registry_mut()
        .register_query::<functions::ListSupportTicketsQuery>();
    builder
        .function_registry_mut()
        .register_mutation::<functions::CreateSupportTicketMutation>();
    builder
        .function_registry_mut()
        .register_mutation::<functions::SetTicketStatusMutation>();
    builder
        .function_registry_mut()
        .register_mutation::<functions::SetTicketPriorityMutation>();
    builder
        .function_registry_mut()
        .register_mutation::<functions::AddTicketNoteMutation>();

    builder
        .mcp_registry_mut()
        .register::<functions::McpListSupportTicketsMcpTool>();
    builder
        .mcp_registry_mut()
        .register::<functions::McpCreateSupportTicketMcpTool>();
    builder
        .mcp_registry_mut()
        .register::<functions::McpSetTicketStatusMcpTool>();
    builder
        .mcp_registry_mut()
        .register::<functions::McpSetTicketPriorityMcpTool>();
    builder
        .mcp_registry_mut()
        .register::<functions::McpAddTicketNoteMcpTool>();

    #[cfg(feature = "embedded-frontend")]
    builder.frontend_handler(embedded::serve_frontend);

    builder.config(config).build()?.run().await
}
