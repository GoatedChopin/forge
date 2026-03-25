use dioxus::prelude::*;

use crate::Route;

#[component]
pub fn AppLayout() -> Element {
    rsx! {
        div { class: "app-layout",
            nav { class: "app-nav",
                Link { to: Route::DemoPage {}, class: "nav-brand", "Forge Demo" }
            }
            Outlet::<Route> {}
        }
    }
}
