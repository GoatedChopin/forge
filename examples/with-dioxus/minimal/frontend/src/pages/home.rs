use dioxus::prelude::*;

use crate::api_url;

#[component]
pub fn Home() -> Element {
    rsx! {
        h1 { style: "font-size: 3rem; margin-bottom: 1rem;", "minimal" }
        p {
            style: "font-size: 1.1rem; max-width: 40rem;",
            "Your Forge backend is ready. Add models and functions, then run "
            code { "forge generate" }
            " to create typed Dioxus bindings in "
            code { "frontend/src/forge" }
            "."
        }
        p {
            style: "margin-top: 1.5rem;",
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
