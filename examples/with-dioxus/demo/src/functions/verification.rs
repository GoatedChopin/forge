use forge::prelude::*;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationInput {
    pub account_id: String,
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationOutput {
    pub verified: bool,
    pub token: String,
}

/// Multi-step account verification with durable sleep
///
/// Demonstrates:
/// - Step tracking with is_step_completed/record_step_*
/// - Durable sleep that survives server restarts
/// - Resumption detection with is_resumed()
#[forge::workflow(version = 1, timeout = "24h", public)]
pub async fn account_verification(
    ctx: &WorkflowContext,
    input: VerificationInput,
) -> Result<VerificationOutput> {
    if ctx.is_resumed() {
        tracing::info!(workflow_id = %ctx.run_id, "Resuming verification workflow");
    }

    // Step 1: Generate token
    let token = if ctx.is_step_completed("generate_token") {
        ctx.get_step_result::<String>("generate_token")
            .unwrap_or_else(|| format!("verify_{}", Uuid::new_v4()))
    } else {
        ctx.record_step_start("generate_token");
        tracing::info!("Generating verification token");
        tokio::time::sleep(Duration::from_millis(600)).await;
        let token = format!("verify_{}", Uuid::new_v4());
        ctx.record_step_complete("generate_token", serde_json::json!(token));
        token
    };

    // Step 2: Store token
    if !ctx.is_step_completed("store_token") {
        ctx.record_step_start("store_token");
        tracing::info!("Storing verification token");
        tokio::time::sleep(Duration::from_millis(600)).await;
        ctx.record_step_complete("store_token", serde_json::json!({"stored": true}));
    }

    // Step 3: Send email
    if !ctx.is_step_completed("send_email") {
        ctx.record_step_start("send_email");
        tracing::info!(email = %input.email, "Sending verification email");
        tokio::time::sleep(Duration::from_millis(600)).await;
        ctx.record_step_complete("send_email", serde_json::json!({"sent": true}));
    }

    // Step 4: Durable sleep (survives server restarts)
    if !ctx.is_step_completed("wait_period") {
        ctx.record_step_start("wait_period");
        tracing::info!("Entering durable sleep (2 seconds)");
        ctx.sleep(Duration::from_secs(2)).await?;
        ctx.record_step_complete_async("wait_period", serde_json::json!({"waited": true}))
            .await;
    }

    // Step 5: Mark verified
    if !ctx.is_step_completed("mark_verified") {
        ctx.record_step_start("mark_verified");
        tracing::info!(account_id = %input.account_id, "Marking account verified");
        tokio::time::sleep(Duration::from_millis(600)).await;
        ctx.record_step_complete("mark_verified", serde_json::json!(true));
    }

    tracing::info!(workflow_id = %ctx.run_id, "Verification complete");

    Ok(VerificationOutput {
        verified: true,
        token,
    })
}
