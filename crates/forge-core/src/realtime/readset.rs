use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use smallvec::SmallVec;
use uuid::Uuid;

/// Tracking mode for read sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrackingMode {
    /// No tracking (disabled).
    None,
    /// Track only tables (coarse-grained).
    Table,
    /// Track individual rows (fine-grained).
    Row,
    /// Adaptive mode - automatically choose based on query characteristics.
    #[default]
    Adaptive,
}

impl TrackingMode {
    /// Convert to string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Table => "table",
            Self::Row => "row",
            Self::Adaptive => "adaptive",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseTrackingModeError(pub String);

impl std::fmt::Display for ParseTrackingModeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid tracking mode: {}", self.0)
    }
}

impl std::error::Error for ParseTrackingModeError {}

impl FromStr for TrackingMode {
    type Err = ParseTrackingModeError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "none" => Ok(Self::None),
            "table" => Ok(Self::Table),
            "row" => Ok(Self::Row),
            "adaptive" => Ok(Self::Adaptive),
            _ => Err(ParseTrackingModeError(s.to_string())),
        }
    }
}

/// Bloom filter for probabilistic row-level tracking.
/// False positives cause one unnecessary re-execution (caught by result hash comparison).
/// False negatives never happen.
#[derive(Debug, Clone)]
pub struct BloomFilter {
    bits: Vec<u64>,
    num_hashes: u32,
    num_bits: u64,
}

impl BloomFilter {
    /// Create a bloom filter sized for `expected_items` with ~1% false positive rate.
    pub fn new(expected_items: usize) -> Self {
        // ~10 bits per element for ~1% FPR
        let num_bits = (expected_items as u64 * 10).max(64);
        let num_words = ((num_bits + 63) / 64) as usize;
        // Optimal number of hash functions: (m/n) * ln(2) ~ 7 for 10 bits/element
        let num_hashes = 7;

        Self {
            bits: vec![0u64; num_words],
            num_hashes,
            num_bits,
        }
    }

    /// Insert an item into the bloom filter.
    pub fn insert(&mut self, item: Uuid) {
        let bytes = item.as_bytes();
        for i in 0..self.num_hashes {
            let idx = self.hash(bytes, i);
            let word = (idx / 64) as usize;
            let bit = idx % 64;
            if let Some(w) = self.bits.get_mut(word) {
                *w |= 1u64 << bit;
            }
        }
    }

    /// Test if an item might be in the filter (probabilistic).
    pub fn might_contain(&self, item: Uuid) -> bool {
        let bytes = item.as_bytes();
        for i in 0..self.num_hashes {
            let idx = self.hash(bytes, i);
            let word = (idx / 64) as usize;
            let bit = idx % 64;
            match self.bits.get(word) {
                Some(w) if (w >> bit) & 1 == 1 => continue,
                _ => return false,
            }
        }
        true
    }

    fn hash(&self, bytes: &[u8; 16], seed: u32) -> u64 {
        // Double hashing: h(i) = h1 + i*h2
        let h1 = u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);
        let h2 = u64::from_le_bytes([
            bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
        ]);
        h1.wrapping_add((seed as u64).wrapping_mul(h2)) % self.num_bits
    }

    /// Approximate memory usage in bytes.
    pub fn memory_bytes(&self) -> usize {
        self.bits.len() * 8 + 16
    }
}

/// Read set tracking tables and rows read during query execution.
#[derive(Debug, Clone, Default)]
pub struct ReadSet {
    /// Tables accessed (stack-allocated for common case of 1-4 tables).
    pub tables: SmallVec<[String; 4]>,
    /// Probabilistic row tracking per table. None means table-level only.
    pub row_filter: HashMap<String, BloomFilter>,
    /// Row counts per table (for memory estimation).
    pub row_counts: HashMap<String, usize>,
    /// Columns used in filters.
    pub filter_columns: HashMap<String, HashSet<String>>,
    /// Tracking mode used.
    pub mode: TrackingMode,
}

impl ReadSet {
    /// Create a new empty read set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a read set with table-level tracking.
    pub fn table_level() -> Self {
        Self {
            mode: TrackingMode::Table,
            ..Default::default()
        }
    }

    /// Create a read set with row-level tracking.
    pub fn row_level() -> Self {
        Self {
            mode: TrackingMode::Row,
            ..Default::default()
        }
    }

    /// Add a table to the read set.
    pub fn add_table(&mut self, table: impl Into<String>) {
        let table = table.into();
        if !self.tables.contains(&table) {
            self.tables.push(table);
        }
    }

