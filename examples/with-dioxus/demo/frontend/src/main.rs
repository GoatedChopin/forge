mod components;
mod forge;

use dioxus::prelude::*;

use components::{
    AuthCard, CacheCard, ExportCard, IssCard, McpCard, TradesCard, UsersSection,
    VerificationCard, WebhookCard,
};
use forge::{ForgeProvider, User};

const API_URL: &str = match option_env!("FORGE_API_URL") {
    Some(url) => url,
    None => "http://localhost:8080",
};

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        document::Title { "Forge Demo" }
        document::Stylesheet { href: asset!("/public/style.css") }
        ForgeProvider { url: API_URL.to_string(), DemoPage {} }
    }
}

#[component]
fn DemoPage() -> Element {
    let selected_user = use_signal(|| None::<User>);

    rsx! {
        main { class: "shell",
            h1 { "Forge Demo" }

            div { class: "columns",
                div { class: "col",
                    IssCard {}
                    CacheCard {}
                    ExportCard {}
                    McpCard { api_url: API_URL.to_string() }
                }
                div { class: "col",
                    TradesCard {}
                    AuthCard {}
                    WebhookCard { api_url: API_URL.to_string() }
                    VerificationCard { selected_user }
                }
            }

            UsersSection { selected_user }
        }
    }
}
