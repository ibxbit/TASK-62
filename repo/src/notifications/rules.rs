/// Subscription rule evaluation engine.
///
/// Rule types and when they fire
/// ─────────────────────────────
///
///  ┌──────────────────────┬──────────────────┬──────────────────────────────────┐
///  │ Type                 │ Trigger           │ Matches on                       │
///  ├──────────────────────┼──────────────────┼──────────────────────────────────┤
///  │ keyword              │ Every event       │ Payload/event_type contains any  │
///  │                      │                   │ (or all) of the listed keywords  │
///  ├──────────────────────┼──────────────────┼──────────────────────────────────┤
///  │ topic                │ Every event       │ event_type matches glob pattern  │
///  │                      │                   │ (e.g. "ops.trip.*") or prefix    │
///  ├──────────────────────┼──────────────────┼──────────────────────────────────┤
///  │ entity_threshold     │ Bus poll (5 s)    │ Metric crosses operator+threshold│
///  │                      │                   │ Metrics: open_conflicts,         │
///  │                      │                   │ unassigned_trips, active_trips   │
///  ├──────────────────────┼──────────────────┼──────────────────────────────────┤
///  │ spike                │ Bus poll (5 s)    │ Metric changes > threshold_pct % │
///  │                      │                   │ in one window vs the previous    │
///  │                      │                   │ Metrics: conflict_rate,          │
///  │                      │                   │ cancellation_rate,               │
///  │                      │                   │ driver_assignment_rate           │
///  └──────────────────────┴──────────────────┴──────────────────────────────────┘
///
/// Cooldown: each rule tracks `last_triggered_at`. Rules are skipped until
/// `now() - last_triggered_at >= cooldown_minutes`.
///
/// Alert delivery: rules fire `sys.rule_alert` events (processed_at set immediately)
/// and create deliveries directly. DND is respected per-user except for
/// `severity_override = "critical"` rules.
use chrono::Utc;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::notifications::{
    bus::check_dnd,
    models::{PendingEventRow, SubscriptionRuleRow},
};

// ── Config validation ─────────────────────────────────────────────────────────

/// Validate that `config` contains the required fields for `rule_type`.
/// Returns `Ok(())` or an error string describing the problem.
pub fn validate_rule_config(rule_type: &str, config: &Value) -> Result<(), String> {
    match rule_type {
        "keyword" => {
            let kws = config
                .get("keywords")
                .and_then(|v| v.as_array())
                .ok_or("'keywords' array is required")?;

            if kws.is_empty() {
                return Err("'keywords' must not be empty".into());
            }
            if !kws.iter().all(|k| k.is_string()) {
                return Err("'keywords' must all be strings".into());
            }
            if let Some(mode) = config.get("match_mode").and_then(|v| v.as_str()) {
                if !["any", "all"].contains(&mode) {
                    return Err("'match_mode' must be 'any' or 'all'".into());
                }
            }
            Ok(())
        }

        "topic" => {
            let has_pattern = config.get("pattern").and_then(|v| v.as_str()).is_some();
            let has_topics = config
                .get("topics")
                .and_then(|v| v.as_array())
                .map_or(false, |a| !a.is_empty());

            if !has_pattern && !has_topics {
                return Err(
                    "'pattern' (string) or non-empty 'topics' (array of strings) is required".into(),
                );
            }
            Ok(())
        }

        "entity_threshold" => {
            const VALID_METRICS: &[&str] =
                &["open_conflicts", "unassigned_trips", "active_trips"];
            const VALID_OPS: &[&str] = &[">", ">=", "==", "<=", "<"];

            let metric = config
                .get("metric")
                .and_then(|v| v.as_str())
                .ok_or("'metric' string is required")?;
            if !VALID_METRICS.contains(&metric) {
                return Err(format!(
                    "'metric' must be one of: {}",
                    VALID_METRICS.join(", ")
                ));
            }

            config
                .get("threshold")
                .and_then(|v| v.as_f64())
                .ok_or("'threshold' number is required")?;

            let op = config
                .get("operator")
                .and_then(|v| v.as_str())
                .ok_or("'operator' string is required")?;
            if !VALID_OPS.contains(&op) {
                return Err(format!("'operator' must be one of: {}", VALID_OPS.join(", ")));
            }

            if let Some(eid) = config.get("entity_id").and_then(|v| v.as_str()) {
                Uuid::parse_str(eid).map_err(|_| "'entity_id' is not a valid UUID")?;
            }
            Ok(())
        }

        "spike" => {
            const VALID_METRICS: &[&str] =
                &["conflict_rate", "cancellation_rate", "driver_assignment_rate"];

            let metric = config
                .get("metric")
                .and_then(|v| v.as_str())
                .ok_or("'metric' string is required")?;
            if !VALID_METRICS.contains(&metric) {
                return Err(format!(
                    "'metric' must be one of: {}",
                    VALID_METRICS.join(", ")
                ));
            }

            let pct = config
                .get("threshold_pct")
                .and_then(|v| v.as_f64())
                .ok_or("'threshold_pct' number is required")?;
            if pct <= 0.0 {
                return Err("'threshold_pct' must be > 0".into());
            }

            if let Some(dir) = config.get("direction").and_then(|v| v.as_str()) {
                if !["up", "down", "either"].contains(&dir) {
                    return Err("'direction' must be 'up', 'down', or 'either'".into());
                }
            }
            Ok(())
        }

        other => Err(format!("Unknown rule_type: '{}'", other)),
    }
}

