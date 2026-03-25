use dioxus::prelude::*;

use crate::Route;

#[component]
pub fn NotFound(segments: Vec<String>) -> Element {
    let path = format!("/{}", segments.join("/"));
    rsx! {
        div {
            style: "font-family: ui-sans-serif, system-ui, sans-serif; max-width: 56rem; margin: 0 auto; padding: 4rem 1.5rem; text-align: center;",
            h1 { style: "font-size: 4rem; margin-bottom: 0.5rem;", "404" }
            p { style: "font-size: 1.1rem; color: #6b7280; margin-bottom: 2rem;",
                "Nothing here at "
                code { "{path}" }
            }
            Link {
                to: Route::Home {},
                style: "color: #3b82f6; text-decoration: underline;",
                "Go home"
            }
        }
    }
}
