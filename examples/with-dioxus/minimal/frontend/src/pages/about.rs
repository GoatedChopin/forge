use dioxus::prelude::*;

#[component]
pub fn About() -> Element {
    rsx! {
        h1 { style: "font-size: 2rem; margin-bottom: 1rem;", "About" }
        p {
            style: "max-width: 40rem;",
            "This app was scaffolded with Forge. Add new pages by creating a component in "
            code { "src/pages/" }
            " and registering a route in "
            code { "src/main.rs" }
            "."
        }
    }
}
