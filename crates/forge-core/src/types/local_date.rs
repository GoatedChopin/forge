//! Local date type (date without time).

use std::fmt;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// A local date without time information.
///
/// Represents a calendar date like "2024-01-15" without any time or timezone.
/// Useful for birthdays, due dates, and other date-only values.
///
/// # Examples
///
/// ```
/// use forge_core::types::LocalDate;
///
/// let date = LocalDate::today();
/// let christmas = LocalDate::from_ymd(2024, 12, 25).unwrap();
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LocalDate(NaiveDate);

impl LocalDate {
    /// Create a date from year, month, and day.
    pub fn from_ymd(year: i32, month: u32, day: u32) -> Option<Self> {
        NaiveDate::from_ymd_opt(year, month, day).map(Self)
    }

    /// Get today's date in the local timezone.
    pub fn today() -> Self {
        Self(chrono::Local::now().date_naive())
    }

    /// Get today's date in UTC.
    pub fn today_utc() -> Self {
        Self(chrono::Utc::now().date_naive())
    }

    /// Get the inner `NaiveDate`.
    pub fn into_inner(self) -> NaiveDate {
        self.0
    }

    /// Format to ISO 8601 date string (YYYY-MM-DD).
    pub fn to_iso8601(&self) -> String {
        self.0.format("%Y-%m-%d").to_string()
    }

    /// Parse from ISO 8601 date string (YYYY-MM-DD).
    pub fn parse_iso8601(s: &str) -> Result<Self, chrono::ParseError> {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").map(Self)
    }

    /// Add days to this date. Use negative values to subtract.
    pub fn add_days(self, days: i64) -> Option<Self> {
        if days >= 0 {
            self.0
                .checked_add_days(chrono::Days::new(days as u64))
                .map(Self)
        } else {
            self.0
                .checked_sub_days(chrono::Days::new(days.unsigned_abs()))
                .map(Self)
        }
    }

    /// Calculate days between two dates.
    pub fn days_until(self, other: LocalDate) -> i64 {
        (other.0 - self.0).num_days()
    }
}

impl Default for LocalDate {
    fn default() -> Self {
        Self::today_utc()
    }
}

impl fmt::Display for LocalDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_iso8601())
    }
}

impl From<NaiveDate> for LocalDate {
    fn from(date: NaiveDate) -> Self {
        Self(date)
    }
}

impl From<LocalDate> for NaiveDate {
    fn from(date: LocalDate) -> Self {
        date.0
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use chrono::Datelike;

    #[test]
    fn test_from_ymd() {
        let date = LocalDate::from_ymd(2024, 1, 15).unwrap();
        let inner = date.into_inner();
        assert_eq!(inner.year(), 2024);
        assert_eq!(inner.month(), 1);
        assert_eq!(inner.day(), 15);
    }

    #[test]
    fn test_invalid_date() {
        assert!(LocalDate::from_ymd(2024, 2, 30).is_none());
        assert!(LocalDate::from_ymd(2024, 13, 1).is_none());
    }

    #[test]
    fn test_serialization() {
        let date = LocalDate::from_ymd(2024, 1, 15).unwrap();
        let json = serde_json::to_string(&date).unwrap();
        assert_eq!(json, "\"2024-01-15\"");
        let parsed: LocalDate = serde_json::from_str(&json).unwrap();
        assert_eq!(date, parsed);
    }

    #[test]
    fn test_iso8601_roundtrip() {
        let date = LocalDate::from_ymd(2024, 12, 25).unwrap();
        let iso = date.to_iso8601();
        assert_eq!(iso, "2024-12-25");
        let parsed = LocalDate::parse_iso8601(&iso).unwrap();
        assert_eq!(date, parsed);
    }

    #[test]
    fn test_days_until() {
        let date1 = LocalDate::from_ymd(2024, 1, 1).unwrap();
        let date2 = LocalDate::from_ymd(2024, 1, 10).unwrap();
        assert_eq!(date1.days_until(date2), 9);
        assert_eq!(date2.days_until(date1), -9);
    }
}
