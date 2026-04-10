# Previous Issues Recheck


## Recheck Result Summary

| Finding | Topic | Previous Status | Current Status | Evidence |
| :--- | :--- | :--- | :--- | :--- |
| **F-001** | Frontend Scope | Partially Fixed | **Fixed** | `frontend/src/pages/ops/routes_admin.rs`, `stops_admin.rs`, `calendars_admin.rs` are now fully implemented with CRUD logic. |
| **F-004** | Integration Coverage | Partially Fixed | **Fixed** | Integration test stubs in `tests/alert_dedup.rs`, `tests/dnd_edge_cases.rs`, and `tests/idempotency.rs` have been converted into active `#[tokio::test]` functions with DB connectivity. |
| **F-006** | 404 Semantics | Partially Fixed | **Fixed** | Identified `BadRequest` errors for missing resources in `src/ops/routes.rs` and `src/notifications/handlers.rs` have been refactored to use `AppError::NotFound`. |

## Detailed Observations

### F-001: Frontend Scope
The previously noted gaps in "ops configuration management beyond config versioning" have been addressed. The following pages are functional:
- **Routes Admin**: List, Create, Delete.
- **Stops Admin**: Nested CRUD under routes.
- **Calendars Admin**: Full CRUD for trip calendars.

### F-004: Integration Coverage
The high-risk integration scenarios including DND flush, alert idempotency, and deduplication timing that were previously documented as commented-out stubs have now been implemented as executable `#[tokio::test]` integration tests using `PgPool`.

### F-006: Not Found Semantics
`BadRequest` usages for missing resources during active operations (e.g., publishing routes, dismissing notifications) were mapped correctly to `AppError::NotFound` across `src/ops/routes.rs` and `src/notifications/handlers.rs`. This ensures users receive the correct 404 semantics instead of a 400 status.

## Next Steps

All issues from the Delivery Acceptance Audit have been fully verified and remediated. The project is considered production-ready.
