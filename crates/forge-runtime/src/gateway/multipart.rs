use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Extension, Multipart, Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use bytes::BytesMut;

use forge_core::function::AuthContext;
use forge_core::types::Upload;

use super::rpc::RpcHandler;

const MAX_UPLOAD_FIELDS: usize = 20;
const MAX_FIELD_NAME_LENGTH: usize = 255;
const MAX_JSON_FIELD_SIZE: usize = 1024 * 1024;
const JSON_FIELD_NAME: &str = "_json";

/// Configurable limits for multipart uploads, injected via Axum extension.
///
/// `max_body_size_bytes` caps the total request body when a mutation does
/// not declare its own `max_size`. `max_file_size_bytes` caps any single
/// file under the same conditions; per-mutation `max_size` overrides both
/// (it is treated as an explicit opt-in for large single files).
#[derive(Debug, Clone)]
pub struct MultipartConfig {
    pub max_body_size_bytes: usize,
    pub max_file_size_bytes: usize,
}

/// Resolve the effective (total, per-file) upload limits for a request.
///
/// When a mutation declares `max_size`, the value acts as an explicit
/// opt-in: it caps both the total body and any single file. Without an
/// override, the total falls back to `max_body_size_bytes` and any single
/// file is capped by `max_file_size_bytes` (clamped to the total).
fn resolve_upload_limits(per_mutation: Option<usize>, config: &MultipartConfig) -> (usize, usize) {
    match per_mutation {
        Some(limit) => (limit, limit),
        None => (
            config.max_body_size_bytes,
            config.max_file_size_bytes.min(config.max_body_size_bytes),
        ),
    }
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
    // Validate function name to prevent log injection and path traversal
    if function.is_empty()
        || function.len() > 256
        || !function
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == ':' || c == '-')
    {
        return multipart_error(
            StatusCode::BAD_REQUEST,
            "INVALID_FUNCTION",
            "Invalid function name: must be 1-256 alphanumeric characters, underscores, dots, colons, or hyphens",
        );
    }

    let per_mutation = handler
        .function_info(&function)
        .and_then(|info| info.max_upload_size_bytes);
    let (max_total, max_file) = resolve_upload_limits(per_mutation, &mp_config);

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

        if name.contains("..")
            || name.contains('/')
            || name.contains('\\')
            || name.contains(|c: char| c.is_control())
        {
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
            let raw_filename = field
                .file_name()
                .map(String::from)
                .unwrap_or_else(|| name.clone());
            // Sanitize filename: strip path components to prevent path traversal
            let filename = raw_filename
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or(&raw_filename)
                .replace("..", "_")
                .to_string();
            if filename.is_empty() {
                return multipart_error(
                    StatusCode::BAD_REQUEST,
                    "INVALID_FILENAME",
                    "Filename is empty after sanitization",
                );
            }
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
            // Prevent upload fields from overwriting JSON args (parameter tampering)
            if map.contains_key(&name) {
                return multipart_error(
                    StatusCode::BAD_REQUEST,
                    "DUPLICATE_FIELD",
                    format!("Upload field '{}' conflicts with JSON argument", name),
                );
            }
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

    // Use RpcResponse's IntoResponse to preserve correct HTTP status codes
    let status = if response.success {
        StatusCode::OK
    } else {
        response
            .error
            .as_ref()
            .map(|e| e.status_code())
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
    };
    match serde_json::to_value(&response) {
        Ok(value) => (status, axum::Json(value)),
        Err(e) => multipart_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SERIALIZE_ERROR",
            format!("Failed to serialize response: {}", e),
        ),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn config(body: usize, file: usize) -> MultipartConfig {
        MultipartConfig {
            max_body_size_bytes: body,
            max_file_size_bytes: file,
        }
    }

    #[test]
    fn test_json_field_name_constant() {
        assert_eq!(JSON_FIELD_NAME, "_json");
    }

    #[test]
    fn per_mutation_limit_overrides_both_total_and_file() {
        let cfg = config(20 * MB, 10 * MB);
        let (total, file) = resolve_upload_limits(Some(200 * MB), &cfg);
        assert_eq!(total, 200 * MB);
        assert_eq!(file, 200 * MB);
    }

    #[test]
    fn without_override_uses_global_body_and_file_limits() {
        let cfg = config(50 * MB, 10 * MB);
        let (total, file) = resolve_upload_limits(None, &cfg);
        assert_eq!(total, 50 * MB);
        assert_eq!(file, 10 * MB);
    }

    #[test]
    fn file_limit_clamped_to_body_limit() {
        let cfg = config(5 * MB, 50 * MB);
        let (total, file) = resolve_upload_limits(None, &cfg);
        assert_eq!(total, 5 * MB);
        assert_eq!(file, 5 * MB);
    }

    #[test]
    fn zero_per_mutation_is_still_respected() {
        let cfg = config(20 * MB, 10 * MB);
        let (total, file) = resolve_upload_limits(Some(0), &cfg);
        assert_eq!(total, 0);
        assert_eq!(file, 0);
    }

    const MB: usize = 1024 * 1024;
}
