//! Built-in embedded frontend handler.
//!
//! Provides `serve_embedded_assets` which creates an SPA-aware handler from
//! any type implementing `rust_embed::Embed`. Users just derive the embed
//! struct and pass it as a type parameter.
//!
//! # Example
//!
//! ```ignore
//! #[derive(rust_embed::Embed)]
//! #[folder = "frontend/dist"]
//! struct Assets;
//!
//! let builder = builder.frontend_handler(forge::serve_embedded_assets::<Assets>);
//! ```

use std::future::Future;
use std::pin::Pin;

use axum::body::Body;
use axum::http::Request;
use axum::response::Response;

/// Serve embedded assets from a `rust_embed::Embed` type with SPA fallback.
///
/// Handles MIME type detection and falls back to `index.html` for
/// client-side routing. Use as a type-parameterized function pointer
/// with `ForgeBuilder::frontend_handler()`.
pub fn serve_embedded_assets<E: rust_embed::Embed + 'static>(
    req: Request<Body>,
) -> Pin<Box<dyn Future<Output = Response> + Send>> {
    Box::pin(async move {
        use axum::http::{StatusCode, header};
        use axum::response::IntoResponse;

        let path = req.uri().path().trim_start_matches('/');
        let path = if path.is_empty() { "index.html" } else { path };

        match E::get(path) {
            Some(content) => {
                let mime = mime_guess::from_path(path).first_or_octet_stream();
                ([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response()
            }
            None => match E::get("index.html") {
                Some(content) => {
                    ([(header::CONTENT_TYPE, "text/html")], content.data).into_response()
                }
                None => (StatusCode::NOT_FOUND, "not found").into_response(),
            },
        }
    })
}
