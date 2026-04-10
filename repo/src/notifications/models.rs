use chrono::{DateTime, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================
// Shared bus type (used by bus.rs and rules.rs)
// ============================================================

/// Pending event fetched by the bus for fan-out processing.
#[derive(sqlx::FromRow)]
pub struct PendingEventRow {
    pub id:                 Uuid,
    pub event_type:         String,
    pub source_entity_id:   Option<Uuid>,
    pub actor_id:           Option<Uuid>,
    pub payload:            serde_json::Value,
    /// COALESCE(payload->>'severity', event_definitions.severity, 'info')
    pub effective_severity: String,
}

// ============================================================
// Subscription rule DB row
// ============================================================

#[derive(sqlx::FromRow)]
pub struct SubscriptionRuleRow {
    pub id:                Uuid,
    pub user_id:           Uuid,
    pub rule_name:         String,
    pub rule_type:         String,
    pub is_enabled:        bool,
    pub config:            serde_json::Value,
    pub severity_override: Option<String>,
    pub cooldown_minutes:  i32,
    pub last_triggered_at: Option<DateTime<Utc>>,
    pub created_at:        DateTime<Utc>,
    pub updated_at:        DateTime<Utc>,
}

// ============================================================
// Inbox request bodies
// ============================================================

#[derive(Deserialize)]
pub struct DeliveryQuery {
    /// `unread` (default) | `read` | `dismissed` | `queued` | `all`
    pub status: Option<String>,
    pub limit:  Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Deserialize)]
pub struct UpdatePreferencesRequest {
    pub dnd_enabled: bool,
    /// UTC time window start (e.g. "22:00:00"). Both fields required together,
    /// or both omitted for all-day DND.
    pub dnd_start:   Option<NaiveTime>,
    pub dnd_end:     Option<NaiveTime>,
}

#[derive(Deserialize)]
pub struct UpdateSubscriptionsRequest {
    /// Complete replacement of the user's event-type subscriptions.
    pub event_types: Vec<String>,
}

#[derive(Deserialize)]
pub struct AnnounceRequest {
    pub title:        String,
    pub message:      String,
    /// `info` | `warning` | `critical`  (default: `info`)
    pub severity:     Option<String>,
    /// Role names to target, or `["all"]` / omit for all active users.
    pub target_roles: Option<Vec<String>>,
}

/// Bulk delivery receipt — sent by the frontend when it displays notifications.
/// Queued deliveries in the list are promoted to 'delivered' so the user sees
/// everything in their inbox regardless of the DND window they had when the
/// notifications were originally queued.
#[derive(Deserialize)]
pub struct ReceiptRequest {
    pub delivery_ids: Vec<Uuid>,
}

// ── Subscription rule request bodies ─────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateRuleRequest {
    pub rule_name:         String,
    /// `keyword` | `topic` | `entity_threshold` | `spike`
    pub rule_type:         String,
    /// Type-specific config (validated on create).
    pub config:            serde_json::Value,
    /// Overrides the fired alert's severity. `null` = use rule-type default.
    pub severity_override: Option<String>,
    /// Minimum minutes between successive firings (default: 15, min: 1).
    pub cooldown_minutes:  Option<i32>,
}

#[derive(Deserialize)]
pub struct UpdateRuleRequest {
    pub rule_name:         Option<String>,
    pub config:            Option<serde_json::Value>,
    pub severity_override: Option<String>,
    pub cooldown_minutes:  Option<i32>,
}

// ============================================================
// Response types
// ============================================================

#[derive(Serialize)]
pub struct DeliveryResponse {
    pub id:               Uuid,
    pub event_id:         Uuid,
    pub event_type:       String,
    pub severity:         String,
    pub source_entity_id: Option<Uuid>,
    pub payload:          serde_json::Value,
    pub status:           String,
    pub delivered_at:     Option<DateTime<Utc>>,
    pub read_at:          Option<DateTime<Utc>>,
    pub created_at:       DateTime<Utc>,
}

#[derive(Serialize)]
pub struct UnreadCountResponse {
    pub unread:  i64,
    pub queued:  i64,
}

