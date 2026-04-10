//! Reconciliation discrepancy detection tests.
//!
//! All tests in this file exercise pure in-memory functions from the
//! `reconciliation::discrepancy` module.  They require no database or file I/O.
//! Integration scenarios are documented as commented stubs at the bottom.
//!
//! Run: `cargo test --test reconciliation`

use chrono::NaiveDate;
use transitops_backend::reconciliation::{
    discrepancy::{
        canonical_index, classify_amounts, find_duplicates, is_duplicate,
        net_discrepancy, DiscrepancySummary, AMOUNT_TOLERANCE,
    },
    models::{DiscrepancyType, EntryType, StatementRecord},
};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn credit(reference: &str, amount: f64) -> StatementRecord {
    StatementRecord {
        reference:   reference.to_string(),
        amount,
        entry_type:  EntryType::Credit,
        date:        NaiveDate::from_ymd_opt(2025, 6, 15).unwrap(),
        description: None,
    }
}

fn debit(reference: &str, amount: f64) -> StatementRecord {
    StatementRecord {
        reference:   reference.to_string(),
        amount,
        entry_type:  EntryType::Debit,
        date:        NaiveDate::from_ymd_opt(2025, 6, 15).unwrap(),
        description: Some("fee".to_string()),
    }
}

// ── Tolerance constant ────────────────────────────────────────────────────────

#[test]
fn tolerance_constant_is_one_cent() {
    assert!((AMOUNT_TOLERANCE - 0.01).abs() < 1e-10);
}

// ── Amount classification ─────────────────────────────────────────────────────

#[test]
fn exact_match_classified_as_matched() {
    assert_eq!(classify_amounts(100.00, 100.00), DiscrepancyType::Matched);
}

#[test]
fn amount_one_cent_over_classified_as_matched() {
    // 0.01 == AMOUNT_TOLERANCE → still Matched (<=)
    assert_eq!(classify_amounts(100.00, 100.01), DiscrepancyType::Matched);
}

#[test]
fn amount_one_cent_under_classified_as_matched() {
    assert_eq!(classify_amounts(100.00, 99.99), DiscrepancyType::Matched);
}

#[test]
fn amount_two_cents_over_classified_as_mismatch() {
    // 0.02 > AMOUNT_TOLERANCE → AmountMismatch
    assert_eq!(classify_amounts(100.00, 100.02), DiscrepancyType::AmountMismatch);
}

#[test]
fn amount_two_cents_under_classified_as_mismatch() {
    assert_eq!(classify_amounts(100.00, 99.98), DiscrepancyType::AmountMismatch);
}

#[test]
fn large_discrepancy_classified_as_mismatch() {
    assert_eq!(classify_amounts(500.00, 300.00), DiscrepancyType::AmountMismatch);
}

#[test]
fn zero_vs_positive_classified_as_mismatch() {
    assert_eq!(classify_amounts(0.00, 100.00), DiscrepancyType::AmountMismatch);
    assert_eq!(classify_amounts(100.00, 0.00), DiscrepancyType::AmountMismatch);
}

#[test]
fn symmetry_around_tolerance_boundary() {
    // Just inside tolerance on both sides
    assert_eq!(classify_amounts(100.0, 100.0 + AMOUNT_TOLERANCE),       DiscrepancyType::Matched);
    assert_eq!(classify_amounts(100.0, 100.0 - AMOUNT_TOLERANCE),       DiscrepancyType::Matched);
    // Just outside
    let epsilon = 1e-9_f64;
    assert_eq!(classify_amounts(100.0, 100.0 + AMOUNT_TOLERANCE + epsilon), DiscrepancyType::AmountMismatch);
}

// ── Net discrepancy ───────────────────────────────────────────────────────────

#[test]
fn net_discrepancy_zero_on_exact_match() {
    assert!(net_discrepancy(100.0, 100.0).abs() < 1e-9);
}

#[test]
fn net_discrepancy_positive_means_over_collected() {
    let d = net_discrepancy(100.0, 120.0);
    assert!(d > 0.0);
    assert!((d - 20.0).abs() < 1e-9);
}

#[test]
fn net_discrepancy_negative_means_under_collected() {
    let d = net_discrepancy(100.0, 80.0);
    assert!(d < 0.0);
    assert!((d + 20.0).abs() < 1e-9);
}

// ── Duplicate detection ───────────────────────────────────────────────────────

#[test]
fn no_duplicates_empty_map() {
    let recs = vec![credit("A", 10.0), credit("B", 20.0), credit("C", 30.0)];
    assert!(find_duplicates(&recs).is_empty());
}

