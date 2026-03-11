
#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone)]
pub enum ForgeUpload {
    File(web_sys::File),
    Blob {
        blob: web_sys::Blob,
        file_name: Option<String>,
    },
}

#[cfg(target_arch = "wasm32")]
impl ForgeUpload {
    pub fn from_blob(blob: web_sys::Blob) -> Self {
        Self::Blob {
            blob,
            file_name: None,
        }
    }

    pub fn from_blob_with_name(blob: web_sys::Blob, file_name: impl Into<String>) -> Self {
        Self::Blob {
            blob,
            file_name: Some(file_name.into()),
        }
    }

    pub(crate) fn append_to_form(
        &self,
        form: &web_sys::FormData,
        field_name: &str,
    ) -> Result<(), crate::ForgeClientError> {
        match self {
            Self::File(file) => form
                .append_with_blob_and_filename(field_name, file, &file.name())
                .map_err(|_| crate::ForgeClientError::new("UPLOAD_FAILED", "Failed to append file", None)),
            Self::Blob { blob, file_name } => match file_name {
                Some(file_name) => form
                    .append_with_blob_and_filename(field_name, blob, file_name)
                    .map_err(|_| crate::ForgeClientError::new("UPLOAD_FAILED", "Failed to append blob", None)),
                None => form
                    .append_with_blob(field_name, blob)
                    .map_err(|_| crate::ForgeClientError::new("UPLOAD_FAILED", "Failed to append blob", None)),
            },
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl From<web_sys::File> for ForgeUpload {
    fn from(value: web_sys::File) -> Self {
        Self::File(value)
    }
}

#[cfg(target_arch = "wasm32")]
impl From<web_sys::Blob> for ForgeUpload {
    fn from(value: web_sys::Blob) -> Self {
        Self::from_blob(value)
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone)]
pub struct ForgeUpload {
    bytes: Vec<u8>,
    file_name: Option<String>,
    content_type: Option<String>,
}

#[cfg(not(target_arch = "wasm32"))]
impl ForgeUpload {
    pub fn from_bytes(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            bytes: bytes.into(),
            file_name: None,
            content_type: None,
        }
    }

    pub fn from_bytes_with_name(
        bytes: impl Into<Vec<u8>>,
        file_name: impl Into<String>,
    ) -> Self {
        Self {
            bytes: bytes.into(),
            file_name: Some(file_name.into()),
            content_type: None,
        }
    }

    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }

    pub fn into_part(self) -> Result<reqwest::multipart::Part, crate::ForgeClientError> {
        let mut part = reqwest::multipart::Part::bytes(self.bytes);
        if let Some(file_name) = self.file_name {
            part = part.file_name(file_name);
        }
        if let Some(content_type) = self.content_type {
            part = part
                .mime_str(&content_type)
                .map_err(|err| crate::ForgeClientError::new("UPLOAD_FAILED", err.to_string(), None))?;
        }
        Ok(part)
    }
}
