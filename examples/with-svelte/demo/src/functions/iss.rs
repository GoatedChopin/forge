use forge::prelude::*;

/// ISS Location record
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct IssLocation {
    pub id: Uuid,
    pub latitude: f64,
    pub longitude: f64,
    pub api_timestamp: Timestamp,
    pub created_at: Timestamp,
}

#[derive(Debug, serde::Deserialize)]
struct IssApiResponse {
    iss_position: IssPosition,
    timestamp: i64,
    message: String,
}

#[derive(Debug, serde::Deserialize)]
struct IssPosition {
    latitude: String,
    longitude: String,
}

/// Get the latest ISS location from database
#[forge::query(public)]
pub async fn get_iss_location(ctx: &QueryContext) -> Result<Option<IssLocation>> {
    sqlx::query_as!(
        IssLocation,
        r#"
        SELECT id, latitude, longitude, api_timestamp, created_at
        FROM iss_location
        ORDER BY created_at DESC
        LIMIT 1
        "#
    )
    .fetch_optional(ctx.db())
    .await
    .map_err(Into::into)
}

/// Polls ISS location every minute from Open Notify API
#[forge::cron("* * * * *", timezone = "UTC")]
pub async fn iss_location(ctx: &CronContext) -> Result<()> {
    ctx.log.info(
        "Fetching ISS location",
        serde_json::json!({"run_id": ctx.run_id.to_string()}),
    );

    let response = ctx
        .http()
        .get("http://api.open-notify.org/iss-now.json")
        .send()
        .await
        .map_err(|e| ForgeError::Internal(format!("HTTP request failed: {}", e)))?;

    if !response.status().is_success() {
        ctx.log.error(
            "Failed to fetch ISS location",
            serde_json::json!({"status": response.status().as_u16()}),
        );
        return Err(ForgeError::Internal("Failed to fetch ISS location".into()));
    }

    let data: IssApiResponse = response
        .json()
        .await
        .map_err(|e| ForgeError::Deserialization(format!("Failed to parse: {}", e)))?;

    if data.message != "success" {
        ctx.log.warn(
            "ISS API non-success",
            serde_json::json!({"message": data.message}),
        );
    }

    let latitude: f64 = data.iss_position.latitude.parse().unwrap_or(0.0);
    let longitude: f64 = data.iss_position.longitude.parse().unwrap_or(0.0);

    sqlx::query!(
        "INSERT INTO iss_location (id, latitude, longitude, api_timestamp, created_at) \
         VALUES (gen_random_uuid(), $1, $2, to_timestamp($3), NOW())",
        latitude,
        longitude,
        data.timestamp as f64
    )
    .execute(ctx.db())
    .await?;

    ctx.log.debug(
        "ISS location stored",
        serde_json::json!({
            "latitude": latitude,
            "longitude": longitude
        }),
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use forge::testing::TestCronContext;

    #[test]
    fn test_cron_context_creation() {
        let ctx = TestCronContext::builder("iss_location").build();

        assert_eq!(ctx.cron_name, "iss_location");
        assert!(!ctx.is_catch_up);
        assert!(!ctx.is_late());
    }

    #[test]
    fn test_cron_logging() {
        let ctx = TestCronContext::builder("iss_location").build();

        ctx.log.info("Starting");
        ctx.log.warn("Warning message");
        ctx.log.error("Error occurred");

        let entries = ctx.log.entries();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].level, "info");
        assert_eq!(entries[1].level, "warn");
        assert_eq!(entries[2].level, "error");
    }

    #[test]
    fn test_cron_late_detection() {
        let ctx = TestCronContext::builder("iss_location")
            .scheduled_at(Utc::now() - Duration::minutes(5))
            .build();

        assert!(ctx.is_late());
        assert!(ctx.delay() >= Duration::minutes(4));
    }

    #[test]
    fn test_cron_on_time() {
        let ctx = TestCronContext::builder("iss_location")
            .scheduled_at(Utc::now())
            .build();

        assert!(!ctx.is_late());
    }

    #[test]
    fn test_cron_catch_up_mode() {
        let ctx = TestCronContext::builder("iss_location")
            .as_catch_up()
            .build();

        assert!(ctx.is_catch_up);
    }

    #[test]
    fn test_cron_timezone() {
        let ctx = TestCronContext::builder("iss_location")
            .with_timezone("America/New_York")
            .build();

        assert_eq!(ctx.timezone, "America/New_York");
    }
}
