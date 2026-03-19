mod components;
mod forge;

use dioxus::prelude::*;

use components::{ExportCard, IssCard, TradesCard, UsersSection, VerificationCard, WebhookCard};
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
            div { class: "grid",
                div { class: "stack",
                    IssCard {}
                    ExportCard {}
                    VerificationCard { selected_user }
                }
                div { class: "stack",
                    TradesCard {}
                    WebhookCard { api_url: API_URL.to_string() }
                }
            }
            UsersSection { selected_user }
        }
    }
}
