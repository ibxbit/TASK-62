use bigdecimal::BigDecimal;
use bigdecimal::FromPrimitive;
/// Reconciliation statement importer — strict CSV format validation and
/// SHA-256 fingerprint verification.
///
/// ## Accepted CSV format
///
/// ```csv
/// date,ref,amount,type,description
/// 2025-01-15,TXN-001,100.00,credit,Bus fare
/// 2025-01-15,TXN-002,50.50,debit,Refund
/// ```
///
/// Rules enforced by `validate_and_parse`:
///   1. Content must be valid UTF-8.
///   2. First non-blank row must contain exactly the headers:
///      `date`, `ref`, `amount`, `type`, `description`
///      (case-insensitive; any column order; `description` is optional).
///   3. Required columns: `date`, `ref`, `amount`, `type`.
///   4. `date` must parse as `YYYY-MM-DD`.
///   5. `amount` must be a non-negative decimal (commas stripped).
///   6. `type` must be `debit` or `credit` (case-insensitive).
///   7. `ref` must be non-empty.
///
/// A parse result is returned even when errors exist; the caller decides
/// whether to abort or continue with warnings.
use chrono::NaiveDate;
use sha2::{Digest, Sha256};

use super::models::{EntryType, ParseResult, StatementRecord};

// ============================================================
// SHA-256 fingerprint
// ============================================================

/// Compute the SHA-256 fingerprint of raw file bytes, hex-encoded.
pub fn fingerprint(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex::encode(h.finalize())
}

/// Verify that the SHA-256 of `data` matches `expected_hex`.
/// Returns `true` on match, `false` on mismatch.
pub fn verify_fingerprint(data: &[u8], expected_hex: &str) -> bool {
    fingerprint(data) == expected_hex.to_lowercase()
}

// ============================================================
// Required CSV column set
// ============================================================

const REQUIRED_HEADERS: &[&str] = &["date", "ref", "amount", "type"];
const OPTIONAL_HEADER:   &str    = "description";

// ============================================================
// Strict CSV parser / validator
// ============================================================

/// Validate file format and parse records from raw CSV bytes.
///
/// Always returns a `ParseResult`; check `is_valid` before using `records`
/// for production reconciliation.
pub fn validate_and_parse(content: &[u8]) -> ParseResult {
    let mut errors:  Vec<String> = Vec::new();
    let mut records: Vec<StatementRecord> = Vec::new();

    // ---- UTF-8 check ----
    let text = match std::str::from_utf8(content) {
        Ok(s)  => s,
        Err(_) => {
            return ParseResult {
                is_valid:    false,
                errors:      vec!["File is not valid UTF-8".to_string()],
                records:     vec![],
                fingerprint: fingerprint(content),
            };
        }
    };

    // ---- CSV reader ----
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(false)          // require consistent column count
        .trim(csv::Trim::All)
        .from_reader(text.as_bytes());

    // ---- Header validation ----
    let headers = match reader.headers() {
        Ok(h)  => h.clone(),
        Err(e) => {
            return ParseResult {
                is_valid:    false,
                errors:      vec![format!("Cannot read CSV headers: {}", e)],
                records:     vec![],
                fingerprint: fingerprint(content),
            };
        }
    };

    let norm_headers: Vec<String> = headers.iter().map(|h| h.to_lowercase()).collect();

    for &required in REQUIRED_HEADERS {
        if !norm_headers.contains(&required.to_string()) {
            errors.push(format!("Missing required column: '{}'", required));
        }
    }

    let col = |name: &str| -> Option<usize> {
        norm_headers.iter().position(|h| h == name)
    };

    let Some(date_col)   = col("date")   else { /* already in errors */ return ParseResult { is_valid: false, errors, records, fingerprint: fingerprint(content) }; };
    let Some(ref_col)    = col("ref")    else { return ParseResult { is_valid: false, errors, records, fingerprint: fingerprint(content) }; };
    let Some(amount_col) = col("amount") else { return ParseResult { is_valid: false, errors, records, fingerprint: fingerprint(content) }; };
    let Some(type_col)   = col("type")   else { return ParseResult { is_valid: false, errors, records, fingerprint: fingerprint(content) }; };
    let desc_col = col(OPTIONAL_HEADER);

    if !errors.is_empty() {
        return ParseResult {
            is_valid: false,
            errors,
            records,
            fingerprint: fingerprint(content),
        };
    }

    // ---- Row validation and parsing ----
    for (row_idx, result) in reader.records().enumerate() {
        let line_no = row_idx + 2; // 1-based, row 1 is headers

        let record = match result {
            Ok(r)  => r,
            Err(e) => {
                errors.push(format!("Line {}: CSV parse error — {}", line_no, e));
                continue;
            }
        };

        let get = |idx: usize| record.get(idx).unwrap_or("").trim().to_string();

        // --- ref ---
        let reference = get(ref_col);
        if reference.is_empty() {
            errors.push(format!("Line {}: 'ref' must not be empty", line_no));
            continue;
        }

        // --- date ---
        let date_str = get(date_col);
        let date = match NaiveDate::parse_from_str(&date_str, "%Y-%m-%d") {
            Ok(d)  => d,
            Err(_) => {
                errors.push(format!(
                    "Line {}: invalid 'date' '{}' — expected YYYY-MM-DD",
                    line_no, date_str
                ));
                continue;
            }
        };

        // --- amount ---
        let amount_str = get(amount_col).replace(',', "");
        let amount: f64 = match amount_str.parse() {
            Ok(a)  => a,
            Err(_) => {
                errors.push(format!(
                    "Line {}: invalid 'amount' '{}' — expected a decimal number",
                    line_no, amount_str
                ));
                continue;
            }
        };
        if amount < 0.0 {
            errors.push(format!(
                "Line {}: 'amount' must be non-negative, got {}",
                line_no, amount
            ));
            continue;
        }
        let amount = BigDecimal::from_f64(amount).unwrap();

        // --- type ---
        let entry_type = match get(type_col).to_lowercase().as_str() {
            "debit"  => EntryType::Debit,
            "credit" => EntryType::Credit,
            other    => {
                errors.push(format!(
                    "Line {}: invalid 'type' '{}' — expected 'debit' or 'credit'",
                    line_no, other
                ));
                continue;
            }
        };

        records.push(StatementRecord {
            reference,
            amount,
            entry_type,
            date,
            description: desc_col.map(|i| get(i)).filter(|s| !s.is_empty()),
        });
    }

    ParseResult {
        is_valid:    errors.is_empty(),
        errors,
        records,
        fingerprint: fingerprint(content),
    }
}

