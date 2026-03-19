mod export_card;
mod iss_card;
mod trades_card;
mod users_section;
mod verification_card;
mod webhook_card;

pub use export_card::ExportCard;
pub use iss_card::IssCard;
pub use trades_card::TradesCard;
pub use users_section::UsersSection;
pub use verification_card::VerificationCard;
pub use webhook_card::WebhookCard;

pub fn format_time(ts: &str) -> String {
    if ts.is_empty() {
        return "-".into();
    }
    #[cfg(target_arch = "wasm32")]
    {
        let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_str(ts));
        date.to_locale_time_string("en-US")
            .as_string()
            .unwrap_or_else(|| ts.to_string())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        chrono::DateTime::parse_from_rfc3339(ts)
            .map(|dt| dt.format("%I:%M:%S %p").to_string())
            .unwrap_or_else(|_| ts.to_string())
    }
}

pub fn generate_key() -> String {
    #[cfg(target_arch = "wasm32")]
    let ms = js_sys::Date::now() as u64;
    #[cfg(not(target_arch = "wasm32"))]
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let suffix = format!("{:06x}", rand::random::<u32>() & 0xFF_FFFF);
    format!("{ms}-{suffix}")
}
