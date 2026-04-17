"""
Notifications endpoint tests.

Endpoints covered
-----------------
  GET  /notifications                  list inbox deliveries
  GET  /notifications/unread-count     unread + queued counters
  POST /notifications/read-all         bulk mark-read
  GET  /notifications/{id}             single delivery
  POST /notifications/{id}/read        mark single read
  POST /notifications/{id}/dismiss     dismiss single
  GET  /notifications/preferences      DND preferences
  PUT  /notifications/preferences      update DND settings
  GET  /notifications/subscriptions    event subscriptions
  PUT  /notifications/subscriptions    update subscriptions
  GET  /notifications/rules            custom notification rules
  POST /notifications/rules            create rule
  GET  /notifications/rules/{id}       get single rule
  PUT  /notifications/rules/{id}       update rule
  DELETE /notifications/rules/{id}     delete rule
  POST /notifications/rules/{id}/toggle toggle rule enabled/disabled
  POST /notifications/announce         system announcement (admin only)
  GET  /notifications/channels         channel preferences
  PUT  /notifications/channels/{ch}    upsert channel pref
  DELETE /notifications/channels/{ch}  delete channel pref
"""

import uuid

import pytest

from conftest import TEST_USERS

NON_EXISTENT_ID = "00000000-0000-0000-0000-000000000000"


# ── Inbox ─────────────────────────────────────────────────────────────────────

class TestNotificationsList:
    def test_list_returns_200(self, api, admin_token):
        r = api("GET", "/notifications", token=admin_token)
        assert r.status_code == 200

    def test_list_returns_array_of_delivery_items(self, api, admin_token):
        """Every item must carry the inbox contract fields."""
        r = api("GET", "/notifications", token=admin_token, params={"status": "all"})
        assert r.status_code == 200
        body = r.json()
        assert isinstance(body, list)
        for item in body:
            for field in ("id", "event_type", "severity", "status", "payload", "created_at"):
                assert field in item, f"inbox item missing {field!r}: {item!r}"
            assert item["status"] in ("queued", "delivered", "read", "dismissed")

    def test_list_unauthenticated_returns_401(self, api):
        r = api("GET", "/notifications")
        assert r.status_code == 401

    def test_list_status_filter_unread(self, api, admin_token):
        r = api("GET", "/notifications", token=admin_token,
                params={"status": "unread"})
        assert r.status_code == 200

    def test_list_status_filter_all(self, api, admin_token):
        r = api("GET", "/notifications", token=admin_token,
                params={"status": "all"})
        assert r.status_code == 200

    def test_list_status_filter_dismissed(self, api, admin_token):
        r = api("GET", "/notifications", token=admin_token,
                params={"status": "dismissed"})
        assert r.status_code == 200

    def test_list_invalid_status_returns_error(self, api, admin_token):
        r = api("GET", "/notifications", token=admin_token,
                params={"status": "invalid_status_xyz"})
        assert r.status_code in (400, 422)

    def test_list_limit_param(self, api, admin_token):
        r = api("GET", "/notifications", token=admin_token,
                params={"limit": 5, "offset": 0})
        assert r.status_code == 200


class TestUnreadCount:
    def test_unread_count_response_contract(self, api, admin_token):
        r = api("GET", "/notifications/unread-count", token=admin_token)
        assert r.status_code == 200
        body = r.json()
        assert isinstance(body, dict)
        assert "unread" in body and isinstance(body["unread"], int) and body["unread"] >= 0
        assert "queued" in body and isinstance(body["queued"], int) and body["queued"] >= 0

    def test_unread_count_unauthenticated_returns_401(self, api):
        r = api("GET", "/notifications/unread-count")
        assert r.status_code == 401
        assert r.json().get("code") == "UNAUTHORIZED"


class TestMarkRead:
    def test_read_all_returns_200(self, api, admin_token):
        r = api("POST", "/notifications/read-all", token=admin_token, json={})
        assert r.status_code == 200

    def test_read_all_unauthenticated_returns_401(self, api):
        r = api("POST", "/notifications/read-all", json={})
        assert r.status_code == 401

    def test_mark_single_nonexistent_returns_404(self, api, admin_token):
        r = api("POST", f"/notifications/{NON_EXISTENT_ID}/read",
                token=admin_token)
        assert r.status_code == 404

    def test_dismiss_nonexistent_returns_404(self, api, admin_token):
        r = api("POST", f"/notifications/{NON_EXISTENT_ID}/dismiss",
                token=admin_token)
        assert r.status_code == 404

    def test_get_nonexistent_notification_returns_404(self, api, admin_token):
        r = api("GET", f"/notifications/{NON_EXISTENT_ID}",
                token=admin_token)
        assert r.status_code == 404


