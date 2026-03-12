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
                style: "font-family: ui-sans-serif, system-ui, sans-serif; max-width: 56rem; margin: 0 auto; padding: 4rem 1.5rem; line-height: 1.5;",
                h1 { style: "font-size: 3rem; margin-bottom: 1rem;", "minimal" }
                p { style: "font-size: 1.1rem; max-width: 40rem;", "Your Forge backend is ready. Add models and functions, then run `forge generate` to create typed Dioxus bindings in `frontend/src/forge`." }
                p { style: "margin-top: 1.5rem;", "Backend: " a { href: format!("{}/_api/health", api_url()), target: "_blank", rel: "noopener noreferrer", "{api_url()}" } }
            }
        }
    }
}
