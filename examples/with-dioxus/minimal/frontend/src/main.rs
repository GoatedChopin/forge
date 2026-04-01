mod forge;
mod layout;
mod pages;

use dioxus::prelude::*;
use forge_dioxus::ForgeProvider;

use layout::AppLayout;
use pages::{About, Home, NotFound};

fn api_url() -> &'static str {
    option_env!("FORGE_API_URL").unwrap_or("http://localhost:9081")
}

#[derive(Routable, Clone)]
#[rustfmt::skip]
enum Route {
    #[layout(AppLayout)]
        #[route("/")]
        Home {},
        #[route("/about")]
        About {},
    #[end_layout]
    #[route("/:..segments")]
    NotFound { segments: Vec<String> },
}

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        document::Title { "minimal" }
        document::Stylesheet { href: asset!("/public/style.css") }
        ForgeProvider {
            url: api_url().to_string(),
            Router::<Route> {}
        }
    }
}
