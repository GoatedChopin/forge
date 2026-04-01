use dioxus::prelude::*;

use crate::api_url;

#[component]
pub fn Home() -> Element {
    rsx! {
        h1 { class: "page-title", "minimal" }
        p { class: "page-text",
            "Your Forge backend is ready. Add models and functions, then run "
            code { "forge generate" }
            " to create typed Dioxus bindings in "
            code { "frontend/src/forge" }
            "."
        }
        p { class: "page-link",
            "Backend: "
            a {
                href: format!("{}/_api/health", api_url()),
                target: "_blank",
                rel: "noopener noreferrer",
                "{api_url()}"
            }
        }
    }
}
