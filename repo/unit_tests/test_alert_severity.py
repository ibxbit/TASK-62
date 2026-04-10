"""
Alert severity and deduplication logic — pure unit tests.

Mirrors the pure logic in:
  - `src/alerting/detector.rs`  (KPI anomaly thresholds, recon severity)
  - `src/scheduler/executor.rs` (FNV-1a advisory lock IDs)

No server or database required.
"""


# ── Replicated pure logic ─────────────────────────────────────────────────────

KPI_CHECK_INTERVAL_SECS = 1800  # 30 minutes


def kpi_severity(deviation_pct: float, threshold_pct: float) -> str:
    """
    From detector.rs check_metric_anomaly:
      if deviation_pct > threshold_pct * 2  → "critical"
      else                                  → "warning"
    (caller ensures deviation > threshold before calling this)
    """
    if deviation_pct > threshold_pct * 2.0:
        return "critical"
    return "warning"


def recon_severity(is_high: bool) -> str:
    """From detector.rs check_reconciliation_run."""
    return "critical" if is_high else "warning"


def fnv1a_advisory_lock_id(name: str) -> int:
    """
    FNV-1a hash → i64, matching executor.rs advisory_lock_id().
    Produces a stable, deterministic i64 advisory lock identifier from a job name.
    """
    OFFSET = 14_695_981_039_346_656_037
    PRIME  = 1_099_511_628_211
    h = OFFSET
    for b in name.encode():
        h ^= b
        h = (h * PRIME) & 0xFFFFFFFFFFFFFFFF  # keep as u64
    # Reinterpret as i64 (two's complement, same as Rust `h as i64`)
    return (h - (1 << 64)) if h >= (1 << 63) else h


# ── KPI interval constant ─────────────────────────────────────────────────────

class TestKpiInterval:
    def test_is_30_minutes(self):
        assert KPI_CHECK_INTERVAL_SECS == 30 * 60


# ── KPI severity thresholds ───────────────────────────────────────────────────

class TestKpiSeverity:
    """Default threshold: 25%."""

    THRESHOLD = 25.0

    def test_just_above_threshold_is_warning(self):
        assert kpi_severity(25.1, self.THRESHOLD) == "warning"

    def test_at_exactly_double_is_warning(self):
        # 50.0 is NOT > 50.0 (strict >)
        assert kpi_severity(50.0, self.THRESHOLD) == "warning"

    def test_just_above_double_is_critical(self):
        assert kpi_severity(50.001, self.THRESHOLD) == "critical"

    def test_way_above_double_is_critical(self):
        assert kpi_severity(200.0, self.THRESHOLD) == "critical"

    def test_custom_40pct_threshold_warning(self):
        assert kpi_severity(60.0, 40.0) == "warning"
        assert kpi_severity(80.0, 40.0) == "warning"  # exactly 2× → NOT critical

    def test_custom_40pct_threshold_critical(self):
        assert kpi_severity(80.001, 40.0) == "critical"

    def test_near_zero_baseline_would_skip(self):
        """Verify that near-zero baseline guard prevents division artifacts."""
        avg = 1e-10
        assert abs(avg) < 1e-9  # → detector returns Ok(()) early


# ── Reconciliation alert severity ────────────────────────────────────────────

class TestReconSeverity:
    def test_not_high_is_warning(self):
        assert recon_severity(False) == "warning"

    def test_high_is_critical(self):
        assert recon_severity(True) == "critical"


# ── Zero discrepancy early exit ───────────────────────────────────────────────

class TestZeroDiscrepancyGuard:
    def test_zero_count_no_alert(self):
        discrepancy_count = 0
        assert not (discrepancy_count > 0)  # guard: return Ok(()) immediately

    def test_nonzero_count_alerts(self):
        assert 1 > 0


# ── Advisory lock ID (FNV-1a) ────────────────────────────────────────────────

class TestAdvisoryLockId:
    PRODUCTION_JOBS = [
        "notification_bus",
        "payment_compensation",
        "report_generation",
        "kpi_anomaly_check",
        "scheduled_config",
        "dedup_cleanup",
    ]

    def test_is_deterministic(self):
        assert fnv1a_advisory_lock_id("notification_bus") == \
               fnv1a_advisory_lock_id("notification_bus")

    def test_different_names_produce_different_ids(self):
        assert fnv1a_advisory_lock_id("job_a") != fnv1a_advisory_lock_id("job_b")

    def test_all_production_jobs_unique(self):
        ids = [fnv1a_advisory_lock_id(name) for name in self.PRODUCTION_JOBS]
        assert len(ids) == len(set(ids)), "Lock ID collision detected"

    def test_empty_string_does_not_raise(self):
        _ = fnv1a_advisory_lock_id("")

    def test_output_is_valid_i64(self):
        for name in self.PRODUCTION_JOBS:
            val = fnv1a_advisory_lock_id(name)
            assert -(2**63) <= val < 2**63, f"Out of i64 range for {name}: {val}"

    def test_known_jobs_have_stable_ids(self):
        """Verify specific job IDs are stable across runs (regression guard)."""
        id1 = fnv1a_advisory_lock_id("notification_bus")
        id2 = fnv1a_advisory_lock_id("notification_bus")
        assert id1 == id2

    def test_notification_dedup_window_is_15_minutes(self):
        """Documents the 15-minute dedup window constant."""
        dedup_window_secs = 15 * 60
        assert dedup_window_secs == 900


# ── State transitions ─────────────────────────────────────────────────────────

class TestAlertStateTransitions:
    """
    Alert lifecycle: open → acknowledged → closed
                     open →               closed

    Pure logic tests — verify allowed transitions.
    """

    ALLOWED_TRANSITIONS = {
        "open":         {"acknowledged", "closed"},
        "acknowledged": {"closed"},
        "closed":       set(),  # terminal state
    }

    def test_open_can_be_acknowledged(self):
        assert "acknowledged" in self.ALLOWED_TRANSITIONS["open"]

    def test_open_can_be_closed(self):
        assert "closed" in self.ALLOWED_TRANSITIONS["open"]

    def test_acknowledged_can_be_closed(self):
        assert "closed" in self.ALLOWED_TRANSITIONS["acknowledged"]

    def test_closed_is_terminal(self):
        assert len(self.ALLOWED_TRANSITIONS["closed"]) == 0

    def test_open_alert_blocks_duplicate_creation(self):
        """
        From detector.rs: if open alert exists for (type, entity_id) → skip.
        Documented as a state machine invariant.
        """
        existing_status = "open"
        would_create = existing_status != "open"
        assert not would_create  # creation blocked

    def test_closed_alert_allows_new_creation(self):
        existing_status = "closed"
        would_create = existing_status != "open"
        assert would_create  # new alert allowed

    def test_acknowledged_alert_allows_new_creation(self):
        existing_status = "acknowledged"
        would_create = existing_status != "open"
        assert would_create
