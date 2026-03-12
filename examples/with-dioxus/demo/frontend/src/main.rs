mod forge;

use dioxus::prelude::*;
use forge_dioxus::ForgeProvider;

fn api_url() -> &'static str {
    option_env!("FORGE_API_URL").unwrap_or("http://localhost:8080")
}

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        ForgeProvider {
            url: api_url().to_string(),
            main {
                style: "font-family: ui-sans-serif, system-ui, sans-serif; max-width: 64rem; margin: 0 auto; padding: 4rem 1.5rem; line-height: 1.5;",
                h1 { style: "font-size: 3rem; margin-bottom: 1rem;", "demo" }
                p { style: "font-size: 1.1rem; max-width: 46rem;", "The demo backend is scaffolded. Run `forge generate` after backend changes to refresh the Dioxus bindings in `frontend/src/forge`, then build out your UI with the generated client and hooks." }
                ul {
                    style: "margin-top: 2rem; display: grid; gap: 0.75rem; padding-left: 1.25rem;",
                    li { "Generated bindings land in `frontend/src/forge`." }
                    li { "The `forge-dioxus` runtime crate is pulled from crates.io." }
                    li { "The release build embeds `frontend/dist` into the Rust binary." }
                }
            }
        }
    }
}
