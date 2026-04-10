use uuid::Uuid;

use crate::{
    services::api::{api_delete, api_get, api_post, api_post_empty},
    types::reporting::{
        CreateMetricRequest, CreateScheduledReportRequest,
        MetricDefinition, MetricValue, ReportRun, ScheduledReport,
    },
};

pub async fn list_metrics() -> Result<Vec<MetricDefinition>, String> {
    api_get("/reporting/metrics").await
}

pub async fn create_metric(body: &CreateMetricRequest) -> Result<MetricDefinition, String> {
    api_post("/reporting/metrics", body).await
}

pub async fn delete_metric(id: Uuid) -> Result<(), String> {
    api_delete(&format!("/reporting/metrics/{}", id)).await
}

pub async fn list_schedules() -> Result<Vec<ScheduledReport>, String> {
    api_get("/reporting/schedules").await
}

pub async fn create_schedule(body: &CreateScheduledReportRequest) -> Result<ScheduledReport, String> {
    api_post("/reporting/schedules", body).await
}

pub async fn delete_schedule(id: Uuid) -> Result<(), String> {
    api_delete(&format!("/reporting/schedules/{}", id)).await
}

pub async fn trigger_run(schedule_id: Uuid) -> Result<serde_json::Value, String> {
    api_post_empty(&format!("/reporting/schedules/{}/trigger", schedule_id)).await
}

pub async fn list_runs() -> Result<Vec<ReportRun>, String> {
    api_get("/reporting/runs").await
}

/// Build an export download URL.
///
/// `viewer` and `exported_at` are included as query params so the backend can
/// embed them as watermark metadata in the generated PDF/CSV.
pub fn export_run_url(run_id: Uuid, format: &str) -> String {
    format!("/reporting/runs/{}/export?format={}", run_id, format)
}

/// Export URL with explicit watermark context (viewer identity + export timestamp).
///
/// The backend uses `viewer` and `exported_at` to stamp the document with who
/// exported it and when, ensuring audit-trail traceability.
pub fn export_run_url_with_watermark(
    run_id:      Uuid,
    format:      &str,
    viewer:      &str,
    exported_at: &str,
) -> String {
    format!(
        "/reporting/runs/{}/export?format={}&viewer={}&exported_at={}",
        run_id, format, viewer, exported_at
    )
}

/// Fetch computed metric values for a given time range.
///
/// Optional `route_id` and `depot_id` narrow the computation to a specific
/// route or depot, enabling drilldown filtering in the KPI metrics page.
pub async fn get_metric_values(
    metric_id: Uuid,
    from:      &str,
    to:        &str,
    route_id:  Option<&str>,
    depot_id:  Option<&str>,
) -> Result<Vec<MetricValue>, String> {
    let mut url = format!(
        "/reporting/metrics/{}/values?from={}&to={}",
        metric_id, from, to
    );
    if let Some(r) = route_id.filter(|s| !s.is_empty()) {
        url.push_str(&format!("&route_id={}", r));
    }
    if let Some(d) = depot_id.filter(|s| !s.is_empty()) {
        url.push_str(&format!("&depot_id={}", d));
    }
    api_get(&url).await
}
