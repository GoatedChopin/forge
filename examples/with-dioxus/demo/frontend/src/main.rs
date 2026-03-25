mod components;
mod forge;
mod layout;
mod pages;

use dioxus::prelude::*;

use forge::ForgeAuthProvider;
use layout::AppLayout;
use pages::{DemoPage, NotFound};

const API_URL: &str = match option_env!("FORGE_API_URL") {
    Some(url) => url,
    None => "http://localhost:9081",
};

#[derive(Routable, Clone)]
#[rustfmt::skip]
enum Route {
    #[layout(AppLayout)]
        #[route("/")]
        DemoPage {},
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
        document::Title { "Forge Demo" }
        document::Stylesheet { href: asset!("/public/style.css") }
        ForgeAuthProvider { url: API_URL.to_string(), app_name: "forge-demo".to_string(), Router::<Route> {} }
    }
}
