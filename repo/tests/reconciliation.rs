//! Reconciliation discrepancy detection tests.
//!
//! All tests in this file exercise pure in-memory functions from the
//! `reconciliation::discrepancy` module.  They require no database or file I/O.
//!
//! Run: `cargo test --test reconciliation`

use bigdecimal::BigDecimal;
use chrono::NaiveDate;
use std::str::FromStr;
use transitops_backend::reconciliation::{
    discrepancy::{
        canonical_index, classify_amounts, find_duplicates, is_duplicate,
        net_discrepancy, DiscrepancySummary, AMOUNT_TOLERANCE,
    },
    models::{DiscrepancyType, EntryType, StatementRecord},
};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn bd(value: &str) -> BigDecimal {
    BigDecimal::from_str(value).expect("valid decimal")
}

fn credit(reference: &str, amount: &str) -> StatementRecord {
    StatementRecord {
        reference:   reference.to_string(),
        amount:      bd(amount),
        entry_type:  EntryType::Credit,
        date:        NaiveDate::from_ymd_opt(2025, 6, 15).unwrap(),
        description: None,
    }
}

fn debit(reference: &str, amount: &str) -> StatementRecord {
    StatementRecord {
        reference:   reference.to_string(),
        amount:      bd(amount),
        entry_type:  EntryType::Debit,
        date:        NaiveDate::from_ymd_opt(2025, 6, 15).unwrap(),
        description: Some("fee".to_string()),
    }
}

// ── Tolerance constant ────────────────────────────────────────────────────────

#[test]
fn tolerance_constant_is_one_cent() {
    assert_eq!(AMOUNT_TOLERANCE, "0.01");
}

// ── Amount classification ─────────────────────────────────────────────────────

#[test]
fn exact_match_classified_as_matched() {
    assert_eq!(classify_amounts(&bd("100.00"), &bd("100.00")), DiscrepancyType::Matched);
}

#[test]
fn amount_one_cent_over_classified_as_matched() {
    // 0.01 == AMOUNT_TOLERANCE → still Matched (<=)
    assert_eq!(classify_amounts(&bd("100.00"), &bd("100.01")), DiscrepancyType::Matched);
}

#[test]
fn amount_one_cent_under_classified_as_matched() {
    assert_eq!(classify_amounts(&bd("100.00"), &bd("99.99")), DiscrepancyType::Matched);
}

#[test]
fn amount_two_cents_over_classified_as_mismatch() {
    assert_eq!(
        classify_amounts(&bd("100.00"), &bd("100.02")),
        DiscrepancyType::AmountMismatch
    );
}

#[test]
fn amount_two_cents_under_classified_as_mismatch() {
    assert_eq!(
        classify_amounts(&bd("100.00"), &bd("99.98")),
        DiscrepancyType::AmountMismatch
    );
}

#[test]
fn large_discrepancy_classified_as_mismatch() {
    assert_eq!(
        classify_amounts(&bd("500.00"), &bd("300.00")),
        DiscrepancyType::AmountMismatch
    );
}

#[test]
fn zero_vs_positive_classified_as_mismatch() {
    assert_eq!(classify_amounts(&bd("0.00"), &bd("100.00")), DiscrepancyType::AmountMismatch);
    assert_eq!(classify_amounts(&bd("100.00"), &bd("0.00")), DiscrepancyType::AmountMismatch);
}

// ── Net discrepancy ───────────────────────────────────────────────────────────

#[test]
fn net_discrepancy_zero_on_exact_match() {
    assert_eq!(net_discrepancy(&bd("100.00"), &bd("100.00")), bd("0.00"));
}

#[test]
fn net_discrepancy_positive_means_over_collected() {
    let d = net_discrepancy(&bd("100.00"), &bd("120.00"));
    assert_eq!(d, bd("20.00"));
}

#[test]
fn net_discrepancy_negative_means_under_collected() {
    let d = net_discrepancy(&bd("100.00"), &bd("80.00"));
    assert_eq!(d, bd("-20.00"));
}

// ── Duplicate detection ───────────────────────────────────────────────────────

#[test]
fn no_duplicates_empty_map() {
    let recs = vec![credit("A", "10.00"), credit("B", "20.00"), credit("C", "30.00")];
    assert!(find_duplicates(&recs).is_empty());
}

