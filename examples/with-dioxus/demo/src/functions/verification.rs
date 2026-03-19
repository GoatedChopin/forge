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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use forge::testing::TestWorkflowContext;

    #[test]
    fn test_workflow_context_creation() {
        let ctx = TestWorkflowContext::builder("account_verification").build();

        assert_eq!(ctx.workflow_name, "account_verification");
        assert_eq!(ctx.version, 1);
        assert!(!ctx.is_resumed());
    }

    #[test]
    fn test_workflow_step_tracking() {
        let ctx = TestWorkflowContext::builder("account_verification").build();

        assert!(!ctx.is_step_completed("step1"));

        ctx.record_step_start("step1");
        ctx.record_step_complete("step1", serde_json::json!({"result": "ok"}));

        assert!(ctx.is_step_completed("step1"));

        let result: Option<serde_json::Value> = ctx.get_step_result("step1");
        assert!(result.is_some());
    }

    #[test]
    fn test_workflow_resume_with_completed_steps() {
        let ctx = TestWorkflowContext::builder("account_verification")
            .as_resumed()
            .with_completed_step("generate_token", serde_json::json!("verify_abc123"))
            .with_completed_step("store_token", serde_json::json!({"stored": true}))
            .build();

        assert!(ctx.is_resumed());
        assert!(ctx.is_step_completed("generate_token"));
        assert!(ctx.is_step_completed("store_token"));
        assert!(!ctx.is_step_completed("send_email"));

        let token: String = ctx.get_step_result("generate_token").unwrap();
        assert_eq!(token, "verify_abc123");
    }

    #[test]
    fn test_workflow_step_ordering() {
        let ctx = TestWorkflowContext::builder("account_verification").build();

        ctx.record_step_complete("step_a", serde_json::json!(1));
        ctx.record_step_complete("step_b", serde_json::json!(2));
        ctx.record_step_complete("step_c", serde_json::json!(3));

        let names = ctx.completed_step_names();
        assert_eq!(names, vec!["step_a", "step_b", "step_c"]);
    }

    #[tokio::test]
    async fn test_workflow_durable_sleep() {
        let ctx = TestWorkflowContext::builder("account_verification").build();

        assert!(!ctx.sleep_called());
        ctx.sleep(std::time::Duration::from_secs(3600))
            .await
            .unwrap();
        assert!(ctx.sleep_called());
    }

    #[test]
    fn test_workflow_deterministic_time() {
        let fixed_time = Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap();
        let ctx = TestWorkflowContext::builder("account_verification")
            .with_workflow_time(fixed_time)
            .build();

        assert_eq!(ctx.workflow_time(), fixed_time);
    }

    #[test]
    fn test_workflow_with_tenant() {
        let tenant_id = Uuid::new_v4();
        let ctx = TestWorkflowContext::builder("account_verification")
            .with_tenant(tenant_id)
            .build();

        assert_eq!(ctx.tenant_id(), Some(tenant_id));
    }
}