# ── DND Preferences ───────────────────────────────────────────────────────────

class TestPreferences:
    def test_get_preferences_returns_200(self, api, admin_token):
        r = api("GET", "/notifications/preferences", token=admin_token)
        assert r.status_code == 200

    def test_preferences_has_dnd_enabled_field(self, api, admin_token):
        body = api("GET", "/notifications/preferences", token=admin_token).json()
        assert "dnd_enabled" in body

    def test_get_preferences_unauthenticated_returns_401(self, api):
        r = api("GET", "/notifications/preferences")
        assert r.status_code == 401

    def test_disable_dnd_succeeds(self, api, admin_token):
        r = api("PUT", "/notifications/preferences", token=admin_token,
                json={"dnd_enabled": False})
        assert r.status_code == 200

    def test_enable_dnd_with_window_succeeds(self, api, admin_token):
        r = api("PUT", "/notifications/preferences", token=admin_token,
                json={"dnd_enabled": True, "dnd_start": "22:00", "dnd_end": "07:00"})
        assert r.status_code == 200

    def test_preferences_update_persists(self, api, admin_token):
        api("PUT", "/notifications/preferences", token=admin_token,
            json={"dnd_enabled": False})
        body = api("GET", "/notifications/preferences", token=admin_token).json()
        assert body["dnd_enabled"] is False

    def test_update_preferences_unauthenticated_returns_401(self, api):
        r = api("PUT", "/notifications/preferences",
                json={"dnd_enabled": False})
        assert r.status_code == 401

    def test_staff_can_manage_own_dnd(self, api, staff_token):
        r = api("GET", "/notifications/preferences", token=staff_token)
        assert r.status_code == 200

    def test_dispatcher_can_manage_own_dnd(self, api, dispatcher_token):
        r = api("GET", "/notifications/preferences", token=dispatcher_token)
        assert r.status_code == 200


# ── Subscriptions ─────────────────────────────────────────────────────────────

class TestSubscriptions:
    def test_list_subscriptions_returns_200(self, api, admin_token):
        r = api("GET", "/notifications/subscriptions", token=admin_token)
        assert r.status_code == 200

    def test_list_subscriptions_returns_array(self, api, admin_token):
        body = api("GET", "/notifications/subscriptions", token=admin_token).json()
        assert isinstance(body, list)

    def test_list_subscriptions_unauthenticated_returns_401(self, api):
        r = api("GET", "/notifications/subscriptions")
        assert r.status_code == 401

    def test_update_subscriptions_with_empty_list(self, api, admin_token):
        r = api("PUT", "/notifications/subscriptions", token=admin_token,
                json={"event_types": []})
        assert r.status_code == 200

    def test_update_subscriptions_with_event_types(self, api, admin_token):
        r = api("PUT", "/notifications/subscriptions", token=admin_token,
                json={"event_types": ["trip.completed", "payment.captured"]})
        assert r.status_code == 200

    def test_staff_can_list_own_subscriptions(self, api, staff_token):
        """staff_user can list their own subscriptions (own-resource, not manage)."""
        r = api("GET", "/notifications/subscriptions", token=staff_token)
        assert r.status_code == 200
        assert isinstance(r.json(), list)


# ── Notification Rules ────────────────────────────────────────────────────────

