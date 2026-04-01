use dioxus::prelude::*;

#[component]
pub fn About() -> Element {
    rsx! {
        h1 { class: "page-title-sm", "About" }
        p { class: "page-text",
            "This app was scaffolded with Forge. Add new pages by creating a component in "
            code { "src/pages/" }
            " and registering a route in "
            code { "src/main.rs" }
            "."
        }
    }
}