#[derive(Serialize)]
pub struct PreferencesResponse {
    pub dnd_enabled: bool,
    pub dnd_start:   Option<NaiveTime>,
    pub dnd_end:     Option<NaiveTime>,
    pub updated_at:  DateTime<Utc>,
}

#[derive(Serialize)]
pub struct SubscriptionInfo {
    pub event_type:  String,
    pub description: String,
    pub severity:    String,
    pub subscribed:  bool,
}

#[derive(Serialize)]
pub struct SubscriptionRuleResponse {
    pub id:                Uuid,
    pub rule_name:         String,
    pub rule_type:         String,
    pub is_enabled:        bool,
    pub config:            serde_json::Value,
    pub severity_override: Option<String>,
    pub cooldown_minutes:  i32,
    pub last_triggered_at: Option<DateTime<Utc>>,
    pub created_at:        DateTime<Utc>,
    pub updated_at:        DateTime<Utc>,
}

// ============================================================
// DB row types (inbox + preferences)
// ============================================================

#[derive(sqlx::FromRow)]
pub struct DeliveryRow {
    pub id:               Uuid,
    pub event_id:         Uuid,
    pub event_type:       String,
    pub severity:         String,
    pub source_entity_id: Option<Uuid>,
    pub payload:          serde_json::Value,
    pub status:           String,
    pub delivered_at:     Option<DateTime<Utc>>,
    pub read_at:          Option<DateTime<Utc>>,
    pub created_at:       DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
pub struct PreferencesRow {
    pub dnd_enabled: bool,
    pub dnd_start:   Option<NaiveTime>,
    pub dnd_end:     Option<NaiveTime>,
    pub updated_at:  DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
pub struct EventDefRow {
    pub event_type:  String,
    pub description: String,
    pub severity:    String,
}

// ============================================================
// Conversions
// ============================================================

impl From<DeliveryRow> for DeliveryResponse {
    fn from(r: DeliveryRow) -> Self {
        DeliveryResponse {
            id:               r.id,
            event_id:         r.event_id,
            event_type:       r.event_type,
            severity:         r.severity,
            source_entity_id: r.source_entity_id,
            payload:          r.payload,
            status:           r.status,
            delivered_at:     r.delivered_at,
            read_at:          r.read_at,
            created_at:       r.created_at,
        }
    }
}

impl From<PreferencesRow> for PreferencesResponse {
    fn from(r: PreferencesRow) -> Self {
        PreferencesResponse {
            dnd_enabled: r.dnd_enabled,
            dnd_start:   r.dnd_start,
            dnd_end:     r.dnd_end,
            updated_at:  r.updated_at,
        }
    }
}

impl From<SubscriptionRuleRow> for SubscriptionRuleResponse {
    fn from(r: SubscriptionRuleRow) -> Self {
        SubscriptionRuleResponse {
            id:                r.id,
            rule_name:         r.rule_name,
            rule_type:         r.rule_type,
            is_enabled:        r.is_enabled,
            config:            r.config,
            severity_override: r.severity_override,
            cooldown_minutes:  r.cooldown_minutes,
            last_triggered_at: r.last_triggered_at,
            created_at:        r.created_at,
            updated_at:        r.updated_at,
        }
    }
}

// ============================================================
// Channel preference models
// ============================================================

/// Valid channel identifiers (mirrors the DB CHECK constraint).
pub const VALID_CHANNELS: &[&str] = &["email", "sms", "wecom"];

/// Request body for `PUT /notifications/channels/{channel}`.
#[derive(Deserialize)]
pub struct UpsertChannelRequest {
    /// Delivery address: email address | E.164 phone | WeCom user ID.
    /// Omit to keep the existing address (update-only mode).
    pub channel_address: Option<String>,
    /// Whether this channel is active (default: `true`).
    /// Accepts both `enabled` and `is_enabled` for client compatibility.
    #[serde(alias = "is_enabled")]
    pub enabled:         Option<bool>,
}

/// DB row + API response for a channel preference.
#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct ChannelPreferenceResponse {
    pub channel:         String,
    pub enabled:         bool,
    pub channel_address: String,
    pub updated_at:      DateTime<Utc>,
}
