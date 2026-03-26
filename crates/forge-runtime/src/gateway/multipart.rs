use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Extension, Multipart, Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use bytes::BytesMut;

use forge_core::function::AuthContext;
use forge_core::types::Upload;

use super::rpc::RpcHandler;

const DEFAULT_MAX_TOTAL_UPLOAD_SIZE: usize = 20 * 1024 * 1024;
const MAX_UPLOAD_FIELDS: usize = 20;
const MAX_FIELD_NAME_LENGTH: usize = 255;
const MAX_JSON_FIELD_SIZE: usize = 1024 * 1024;
const JSON_FIELD_NAME: &str = "_json";

/// Configurable limits for multipart uploads, injected via Axum extension.
#[derive(Debug, Clone)]
pub struct MultipartConfig {
    pub max_body_size_bytes: usize,
}

/// Create a multipart error response.
fn multipart_error(
    status: StatusCode,
    code: &str,
    message: impl Into<String>,
) -> (StatusCode, axum::Json<serde_json::Value>) {
    (
        status,
        axum::Json(serde_json::json!({
            "success": false,
            "error": {
                "code": code,
                "message": message.into()
            }
        })),
    )
}

/// Handle multipart form data for RPC calls with file uploads.
pub async fn rpc_multipart_handler(
    State(handler): State<Arc<RpcHandler>>,
    Extension(auth): Extension<AuthContext>,
    Extension(mp_config): Extension<MultipartConfig>,
    Path(function): Path<String>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let max_total = mp_config.max_body_size_bytes.max(DEFAULT_MAX_TOTAL_UPLOAD_SIZE);
    let max_file = max_total;

    let mut json_args: Option<serde_json::Value> = None;
    let mut uploads: HashMap<String, Upload> = HashMap::new();
    let mut total_read: usize = 0;

    // Parse multipart fields
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => {
                return multipart_error(StatusCode::BAD_REQUEST, "MULTIPART_ERROR", e.to_string());
            }
        };

        let name = match field.name().map(String::from).filter(|n| !n.is_empty()) {
            Some(n) => n,
            None => {
                return multipart_error(
                    StatusCode::BAD_REQUEST,
                    "INVALID_FIELD",
                    "Field name is required",
                );
            }
        };

        // Validate field name length
        if name.len() > MAX_FIELD_NAME_LENGTH {
            return multipart_error(
                StatusCode::BAD_REQUEST,
                "INVALID_FIELD",
                format!("Field name too long (max {} chars)", MAX_FIELD_NAME_LENGTH),
            );
        }

        if name.contains("..") || name.contains('/') || name.contains('\\') {
            return multipart_error(
                StatusCode::BAD_REQUEST,
                "INVALID_FIELD",
                "Field name contains invalid characters",
            );
        }

        // Check upload count before processing to prevent bypass via _json field ordering
        if name != JSON_FIELD_NAME && uploads.len() >= MAX_UPLOAD_FIELDS {
            return multipart_error(
                StatusCode::BAD_REQUEST,
                "TOO_MANY_FIELDS",
                format!("Maximum {} upload fields allowed", MAX_UPLOAD_FIELDS),
            );
        }

        if name == JSON_FIELD_NAME {
            let mut buffer = BytesMut::new();
            let mut json_field = field;

            loop {
                match json_field.chunk().await {
                    Ok(Some(chunk)) => {
                        if total_read + chunk.len() > max_total {
                            return multipart_error(
                                StatusCode::PAYLOAD_TOO_LARGE,
                                "PAYLOAD_TOO_LARGE",
                                format!(
                                    "Multipart payload exceeds maximum size of {} bytes",
                                    max_total
                                ),
                            );
                        }
                        if buffer.len() + chunk.len() > MAX_JSON_FIELD_SIZE {
                            return multipart_error(
                                StatusCode::PAYLOAD_TOO_LARGE,
                                "JSON_TOO_LARGE",
                                format!(
                                    "_json field exceeds maximum size of {} bytes",
                                    MAX_JSON_FIELD_SIZE
                                ),
                            );
                        }
                        total_read += chunk.len();
                        buffer.extend_from_slice(&chunk);
                    }
                    Ok(None) => break,
                    Err(e) => {
                        return multipart_error(
                            StatusCode::BAD_REQUEST,
                            "READ_ERROR",
                            format!("Failed to read _json field: {}", e),
                        );
                    }
                }
            }

            let text = match std::str::from_utf8(&buffer) {
                Ok(s) => s,
                Err(_) => {
                    return multipart_error(
                        StatusCode::BAD_REQUEST,
                        "INVALID_JSON",
                        "Invalid UTF-8 in _json field",
                    );
                }
            };

            match serde_json::from_str(text) {
                Ok(value) => json_args = Some(value),
                Err(e) => {
                    return multipart_error(
                        StatusCode::BAD_REQUEST,
                        "INVALID_JSON",
                        format!("Invalid JSON in _json field: {}", e),
                    );
                }
            }
        } else {
            let filename = field
                .file_name()
                .map(String::from)
                .unwrap_or_else(|| name.clone());
            let content_type = field
                .content_type()
                .map(String::from)
                .unwrap_or_else(|| "application/octet-stream".to_string());

            let mut buffer = BytesMut::new();
            let mut field = field;

            loop {
                match field.chunk().await {
                    Ok(Some(chunk)) => {
                        if total_read + chunk.len() > max_total {
                            return multipart_error(
                                StatusCode::PAYLOAD_TOO_LARGE,
                                "PAYLOAD_TOO_LARGE",
                                format!(
                                    "Multipart payload exceeds maximum size of {} bytes",
                                    max_total
                                ),
                            );
                        }
                        if buffer.len() + chunk.len() > max_file {
                            return multipart_error(
                                StatusCode::PAYLOAD_TOO_LARGE,
                                "FILE_TOO_LARGE",
                                format!(
                                    "File '{}' exceeds maximum size of {} bytes",
                                    filename, max_file
                                ),
                            );
                        }
                        total_read += chunk.len();
                        buffer.extend_from_slice(&chunk);
                    }
                    Ok(None) => break,
                    Err(e) => {
                        return multipart_error(
                            StatusCode::BAD_REQUEST,
                            "READ_ERROR",
                            format!("Failed to read file field: {}", e),
                        );
                    }
                }
            }

            let upload = Upload::new(filename, content_type, buffer.freeze());
            uploads.insert(name, upload);
        }
    }

    let mut args = json_args.unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    if let serde_json::Value::Object(ref mut map) = args {
        for (name, upload) in uploads {
            match serde_json::to_value(&upload) {
                Ok(value) => {
                    map.insert(name, value);
                }
                Err(e) => {
                    return multipart_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "SERIALIZE_ERROR",
                        format!("Failed to serialize upload: {}", e),
                    );
                }
            }
        }
    }

    let request = super::request::RpcRequest::new(function, args);
    let metadata = forge_core::function::RequestMetadata::new();

    let response = handler.handle(request, auth, metadata).await;

    match serde_json::to_value(&response) {
        Ok(value) => (StatusCode::OK, axum::Json(value)),
        Err(e) => multipart_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SERIALIZE_ERROR",
            format!("Failed to serialize response: {}", e),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_field_name_constant() {
        assert_eq!(JSON_FIELD_NAME, "_json");
    }
}
