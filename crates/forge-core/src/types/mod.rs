//! Compile-time safe datetime and upload types for Forge.
//!
//! These types provide runtime safety guarantees:
//! - `Instant`: UTC timestamp, no `From<NaiveDateTime>` to prevent timezone confusion
//! - `LocalDate`: Date without time (YYYY-MM-DD)
//! - `LocalTime`: Time without date (HH:MM:SS)
//! - `Upload`: File upload, panics if you try to store in DB

mod instant;
mod local_date;
mod local_time;
mod upload;

pub use instant::Instant;
pub use local_date::LocalDate;
pub use local_time::LocalTime;
pub use upload::Upload;