#[test]
fn single_pair_duplicate_detected() {
    let recs = vec![credit("REF-1", "10.00"), credit("REF-2", "20.00"), credit("REF-1", "10.00")];
    let dups = find_duplicates(&recs);
    assert_eq!(dups.len(), 1);
    assert_eq!(dups["REF-1"], vec![0, 2]);
}

#[test]
fn multiple_refs_each_duplicated() {
    let recs = vec![
        credit("X", "1.00"), credit("Y", "2.00"),
        credit("X", "1.00"), credit("Y", "2.00"),
    ];
    let dups = find_duplicates(&recs);
    assert_eq!(dups.len(), 2);
    assert!(dups.contains_key("X"));
    assert!(dups.contains_key("Y"));
}

#[test]
fn triple_occurrence_all_indices_in_dup_map() {
    let recs = vec![
        credit("Z", "5.00"), credit("Z", "5.00"), credit("Z", "5.00"), credit("A", "1.00"),
    ];
    let dups = find_duplicates(&recs);
    assert_eq!(dups["Z"].len(), 3);
    assert!(!dups.contains_key("A"));
}

#[test]
fn single_record_no_duplicates() {
    let recs = vec![credit("ONLY", "100.00")];
    assert!(find_duplicates(&recs).is_empty());
}

#[test]
fn empty_input_no_duplicates() {
    let recs: Vec<StatementRecord> = vec![];
    assert!(find_duplicates(&recs).is_empty());
}

/// Duplicate detection works regardless of EntryType.
#[test]
fn duplicate_detection_ignores_entry_type_and_amount() {
    let recs = vec![credit("REF", "100.00"), debit("REF", "999.00")];
    let dups = find_duplicates(&recs);
    assert_eq!(dups["REF"].len(), 2);
}

// ── Canonical index ───────────────────────────────────────────────────────────

#[test]
fn canonical_index_is_minimum_of_indices() {
    assert_eq!(canonical_index(&[5, 1, 3]), 1);
    assert_eq!(canonical_index(&[9, 2, 7]), 2);
}

#[test]
fn canonical_index_single_element() {
    assert_eq!(canonical_index(&[42]), 42);
}

#[test]
fn canonical_index_already_sorted() {
    assert_eq!(canonical_index(&[0, 1, 2]), 0);
}

// ── is_duplicate helper ───────────────────────────────────────────────────────

#[test]
fn is_duplicate_true_for_dup_indices() {
    let recs = vec![credit("A", "1.00"), credit("B", "2.00"), credit("A", "1.00")];
    let dups = find_duplicates(&recs);
    assert!( is_duplicate(0, &dups));
    assert!(!is_duplicate(1, &dups));
    assert!( is_duplicate(2, &dups));
}

// ── DiscrepancySummary ────────────────────────────────────────────────────────

#[test]
fn summary_is_clean_when_only_matched() {
    let s = DiscrepancySummary { matched: 100, ..Default::default() };
    assert!(s.is_clean());
    assert_eq!(s.total_discrepancies(), 0);
}

#[test]
fn summary_total_sums_all_discrepancy_types_not_matched() {
    let s = DiscrepancySummary {
        matched:                 10,
        amount_mismatches:        2,
        missing_from_statement:   3,
        extra_in_statement:       4,
        duplicates:               5,
    };
    assert_eq!(s.total_discrepancies(), 14);
    assert!(!s.is_clean());
}

#[test]
fn summary_high_by_absolute_count_above_10() {
    let s = DiscrepancySummary { amount_mismatches: 11, ..Default::default() };
    assert!(s.is_high(1000));
}

#[test]
fn summary_not_high_at_exactly_10() {
    let s = DiscrepancySummary { amount_mismatches: 10, ..Default::default() };
    assert!(!s.is_high(1000));
}

#[test]
fn summary_high_by_percentage_above_5pct() {
    let s = DiscrepancySummary { missing_from_statement: 6, ..Default::default() };
    assert!(s.is_high(100));
}

#[test]
fn summary_not_high_at_exactly_5pct() {
    let s = DiscrepancySummary { missing_from_statement: 5, ..Default::default() };
    assert!(!s.is_high(100));
}

#[test]
fn summary_high_pct_check_skipped_for_zero_total_records() {
    let s = DiscrepancySummary { amount_mismatches: 3, ..Default::default() };
    assert!(!s.is_high(0));
}

#[test]
fn summary_high_by_count_wins_even_when_pct_low() {
    let s = DiscrepancySummary { duplicates: 11, ..Default::default() };
    assert!(s.is_high(10_000));
}
