# TransitOps Backoffice Platform — Business Gap Questions
**Project:** TASK-62  **Date:** 2026-04-10

---

## Q-001: How should the system handle scheduled config versions when the effective time passes but the server is offline?

**Gap:** The Prompt states configs can be scheduled to publish at a future time (e.g., 12:01 AM on 04/15/2026). If the server or scheduler restarts after the effective_from timestamp, the version may be stuck in `scheduled` status until the next 60-second `SystemMaintenanceJob` tick.

**Hypothesis:** The scheduler uses `effective_from <= now()` with `FOR UPDATE SKIP LOCKED`, so on restart it will auto-publish all overdue scheduled versions within one job cycle (≤ 60 seconds). This is acceptable for an offline intranet system where sub-minute precision is not a hard requirement.

**Solution:** Implemented. `system_maintenance.rs:45-73` queries `WHERE status = 'scheduled' AND effective_from <= now()` and publishes all overdue versions on each tick. On restart, the first tick catches all missed schedules. A `recover_stale_runs()` call in `main.rs:80-82` also clears any `running` job-run records left from a crash.

---

## Q-002: How should the system handle duplicate notification delivery when the event bus restarts mid-fan-out?

**Gap:** The Prompt requires that every event is persisted and fan-out receipts are recorded. If the notification bus crashes between inserting a delivery row and marking the event as `processed_at`, the bus will re-process the same event on restart.

**Hypothesis:** Use `ON CONFLICT (event_id, user_id) DO NOTHING` on the `notifications.deliveries` INSERT so repeated fan-out attempts for the same (event, user) pair are silently skipped.

**Solution:** Implemented. `src/notifications/bus.rs:247-254` and `257-263` both use `ON CONFLICT (event_id, user_id) DO NOTHING`. Events without a `processed_at` are retried on the next bus tick, but duplicate delivery rows cannot be created. The UNIQUE constraint acts as a safety net independent of the application-layer dedup window.

---

## Q-003: How should the 15-minute notification dedup window interact with DND-queued deliveries?

**Gap:** If a delivery is queued (DND active), it counts as `status != 'dismissed'` and would therefore suppress a second delivery arriving within 15 minutes. But the user has not actually seen the notification yet — is suppression correct in this case?

**Hypothesis:** Yes. The dedup window is intended as a spam filter, not a read-confirmation gate. If a delivery is queued, the user will receive it when DND ends. A second identical event within 15 minutes adds no value and should be suppressed.

**Solution:** Implemented. `src/notifications/bus.rs:430-452` checks `d.status != 'dismissed'`, which means `queued` deliveries are counted in the dedup window. This is the correct semantic: dismissed deliveries (user actively dismissed without reading) reset the window, while queued deliveries do not.

---

## Q-004: How are refunds handled for transactions that have already been partially refunded?

**Gap:** The Prompt says refunds must have a full audit trail, but does not specify whether multiple partial refunds are allowed on a single transaction, or whether the system prevents over-refunding.

**Hypothesis:** Allow multiple partial refunds but validate that the sum of approved refunds does not exceed the original transaction amount. Mark the parent transaction as `partially_refunded` until the total refunded equals the original amount, then mark as `refunded`.

**Solution:** Implemented. `src/payments/handlers.rs:777-810` uses `CASE WHEN refund_amount >= transaction_amount THEN 'refunded' ELSE 'partially_refunded' END`. The `create_refund` handler validates that the transaction is in `completed` or `partially_refunded` status before allowing a new refund request. Over-refund prevention (sum check) is a gap — see Q-004a below.

**Open gap (Q-004a):** There is no explicit validation that `sum(approved_refunds) + new_refund.amount <= transaction.amount`. A finance analyst could theoretically request multiple refunds that in total exceed the original charge. **Recommendation:** Add a pre-insert check in `create_refund` that queries `SUM(amount) FROM refunds WHERE transaction_id = $1 AND status NOT IN ('rejected', 'cancelled')` and rejects if total would exceed the transaction amount.

---

## Q-005: What happens to alert rules when the subscribing user's account is deactivated?

**Gap:** The Prompt does not specify lifecycle rules for subscription data when a user is deleted or deactivated. Alert rules are scoped by `user_id`. If the user is deactivated (`is_active = FALSE`) but the rule remains, the scheduler's KPI anomaly job could still attempt to fan-out alerts to that user.

