use dioxus::prelude::*;
use forge_dioxus::use_signals;
use serde_json::json;

use crate::forge::{ForgeUpload, UploadFileParams, UploadResult, use_upload_file};

#[component]
pub fn UploadCard() -> Element {
    let upload_file = use_upload_file();
    let signals = use_signals();
    let mut result = use_signal(|| None::<UploadResult>);
    let mut error = use_signal(|| None::<String>);
    let mut is_uploading = use_signal(|| false);

    let handle_upload = {
        let upload_file = upload_file.clone();
        let signals = signals.clone();
        move |_: MouseEvent| {
            let upload_file = upload_file.clone();
            let signals = signals.clone();
            spawn(async move {
                // Extract the file from the input element
                let file = {
                    #[cfg(target_arch = "wasm32")]
                    {
                        use wasm_bindgen::JsCast;
                        let document = web_sys::window()
                            .and_then(|w| w.document());
                        document.and_then(|doc| {
                            doc.get_element_by_id("upload-input")
                                .and_then(|el| el.dyn_into::<web_sys::HtmlInputElement>().ok())
                                .and_then(|input| input.files())
                                .and_then(|files| files.get(0))
                        })
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        None::<()>
                    }
                };

                #[cfg(target_arch = "wasm32")]
                if let Some(file) = file {
                    is_uploading.set(true);
                    error.set(None);
                    result.set(None);
                    let upload = ForgeUpload::from(file);
                    match upload_file.call(UploadFileParams::new(upload)).await {
                        Ok(res) => {
                            signals.track(
                                "file_uploaded",
                                json!({"name": &res.name, "size": res.size}),
                            );
                            result.set(Some(res));
                        }
                        Err(err) => {
                            signals.track("file_upload_error", json!({}));
                            error.set(Some(err.message));
                        }
                    }
                    is_uploading.set(false);
                }

                #[cfg(not(target_arch = "wasm32"))]
                {
                    let _ = (upload_file, signals, is_uploading, error, result, file);
                }
            });
        }
    };

    rsx! {
        section { class: "card",
            h2 {
                "File Upload "
                span { class: "badge green", "multipart" }
            }

            div { class: "upload-row",
                input { id: "upload-input", r#type: "file" }
                button {
                    onclick: handle_upload,
                    disabled: is_uploading(),
                    if is_uploading() { "Uploading..." } else { "Upload" }
                }
            }

            if let Some(res) = result() {
                div { class: "upload-result",
                    div { class: "stat-row",
                        span { class: "meta-key", "Name" }
                        span { class: "mono", "{res.name}" }
                    }
                    div { class: "stat-row",
                        span { class: "meta-key", "Type" }
                        span { class: "mono", "{res.content_type}" }
                    }
                    div { class: "stat-row",
                        span { class: "meta-key", "Size" }
                        span { class: "mono", "{res.size} bytes" }
                    }
                }
            }

            if let Some(err) = error() {
                p { class: "hint warning", "{err}" }
            }
        }
    }
}
