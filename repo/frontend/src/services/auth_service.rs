use crate::{
    services::api::{api_get, api_post},
    types::auth::{LoginRequest, LoginResponse, ReauthRequest, SessionInfo},
};

pub async fn login(username: &str, password: &str) -> Result<LoginResponse, String> {
    api_post("/auth/login", &LoginRequest {
        username: username.to_string(),
        password: password.to_string(),
    })
    .await
}

pub async fn get_session() -> Result<SessionInfo, String> {
    api_get("/auth/session").await
}

pub async fn logout() -> Result<serde_json::Value, String> {
    api_post("/auth/logout", &serde_json::json!({})).await
}

pub async fn reauth(password: &str) -> Result<serde_json::Value, String> {
    api_post("/auth/reauth", &ReauthRequest {
        password: password.to_string(),
    })
    .await
}
