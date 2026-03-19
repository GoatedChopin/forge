use dioxus::prelude::*;
use forge_dioxus::JobStatus;

use crate::forge::{ExportInput, use_export_users};

#[component]
pub fn ExportCard() -> Element {
    let mut run_nonce = use_signal(|| 0_u64);

    rsx! {
        section { class: "card",
            h2 { "Export Job " span { class: "badge", "job" } }
            if run_nonce() == 0 {
                p { class: "muted small export-desc", "Ready to export users to CSV" }
                button { onclick: move |_| run_nonce += 1, "Start Export" }
            } else {
                ExportRun { key: "{run_nonce()}", on_restart: move |_| run_nonce += 1 }
            }
        }
    }
}

#[component]
fn ExportRun(on_restart: EventHandler<MouseEvent>) -> Element {
    let job = use_export_users(ExportInput::new("csv"));
    let progress = job.state.progress.unwrap_or(0.0).clamp(0.0, 100.0);
    let message = job
        .state
        .message
        .clone()
        .unwrap_or_else(|| format!("{:?}", job.state.status).to_lowercase());
    let can_restart = matches!(
        job.state.status,
        JobStatus::Completed | JobStatus::Failed | JobStatus::Pending
    );

    rsx! {
        div { class: "progress-bar",
            div { class: "fill", style: "width: {progress}%;" }
        }
        p { class: "progress-text", "{progress:.0}% - {message}" }
        if can_restart {
            button { onclick: move |e| on_restart.call(e), "Run Again" }
        }
    }
}
