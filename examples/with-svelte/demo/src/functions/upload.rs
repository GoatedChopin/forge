use forge::prelude::*;

/// Result of a file upload, returning metadata about the received file.
#[derive(Debug, Serialize, Deserialize)]
pub struct UploadResult {
    pub name: String,
    pub content_type: String,
    pub size: usize,
}

/// Accept a file upload and return its metadata.
#[forge::mutation(public)]
pub async fn upload_file(_ctx: &MutationContext, file: Upload) -> Result<UploadResult> {
    Ok(UploadResult {
        name: file.name().to_string(),
        content_type: file.content_type().to_string(),
        size: file.len(),
    })
}
