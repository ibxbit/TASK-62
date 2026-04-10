use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A config template (e.g. "routes", "stops", "trips", "calendars").
#[derive(Clone, PartialEq, Deserialize, Debug)]
pub struct ConfigTemplate {
    pub id:  Uuid,
    pub key: String,
}

/// A config version (draft / published / scheduled / archived).
#[derive(Clone, PartialEq, Deserialize, Debug)]
pub struct ConfigVersion {
    pub id:             Uuid,
    pub template_id:    Uuid,
    pub template_key:   String,
    pub version_number: i32,
    pub status:         String,
    pub payload:        serde_json::Value,
    pub effective_from: Option<DateTime<Utc>>,
    pub effective_to:   Option<DateTime<Utc>>,
    pub published_at:   Option<DateTime<Utc>>,
    pub scheduled_at:   Option<DateTime<Utc>>,
    pub created_at:     DateTime<Utc>,
    pub updated_at:     DateTime<Utc>,
}

impl ConfigVersion {
    pub fn status_label(&self) -> &str {
        match self.status.as_str() {
            "draft"     => "Draft",
            "published" => "Published",
            "scheduled" => "Scheduled",
            "archived"  => "Archived",
            other       => other,
        }
    }
    pub fn is_draft(&self)     -> bool { self.status == "draft" }
    pub fn is_published(&self) -> bool { self.status == "published" }
    pub fn is_scheduled(&self) -> bool { self.status == "scheduled" }
}

/// Diff between two config versions.
#[derive(Clone, PartialEq, Deserialize, Debug, Default)]
pub struct VersionDiff {
    pub old_version: String,
    pub new_version: String,
    pub added:       Vec<String>,
    pub removed:     Vec<String>,
    pub changed:     Vec<DiffEntry>,
    pub unchanged:   Vec<String>,
}

#[derive(Clone, PartialEq, Deserialize, Debug)]
pub struct DiffEntry {
    pub key:       String,
    pub old_value: serde_json::Value,
    pub new_value: serde_json::Value,
}

/// Rollout plan (multi-stage depot deployment).
#[derive(Clone, PartialEq, Deserialize, Debug)]
pub struct RolloutPlan {
    pub id:                Uuid,
    pub config_version_id: Uuid,
    pub status:            String,
    pub total_depots:      i32,
    pub current_stage:     i32,
    pub stages:            Vec<RolloutStage>,
    pub created_at:        DateTime<Utc>,
}

#[derive(Clone, PartialEq, Deserialize, Debug)]
pub struct RolloutStage {
    pub id:                Uuid,
    pub stage_number:      i16,
    pub target_percentage: i16,
    pub depot_count:       usize,
    pub status:            String,
    pub scheduled_at:      Option<DateTime<Utc>>,
    pub activated_at:      Option<DateTime<Utc>>,
}

/// Request to create a new config version.
#[derive(Serialize, Default)]
pub struct CreateVersionRequest {
    pub payload:          serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub based_on_version: Option<Uuid>,
}

/// Request to schedule a config version for future publication.
#[derive(Serialize)]
pub struct ScheduleVersionRequest {
    pub effective_from: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_to:   Option<DateTime<Utc>>,
}

/// One stage spec for creating a rollout plan.
#[derive(Serialize)]
pub struct RolloutStageSpec {
    pub target_percentage: i16,
    pub depot_ids:         Vec<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_at:      Option<DateTime<Utc>>,
}

/// Request to create a rollout plan.
#[derive(Serialize)]
pub struct CreateRolloutRequest {
    pub stages: Vec<RolloutStageSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes:  Option<String>,
}

/// A transit route (legacy type — kept for backward compat with dispatcher views).
#[derive(Clone, PartialEq, Deserialize, Debug)]
pub struct Route {
    pub id:          Uuid,
    pub route_code:  String,
    pub name:        String,
    pub description: Option<String>,
    pub is_active:   bool,
    pub created_at:  DateTime<Utc>,
}

// ── Admin-level ops types (matching backend RouteResponse / StopResponse) ─────

/// Paginated list wrapper returned by /ops/routes.
#[derive(Clone, PartialEq, Deserialize, Debug, Default)]
pub struct OpsListPage<T: Clone + PartialEq> {
    pub data:     Vec<T>,
    pub total:    i64,
    pub page:     i64,
    pub per_page: i64,
}

/// Route entity as returned by the backend (admin CRUD).
#[derive(Clone, PartialEq, Deserialize, Debug)]
pub struct OpsRoute {
    pub id:             Uuid,
    pub code:           String,
    pub name:           String,
    pub description:    Option<String>,
    pub status:         String,
    pub effective_from: Option<DateTime<Utc>>,
    pub version:        i32,
    pub created_at:     DateTime<Utc>,
    pub updated_at:     DateTime<Utc>,
}

/// Stop entity as returned by the backend.
#[derive(Clone, PartialEq, Deserialize, Debug)]
pub struct OpsStop {
    pub id:             Uuid,
    pub route_id:       Uuid,
    pub code:           String,
    pub name:           String,
    pub sequence_order: i16,
    pub latitude:       Option<f64>,
    pub longitude:      Option<f64>,
    pub created_at:     DateTime<Utc>,
    pub updated_at:     DateTime<Utc>,
}

