use dioxus::prelude::*;

use crate::Route;

#[component]
pub fn NotFound(segments: Vec<String>) -> Element {
    let path = format!("/{}", segments.join("/"));
    rsx! {
        main { class: "shell",
            h1 { "404" }
            p { "Nothing here at " code { "{path}" } }
            p {
                Link { to: Route::DemoPage {}, "Back to demo" }
            }
        }
    }
}