    /// Add a row to the read set using bloom filter.
    pub fn add_row(&mut self, table: impl Into<String>, row_id: Uuid) {
        let table = table.into();
        if !self.tables.contains(&table) {
            self.tables.push(table.clone());
        }
        let filter = self
            .row_filter
            .entry(table.clone())
            .or_insert_with(|| BloomFilter::new(1000));
        filter.insert(row_id);
        *self.row_counts.entry(table).or_insert(0) += 1;
    }

    /// Add a filter column.
    pub fn add_filter_column(&mut self, table: impl Into<String>, column: impl Into<String>) {
        self.filter_columns
            .entry(table.into())
            .or_default()
            .insert(column.into());
    }

    /// Check if this read set includes a specific table.
    pub fn includes_table(&self, table: &str) -> bool {
        self.tables.iter().any(|t| t == table)
    }

    /// Check if this read set includes a specific row.
    pub fn includes_row(&self, table: &str, row_id: Uuid) -> bool {
        if !self.includes_table(table) {
            return false;
        }

        if self.mode == TrackingMode::Table {
            return true;
        }

        if let Some(filter) = self.row_filter.get(table) {
            filter.might_contain(row_id)
        } else {
            // No bloom filter means all rows tracked at table level
            true
        }
    }

    /// Estimate memory usage in bytes.
    pub fn memory_bytes(&self) -> usize {
        let table_bytes = self.tables.iter().map(|s| s.len() + 24).sum::<usize>();
        let filter_bytes: usize = self.row_filter.values().map(|f| f.memory_bytes()).sum();
        let col_bytes = self
            .filter_columns
            .values()
            .map(|set| set.iter().map(|s| s.len() + 24).sum::<usize>())
            .sum::<usize>();

        table_bytes + filter_bytes + col_bytes + 64
    }

    /// Get total row count tracked.
    pub fn row_count(&self) -> usize {
        self.row_counts.values().sum()
    }

    /// Merge another read set into this one.
    pub fn merge(&mut self, other: &ReadSet) {
        for table in &other.tables {
            if !self.tables.contains(table) {
                self.tables.push(table.clone());
            }
        }

        for (table, columns) in &other.filter_columns {
            self.filter_columns
                .entry(table.clone())
                .or_default()
                .extend(columns.iter().cloned());
        }
    }
}

/// Change operation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeOperation {
    /// Row inserted.
    Insert,
    /// Row updated.
    Update,
    /// Row deleted.
    Delete,
}

impl ChangeOperation {
    /// Convert to string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Insert => "INSERT",
            Self::Update => "UPDATE",
            Self::Delete => "DELETE",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseChangeOperationError(pub String);

impl std::fmt::Display for ParseChangeOperationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid change operation: {}", self.0)
    }
}

impl std::error::Error for ParseChangeOperationError {}

impl FromStr for ChangeOperation {
    type Err = ParseChangeOperationError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "INSERT" | "I" => Ok(Self::Insert),
            "UPDATE" | "U" => Ok(Self::Update),
            "DELETE" | "D" => Ok(Self::Delete),
            _ => Err(ParseChangeOperationError(s.to_string())),
        }
    }
}

/// A database change event.
#[derive(Debug, Clone)]
pub struct Change {
    /// Table that changed.
    pub table: String,
    /// Type of operation.
    pub operation: ChangeOperation,
    /// Row ID that changed.
    pub row_id: Option<Uuid>,
    /// Columns that changed (for updates).
    pub changed_columns: Vec<String>,
}

impl Change {
    /// Create a new change event.
    pub fn new(table: impl Into<String>, operation: ChangeOperation) -> Self {
        Self {
            table: table.into(),
            operation,
            row_id: None,
            changed_columns: Vec::new(),
        }
    }

    /// Set the row ID.
    pub fn with_row_id(mut self, row_id: Uuid) -> Self {
        self.row_id = Some(row_id);
        self
    }

    /// Set the changed columns.
    pub fn with_columns(mut self, columns: Vec<String>) -> Self {
        self.changed_columns = columns;
        self
    }

    /// Check if this change should invalidate a read set, optionally filtering
    /// by compile-time selected columns from the query.
    pub fn invalidates(&self, read_set: &ReadSet) -> bool {
        if !read_set.includes_table(&self.table) {
            return false;
        }

        // For row-level tracking, check if the specific row was read
        if read_set.mode == TrackingMode::Row
            && let Some(row_id) = self.row_id
        {
            match self.operation {
                ChangeOperation::Update | ChangeOperation::Delete => {
                    return read_set.includes_row(&self.table, row_id);
                }
                // Inserts always potentially invalidate (new row might match filter)
                ChangeOperation::Insert => {}
            }
        }

        true
    }

