//! UTC instant type with compile-time safety.

use std::fmt;
use std::time::SystemTime;

use chrono::{DateTime, FixedOffset, Utc};
use serde::{Deserialize, Serialize};

/// A UTC instant in time.
///
/// This type intentionally does NOT implement `From<NaiveDateTime>` to prevent
/// accidental timezone confusion. If you have a naive datetime, you must
/// explicitly specify how to interpret it.
///
/// # Examples
///
/// ```
/// use forge_core::types::Instant;
/// use chrono::Utc;
///
/// // From UTC datetime
/// let now = Instant::now();
///
/// // From chrono DateTime<Utc>
/// let instant: Instant = Utc::now().into();
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Instant(DateTime<Utc>);

impl Instant {
    /// Create an instant representing the current time.
    pub fn now() -> Self {
        Self(Utc::now())
    }

    /// Create an instant from a Unix timestamp (seconds since epoch).
    pub fn from_timestamp(secs: i64) -> Option<Self> {
        DateTime::from_timestamp(secs, 0).map(Self)
    }

    /// Create an instant from a Unix timestamp with nanoseconds.
    pub fn from_timestamp_nanos(nanos: i64) -> Self {
        Self(DateTime::from_timestamp_nanos(nanos))
    }

    /// Get the Unix timestamp (seconds since epoch).
    pub fn timestamp(&self) -> i64 {
        self.0.timestamp()
    }

    /// Get the Unix timestamp with millisecond precision.
    pub fn timestamp_millis(&self) -> i64 {
        self.0.timestamp_millis()
    }

    /// Get the inner `DateTime<Utc>`.
    pub fn into_inner(self) -> DateTime<Utc> {
        self.0
    }

    /// Format to ISO 8601 string.
    pub fn to_iso8601(&self) -> String {
        self.0.to_rfc3339()
    }

    /// Parse from ISO 8601 string.
    pub fn parse_iso8601(s: &str) -> Result<Self, chrono::ParseError> {
        DateTime::parse_from_rfc3339(s).map(|dt| Self(dt.with_timezone(&Utc)))
    }
}

impl Default for Instant {
    fn default() -> Self {
        Self::now()
    }
}

impl fmt::Display for Instant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_iso8601())
    }
}

impl From<DateTime<Utc>> for Instant {
    fn from(dt: DateTime<Utc>) -> Self {
        Self(dt)
    }
}

impl From<DateTime<FixedOffset>> for Instant {
    fn from(dt: DateTime<FixedOffset>) -> Self {
        Self(dt.with_timezone(&Utc))
    }
}

impl From<SystemTime> for Instant {
    fn from(st: SystemTime) -> Self {
        Self(DateTime::from(st))
    }
}

impl From<Instant> for DateTime<Utc> {
    fn from(instant: Instant) -> Self {
        instant.0
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use chrono::Timelike;

    #[test]
    fn test_now() {
        let instant = Instant::now();
        assert!(instant.timestamp() > 0);
    }

    #[test]
    fn test_from_timestamp() {
        let instant = Instant::from_timestamp(1704067200).unwrap();
        assert_eq!(instant.timestamp(), 1704067200);
    }

    #[test]
    fn test_serialization() {
        let instant = Instant::from_timestamp(1704067200).unwrap();
        let json = serde_json::to_string(&instant).unwrap();
        let parsed: Instant = serde_json::from_str(&json).unwrap();
        assert_eq!(instant, parsed);
    }

    #[test]
    fn test_from_utc_datetime() {
        let dt = Utc::now();
        let instant: Instant = dt.into();
        assert_eq!(instant.into_inner(), dt);
    }

    #[test]
    fn test_from_fixed_offset() {
        use chrono::TimeZone;
        let offset = FixedOffset::east_opt(5 * 3600).unwrap();
        let dt = offset.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let instant: Instant = dt.into();
        // Should be converted to UTC (12:00 +5:00 = 07:00 UTC)
        assert_eq!(instant.into_inner().hour(), 7);
    }

    #[test]
    fn test_iso8601_roundtrip() {
        let instant = Instant::now();
        let iso = instant.to_iso8601();
        let parsed = Instant::parse_iso8601(&iso).unwrap();
        assert_eq!(instant.timestamp(), parsed.timestamp());
    }
}