// ── Matching — pure functions, no I/O ────────────────────────────────────────

/// Returns true if the event's payload or type contains the configured keywords.
fn matches_keyword(event_type: &str, payload: &Value, config: &Value) -> bool {
    let keywords = match config.get("keywords").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None      => return false,
    };

    let mode = config.get("match_mode").and_then(|v| v.as_str()).unwrap_or("any");

    // Search space: serialised payload + event_type, all lower-case
    let mut corpus = payload.to_string();
    corpus.push(' ');
    corpus.push_str(event_type);
    let corpus = corpus.to_lowercase();

    if mode == "all" {
        keywords
            .iter()
            .filter_map(|k| k.as_str())
            .all(|k| corpus.contains(&k.to_lowercase()))
    } else {
        keywords
            .iter()
            .filter_map(|k| k.as_str())
            .any(|k| corpus.contains(&k.to_lowercase()))
    }
}

/// Returns true if `event_type` matches the rule's pattern or topic prefix list.
///
/// Pattern semantics:
///   - `"ops.trip.*"` → prefix `"ops.trip"` (everything under that namespace)
///   - `"ops.trip.cancelled"` → exact match
///   - topics `["ops.trip", "ops.request"]` → prefix match on any listed prefix
fn matches_topic(event_type: &str, config: &Value) -> bool {
    if let Some(pattern) = config.get("pattern").and_then(|v| v.as_str()) {
        return if let Some(prefix) = pattern.strip_suffix(".*") {
            event_type == prefix || event_type.starts_with(&format!("{}.", prefix))
        } else {
            event_type == pattern
        };
    }

    if let Some(topics) = config.get("topics").and_then(|v| v.as_array()) {
        return topics
            .iter()
            .filter_map(|v| v.as_str())
            .any(|t| event_type == t || event_type.starts_with(&format!("{}.", t)));
    }

    false
}

// ── Threshold helpers ─────────────────────────────────────────────────────────

async fn query_threshold_metric(
    pool:      &PgPool,
    metric:    &str,
    entity_id: Option<Uuid>,
) -> Result<i64, sqlx::Error> {
    match metric {
        "open_conflicts" => {
            if let Some(eid) = entity_id {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM ops.trip_conflicts \
                     WHERE (trip_id_1 = $1 OR trip_id_2 = $1) AND status = 'open'",
                )
                .bind(eid)
                .fetch_one(pool)
                .await
            } else {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM ops.trip_conflicts WHERE status = 'open'",
                )
                .fetch_one(pool)
                .await
            }
        }
        "unassigned_trips" => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM ops.trips \
                 WHERE  assigned_driver_id IS NULL \
                   AND  status IN ('published', 'scheduled') \
                   AND  scheduled_departure BETWEEN now() AND now() + interval '2 hours' \
                   AND  deleted_at IS NULL",
            )
            .fetch_one(pool)
            .await
        }
        "active_trips" => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM ops.trips \
                 WHERE status = 'in_progress' AND deleted_at IS NULL",
            )
            .fetch_one(pool)
            .await
        }
        _ => Ok(0),
    }
}

fn threshold_triggered(current: i64, threshold: i64, operator: &str) -> bool {
    match operator {
        ">"  => current >  threshold,
        ">=" => current >= threshold,
        "==" => current == threshold,
        "<=" => current <= threshold,
        "<"  => current <  threshold,
        _    => false,
    }
}

// ── Spike helpers ─────────────────────────────────────────────────────────────

