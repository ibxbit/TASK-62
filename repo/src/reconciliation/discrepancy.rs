/// Discrepancy detection logic.
///
/// This module answers two questions independently of any database:
///   1. Which statement records are duplicates of each other?
///   2. What discrepancy type applies to a (DB amount, statement amount) pair?
use std::collections::HashMap;

use super::models::{DiscrepancyType, StatementRecord};
use bigdecimal::BigDecimal;
use std::str::FromStr;

// ============================================================
// Tolerance constant
// ============================================================

/// Maximum permitted difference between DB amount and statement amount for
/// a pair to be classified as `matched`.  Anything strictly greater than this
/// is `amount_mismatch`.
pub const AMOUNT_TOLERANCE: &str = "0.01"; // $0.01

// ============================================================
// Duplicate detection
// ============================================================

/// Group statement records by `reference` and identify duplicates.
///
/// Returns a map from each reference that appears **more than once** to the
/// indices of all rows carrying that reference.  References appearing exactly
/// once are not included in the map.
///
/// # Example
/// ```
/// // ["TXN001", "TXN002", "TXN001"]  →  {"TXN001": [0, 2]}
/// ```
pub fn find_duplicates<'a>(records: &'a [StatementRecord]) -> HashMap<&'a str, Vec<usize>> {
    let mut groups: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, rec) in records.iter().enumerate() {
        groups.entry(rec.reference.as_str()).or_default().push(i);
    }
    groups.retain(|_, indices| indices.len() > 1);
    groups
}

/// Returns `true` if the record at `idx` is considered a duplicate
/// (i.e. its reference appears more than once in the full record set).
pub fn is_duplicate(idx: usize, dup_map: &HashMap<&str, Vec<usize>>) -> bool {
    dup_map.values().any(|indices| indices.contains(&idx))
}

/// For duplicate groups, the first occurrence (lowest index) is considered the
/// canonical entry; all others are the duplicates.
pub fn canonical_index(indices: &[usize]) -> usize {
    *indices.iter().min().expect("non-empty")
}

// ============================================================
// Amount comparison
// ============================================================

/// Classify the relationship between a DB-side expected amount and a
/// statement-side actual amount.
///
/// Assumes both amounts are non-negative and the pair has already been matched
/// on transaction reference (i.e. they refer to the same transaction).
pub fn classify_amounts(expected: &BigDecimal, actual: &BigDecimal) -> DiscrepancyType {
    let tolerance = BigDecimal::from_str(AMOUNT_TOLERANCE).unwrap();
    if (expected - actual).abs() <= tolerance {
        DiscrepancyType::Matched
    } else {
        DiscrepancyType::AmountMismatch
    }
}

/// Compute the net financial discrepancy: `actual − expected`.
///
/// Positive → over-collected.  Negative → under-collected.
pub fn net_discrepancy(expected: &BigDecimal, actual: &BigDecimal) -> BigDecimal {
    actual - expected
}

// ============================================================
// Discrepancy summary
// ============================================================

/// Counts across all discrepancy types in a run.
#[derive(Debug, Default)]
pub struct DiscrepancySummary {
    pub matched:               usize,
    pub amount_mismatches:     usize,
    pub missing_from_statement: usize,
    pub extra_in_statement:    usize,
    pub duplicates:            usize,
}

impl DiscrepancySummary {
    pub fn total_discrepancies(&self) -> usize {
        self.amount_mismatches
            + self.missing_from_statement
            + self.extra_in_statement
            + self.duplicates
    }

    pub fn is_clean(&self) -> bool { self.total_discrepancies() == 0 }

    /// Returns true when discrepancy count exceeds 10 or 5% of total records.
    pub fn is_high(&self, total_records: usize) -> bool {
        let n = self.total_discrepancies();
        n > 10 || (total_records > 0 && n * 100 / total_records > 5)
    }
}

// ============================================================
// Tests
// ============================================================
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use super::super::models::EntryType;

    fn rec(r: &str) -> StatementRecord {
        StatementRecord {
            reference:   r.to_string(),
            amount:      100.0,
            entry_type:  EntryType::Credit,
            date:        NaiveDate::from_ymd_opt(2025, 1, 15).unwrap(),
            description: None,
        }
    }

    #[test]
    fn no_duplicates_returns_empty_map() {
        let records = vec![rec("A"), rec("B"), rec("C")];
        let dups = find_duplicates(&records);
        assert!(dups.is_empty());
    }

    #[test]
    fn single_duplicate_pair_detected() {
        let records = vec![rec("A"), rec("B"), rec("A")];
        let dups = find_duplicates(&records);
        assert_eq!(dups.len(), 1);
        assert!(dups.contains_key("A"));
        assert_eq!(dups["A"], vec![0, 2]);
    }

    #[test]
    fn triple_occurrence_detected() {
        let records = vec![rec("X"), rec("X"), rec("X"), rec("Y")];
        let dups = find_duplicates(&records);
        assert_eq!(dups["X"].len(), 3);
    }

    #[test]
    fn canonical_index_is_first() {
        assert_eq!(canonical_index(&[3, 1, 5]), 1);
    }

    #[test]
    fn classify_amounts_within_tolerance() {
        assert_eq!(classify_amounts(100.00, 100.00), DiscrepancyType::Matched);
        assert_eq!(classify_amounts(100.00, 100.01), DiscrepancyType::Matched);
        assert_eq!(classify_amounts(100.00,  99.99), DiscrepancyType::Matched);
    }

    #[test]
    fn classify_amounts_exceeds_tolerance() {
        assert_eq!(classify_amounts(100.00, 100.02), DiscrepancyType::AmountMismatch);
        assert_eq!(classify_amounts(100.00,  99.98), DiscrepancyType::AmountMismatch);
    }

    #[test]
    fn net_discrepancy_positive_and_negative() {
        assert!((net_discrepancy(100.0, 120.0) -  20.0).abs() < 1e-9);
        assert!((net_discrepancy(100.0,  80.0) - -20.0).abs() < 1e-9);
    }

    #[test]
    fn summary_high_discrepancy_count() {
        let s = DiscrepancySummary {
            matched: 5,
            amount_mismatches: 11,
            ..Default::default()
        };
        assert!(s.is_high(100));
    }

    #[test]
    fn summary_high_discrepancy_percentage() {
        let s = DiscrepancySummary {
            matched:            90,
            missing_from_statement: 6,
            ..Default::default()
        };
        assert!(s.is_high(100)); // 6/100 = 6% > 5%
    }

    #[test]
    fn summary_clean() {
        let s = DiscrepancySummary { matched: 50, ..Default::default() };
        assert!(s.is_clean());
    }

    #[test]
    fn is_duplicate_works() {
        let records = vec![rec("A"), rec("B"), rec("A")];
        let dups = find_duplicates(&records);
        assert!(is_duplicate(0, &dups));
        assert!(is_duplicate(2, &dups));
        assert!(!is_duplicate(1, &dups));
    }
}