class TestNotificationRules:
    def test_list_rules_returns_200(self, api, admin_token):
        r = api("GET", "/notifications/rules", token=admin_token)
        assert r.status_code == 200

    def test_list_rules_returns_array(self, api, admin_token):
        body = api("GET", "/notifications/rules", token=admin_token).json()
        assert isinstance(body, list)

    def test_list_rules_unauthenticated_returns_401(self, api):
        r = api("GET", "/notifications/rules")
        assert r.status_code == 401

    def test_create_rule_succeeds(self, api, admin_token):
        r = api("POST", "/notifications/rules", token=admin_token,
                json={
                    "rule_name": "api_test_keyword_rule",
                    "rule_type": "keyword",
                    "config": {"keywords": ["urgent", "critical"]},
                })
        assert r.status_code in (200, 201)

    def test_create_rule_returns_id_and_fields(self, api, admin_token):
        r = api("POST", "/notifications/rules", token=admin_token,
                json={
                    "rule_name": "api_test_rule_id_check",
                    "rule_type": "keyword",
                    "config": {"keywords": ["test"]},
                })
        assert r.status_code in (200, 201), r.text
        body = r.json()
        assert "id" in body
        uuid.UUID(body["id"])
        assert body.get("rule_name") == "api_test_rule_id_check"

    def test_create_rule_missing_name_returns_error(self, api, admin_token):
        r = api("POST", "/notifications/rules", token=admin_token,
                json={"rule_type": "keyword", "config": {}})
        assert r.status_code in (400, 422)

    def test_get_nonexistent_rule_returns_404(self, api, admin_token):
        r = api("GET", f"/notifications/rules/{NON_EXISTENT_ID}",
                token=admin_token)
        assert r.status_code == 404

    def test_update_nonexistent_rule_returns_404(self, api, admin_token):
        r = api("PUT", f"/notifications/rules/{NON_EXISTENT_ID}",
                token=admin_token,
                json={"rule_name": "updated_name"})
        assert r.status_code == 404

    def test_delete_nonexistent_rule_returns_404(self, api, admin_token):
        r = api("DELETE", f"/notifications/rules/{NON_EXISTENT_ID}",
                token=admin_token)
        assert r.status_code == 404

    def test_toggle_nonexistent_rule_returns_404(self, api, admin_token):
        r = api("POST", f"/notifications/rules/{NON_EXISTENT_ID}/toggle",
                token=admin_token)
        assert r.status_code == 404

    def test_create_update_delete_rule_lifecycle(self, api, admin_token):
        """Create → GET → toggle → DELETE lifecycle."""
        create_r = api("POST", "/notifications/rules", token=admin_token,
                       json={
                           "rule_name": "api_test_lifecycle_rule",
                           "rule_type": "topic",
                           "config": {"topics": ["payments"]},
                       })
        assert create_r.status_code in (200, 201)
        rule_id = create_r.json()["id"]

        get_r = api("GET", f"/notifications/rules/{rule_id}", token=admin_token)
        assert get_r.status_code == 200

        toggle_r = api("POST", f"/notifications/rules/{rule_id}/toggle",
                       token=admin_token)
        assert toggle_r.status_code == 200

        delete_r = api("DELETE", f"/notifications/rules/{rule_id}",
                       token=admin_token)
        assert delete_r.status_code in (200, 204)


# ── Announcements ─────────────────────────────────────────────────────────────

class TestAnnounce:
    def test_admin_can_announce(self, api, admin_token, test_user_ids):
        r = api("POST", "/notifications/announce", token=admin_token,
                json={
                    "title": "Test Announcement",
                    "message": "This is an API test announcement.",
                    "severity": "info",
                    "target_roles": ["all"],
                })
        assert r.status_code in (200, 201)

    def test_announce_unauthenticated_returns_401(self, api):
        r = api("POST", "/notifications/announce",
                json={"title": "x", "message": "y"})
        assert r.status_code == 401

    def test_dispatcher_cannot_announce(self, api, dispatcher_token, test_user_ids):
        r = api("POST", "/notifications/announce", token=dispatcher_token,
                json={"title": "Unauthorized", "message": "Should be blocked."})
        assert r.status_code == 403

    def test_finance_cannot_announce(self, api, finance_token, test_user_ids):
        r = api("POST", "/notifications/announce", token=finance_token,
                json={"title": "Unauthorized", "message": "Should be blocked."})
        assert r.status_code == 403

    def test_staff_cannot_announce(self, api, staff_token, test_user_ids):
        r = api("POST", "/notifications/announce", token=staff_token,
                json={"title": "Unauthorized", "message": "Should be blocked."})
        assert r.status_code == 403

    def test_announce_missing_title_returns_error(self, api, admin_token, test_user_ids):
        r = api("POST", "/notifications/announce", token=admin_token,
                json={"message": "No title provided."})
        assert r.status_code in (400, 422)


# ── Channel Preferences ───────────────────────────────────────────────────────

class TestChannelPreferences:
    def test_list_channel_prefs_returns_200(self, api, admin_token):
        r = api("GET", "/notifications/channels", token=admin_token)
        assert r.status_code == 200

    def test_list_channel_prefs_returns_array(self, api, admin_token):
        body = api("GET", "/notifications/channels", token=admin_token).json()
        assert isinstance(body, list)

    def test_list_channel_prefs_unauthenticated_returns_401(self, api):
        r = api("GET", "/notifications/channels")
        assert r.status_code == 401

    def test_upsert_email_channel_pref(self, api, admin_token):
        r = api("PUT", "/notifications/channels/email", token=admin_token,
                json={"is_enabled": True})
        assert r.status_code in (200, 201)

    def test_delete_email_channel_pref(self, api, admin_token):
        # Upsert first so delete has something to remove
        api("PUT", "/notifications/channels/email", token=admin_token,
            json={"is_enabled": True})
        r = api("DELETE", "/notifications/channels/email", token=admin_token)
        assert r.status_code in (200, 204)
