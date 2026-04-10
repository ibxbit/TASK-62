/// Pluggable notification channel adapters.
///
/// ## Interface contract
///
/// Each adapter implements [`NotificationAdapter`]:
///
/// ```text
/// channel()       → canonical name: "email" | "sms" | "wecom"
/// is_available()  → true only when the on-prem connector is configured
/// send()          → dispatch one outbound notification; non-fatal failure
/// ```
///
/// ## Enable / disable mechanism
///
/// Adapters are **inert by default**.  An adapter becomes active only when its
/// connector URL environment variable is present:
///
/// | Channel | Env var            | On-prem endpoint      |
/// |---------|--------------------|-----------------------|
/// | email   | `EMAIL_RELAY_URL`  | Internal SMTP relay   |
/// | sms     | `SMS_GATEWAY_URL`  | On-prem SMS appliance |
/// | wecom   | `WECOM_WEBHOOK_URL`| WeCom Bot proxy       |
///
/// If the env var is absent the adapter's `is_available()` returns `false` and
/// the bus skips it entirely — no network calls, no errors.  The service
/// therefore works offline by default.
///
/// ## Registry
///
/// [`AdapterRegistry`] is `Clone` (cheap — wraps an `Arc`) and is passed to
/// the event-bus background task at startup.  HTTP handlers do not hold the
/// registry; they only manage per-user channel preferences in the DB.
///
/// ## Adding a new channel
///
/// 1. Create `src/notifications/adapters/new_channel.rs` implementing `NotificationAdapter`.
/// 2. Expose it here with `pub mod new_channel`.
/// 3. Instantiate it in `main.rs` and push it onto the registry builder.
/// 4. Add the new channel name to the CHECK constraint in migration 012 and
///    to `models::VALID_CHANNELS`.
pub mod email;
pub mod sms;
pub mod wecom;

use std::sync::Arc;

use uuid::Uuid;

// ============================================================
// Outbound notification payload
// ============================================================

/// All information an adapter needs to dispatch one notification.
#[derive(Debug, Clone)]
pub struct OutboundNotification {
    /// e.g. `"ops.trip.conflict_detected"`
    pub event_type:      String,
    /// `"info"` | `"warning"` | `"critical"`
    pub severity:        String,
    /// Short human-readable title derived from the event payload or event type.
    pub title:           String,
    /// Longer description for the message body.
    pub body:            String,
    /// Raw event payload for adapters that want structured data.
    pub payload:         serde_json::Value,
    /// Internal user ID — used for audit / correlation, not sent to channel.
    pub recipient_id:    Uuid,
    /// Channel-specific delivery address: email addr / E.164 phone / WeCom user ID.
    pub channel_address: String,
}

impl OutboundNotification {
    /// Build from a bus `PendingEventRow` plus channel address.
    pub fn from_event(
        event:           &crate::notifications::models::PendingEventRow,
        recipient_id:    Uuid,
        channel_address: String,
    ) -> Self {
        let (title, body) = derive_title_body(event);
        Self {
            event_type:   event.event_type.clone(),
            severity:     event.effective_severity.clone(),
            title,
            body,
            payload:      event.payload.clone(),
            recipient_id,
            channel_address,
        }
    }
}

/// Extract a human-readable title and body from an event's payload.
///
/// Respects well-known payload keys first (`"title"`, `"message"`,
/// `"description"`), then falls back to the event_type string.
fn derive_title_body(event: &crate::notifications::models::PendingEventRow) -> (String, String) {
    let title = event.payload
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or(&event.event_type)
        .to_string();

    let body = event.payload
        .get("message")
        .or_else(|| event.payload.get("description"))
        .or_else(|| event.payload.get("body"))
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| {
            format!(
                "Event {} triggered (severity: {}).",
                event.event_type, event.effective_severity
            )
        });

    (title, body)
}

// ============================================================
// Error type
// ============================================================

#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("adapter not configured — connector URL not set")]
    NotConfigured,
    #[error("delivery failed: {0}")]
    DeliveryFailed(String),
}

// ============================================================
// Trait
// ============================================================

/// Implemented by each channel adapter.
///
/// The `#[async_trait]` attribute rewrites the async method into a
/// `Box<dyn Future>` return type required for object-safe dispatch via
/// `Box<dyn NotificationAdapter>`.
#[async_trait::async_trait]
pub trait NotificationAdapter: Send + Sync {
    /// Canonical channel name stored in `channel_preferences.channel`.
    fn channel(&self) -> &'static str;

    /// Returns `true` when the on-prem connector URL env var is configured.
    ///
    /// The bus calls this before every dispatch; no network I/O is performed
    /// when `false`.
    fn is_available(&self) -> bool;

    /// Dispatch a single notification.
    ///
    /// Failures are non-fatal from the bus's perspective: the bus logs the
    /// error and records `status = 'failed'` in `channel_deliveries`, but does
    /// NOT abort inbox delivery or mark the event unprocessed.
    async fn send(&self, notif: &OutboundNotification) -> Result<(), AdapterError>;
}

// ============================================================
// Registry
// ============================================================

/// Holds all registered channel adapters.
///
/// Cheaply cloneable — the inner `Vec` is behind an `Arc`.
#[derive(Clone)]
pub struct AdapterRegistry {
    inner: Arc<Vec<Box<dyn NotificationAdapter>>>,
}

impl AdapterRegistry {
    pub fn new(adapters: Vec<Box<dyn NotificationAdapter>>) -> Self {
        Self { inner: Arc::new(adapters) }
    }

    /// Iterate over all adapters regardless of availability.
    pub fn all(&self) -> &[Box<dyn NotificationAdapter>] {
        &self.inner
    }

    /// `true` when at least one adapter's connector is configured.
    pub fn any_available(&self) -> bool {
        self.inner.iter().any(|a| a.is_available())
    }
}