/// Calendar entity as returned by the backend.
#[derive(Clone, PartialEq, Deserialize, Debug)]
pub struct OpsCalendar {
    pub id:              Uuid,
    pub name:            String,
    pub description:     Option<String>,
    pub days_of_week:    Vec<i16>,
    pub valid_from:      chrono::NaiveDate,
    pub valid_to:        Option<chrono::NaiveDate>,
    pub exception_dates: serde_json::Value,
    pub created_at:      DateTime<Utc>,
    pub updated_at:      DateTime<Utc>,
}

// ── Request bodies ────────────────────────────────────────────────────────────

#[derive(Serialize, Default)]
pub struct CreateOpsRouteRequest {
    pub code:           String,
    pub name:           String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description:    Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_from: Option<DateTime<Utc>>,
}

#[derive(Serialize, Default)]
pub struct UpdateOpsRouteRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name:        Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Serialize)]
pub struct CreateOpsStopRequest {
    pub code:           String,
    pub name:           String,
    pub sequence_order: i16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latitude:       Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longitude:      Option<f64>,
}

#[derive(Serialize, Default)]
pub struct UpdateOpsStopRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name:           Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_order: Option<i16>,
}

#[derive(Serialize)]
pub struct CreateOpsCalendarRequest {
    pub name:         String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description:  Option<String>,
    pub days_of_week: Vec<i16>,
    pub valid_from:   String,   // "YYYY-MM-DD"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_to:     Option<String>,
}

#[derive(Serialize, Default)]
pub struct UpdateOpsCalendarRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name:         Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description:  Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days_of_week: Option<Vec<i16>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_from:   Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_to:     Option<String>,
}

/// A change policy — rules governing ticket/booking modifications.
#[derive(Clone, PartialEq, Deserialize, Debug)]
pub struct ChangePolicy {
    pub id:                  Uuid,
    pub name:                String,
    pub description:         Option<String>,
    pub change_fee:          f64,
    pub change_window_hours: i32,
    pub conditions:          serde_json::Value,
    pub is_active:           bool,
    pub created_at:          DateTime<Utc>,
    pub updated_at:          DateTime<Utc>,
}

/// A refund policy — rules governing refund eligibility and amounts.
#[derive(Clone, PartialEq, Deserialize, Debug)]
pub struct RefundPolicy {
    pub id:                   Uuid,
    pub name:                 String,
    pub description:          Option<String>,
    pub refund_percentage:    f64,
    pub refund_window_hours:  i32,
    pub no_show_fee:          f64,
    pub conditions:           serde_json::Value,
    pub is_active:            bool,
    pub created_at:           DateTime<Utc>,
    pub updated_at:           DateTime<Utc>,
}

/// Request to create a change policy.
#[derive(Serialize, Default)]
pub struct CreateChangePolicyRequest {
    pub name:                String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description:         Option<String>,
    pub change_fee:          f64,
    pub change_window_hours: i32,
}

/// Request to create a refund policy.
#[derive(Serialize, Default)]
pub struct CreateRefundPolicyRequest {
    pub name:                 String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description:          Option<String>,
    pub refund_percentage:    f64,
    pub refund_window_hours:  i32,
    pub no_show_fee:          f64,
}

/// A fare rule (ops-domain pricing configuration per route or network-wide).
#[derive(Clone, PartialEq, Deserialize, Debug)]
pub struct FareRule {
    pub id:         Uuid,
    pub route_id:   Option<Uuid>,
    pub rule_type:  String,
    pub base_fare:  f64,
    pub conditions: serde_json::Value,
    pub is_active:  bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request to create a fare rule.
#[derive(Serialize, Default)]
pub struct CreateFareRuleRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_id:  Option<Uuid>,
    pub rule_type: String,
    pub base_fare: f64,
}

/// A transit trip.
#[derive(Clone, PartialEq, Deserialize, Debug)]
pub struct Trip {
    pub id:             Uuid,
    pub route_id:       Uuid,
    pub scheduled_at:   DateTime<Utc>,
    pub status:         String,
    pub driver_id:      Option<Uuid>,
    pub vehicle_id:     Option<Uuid>,
    pub notes:          Option<String>,
}

impl Trip {
    pub fn status_label(&self) -> &str {
        match self.status.as_str() {
            "scheduled"  => "Scheduled",
            "in_progress" => "In Progress",
            "completed"  => "Completed",
            "cancelled"  => "Cancelled",
            other        => other,
        }
    }
}

/// A conflict detected between trips.
#[derive(Clone, PartialEq, Deserialize, Debug)]
pub struct TripConflict {
    pub id:           Uuid,
    pub trip_id:      Uuid,
    pub conflict_type: String,
    pub description:  String,
    pub detected_at:  DateTime<Utc>,
    pub resolved_at:  Option<DateTime<Utc>>,
    pub is_resolved:  bool,
}