/// Returns (current_window_count, previous_window_count) by counting events
/// in two consecutive windows of equal length.
async fn query_spike_metric(
    pool:           &PgPool,
    metric:         &str,
    window_minutes: i64,
) -> Result<(i64, i64), sqlx::Error> {
    let event_type = match metric {
        "conflict_rate"          => "ops.trip.conflict_detected",
        "cancellation_rate"      => "ops.trip.cancelled",
        "driver_assignment_rate" => "ops.trip.driver_assigned",
        _                        => return Ok((0, 0)),
    };

    let current: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications.events \
         WHERE  event_type = $1 \
           AND  created_at > now() - ($2 * interval '1 minute')",
    )
    .bind(event_type)
    .bind(window_minutes)
    .fetch_one(pool)
    .await?;

    let previous: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications.events \
         WHERE  event_type = $1 \
           AND  created_at BETWEEN \
                now() - ($2 * interval '1 minute' * 2) \
            AND now() - ($2 * interval '1 minute')",
    )
    .bind(event_type)
    .bind(window_minutes)
    .fetch_one(pool)
    .await?;

    Ok((current, previous))
}

/// Returns the percentage change if a spike is detected, or `None`.
/// Treats previous = 0, current > 0 as a 100× the threshold (always triggers).
fn spike_change_pct(
    current:       i64,
    previous:      i64,
    threshold_pct: f64,
    direction:     &str,
) -> Option<f64> {
    if current == 0 && previous == 0 {
        return None;
    }
    let change_pct: f64 = if previous == 0 {
        // Any nonzero current vs zero previous is an infinite spike
        if current > 0 { threshold_pct * 100.0 } else { 0.0 }
    } else {
        (current - previous) as f64 / previous as f64 * 100.0
    };

    let triggered = match direction {
        "up"     => change_pct >= threshold_pct,
        "down"   => change_pct <= -threshold_pct,
        "either" => change_pct.abs() >= threshold_pct,
        _        => false,
    };

    if triggered { Some(change_pct) } else { None }
}

// ── Rule firing ───────────────────────────────────────────────────────────────

/// Create a `sys.rule_alert` event and deliver it to the rule owner.
/// Respects DND (critical severity_override bypasses it).
/// Updates `last_triggered_at` on the rule.
async fn fire_rule(
    pool:        &PgPool,
    rule:        &SubscriptionRuleRow,
    description: String,
) -> Result<(), sqlx::Error> {
    let severity = rule.severity_override.as_deref().unwrap_or("warning");

    // DND check — critical alerts always get through
    if severity != "critical" && check_dnd(pool, rule.user_id).await? {
        tracing::debug!(
            rule_id = %rule.id,
            user_id = %rule.user_id,
            "Rule alert suppressed: DND active (severity = {})", severity
        );
        // Queue the alert for delivery when DND ends
        let event_id: Uuid = create_rule_alert_event(pool, rule, &description, severity).await?;
        sqlx::query(
            "INSERT INTO notifications.deliveries (event_id, user_id, status) \
             VALUES ($1, $2, 'queued') ON CONFLICT (event_id, user_id) DO NOTHING",
        )
        .bind(event_id)
        .bind(rule.user_id)
        .execute(pool)
        .await?;
    } else {
        let event_id: Uuid = create_rule_alert_event(pool, rule, &description, severity).await?;
        sqlx::query(
            "INSERT INTO notifications.deliveries (event_id, user_id, status, delivered_at) \
             VALUES ($1, $2, 'delivered', now()) ON CONFLICT (event_id, user_id) DO NOTHING",
        )
        .bind(event_id)
        .bind(rule.user_id)
        .execute(pool)
        .await?;
    }

    // Advance cooldown timer
    sqlx::query(
        "UPDATE notifications.subscription_rules \
         SET    last_triggered_at = now(), updated_at = now() \
         WHERE  id = $1",
    )
    .bind(rule.id)
    .execute(pool)
    .await?;

    tracing::info!(
        rule_id   = %rule.id,
        rule_type = %rule.rule_type,
        user_id   = %rule.user_id,
        "Rule '{}' fired: {}",
        rule.rule_name, description
    );

    Ok(())
}

/// Insert a `sys.rule_alert` event with `processed_at` pre-set so the bus
/// never tries to fan it out via the subscription table.
async fn create_rule_alert_event(
    pool:        &PgPool,
    rule:        &SubscriptionRuleRow,
    description: &str,
    severity:    &str,
) -> Result<Uuid, sqlx::Error> {
    let payload = serde_json::json!({
        "rule_id":     rule.id,
        "rule_name":   rule.rule_name,
        "rule_type":   rule.rule_type,
        "description": description,
        "severity":    severity,
    });

    sqlx::query_scalar(
        r#"
        INSERT INTO notifications.events
            (event_type, source_domain, source_entity_id, payload, processed_at)
        VALUES ('sys.rule_alert', 'sys', $1, $2, now())
        RETURNING id
        "#,
    )
    .bind(rule.id)
    .bind(&payload)
    .fetch_one(pool)
    .await
}

