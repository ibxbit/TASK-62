/// Generic API client helpers.
///
/// All API calls go through `api_get`, `api_post`, `api_put`, `api_delete`
/// which automatically attach the Bearer token from localStorage.
///
/// Offline-first: if the fetch itself fails (network unavailable), the
/// functions return an `Err` with a descriptive message so components can
/// show an offline state rather than crashing.
use gloo_net::http::{RequestBuilder, Response};
use serde::{de::DeserializeOwned, Serialize};

use crate::store::auth_store::load_persisted_token;

// All backend calls go through `/api/...`.  nginx rewrites this to the API
// container, which prevents the ambiguity between SPA routes (/notifications,
// /alerts, /ops/…) and API endpoints with the same path.
const API_BASE: &str = "/api";

/// Build a request with the Bearer token header if a token is available.
fn bearer_header() -> Option<String> {
    load_persisted_token().map(|t| format!("Bearer {}", t))
}

fn apply_auth(req: RequestBuilder) -> RequestBuilder {
    match bearer_header() {
        Some(h) => req.header("Authorization", &h),
        None    => req,
    }
}

pub async fn api_get<T: DeserializeOwned>(path: &str) -> Result<T, String> {
    let req = apply_auth(gloo_net::http::Request::get(&format!("{}{}", API_BASE, path)))
        .build()
        .map_err(|e| format!("Build error: {}", e))?;
    let resp = req.send().await.map_err(|e| format!("Network error: {}", e))?;
    parse_response(resp).await
}

pub async fn api_post<B: Serialize, T: DeserializeOwned>(
    path: &str,
    body: &B,
) -> Result<T, String> {
    let req = apply_auth(
        gloo_net::http::Request::post(&format!("{}{}", API_BASE, path))
            .header("Content-Type", "application/json"),
    );
    let req = req.json(body).map_err(|e| format!("Serialisation error: {}", e))?;
    let resp = req.send().await.map_err(|e| format!("Network error: {}", e))?;
    parse_response(resp).await
}

pub async fn api_put<B: Serialize, T: DeserializeOwned>(
    path: &str,
    body: &B,
) -> Result<T, String> {
    let req = apply_auth(
        gloo_net::http::Request::put(&format!("{}{}", API_BASE, path))
            .header("Content-Type", "application/json"),
    );
    let req = req.json(body).map_err(|e| format!("Serialisation error: {}", e))?;
    let resp = req.send().await.map_err(|e| format!("Network error: {}", e))?;
    parse_response(resp).await
}

pub async fn api_delete(path: &str) -> Result<(), String> {
    let req = apply_auth(gloo_net::http::Request::delete(&format!("{}{}", API_BASE, path)))
        .build()
        .map_err(|e| format!("Build error: {}", e))?;
    let resp = req.send().await.map_err(|e| format!("Network error: {}", e))?;
    if resp.ok() {
        Ok(())
    } else {
        Err(api_error_text(resp).await)
    }
}

pub async fn api_post_empty<T: DeserializeOwned>(path: &str) -> Result<T, String> {
    let req = apply_auth(
        gloo_net::http::Request::post(&format!("{}{}", API_BASE, path))
            .header("Content-Type", "application/json"),
    );
    let req = req.body("{}").map_err(|e| format!("Body error: {}", e))?;
    let resp = req.send().await.map_err(|e| format!("Network error: {}", e))?;
    parse_response(resp).await
}

async fn parse_response<T: DeserializeOwned>(resp: Response) -> Result<T, String> {
    if resp.ok() {
        resp.json::<T>().await.map_err(|e| format!("Parse error: {}", e))
    } else {
        Err(api_error_text(resp).await)
    }
}

async fn api_error_text(resp: Response) -> String {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    // Try to extract the "error" field from a standard JSON error body
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
        if let Some(msg) = json.get("error").and_then(|v| v.as_str()) {
            return format!("[{}] {}", status, msg);
        }
    }
    format!("[{}] {}", status, body)
}
