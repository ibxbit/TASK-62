/// Payment result file import — parsing and matching.
///
/// Supports two file formats:
///   - CSV: header row `ref,amount,date,description` (case-insensitive)
///   - JSON: array of objects with the same field names
///
/// Flow:
///   1. Caller uploads file via `POST /payments/imports` with base64 content.
///   2. System computes SHA-256 to detect duplicates.
///   3. A `statement_imports` row is created (status = 'pending').
///   4. `POST /payments/imports/{id}/process` triggers `process_import_file`.
///   5. Each line is parsed, stored in `statement_import_lines`, and matched
///      against `payments.transactions` by `idempotency_key` or amount+date.
use chrono::NaiveDate;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

// ============================================================
// Parsed import line
// ============================================================

#[derive(Debug, Clone)]
pub struct ImportLine {
    pub transaction_ref: Option<String>,
    pub amount:          f64,
    pub transaction_date: NaiveDate,
    pub description:     Option<String>,
}

// ============================================================
// SHA-256 helper
// ============================================================

pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex::encode(h.finalize())
}

// ============================================================
// CSV parser
// ============================================================

/// Expected CSV headers (case-insensitive):
///   ref, amount, date, description
///
/// `ref` and `description` are optional — missing fields become `None`.
/// `amount` and `date` are required; rows with unparseable values are skipped.
pub fn parse_csv(content: &[u8]) -> Result<Vec<ImportLine>, String> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(content);

    let headers = reader.headers().map_err(|e| e.to_string())?.clone();
    let col = |name: &str| -> Option<usize> {
        headers.iter().position(|h| h.to_lowercase() == name)
    };

    let ref_col   = col("ref");
    let amt_col   = col("amount").ok_or("CSV missing 'amount' column")?;
    let date_col  = col("date").ok_or("CSV missing 'date' column")?;
    let desc_col  = col("description");

    let mut lines = Vec::new();

    for (row_idx, result) in reader.records().enumerate() {
        let record = match result {
            Ok(r)  => r,
            Err(e) => {
                tracing::warn!(row = row_idx + 2, error = %e, "CSV row parse error, skipping");
                continue;
            }
        };

        let get = |idx: usize| record.get(idx).unwrap_or("").trim().to_string();

        let amount = match get(amt_col).replace(',', "").parse::<f64>() {
            Ok(a)  => a,
            Err(_) => {
                tracing::warn!(row = row_idx + 2, "Invalid amount, skipping row");
                continue;
            }
        };

        let transaction_date = match NaiveDate::parse_from_str(&get(date_col), "%Y-%m-%d")
            .or_else(|_| NaiveDate::parse_from_str(&get(date_col), "%d/%m/%Y"))
            .or_else(|_| NaiveDate::parse_from_str(&get(date_col), "%m/%d/%Y"))
        {
            Ok(d)  => d,
            Err(_) => {
                tracing::warn!(row = row_idx + 2, "Invalid date, skipping row");
                continue;
            }
        };

        lines.push(ImportLine {
            transaction_ref:  ref_col.map(|i| get(i)).filter(|s| !s.is_empty()),
            amount,
            transaction_date,
            description:       desc_col.map(|i| get(i)).filter(|s| !s.is_empty()),
        });
    }

    Ok(lines)
}

// ============================================================
// JSON parser
// ============================================================

#[derive(Debug, Deserialize)]
struct JsonImportLine {
    #[serde(rename = "ref", alias = "reference", alias = "transaction_ref")]
    transaction_ref:  Option<String>,
    amount:           f64,
    #[serde(alias = "transaction_date", alias = "date")]
    date:             String,
    description:      Option<String>,
}

/// Expected JSON format:
/// ```json
/// [
///   { "ref": "TXN001", "amount": 100.00, "date": "2025-01-15", "description": "Bus fare" },
///   ...
/// ]
/// ```
pub fn parse_json(content: &[u8]) -> Result<Vec<ImportLine>, String> {
    let raw: Vec<JsonImportLine> =
        serde_json::from_slice(content).map_err(|e| e.to_string())?;

    let mut lines = Vec::new();
    for (i, item) in raw.into_iter().enumerate() {
        let transaction_date = NaiveDate::parse_from_str(&item.date, "%Y-%m-%d")
            .or_else(|_| NaiveDate::parse_from_str(&item.date, "%d/%m/%Y"))
            .map_err(|_| format!("row {}: invalid date '{}'", i + 1, item.date))?;

        lines.push(ImportLine {
            transaction_ref:  item.transaction_ref,
            amount:           item.amount,
            transaction_date,
            description:      item.description,
        });
    }
    Ok(lines)
}

// ============================================================
// DB processing
// ============================================================

