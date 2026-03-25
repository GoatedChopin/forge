use dioxus::prelude::*;

use crate::Route;

#[component]
pub fn AppLayout() -> Element {
    rsx! {
        div {
            style: "font-family: ui-sans-serif, system-ui, sans-serif; min-height: 100vh; display: flex; flex-direction: column;",
            nav {
                style: "border-bottom: 1px solid #e5e7eb; padding: 1rem 1.5rem;",
                div {
                    style: "max-width: 56rem; margin: 0 auto; display: flex; align-items: center; gap: 2rem;",
                    Link {
                        to: Route::Home {},
                        style: "font-weight: 700; font-size: 1.1rem; text-decoration: none; color: inherit;",
                        "minimal"
                    }
                    div {
                        style: "display: flex; gap: 1.5rem;",
                        Link {
                            to: Route::Home {},
                            style: "text-decoration: none; color: #6b7280;",
                            "Home"
                        }
                        Link {
                            to: Route::About {},
                            style: "text-decoration: none; color: #6b7280;",
                            "About"
                        }
                    }
                }
            }
            main {
                style: "flex: 1; max-width: 56rem; width: 100%; margin: 0 auto; padding: 2rem 1.5rem; line-height: 1.5;",
                Outlet::<Route> {}
            }
        }
    }
}