    /// Column-aware invalidation: returns false if the changed columns don't
    /// overlap with the query's selected columns.
    pub fn invalidates_columns(&self, selected_columns: &[&str]) -> bool {
        // If we don't know changed columns or selected columns, be conservative
        if self.changed_columns.is_empty() || selected_columns.is_empty() {
            return true;
        }

        // Only for updates, since inserts/deletes affect row presence
        if self.operation != ChangeOperation::Update {
            return true;
        }

        self.changed_columns
            .iter()
            .any(|c| selected_columns.contains(&c.as_str()))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn test_tracking_mode_conversion() {
        assert_eq!("table".parse::<TrackingMode>(), Ok(TrackingMode::Table));
        assert_eq!("row".parse::<TrackingMode>(), Ok(TrackingMode::Row));
        assert_eq!(
            "adaptive".parse::<TrackingMode>(),
            Ok(TrackingMode::Adaptive)
        );
        assert!("invalid".parse::<TrackingMode>().is_err());
    }

    #[test]
    fn test_read_set_add_table() {
        let mut read_set = ReadSet::new();
        read_set.add_table("projects");

        assert!(read_set.includes_table("projects"));
        assert!(!read_set.includes_table("users"));
    }

    #[test]
    fn test_read_set_add_row() {
        let mut read_set = ReadSet::row_level();
        let row_id = Uuid::new_v4();
        read_set.add_row("projects", row_id);

        assert!(read_set.includes_table("projects"));
        assert!(read_set.includes_row("projects", row_id));
        // Bloom filter: might_contain could return true for other IDs (false positive)
        // but never false for inserted IDs (no false negatives)
    }

    #[test]
    fn test_bloom_filter_no_false_negatives() {
        let mut filter = BloomFilter::new(100);
        let ids: Vec<Uuid> = (0..100).map(|_| Uuid::new_v4()).collect();

        for id in &ids {
            filter.insert(*id);
        }

        for id in &ids {
            assert!(filter.might_contain(*id), "bloom filter should never miss an inserted item");
        }
    }

    #[test]
    fn test_bloom_filter_false_positive_rate() {
        let mut filter = BloomFilter::new(1000);
        let inserted: Vec<Uuid> = (0..1000).map(|_| Uuid::new_v4()).collect();
        for id in &inserted {
            filter.insert(*id);
        }

        let not_inserted: Vec<Uuid> = (0..10000).map(|_| Uuid::new_v4()).collect();
        let false_positives = not_inserted
            .iter()
            .filter(|id| filter.might_contain(**id))
            .count();

        // Expect <2% FPR (we target ~1%)
        assert!(
            false_positives < 200,
            "false positive rate too high: {}/10000",
            false_positives
        );
    }

    #[test]
    fn test_change_invalidates_table_level() {
        let mut read_set = ReadSet::table_level();
        read_set.add_table("projects");

        let change = Change::new("projects", ChangeOperation::Insert);
        assert!(change.invalidates(&read_set));

        let change = Change::new("users", ChangeOperation::Insert);
        assert!(!change.invalidates(&read_set));
    }

    #[test]
    fn test_change_invalidates_row_level() {
        let mut read_set = ReadSet::row_level();
        let tracked_id = Uuid::new_v4();
        read_set.add_row("projects", tracked_id);

        // Update to tracked row should invalidate
        let change = Change::new("projects", ChangeOperation::Update).with_row_id(tracked_id);
        assert!(change.invalidates(&read_set));

        // Insert always potentially invalidates
        let other_id = Uuid::new_v4();
        let change = Change::new("projects", ChangeOperation::Insert).with_row_id(other_id);
        assert!(change.invalidates(&read_set));
    }

    #[test]
    fn test_column_invalidation() {
        let change = Change::new("users", ChangeOperation::Update)
            .with_columns(vec!["name".to_string(), "email".to_string()]);

        // Overlapping columns should invalidate
        assert!(change.invalidates_columns(&["name", "age"]));

        // Non-overlapping columns should not
        assert!(!change.invalidates_columns(&["age", "phone"]));

        // Empty selected columns means unknown, invalidate conservatively
        assert!(change.invalidates_columns(&[]));
    }

    #[test]
    fn test_column_invalidation_non_update() {
        // Inserts and deletes always invalidate regardless of columns
        let change = Change::new("users", ChangeOperation::Insert)
            .with_columns(vec!["name".to_string()]);
        assert!(change.invalidates_columns(&["age"]));
    }

    #[test]
    fn test_read_set_merge() {
        let mut read_set1 = ReadSet::new();
        read_set1.add_table("projects");

        let mut read_set2 = ReadSet::new();
        read_set2.add_table("users");

        read_set1.merge(&read_set2);

        assert!(read_set1.includes_table("projects"));
        assert!(read_set1.includes_table("users"));
    }
}