/// Parse a previously stored import file and populate `statement_import_lines`.
///
/// For each line:
///   - Inserts a `statement_import_lines` row.
///   - Attempts to match against `payments.transactions` by idempotency_key
///     (using `transaction_ref`) and, as a fallback, by amount on the same date.
///
/// Returns `(total, matched)` counts.  Updates the import's status and counters.
pub async fn process_import_file(
    pool:      &PgPool,
    import_id: Uuid,
    format:    &str,
    content:   &[u8],
) -> Result<(i32, i32), sqlx::Error> {
    let lines = match format {
        "json" => parse_json(content).unwrap_or_default(),
        _      => parse_csv(content).unwrap_or_default(),
    };

    let total = lines.len() as i32;
    let mut matched = 0i32;
    let mut errors  = 0i32;

    for (line_no, line) in lines.iter().enumerate() {
        let line_number = (line_no + 1) as i32;

        // Attempt to match by transaction_ref → idempotency_key
        let matched_id: Option<Uuid> = if let Some(ref txn_ref) = line.transaction_ref {
            sqlx::query_scalar!(
                "SELECT id FROM payments.transactions WHERE idempotency_key = $1 LIMIT 1",
                txn_ref,
            )
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
        } else {
            None
        };

        // Fallback: match by amount + date range (same calendar day)
        let matched_id = if matched_id.is_none() {
            let day_start = line.transaction_date.and_hms_opt(0, 0, 0)
                .map(|dt| chrono::DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));
            let day_end   = line.transaction_date.and_hms_opt(23, 59, 59)
                .map(|dt| chrono::DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));

            if let (Some(start), Some(end)) = (day_start, day_end) {
                sqlx::query_scalar!(
                    r#"
                    SELECT id FROM payments.transactions
                    WHERE amount    = $1
                      AND created_at BETWEEN $2 AND $3
                      AND status    = 'completed'
                    LIMIT 1
                    "#,
                    line.amount as f64,
                    start,
                    end,
                )
                .fetch_optional(pool)
                .await
                .ok()
                .flatten()
            } else {
                None
            }
        } else {
            matched_id
        };

        let match_status = if matched_id.is_some() {
            matched += 1;
            "matched"
        } else {
            "unmatched"
        };

        let result = sqlx::query!(
            r#"
            INSERT INTO payments.statement_import_lines
                (import_id, line_number, transaction_ref, amount,
                 transaction_date, description, matched_transaction_id, match_status)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (import_id, line_number) DO NOTHING
            "#,
            import_id,
            line_number,
            line.transaction_ref.as_deref(),
            line.amount,
            line.transaction_date,
            line.description.as_deref(),
            matched_id,
            match_status,
        )
        .execute(pool)
        .await;

        if let Err(e) = result {
            tracing::warn!(line_number, error = %e, "Failed to insert import line");
            errors += 1;
        }
    }

    // Update import record
    sqlx::query!(
        r#"
        UPDATE payments.statement_imports
        SET status            = CASE WHEN $3 = 0 THEN 'completed' ELSE 'failed' END,
            total_records     = $2,
            processed_records = $2 - $3,
            error_count       = $3,
            updated_at        = now()
        WHERE id = $1
        "#,
        import_id,
        total,
        errors,
    )
    .execute(pool)
    .await?;

    Ok((total, matched))
}

// ============================================================
// Tests
// ============================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_csv_basic() {
        let csv = b"ref,amount,date,description\nTXN001,100.50,2025-01-15,Bus fare\nTXN002,200.00,2025-01-16,\n";
        let lines = parse_csv(csv).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].transaction_ref.as_deref(), Some("TXN001"));
        assert!((lines[0].amount - 100.5).abs() < 1e-9);
        assert_eq!(lines[0].transaction_date, NaiveDate::from_ymd_opt(2025, 1, 15).unwrap());
        assert_eq!(lines[0].description.as_deref(), Some("Bus fare"));
        assert_eq!(lines[1].description, None);
    }

    #[test]
    fn parse_csv_skips_invalid_rows() {
        let csv = b"ref,amount,date\nTXN001,not_a_number,2025-01-15\nTXN002,50.00,bad_date\nTXN003,75.00,2025-01-17\n";
        let lines = parse_csv(csv).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].transaction_ref.as_deref(), Some("TXN003"));
    }

    #[test]
    fn parse_csv_comma_in_amount() {
        let csv = b"ref,amount,date\nTXN001,\"1,234.56\",2025-01-15\n";
        let lines = parse_csv(csv).unwrap();
        assert_eq!(lines.len(), 1);
        assert!((lines[0].amount - 1234.56).abs() < 1e-6);
    }

    #[test]
    fn parse_json_basic() {
        let json = br#"[{"ref":"TXN001","amount":99.99,"date":"2025-02-01","description":"Ticket"}]"#;
        let lines = parse_json(json).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].transaction_ref.as_deref(), Some("TXN001"));
        assert!((lines[0].amount - 99.99).abs() < 1e-9);
    }

    #[test]
    fn parse_json_bad_date_errors() {
        let json = br#"[{"amount":50.0,"date":"not-a-date"}]"#;
        assert!(parse_json(json).is_err());
    }

    #[test]
    fn sha256_hex_known_value() {
        // SHA-256 of "abc" = ba7816bf8f01cfea414140de5dae2ec73b00361bbef0469f492c347b5e3
        let h = sha256_hex(b"abc");
        assert!(h.starts_with("ba7816bf"));
    }
}