// ── Cooldown helper ───────────────────────────────────────────────────────────

fn is_in_cooldown(rule: &SubscriptionRuleRow) -> bool {
    rule.last_triggered_at.map_or(false, |last| {
        (Utc::now() - last).num_minutes() < rule.cooldown_minutes as i64
    })
}

// ── Public entry points ───────────────────────────────────────────────────────

/// Evaluate `keyword` and `topic` rules against a batch of events.
/// Called once per bus poll cycle after basic fan-out completes.
/// Rules that match create a `sys.rule_alert` delivery for the rule owner.
pub async fn evaluate_event_rules(
    pool:   &PgPool,
    events: &[PendingEventRow],
) -> Result<(), sqlx::Error> {
    if events.is_empty() {
        return Ok(());
    }

    // Load all active keyword/topic rules once for the whole batch
    let rules = sqlx::query_as::<_, SubscriptionRuleRow>(
        r#"
        SELECT id, user_id, rule_name, rule_type, is_enabled, config,
               severity_override, cooldown_minutes, last_triggered_at,
               created_at, updated_at
        FROM   notifications.subscription_rules
        WHERE  is_enabled = TRUE
          AND  rule_type  IN ('keyword', 'topic')
        "#,
    )
    .fetch_all(pool)
    .await?;

    if rules.is_empty() {
        return Ok(());
    }

    for event in events {
        for rule in &rules {
            if is_in_cooldown(rule) {
                continue;
            }

            let matched = match rule.rule_type.as_str() {
                "keyword" => matches_keyword(&event.event_type, &event.payload, &rule.config),
                "topic"   => matches_topic(&event.event_type, &rule.config),
                _         => false,
            };

            if matched {
                let desc = format!(
                    "{} rule matched event '{}' (entity: {})",
                    rule.rule_type,
                    event.event_type,
                    event
                        .source_entity_id
                        .map(|u| u.to_string())
                        .as_deref()
                        .unwrap_or("—"),
                );
                if let Err(e) = fire_rule(pool, rule, desc).await {
                    tracing::error!(rule_id = %rule.id, "Failed to fire rule: {}", e);
                }
            }
        }
    }

    Ok(())
}

/// Evaluate `entity_threshold` and `spike` rules.
/// Called on every bus poll, independent of pending events.
pub async fn evaluate_periodic_rules(pool: &PgPool) -> Result<(), sqlx::Error> {
    let rules = sqlx::query_as::<_, SubscriptionRuleRow>(
        r#"
        SELECT id, user_id, rule_name, rule_type, is_enabled, config,
               severity_override, cooldown_minutes, last_triggered_at,
               created_at, updated_at
        FROM   notifications.subscription_rules
        WHERE  is_enabled = TRUE
          AND  rule_type  IN ('entity_threshold', 'spike')
        "#,
    )
    .fetch_all(pool)
    .await?;

    for rule in &rules {
        if is_in_cooldown(rule) {
            continue;
        }

        let trigger_desc = match rule.rule_type.as_str() {
            "entity_threshold" => eval_threshold_rule(pool, rule).await?,
            "spike"            => eval_spike_rule(pool, rule).await?,
            _                  => None,
        };

        if let Some(desc) = trigger_desc {
            if let Err(e) = fire_rule(pool, rule, desc).await {
                tracing::error!(rule_id = %rule.id, "Failed to fire rule: {}", e);
            }
        }
    }

    Ok(())
}

// ── Per-rule evaluators ───────────────────────────────────────────────────────

