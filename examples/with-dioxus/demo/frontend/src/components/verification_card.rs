use dioxus::prelude::*;
use forge_dioxus::WorkflowStatus;

use crate::forge::{User, VerificationInput, use_account_verification};

#[component]
pub fn VerificationCard(selected_user: Signal<Option<User>>) -> Element {
    let mut run_request = use_signal(|| None::<(u64, String, String)>);

    let start = move |_| {
        let nonce = run_request().as_ref().map(|(n, _, _)| n + 1).unwrap_or(1);
        let (account_id, email) = match selected_user() {
            Some(u) => (u.id.clone(), u.email.clone()),
            None => ("demo-user".into(), "demo@example.com".into()),
        };
        run_request.set(Some((nonce, account_id, email)));
    };

    rsx! {
        section { class: "card",
            h2 { "Verification " span { class: "badge purple", "workflow" } }
            if let Some((nonce, account_id, email)) = run_request() {
                VerificationRun { key: "{nonce}", account_id, email, on_restart: start }
            } else {
                p { class: "muted small workflow-desc", "Multi-step workflow with durable sleep" }
                button { onclick: start, "Start Workflow" }
            }
        }
    }
}

#[component]
fn VerificationRun(
    account_id: String,
    email: String,
    on_restart: EventHandler<MouseEvent>,
) -> Element {
    let wf = use_account_verification(VerificationInput::new(account_id, email));
    let can_restart = matches!(
        wf.state.status,
        WorkflowStatus::Completed | WorkflowStatus::Failed | WorkflowStatus::Compensated
    );

    rsx! {
        div { class: "steps",
            for step in wf.state.steps.iter() {
                div { key: "{step.name}", class: "step {step.status}",
                    span { class: "icon", {step_icon(&step.status)} }
                    span { "{step.name}" }
                }
            }
        }
        if can_restart {
            button { onclick: move |e| on_restart.call(e), "Run Again" }
        }
    }
}

fn step_icon(status: &str) -> &'static str {
    match status {
        "completed" => "[=]",
        "running" => "[>]",
        "failed" => "[x]",
        _ => "[ ]",
    }
}
