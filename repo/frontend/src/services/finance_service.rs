use uuid::Uuid;

use crate::{
    services::api::{api_get, api_post, api_post_empty},
    types::finance::{
        ReconciliationRun, Refund, RunSummary, StartRunRequest,
        StatementImport, Transaction, UploadStatementRequest, UploadStatementResponse,
    },
};

// ── Transactions ──────────────────────────────────────────────────────────────

pub async fn list_transactions() -> Result<Vec<Transaction>, String> {
    api_get("/payments/transactions").await
}

// ── Statement imports ─────────────────────────────────────────────────────────

pub async fn list_statements() -> Result<Vec<StatementImport>, String> {
    api_get("/reconciliation/statements").await
}

pub async fn upload_statement(
    body: &UploadStatementRequest,
) -> Result<UploadStatementResponse, String> {
    api_post("/reconciliation/statements", body).await
}

// ── Reconciliation runs ───────────────────────────────────────────────────────

pub async fn list_runs() -> Result<Vec<ReconciliationRun>, String> {
    api_get("/reconciliation/runs").await
}

pub async fn start_run(body: &StartRunRequest) -> Result<serde_json::Value, String> {
    api_post("/reconciliation/runs", body).await
}

pub async fn get_run_summary(run_id: Uuid) -> Result<RunSummary, String> {
    api_get(&format!("/reconciliation/runs/{}/summary", run_id)).await
}

// ── Refunds ───────────────────────────────────────────────────────────────────

pub async fn list_refunds() -> Result<Vec<Refund>, String> {
    api_get("/payments/refunds").await
}

pub async fn approve_refund(refund_id: Uuid) -> Result<serde_json::Value, String> {
    api_post_empty(&format!("/payments/refunds/{}/approve", refund_id)).await
}

pub async fn process_refund(refund_id: Uuid) -> Result<serde_json::Value, String> {
    api_post_empty(&format!("/payments/refunds/{}/process", refund_id)).await
}