async fn eval_threshold_rule(
    pool: &PgPool,
    rule: &SubscriptionRuleRow,
) -> Result<Option<String>, sqlx::Error> {
    let metric    = rule.config.get("metric")   .and_then(|v| v.as_str()).unwrap_or("");
    let threshold = rule.config.get("threshold").and_then(|v| v.as_i64()).unwrap_or(0);
    let operator  = rule.config.get("operator") .and_then(|v| v.as_str()).unwrap_or(">=");
    let entity_id = rule
        .config
        .get("entity_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok());

    let current = query_threshold_metric(pool, metric, entity_id).await?;

    if threshold_triggered(current, threshold, operator) {
        Ok(Some(format!(
            "Threshold alert '{}': {} = {} ({} {})",
            rule.rule_name, metric, current, operator, threshold
        )))
    } else {
        Ok(None)
    }
}

async fn eval_spike_rule(
    pool: &PgPool,
    rule: &SubscriptionRuleRow,
) -> Result<Option<String>, sqlx::Error> {
    let metric         = rule.config.get("metric")        .and_then(|v| v.as_str()).unwrap_or("");
    let window_minutes = rule.config.get("window_minutes").and_then(|v| v.as_i64()).unwrap_or(10);
    let threshold_pct  = rule.config.get("threshold_pct") .and_then(|v| v.as_f64()).unwrap_or(50.0);
    let direction      = rule.config.get("direction")     .and_then(|v| v.as_str()).unwrap_or("up");

    let (current, previous) = query_spike_metric(pool, metric, window_minutes).await?;

    if let Some(change_pct) = spike_change_pct(current, previous, threshold_pct, direction) {
        Ok(Some(format!(
            "Spike alert '{}': {} changed {}{:.1}% ({}→{} over {}min window)",
            rule.rule_name,
            metric,
            if change_pct >= 0.0 { "+" } else { "" },
            change_pct,
            previous,
            current,
            window_minutes
        )))
    } else {
        Ok(None)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keyword_any_match() {
        let config = json!({ "keywords": ["conflict", "cancelled"] });
        let payload = json!({ "reason": "route conflict" });
        assert!(matches_keyword("ops.trip.modified", &payload, &config));
    }

    #[test]
    fn keyword_all_no_match() {
        let config = json!({ "keywords": ["conflict", "cancelled"], "match_mode": "all" });
        let payload = json!({ "reason": "conflict detected" });
        assert!(!matches_keyword("ops.trip.modified", &payload, &config));
    }

    #[test]
    fn keyword_all_match() {
        let config = json!({ "keywords": ["conflict", "trip"], "match_mode": "all" });
        let payload = json!({ "type": "conflict" });
        // event_type "ops.trip.modified" contains "trip"; payload contains "conflict"
        assert!(matches_keyword("ops.trip.modified", &payload, &config));
    }

    #[test]
    fn topic_glob_match() {
        let config = json!({ "pattern": "ops.trip.*" });
        assert!(matches_topic("ops.trip.modified",   &config));
        assert!(matches_topic("ops.trip.cancelled",  &config));
        assert!(!matches_topic("ops.request.submitted", &config));
    }

    #[test]
    fn topic_exact_match() {
        let config = json!({ "pattern": "ops.trip.cancelled" });
        assert!(matches_topic("ops.trip.cancelled", &config));
        assert!(!matches_topic("ops.trip.modified", &config));
    }

    #[test]
    fn topic_prefix_list() {
        let config = json!({ "topics": ["ops.trip", "sys"] });
        assert!(matches_topic("ops.trip.started",  &config));
        assert!(matches_topic("sys.announcement",  &config));
        assert!(!matches_topic("ops.request.approved", &config));
    }

    #[test]
    fn spike_up_triggered() {
        let pct = spike_change_pct(10, 5, 50.0, "up");
        assert!(pct.is_some());
        assert!((pct.unwrap() - 100.0).abs() < 0.01);
    }

    #[test]
    fn spike_up_not_triggered() {
        assert!(spike_change_pct(6, 5, 50.0, "up").is_none());
    }

    #[test]
    fn spike_down_triggered() {
        let pct = spike_change_pct(2, 10, 50.0, "down");
        assert!(pct.is_some());
        assert!(pct.unwrap() < 0.0);
    }

    #[test]
    fn spike_from_zero() {
        // previous=0, current=3 → always triggers regardless of threshold
        let pct = spike_change_pct(3, 0, 10.0, "up");
        assert!(pct.is_some());
    }

    #[test]
    fn threshold_operators() {
        assert!( threshold_triggered(5, 3, ">"));
        assert!(!threshold_triggered(3, 3, ">"));
        assert!( threshold_triggered(3, 3, ">="));
        assert!( threshold_triggered(3, 3, "=="));
        assert!(!threshold_triggered(4, 3, "=="));
        assert!( threshold_triggered(2, 3, "<"));
    }

    #[test]
    fn validate_keyword_config_ok() {
        let cfg = json!({ "keywords": ["emergency"], "match_mode": "any" });
        assert!(validate_rule_config("keyword", &cfg).is_ok());
    }

    #[test]
    fn validate_keyword_config_missing() {
        assert!(validate_rule_config("keyword", &json!({})).is_err());
    }

    #[test]
    fn validate_spike_invalid_direction() {
        let cfg = json!({ "metric": "conflict_rate", "threshold_pct": 50.0, "direction": "sideways" });
        assert!(validate_rule_config("spike", &cfg).is_err());
    }
}
