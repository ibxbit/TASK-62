"""
DND (Do-Not-Disturb) window logic — pure unit tests.

Mirrors the Rust `is_in_dnd_window` pure function from
`src/notifications/bus.rs` in Python so tests can run without any
server or database.

Rules:
  - dnd_enabled = False                  → always False
  - dnd_enabled = True, no time window   → always True  (all-day DND)
  - dnd_enabled = True, start ≤ end      → True when now ∈ [start, end]
  - dnd_enabled = True, start > end      → True when now ≥ start OR now ≤ end
                                           (window crosses midnight)
"""

from datetime import time


def is_in_dnd_window(
    dnd_enabled: bool,
    dnd_start: time | None,
    dnd_end: time | None,
    now: time,
) -> bool:
    """Pure Python replica of the Rust `is_in_dnd_window` function."""
    if not dnd_enabled:
        return False
    if dnd_start is not None and dnd_end is not None:
        if dnd_start <= dnd_end:
            return dnd_start <= now <= dnd_end
        else:
            return now >= dnd_start or now <= dnd_end
    # One or both times missing → all-day DND
    return True


# ── DND disabled ──────────────────────────────────────────────────────────────

class TestDndDisabled:
    def test_disabled_all_none(self):
        assert not is_in_dnd_window(False, None, None, time(12, 0))

    def test_disabled_with_window_configured(self):
        assert not is_in_dnd_window(False, time(8, 0), time(18, 0), time(12, 0))

    def test_disabled_midnight_crossing_window(self):
        assert not is_in_dnd_window(False, time(22, 0), time(6, 0), time(23, 0))

    def test_disabled_all_times(self):
        """DND disabled → false at every hour of the day."""
        for h in range(24):
            assert not is_in_dnd_window(False, time(22, 0), time(6, 0), time(h, 0))


# ── All-day DND ───────────────────────────────────────────────────────────────

class TestAllDayDnd:
    def test_all_day_at_midnight(self):
        assert is_in_dnd_window(True, None, None, time(0, 0))

    def test_all_day_at_noon(self):
        assert is_in_dnd_window(True, None, None, time(12, 0))

    def test_all_day_at_end_of_day(self):
        assert is_in_dnd_window(True, None, None, time(23, 59))

    def test_only_start_set_is_all_day(self):
        assert is_in_dnd_window(True, time(22, 0), None, time(10, 0))

    def test_only_end_set_is_all_day(self):
        assert is_in_dnd_window(True, None, time(6, 0), time(10, 0))


# ── Normal window (start ≤ end, same day) ─────────────────────────────────────

class TestNormalWindow:
    """DND 08:00–18:00."""

    START = time(8, 0)
    END = time(18, 0)

    def test_midpoint_inside(self):
        assert is_in_dnd_window(True, self.START, self.END, time(12, 0))

    def test_exact_start_inside(self):
        assert is_in_dnd_window(True, self.START, self.END, self.START)

    def test_exact_end_inside(self):
        assert is_in_dnd_window(True, self.START, self.END, self.END)

    def test_one_minute_before_start_outside(self):
        assert not is_in_dnd_window(True, self.START, self.END, time(7, 59))

    def test_one_minute_after_end_outside(self):
        assert not is_in_dnd_window(True, self.START, self.END, time(18, 1))

    def test_midnight_outside(self):
        assert not is_in_dnd_window(True, self.START, self.END, time(0, 0))

    def test_late_night_outside(self):
        assert not is_in_dnd_window(True, self.START, self.END, time(22, 0))


# ── Midnight-crossing window (start > end) ────────────────────────────────────

class TestMidnightCrossing:
    """DND 22:00–06:00 (night shift quiet window)."""

    START = time(22, 0)
    END = time(6, 0)

    def test_evening_before_midnight_inside(self):
        assert is_in_dnd_window(True, self.START, self.END, time(23, 0))

    def test_midnight_itself_inside(self):
        assert is_in_dnd_window(True, self.START, self.END, time(0, 0))

    def test_early_morning_inside(self):
        assert is_in_dnd_window(True, self.START, self.END, time(5, 0))

    def test_exact_start_boundary_inside(self):
        assert is_in_dnd_window(True, self.START, self.END, self.START)

    def test_exact_end_boundary_inside(self):
        assert is_in_dnd_window(True, self.START, self.END, self.END)

    def test_just_after_end_outside(self):
        assert not is_in_dnd_window(True, self.START, self.END, time(6, 1))

    def test_just_before_start_outside(self):
        assert not is_in_dnd_window(True, self.START, self.END, time(21, 59))

    def test_midday_outside(self):
        assert not is_in_dnd_window(True, self.START, self.END, time(12, 0))


# ── Degenerate window (start == end) ─────────────────────────────────────────

class TestDegenerateWindow:
    def test_single_point_at_exact_time_inside(self):
        t = time(12, 0)
        assert is_in_dnd_window(True, t, t, t)

    def test_single_point_at_other_time_outside(self):
        t = time(12, 0)
        assert not is_in_dnd_window(True, t, t, time(12, 1))


# ── Critical severity bypass (logic documentation) ───────────────────────────

class TestCriticalBypass:
    """
    In the bus, critical events bypass DND:
        if severity != 'critical' and check_dnd(pool, user_id):
            queue   # non-critical during DND
        else:
            deliver # critical OR outside DND

    is_in_dnd_window itself doesn't know about severity — this test
    documents the caller's responsibility.
    """

    def test_dnd_active_does_not_itself_handle_severity(self):
        """is_in_dnd_window returns True during DND regardless of severity."""
        dnd_active = is_in_dnd_window(True, None, None, time(12, 0))
        assert dnd_active  # True

        # The bus logic: critical bypasses DND
        severity = "critical"
        should_deliver = severity == "critical" or not dnd_active
        assert should_deliver  # critical always delivers

    def test_non_critical_queued_during_dnd(self):
        dnd_active = is_in_dnd_window(True, None, None, time(12, 0))
        severity = "info"
        should_queue = severity != "critical" and dnd_active
        assert should_queue
