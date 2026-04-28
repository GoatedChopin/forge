use dioxus::prelude::*;

use crate::forge::use_get_iss_location_subscription;

fn format_coord(value: f64, is_lat: bool) -> String {
    let dir = if is_lat {
        if value >= 0.0 { "N" } else { "S" }
    } else if value >= 0.0 {
        "E"
    } else {
        "W"
    };
    format!("{:.4}\u{a0}{dir}", value.abs())
}

#[component]
pub fn IssCard() -> Element {
    let state = use_get_iss_location_subscription();
    let location = state.data.as_ref().and_then(|l| l.as_ref());

    rsx! {
        section { class: "card dark",
            h2 { "ISS Location " span { class: "badge", "cron" } }
            div { class: "stats",
                div {
                    span { class: "label", "Lat" }
                    span {
                        class: if location.is_some() { "value" } else { "value placeholder" },
                        {location.map(|l| format_coord(l.latitude, true)).unwrap_or("---.----".into())}
                    }
                }
                div {
                    span { class: "label", "Lon" }
                    span {
                        class: if location.is_some() { "value" } else { "value placeholder" },
                        {location.map(|l| format_coord(l.longitude, false)).unwrap_or("---.----".into())}
                    }
                }
                div {
                    span { class: "label", "Time" }
                    span {
                        class: if location.is_some() { "value" } else { "value placeholder" },
                        {location.map(|l| super::format_time(&l.api_timestamp)).unwrap_or("--:--:--".into())}
                    }
                }
            }
            if location.is_some() {
                p { class: "muted small", "Updated every minute via cron" }
            } else {
                p { class: "muted small", "Waiting for first cron run..." }
            }
        }
    }
}
