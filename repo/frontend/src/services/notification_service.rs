/// HTTP client layer for the notification API.
///
/// All functions are async and return `Result<T, gloo_net::Error>`.
/// The auth token is read via `load_persisted_token()` (same key used by
/// `auth_store`) and sent as a Bearer header on every request.
use gloo_net::http::{Request, RequestBuilder};
use uuid::Uuid;

use crate::store::auth_store::load_persisted_token;
use crate::types::notification::{Notification, ReceiptRequest, UnreadCountResponse};

const BASE: &str = "/api/notifications";

// ── Auth header helper ────────────────────────────────────────────────────────

fn bearer() -> Option<String> {
    load_persisted_token().map(|t| format!("Bearer {}", t))
}

fn get(url: &str) -> RequestBuilder {
    let req = Request::get(url);
    match bearer() {
        Some(h) => req.header("Authorization", &h),
        None    => req,
    }
}

fn post(url: &str) -> RequestBuilder {
    let req = Request::post(url);
    match bearer() {
        Some(h) => req.header("Authorization", &h),
        None    => req,
    }
}

#[allow(dead_code)]
fn delete_req(url: &str) -> RequestBuilder {
    let req = Request::delete(url);
    match bearer() {
        Some(h) => req.header("Authorization", &h),
        None    => req,
    }
}

// ── Inbox ─────────────────────────────────────────────────────────────────────

/// Fetch a paginated list of the caller's notifications.
/// `status` maps to the query param: `"unread"` | `"queued"` | `"read"` | `"all"`.
pub async fn fetch_notifications(
    status: &str,
    limit:  i64,
    offset: i64,
) -> Result<Vec<Notification>, gloo_net::Error> {
    get(&format!(
        "{}?status={}&limit={}&offset={}",
        BASE, status, limit, offset
    ))
    .send()
    .await?
    .json::<Vec<Notification>>()
    .await
}

/// Lightweight poll for badge counters. Returns `{unread, queued}`.
pub async fn fetch_unread_count() -> Result<UnreadCountResponse, gloo_net::Error> {
    get(&format!("{}/unread-count", BASE))
        .send()
        .await?
        .json::<UnreadCountResponse>()
        .await
}

/// Fetch the full details of a single notification (for the detail drawer).
pub async fn fetch_one(id: Uuid) -> Result<Notification, gloo_net::Error> {
    get(&format!("{}/{}", BASE, id))
        .send()
        .await?
        .json::<Notification>()
        .await
}

// ── Acknowledge / Close ───────────────────────────────────────────────────────

/// Acknowledge one notification — transitions `delivered` → `read`.
/// Sets `read_at` server-side (the read receipt timestamp).
pub async fn acknowledge(id: Uuid) -> Result<(), gloo_net::Error> {
    post(&format!("{}/{}/read", BASE, id))
        .send()
        .await?;
    Ok(())
}

/// Acknowledge (mark read) all currently unread notifications in one call.
pub async fn acknowledge_all() -> Result<(), gloo_net::Error> {
    post(&format!("{}/read-all", BASE))
        .send()
        .await?;
    Ok(())
}

/// Close/dismiss one notification — hides it from the default inbox view.
/// `dismissed` items are still queryable via `?status=all`.
pub async fn dismiss(id: Uuid) -> Result<(), gloo_net::Error> {
    post(&format!("{}/{}/dismiss", BASE, id))
        .send()
        .await?;
    Ok(())
}

// ── Delivery receipts ─────────────────────────────────────────────────────────

/// Send a delivery receipt for a batch of notification IDs.
///
/// Called by the inbox component immediately after rendering a page of results.
/// The backend promotes any `queued` deliveries in the list to `delivered`,
/// ensuring the user sees DND-held notifications when they actively open
/// their inbox.
///
/// Returns the number of queued items promoted on the server.
pub async fn send_receipt(ids: Vec<Uuid>) -> Result<u64, gloo_net::Error> {
    #[derive(serde::Deserialize)]
    struct Resp { promoted: u64 }

    let body = ReceiptRequest { delivery_ids: ids };
    let resp = post(&format!("{}/receipt", BASE))
        .json(&body)?
        .send()
        .await?
        .json::<Resp>()
        .await?;

    Ok(resp.promoted)
}