// ============================================================
// Tests
// ============================================================
#[cfg(test)]
mod tests {
    use super::*;

    fn csv(body: &str) -> Vec<u8> {
        format!("date,ref,amount,type,description\n{}", body).into_bytes()
    }

    #[test]
    fn valid_csv_parses_all_rows() {
        let content = csv("2025-01-15,TXN001,100.50,credit,Bus fare\n2025-01-15,TXN002,50.00,debit,Refund\n");
        let r = validate_and_parse(&content);
        assert!(r.is_valid, "{:?}", r.errors);
        assert_eq!(r.records.len(), 2);
        assert_eq!(r.records[0].reference, "TXN001");
        assert!((r.records[0].amount - 100.5).abs() < 1e-9);
        assert_eq!(r.records[0].entry_type, EntryType::Credit);
        assert_eq!(r.records[1].entry_type, EntryType::Debit);
    }

    #[test]
    fn missing_required_column_errors() {
        let content = b"ref,amount,type\nTXN001,100,credit\n";
        let r = validate_and_parse(content);
        assert!(!r.is_valid);
        assert!(r.errors.iter().any(|e| e.contains("date")));
    }

    #[test]
    fn invalid_date_format_skips_row() {
        let content = csv("01/15/2025,TXN001,100.00,credit,\n");
        let r = validate_and_parse(&content);
        assert!(!r.is_valid);
        assert!(r.records.is_empty());
        assert!(r.errors.iter().any(|e| e.contains("date")));
    }

    #[test]
    fn invalid_amount_skips_row() {
        let content = csv("2025-01-15,TXN001,not_a_number,credit,\n");
        let r = validate_and_parse(&content);
        assert!(!r.is_valid);
        assert!(r.records.is_empty());
    }

    #[test]
    fn negative_amount_skips_row() {
        let content = csv("2025-01-15,TXN001,-50.00,credit,\n");
        let r = validate_and_parse(&content);
        assert!(!r.is_valid);
    }

    #[test]
    fn invalid_type_skips_row() {
        let content = csv("2025-01-15,TXN001,100,payment,\n");
        let r = validate_and_parse(&content);
        assert!(!r.is_valid);
    }

    #[test]
    fn empty_ref_skips_row() {
        let content = csv("2025-01-15,,100.00,credit,\n");
        let r = validate_and_parse(&content);
        assert!(!r.is_valid);
        assert!(r.records.is_empty());
    }

    #[test]
    fn amount_with_commas_parsed() {
        let content = csv("2025-01-15,TXN001,\"1,234.56\",credit,\n");
        let r = validate_and_parse(&content);
        assert!(r.is_valid, "{:?}", r.errors);
        assert!((r.records[0].amount - 1234.56).abs() < 1e-6);
    }

    #[test]
    fn non_utf8_returns_error() {
        let content = b"\xff\xfe invalid bytes";
        let r = validate_and_parse(content);
        assert!(!r.is_valid);
        assert!(r.errors[0].contains("UTF-8"));
    }

    #[test]
    fn fingerprint_verify_roundtrip() {
        let data = b"hello world";
        let fp = fingerprint(data);
        assert!(verify_fingerprint(data, &fp));
        assert!(!verify_fingerprint(data, "wrong"));
    }

    #[test]
    fn fingerprint_is_sha256() {
        // SHA-256("abc") starts with ba7816bf
        let fp = fingerprint(b"abc");
        assert!(fp.starts_with("ba7816bf"), "got: {}", fp);
    }

    #[test]
    fn case_insensitive_headers() {
        let content = b"Date,REF,Amount,Type\n2025-01-15,TXN001,100.00,credit\n";
        let r = validate_and_parse(content);
        assert!(r.is_valid, "{:?}", r.errors);
        assert_eq!(r.records.len(), 1);
    }
}
