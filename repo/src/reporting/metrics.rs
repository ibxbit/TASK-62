use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::models::{MetricDefinitionRow, MetricQuery, MetricResult, MetricSummary, TimeSeriesPoint};

// ============================================================
// Public entry-point
// ============================================================

/// Compute a metric and return a time-series result.
///
/// Dispatches to the appropriate formula function based on `def.formula_type`.
pub async fn compute_metric(
    pool: &PgPool,
    def: &MetricDefinitionRow,
    query: &MetricQuery,
) -> Result<MetricResult, sqlx::Error> {
    let series = match def.formula_type.as_str() {
        "on_time_departure_rate" => {
            let tolerance: i64 = def.config
                .get("tolerance_minutes")
                .and_then(|v| v.as_i64())
                .unwrap_or(5);
            compute_on_time_departure_rate(pool, query, tolerance).await?
        }
        "refund_rate" => compute_refund_rate(pool, query).await?,
        "reconciliation_mismatch_count" => {
            let threshold: i64 = def.config
                .get("mismatch_threshold_cents")
                .and_then(|v| v.as_i64())
                .unwrap_or(1);
            compute_reconciliation_mismatch_count(pool, query, threshold).await?
        }
        other => {
            tracing::warn!(formula_type = other, "unknown formula_type, returning empty series");
            vec![]
        }
    };

    let unit = unit_for_formula(&def.formula_type);
    let summary = summarise(&series);

    Ok(MetricResult {
        metric_key:   def.metric_key.clone(),
        display_name: def.display_name.clone(),
        unit,
        series,
        summary,
    })
}

// ============================================================
// On-time departure rate
// ============================================================
//
// A trip "departed on time" when:
//   actual_departure IS NOT NULL
//   AND actual_departure <= scheduled_departure + tolerance_minutes
//
// Rate = on_time_trips / total_departed_trips * 100
//
// Dimensions: route_id (ops.trips.route_id), depot_id (ops.routes.depot_id)

async fn compute_on_time_departure_rate(
    pool: &PgPool,
    query: &MetricQuery,
    tolerance_minutes: i64,
) -> Result<Vec<TimeSeriesPoint>, sqlx::Error> {
    // Build the per-period aggregation.
    // We rely on `ops.trips` having columns:
    //   scheduled_departure TIMESTAMPTZ, actual_departure TIMESTAMPTZ, route_id UUID
    // and `ops.routes` having depot_id UUID.
    struct Row {
        period_start: DateTime<Utc>,
        period_end:   DateTime<Utc>,
        on_time:      i64,
        total:        i64,
    }

    let rows: Vec<Row> = sqlx::query_as!(
        Row,
        r#"
        SELECT
            date_trunc($1, t.scheduled_departure)                        AS "period_start!: DateTime<Utc>",
            date_trunc($1, t.scheduled_departure) + $6::text::interval    AS "period_end!: DateTime<Utc>",
            COUNT(*) FILTER (
                WHERE t.actual_departure IS NOT NULL
                  AND t.actual_departure <= t.scheduled_departure + ($2 * INTERVAL '1 minute')
            )                                                             AS "on_time!: i64",
            COUNT(*) FILTER (WHERE t.actual_departure IS NOT NULL)       AS "total!: i64"
        FROM ops.trips t
        JOIN ops.routes r ON r.id = t.route_id
        WHERE t.scheduled_departure >= $3
          AND t.scheduled_departure <  $4
          AND ($5::uuid IS NULL OR t.route_id = $5)
          AND ($7::uuid IS NULL OR r.depot_id  = $7)
        GROUP BY 1
        ORDER BY 1
        "#,
        query.granularity,
        tolerance_minutes as f64,
        query.date_from,
        query.date_to,
        query.route_id as Option<Uuid>,
        granularity_interval(&query.granularity),
        query.depot_id as Option<Uuid>,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let value = if r.total == 0 {
                0.0
            } else {
                r.on_time as f64 / r.total as f64 * 100.0
            };
            TimeSeriesPoint {
                period_start: r.period_start,
                period_end:   r.period_end,
                value,
                sample_count: r.total,
            }
        })
        .collect())
}

// ============================================================
// Refund rate
// ============================================================
//
// Refund rate = refunded_transactions / total_transactions * 100
//
// Joins `payments.transactions` (assumed to have route_id + created_at)
// against `payments.refunds` to identify refunded transactions.
// Depot drill-down goes via ops.routes.depot_id.

async fn compute_refund_rate(
    pool: &PgPool,
    query: &MetricQuery,
) -> Result<Vec<TimeSeriesPoint>, sqlx::Error> {
    struct Row {
        period_start: DateTime<Utc>,
        period_end:   DateTime<Utc>,
        refunded:     i64,
        total:        i64,
    }

    let rows: Vec<Row> = sqlx::query_as!(
        Row,
        r#"
        SELECT
            date_trunc($1, tx.created_at)                                AS "period_start!: DateTime<Utc>",
            date_trunc($1, tx.created_at) + $5::text::interval           AS "period_end!: DateTime<Utc>",
            COUNT(*) FILTER (WHERE rf.id IS NOT NULL)                    AS "refunded!: i64",
            COUNT(*)                                                      AS "total!: i64"
        FROM payments.transactions tx
        LEFT JOIN payments.refunds rf ON rf.transaction_id = tx.id
        LEFT JOIN ops.routes r        ON r.id = tx.route_id
        WHERE tx.created_at >= $2
          AND tx.created_at <  $3
          AND ($4::uuid IS NULL OR tx.route_id = $4)
          AND ($6::uuid IS NULL OR r.depot_id  = $6)
        GROUP BY 1
        ORDER BY 1
        "#,
        query.granularity,
        query.date_from,
        query.date_to,
        query.route_id as Option<Uuid>,
        granularity_interval(&query.granularity),
        query.depot_id as Option<Uuid>,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let value = if r.total == 0 {
                0.0
            } else {
                r.refunded as f64 / r.total as f64 * 100.0
            };
            TimeSeriesPoint {
                period_start: r.period_start,
                period_end:   r.period_end,
                value,
                sample_count: r.total,
            }
        })
        .collect())
}

