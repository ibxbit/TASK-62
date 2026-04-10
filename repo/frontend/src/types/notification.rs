use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Core notification type (mirrors backend DeliveryResponse) ─────────────────

#[derive(Clone, PartialEq, Deserialize)]
pub struct Notification {
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

impl Notification {
    // ── Status helpers ────────────────────────────────────────────────────────

    pub fn is_unread(&self) -> bool { self.status == "delivered" }
    pub fn is_queued(&self) -> bool { self.status == "queued" }
    pub fn is_read(&self)   -> bool { self.status == "read" }

    // ── Display helpers ───────────────────────────────────────────────────────

    /// Extract a human-readable title from the payload, falling back to the
    /// event type formatted with word separators.
    pub fn title(&self) -> String {
        if let Some(t) = self.payload.get("title").and_then(|v| v.as_str()) {
            return t.to_string();
        }
        // e.g. "ops.trip.conflict_detected" → "ops › trip › conflict detected"
        self.event_type
            .replace('.', " › ")
            .replace('_', " ")
    }

    /// Short summary text for the notification card body.
    pub fn message(&self) -> Option<String> {
        // Try explicit message field first (announcements, rule alerts)
        if let Some(m) = self.payload.get("message").and_then(|v| v.as_str()) {
            return Some(m.to_string());
        }
        // Fall back to description
        self.payload.get("description").and_then(|v| v.as_str()).map(String::from)
    }

    /// CSS class suffix for severity colouring (use with `notification--{class}`).
    pub fn severity_class(&self) -> &str {
        match self.severity.as_str() {
            "critical" => "critical",
            "warning"  => "warning",
            _          => "info",
        }
    }

    /// Unicode indicator shown beside the severity label.
    pub fn severity_icon(&self) -> &str {
        match self.severity.as_str() {
            "critical" => "🔴",
            "warning"  => "🟡",
            _          => "🔵",
        }
    }

    /// Category prefix used to group notifications visually.
    pub fn category(&self) -> &str {
        if self.event_type.starts_with("ops.trip") {
            "Trip"
        } else if self.event_type.starts_with("ops.request") {
            "Request"
        } else if self.event_type.starts_with("sys") {
            "System"
        } else {
            "Other"
        }
    }

    pub fn formatted_created_at(&self) -> String {
        self.created_at.format("%b %d, %H:%M UTC").to_string()
    }

    pub fn formatted_delivered_at(&self) -> Option<String> {
        self.delivered_at
            .map(|t| t.format("%b %d, %H:%M UTC").to_string())
    }

    pub fn formatted_read_at(&self) -> Option<String> {
        self.read_at.map(|t| t.format("%b %d, %H:%M UTC").to_string())
    }
}

// ── API response types ────────────────────────────────────────────────────────

#[derive(Clone, PartialEq, Deserialize, Default)]
pub struct UnreadCountResponse {
    pub unread: i64,
    pub queued: i64,
}

// ── API request types ─────────────────────────────────────────────────────────

/// Sent to POST /notifications/receipt when the inbox renders a batch.
#[derive(Serialize)]
pub struct ReceiptRequest {
    pub delivery_ids: Vec<Uuid>,
}

// ── UI filter state ───────────────────────────────────────────────────────────

#[derive(Clone, PartialEq, Default, Debug)]
pub enum StatusFilter {
    #[default]
    Unread,   // shows status='delivered' (unread)
    Queued,   // shows status='queued'   (held during DND)
    All,      // shows everything
}

impl StatusFilter {
    pub fn as_query_param(&self) -> &'static str {
        match self {
            StatusFilter::Unread  => "unread",
            StatusFilter::Queued  => "queued",
            StatusFilter::All     => "all",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            StatusFilter::Unread  => "Unread",
            StatusFilter::Queued  => "Queued (DND held)",
            StatusFilter::All     => "All",
        }
    }
}
