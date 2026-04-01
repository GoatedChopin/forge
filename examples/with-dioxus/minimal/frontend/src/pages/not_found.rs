use dioxus::prelude::*;

use crate::Route;

#[component]
pub fn NotFound(segments: Vec<String>) -> Element {
    let path = format!("/{}", segments.join("/"));
    rsx! {
        div { class: "not-found",
            h1 { class: "not-found-code", "404" }
            p { class: "not-found-text",
                "Nothing here at "
                code { "{path}" }
            }
            Link { to: Route::Home {}, "Go home" }
        }
    }
}