// ============================================================
// Reconciliation mismatch count
// ============================================================
//
// Counts transactions where ABS(settled_amount - expected_amount) > threshold.
// Uses `payments.reconciliation_entries` which is assumed to have:
//   transaction_id, settled_amount_cents BIGINT, expected_amount_cents BIGINT,
//   reconciled_at TIMESTAMPTZ, route_id UUID.

async fn compute_reconciliation_mismatch_count(
    pool: &PgPool,
    query: &MetricQuery,
    threshold_cents: i64,
) -> Result<Vec<TimeSeriesPoint>, sqlx::Error> {
    struct Row {
        period_start:   DateTime<Utc>,
        period_end:     DateTime<Utc>,
        mismatch_count: i64,
        total:          i64,
    }

    let rows: Vec<Row> = sqlx::query_as!(
        Row,
        r#"
        SELECT
            date_trunc($1, re.reconciled_at)                             AS "period_start!: DateTime<Utc>",
            date_trunc($1, re.reconciled_at) + $5::text::interval        AS "period_end!: DateTime<Utc>",
            COUNT(*) FILTER (
                WHERE ABS(re.settled_amount_cents - re.expected_amount_cents) > $2
            )                                                             AS "mismatch_count!: i64",
            COUNT(*)                                                      AS "total!: i64"
        FROM payments.reconciliation_entries re
        LEFT JOIN ops.routes r ON r.id = re.route_id
        WHERE re.reconciled_at >= $3
          AND re.reconciled_at <  $4
          AND ($6::uuid IS NULL OR re.route_id = $6)
          AND ($7::uuid IS NULL OR r.depot_id  = $7)
        GROUP BY 1
        ORDER BY 1
        "#,
        query.granularity,
        threshold_cents,
        query.date_from,
        query.date_to,
        granularity_interval(&query.granularity),
        query.route_id as Option<Uuid>,
        query.depot_id as Option<Uuid>,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| TimeSeriesPoint {
            period_start: r.period_start,
            period_end:   r.period_end,
            value:        r.mismatch_count as f64,
            sample_count: r.total,
        })
        .collect())
}

// ============================================================
// Helpers
// ============================================================

/// PostgreSQL interval literal corresponding to each granularity level.
/// Used to compute `period_end = date_trunc(...) + interval`.
fn granularity_interval(granularity: &str) -> &'static str {
    match granularity {
        "hour"  => "1 hour",
        "week"  => "1 week",
        "month" => "1 month",
        _       => "1 day",   // default "day"
    }
}

fn unit_for_formula(formula_type: &str) -> String {
    match formula_type {
        "on_time_departure_rate"         => "%".to_string(),
        "refund_rate"                    => "%".to_string(),
        "reconciliation_mismatch_count"  => "transactions".to_string(),
        _                                => "".to_string(),
    }
}

fn summarise(series: &[TimeSeriesPoint]) -> MetricSummary {
    if series.is_empty() {
        return MetricSummary { total_samples: 0, average: 0.0, min: 0.0, max: 0.0 };
    }
    let total_samples: i64 = series.iter().map(|p| p.sample_count).sum();
    let values: Vec<f64>   = series.iter().map(|p| p.value).collect();
    let average = values.iter().copied().sum::<f64>() / values.len() as f64;
    let min     = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max     = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    MetricSummary { total_samples, average, min, max }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn granularity_intervals_are_correct() {
        assert_eq!(granularity_interval("hour"),  "1 hour");
        assert_eq!(granularity_interval("day"),   "1 day");
        assert_eq!(granularity_interval("week"),  "1 week");
        assert_eq!(granularity_interval("month"), "1 month");
        assert_eq!(granularity_interval("other"), "1 day");
    }

    #[test]
    fn summarise_empty_returns_zeros() {
        let s = summarise(&[]);
        assert_eq!(s.total_samples, 0);
        assert_eq!(s.average, 0.0);
    }

    #[test]
    fn summarise_single_point() {
        use chrono::TimeZone;
        let p = TimeSeriesPoint {
            period_start:  Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            period_end:    Utc.with_ymd_and_hms(2025, 1, 2, 0, 0, 0).unwrap(),
            value:         75.0,
            sample_count:  10,
        };
        let s = summarise(&[p]);
        assert_eq!(s.total_samples, 10);
        assert!((s.average - 75.0).abs() < 1e-9);
        assert!((s.min - 75.0).abs() < 1e-9);
        assert!((s.max - 75.0).abs() < 1e-9);
    }

    #[test]
    fn summarise_multiple_points() {
        use chrono::TimeZone;
        let make = |v: f64, n: i64| TimeSeriesPoint {
            period_start:  Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            period_end:    Utc.with_ymd_and_hms(2025, 1, 2, 0, 0, 0).unwrap(),
            value: v, sample_count: n,
        };
        let series = vec![make(60.0, 5), make(80.0, 10), make(100.0, 3)];
        let s = summarise(&series);
        assert_eq!(s.total_samples, 18);
        assert!((s.min - 60.0).abs() < 1e-9);
        assert!((s.max - 100.0).abs() < 1e-9);
    }
}
