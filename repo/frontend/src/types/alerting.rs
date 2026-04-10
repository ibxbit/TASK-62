use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// An alert record.
#[derive(Clone, PartialEq, Deserialize, Debug)]
pub struct Alert {
    pub id:               Uuid,
    pub alert_type:       String,
    pub severity:         String,
    pub title:            String,
    pub description:      String,
    pub status:           String,
    pub source_entity_id: Option<Uuid>,
    pub acknowledged_by:  Option<Uuid>,
    pub acknowledged_at:  Option<DateTime<Utc>>,
    pub closed_by:        Option<Uuid>,
    pub closed_at:        Option<DateTime<Utc>>,
    pub created_at:       DateTime<Utc>,
}

impl Alert {
    pub fn is_open(&self)         -> bool { self.status == "open" }
    pub fn is_acknowledged(&self) -> bool { self.status == "acknowledged" }
    pub fn is_closed(&self)       -> bool { self.status == "closed" }

    pub fn severity_class(&self) -> &str {
        match self.severity.as_str() {
            "critical" => "alert--critical",
            "warning"  => "alert--warning",
            _          => "alert--info",
        }
    }

    pub fn type_label(&self) -> &str {
        match self.alert_type.as_str() {
            "kpi_anomaly"        => "KPI Anomaly",
            "reconciliation"     => "Reconciliation",
            "system"             => "System",
            other                => other,
        }
    }
}

/// Alert statistics.
#[derive(Clone, PartialEq, Deserialize, Debug, Default)]
pub struct AlertStats {
    pub total_open:         i64,
    pub total_critical:     i64,
    pub total_acknowledged: i64,
}

// ── Alert rule subscription types ────────────────────────────────────────────

/// An alert rule — defines conditions that, when matched, emit an alert.
///
/// `rule_type` controls which condition fields are meaningful:
///   - `"keyword"`          → conditions.keyword, conditions.match_mode
///   - `"topic"`            → conditions.topic
///   - `"entity_threshold"` → conditions.metric_key, conditions.threshold, conditions.operator
///   - `"spike_detection"`  → conditions.metric_key, conditions.multiplier, conditions.window_minutes
#[derive(Clone, PartialEq, Deserialize, Debug)]
pub struct AlertRule {
    pub id:                                Uuid,
    pub name:                              String,
    pub rule_type:                         String,
    pub severity:                          String,
    pub conditions:                        serde_json::Value,
    pub duplicate_suppression_window_secs: i32,
    pub is_active:                         bool,
    pub created_at:                        DateTime<Utc>,
    pub updated_at:                        DateTime<Utc>,
}

impl AlertRule {
    pub fn rule_type_label(&self) -> &str {
        match self.rule_type.as_str() {
            "keyword"          => "Keyword",
            "topic"            => "Topic",
            "entity_threshold" => "Entity Threshold",
            "spike_detection"  => "Spike Detection",
            other              => other,
        }
    }

    /// Returns a human-readable summary of the conditions for display in the table.
    pub fn conditions_summary(&self) -> String {
        match self.rule_type.as_str() {
            "keyword" => self.conditions.get("keyword")
                .and_then(|v| v.as_str())
                .map(|k| format!("keyword: {}", k))
                .unwrap_or_else(|| "—".to_string()),
            "topic" => self.conditions.get("topic")
                .and_then(|v| v.as_str())
                .map(|t| format!("topic: {}", t))
                .unwrap_or_else(|| "—".to_string()),
            "entity_threshold" => {
                let key = self.conditions.get("metric_key").and_then(|v| v.as_str()).unwrap_or("?");
                let thr = self.conditions.get("threshold").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let op  = self.conditions.get("operator").and_then(|v| v.as_str()).unwrap_or("gt");
                format!("{} {} {:.2}", key, op, thr)
            }
            "spike_detection" => {
                let key = self.conditions.get("metric_key").and_then(|v| v.as_str()).unwrap_or("?");
                let mul = self.conditions.get("multiplier").and_then(|v| v.as_f64()).unwrap_or(2.0);
                format!("{} × {:.1}× spike", key, mul)
            }
            _ => "—".to_string(),
        }
    }
}

/// Request body to create an alert rule.
#[derive(Serialize, Default)]
pub struct CreateAlertRuleRequest {
    pub name:                              String,
    pub rule_type:                         String,
    pub severity:                          String,
    pub conditions:                        serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duplicate_suppression_window_secs: Option<i32>,
}

/// Request body to update an alert rule.
#[derive(Serialize, Default)]
pub struct UpdateAlertRuleRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name:                              Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity:                          Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conditions:                        Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_active:                         Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duplicate_suppression_window_secs: Option<i32>,
}
