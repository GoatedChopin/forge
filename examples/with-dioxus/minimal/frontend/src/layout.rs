use dioxus::prelude::*;

use crate::Route;

#[component]
pub fn AppLayout() -> Element {
    rsx! {
        div { class: "app-layout",
            nav { class: "app-nav",
                div { class: "app-nav-inner",
                    Link { to: Route::Home {}, class: "nav-brand", "minimal" }
                    div { class: "nav-links",
                        Link { to: Route::Home {}, class: "nav-link", "Home" }
                        Link { to: Route::About {}, class: "nav-link", "About" }
                    }
                }
            }
            main { class: "app-main",
                Outlet::<Route> {}
            }
        }
    }
}