#[test]
fn single_pair_duplicate_detected() {
    let recs = vec![credit("REF-1", 10.0), credit("REF-2", 20.0), credit("REF-1", 10.0)];
    let dups = find_duplicates(&recs);
    assert_eq!(dups.len(), 1);
    assert_eq!(dups["REF-1"], vec![0, 2]);
}

#[test]
fn multiple_refs_each_duplicated() {
    let recs = vec![
        credit("X", 1.0), credit("Y", 2.0),
        credit("X", 1.0), credit("Y", 2.0),
    ];
    let dups = find_duplicates(&recs);
    assert_eq!(dups.len(), 2);
    assert!(dups.contains_key("X"));
    assert!(dups.contains_key("Y"));
}

#[test]
fn triple_occurrence_all_indices_in_dup_map() {
    let recs = vec![credit("Z", 5.0), credit("Z", 5.0), credit("Z", 5.0), credit("A", 1.0)];
    let dups = find_duplicates(&recs);
    assert_eq!(dups["Z"].len(), 3);
    assert!(!dups.contains_key("A")); // A appears only once
}

#[test]
fn single_record_no_duplicates() {
    let recs = vec![credit("ONLY", 100.0)];
    assert!(find_duplicates(&recs).is_empty());
}

#[test]
fn empty_input_no_duplicates() {
    let recs: Vec<StatementRecord> = vec![];
    assert!(find_duplicates(&recs).is_empty());
}

/// Duplicate detection works regardless of EntryType — duplicates identified
/// purely by reference string, not by amount or type.
#[test]
fn duplicate_detection_ignores_entry_type_and_amount() {
    let recs = vec![credit("REF", 100.0), debit("REF", 999.0)];
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
    let recs = vec![credit("A", 1.0), credit("B", 2.0), credit("A", 1.0)];
    let dups = find_duplicates(&recs);
    assert!( is_duplicate(0, &dups)); // first  A
    assert!(!is_duplicate(1, &dups)); // B — unique
    assert!( is_duplicate(2, &dups)); // second A
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
    assert_eq!(s.total_discrepancies(), 14); // 2+3+4+5 (matched excluded)
    assert!(!s.is_clean());
}

#[test]
fn summary_high_by_absolute_count_above_10() {
    let s = DiscrepancySummary { amount_mismatches: 11, ..Default::default() };
    assert!(s.is_high(1000));
}

#[test]
fn summary_not_high_at_exactly_10() {
    // The threshold is strictly >, so 10 is NOT high.
    let s = DiscrepancySummary { amount_mismatches: 10, ..Default::default() };
    assert!(!s.is_high(1000));
}

#[test]
fn summary_high_by_percentage_above_5pct() {
    // 6/100 = 6% > 5% → high
    let s = DiscrepancySummary { missing_from_statement: 6, ..Default::default() };
    assert!(s.is_high(100));
}

#[test]
fn summary_not_high_at_exactly_5pct() {
    // 5/100 = 5% — threshold is strictly >, so 5% is NOT high
    let s = DiscrepancySummary { missing_from_statement: 5, ..Default::default() };
    assert!(!s.is_high(100));
}

#[test]
fn summary_high_pct_check_skipped_for_zero_total_records() {
    // With total_records=0 the % branch is guarded; only count check applies.
    let s = DiscrepancySummary { amount_mismatches: 3, ..Default::default() };
    assert!(!s.is_high(0)); // 3 is not > 10
}

#[test]
fn summary_high_by_count_wins_even_when_pct_low() {
    // 11 discrepancies out of 10_000 records = 0.11%, but count 11 > 10 → high
    let s = DiscrepancySummary { duplicates: 11, ..Default::default() };
    assert!(s.is_high(10_000));
}

// ── Integration test stubs ────────────────────────────────────────────────────

// #[tokio::test]
// #[ignore = "requires database + uploaded statement fixture"]
// async fn duplicate_statement_fingerprint_rejected() {
//     // Upload statement CSV; re-upload identical bytes
//     // Assert: HTTP 409 Conflict — duplicate fingerprint
// }

// #[tokio::test]
// #[ignore = "requires database + test fixture"]
// async fn reconciliation_run_on_failed_import_returns_error() {
//     // Create import with status='failed'
//     // Call start_run for that import
//     // Assert: HTTP 400 "Import has no stored content"
// }

// #[tokio::test]
// #[ignore = "requires database"]
// async fn rerun_creates_new_run_id_and_does_not_overwrite_previous() {
//     // Complete one reconciliation run; trigger second run on same import
//     // Assert: new run_id, previous run_results unmodified
// }

// #[tokio::test]
// #[ignore = "requires database"]
// async fn malformed_csv_import_fails_with_parse_error() {
//     // Upload CSV that is syntactically valid but has missing required columns
//     // Assert: import row has status='failed', errors populated
// }