**Hypothesis:** The fan-out query should filter to only `auth.users WHERE is_active = TRUE AND deleted_at IS NULL`. Deactivated users receive no new deliveries.

**Solution:** Implemented. `src/notifications/bus.rs:394-419` — the `get_recipients` function fetches users with `u.is_active = TRUE AND u.deleted_at IS NULL`. Alert rules for deactivated users are effectively dormant — no delivery rows are created, but the rules themselves are not purged. This is acceptable; if the account is reactivated the rules resume.

---

## Q-006: How should exportable reports be watermarked when the export is triggered by the scheduler (not a human user)?

**Gap:** The Prompt states exports must carry a watermark with "viewer name and timestamp." For scheduled automated runs triggered by `SystemMaintenanceJob`, there is no interactive user.

**Hypothesis:** Scheduled runs should record the schedule creator (or a system identity such as `"scheduler"`) as the effective viewer. The watermark should show `"Scheduled Export — <schedule_name>"` plus the generation timestamp.

**Solution:** Implemented. `src/reporting/scheduler.rs` stores `triggered_by = 'scheduler'` for auto-triggered runs. The export handler reads `run.triggered_by` and includes it in the watermark payload. Manual exports use `session.username`.

---

## Q-007: How should reconciliation discrepancies be routed to the alert/notification pipeline?

**Gap:** The Prompt says discrepancies should "raise anomaly alerts routed through the same subscription/acknowledgment pipeline." The reconciliation module needs to emit events that the notification bus can pick up.

**Hypothesis:** After a reconciliation run completes, insert a notification event of type `reconciliation.anomaly.detected` for each discrepancy batch (not one per row). The event payload includes the run ID and counts. Subscribers to that event type receive inbox notifications and the alert is visible in the alert dashboard.

**Solution:** Implemented. `src/reconciliation/handlers.rs` calls `alerting::detector::create_alert(...)` after a run with discrepancies, using `alert_type = "reconciliation_anomaly"` and attaching the run ID as `source_entity_id`. The alert creation function (`src/alerting/detector.rs`) inserts a `notifications.events` row of type `alerts.anomaly.reconciliation`, which the bus picks up within 5 seconds.

---

## Q-008: How should the gradual rollout percentage map to depot selection?

**Gap:** The Prompt says "gradual rollout by depot (e.g., 10%/50%/100% over 7 days)." It is unclear whether percentage refers to the fraction of total depots or is purely a human-readable label for each stage.

**Hypothesis:** The `target_percentage` field is a human-readable label (metadata) rather than an auto-selector. The operator explicitly lists which `depot_ids` are included in each stage. The percentage is stored for UI display and reporting, not used to auto-select depots.

**Solution:** Implemented. `src/payments/handlers.rs:141` and `system_maintenance.rs:119` confirm that `depot_ids` is an explicit array per stage. The `target_percentage` is stored in `ops.rollout_stages` as display metadata. The frontend rollout page shows per-stage depot selection alongside the percentage label.

---

## Q-009: How should the system handle the 5-minute anti-replay window when clocks are slightly skewed between the gateway and the backend?

**Gap:** The Prompt requires rejecting callbacks older than 5 minutes (300 seconds). If the gateway server clock is slightly ahead of the backend, a legitimate callback could be falsely rejected.

**Hypothesis:** Allow a ±30-second grace window (i.e., reject if `|now - timestamp| > 330 seconds`). This is common practice in OAuth and webhook security standards (e.g., Stripe uses ±300 seconds with a tolerance implied by network latency).

**Solution:** Implemented with strict 300-second window (`src/payments/signature.rs`). The current implementation does not include a grace period. **Recommendation:** Consider widening the window to 330 seconds to account for up to 30 seconds of clock skew between on-prem systems, while still meeting the "5-minute" Prompt requirement in spirit.

---

## Q-010: Should staff users be able to see alert rules created by other users?

**Gap:** The Prompt says staff users can "subscribe to operational events and alerts." Alert rules are per-user (scoped by `user_id`). It is ambiguous whether a staff user should be able to browse rules created by admins or see only their own.

**Hypothesis:** Rules are private per user. Listing `GET /notifications/rules` returns only the authenticated user's own rules. An admin's rules are not visible to staff or dispatchers. Cross-user rule access should return 403 or 404.

**Solution:** Implemented. `src/notifications/handlers.rs` filters all rule queries by `user_id = session.user_id`. The object-level ownership test in `API_tests/test_security.py:148-189` confirms that cross-user read attempts return 403 or 404.
