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
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_str(ts));
    date.to_locale_time_string("en-US")
        .as_string()
        .unwrap_or_else(|| ts.to_string())
}

pub fn generate_key() -> String {
    let ms = js_sys::Date::now() as u64;
    let suffix = format!("{:06x}", (js_sys::Math::random() * 16_777_215.0) as u32);
    format!("{ms}-{suffix}")
}
