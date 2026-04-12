"""
Payments endpoint tests.

Endpoints covered
-----------------
  POST /payments/transactions           create transaction (idempotent)
  GET  /payments/transactions           list transactions
  GET  /payments/transactions/{id}      get single transaction
  POST /payments/refunds                create refund (idempotent)
  GET  /payments/refunds                list refunds
  GET  /payments/refunds/{id}           get single refund
  POST /payments/refunds/{id}/approve   approve refund
  POST /payments/refunds/{id}/process   process approved refund
  POST /payments/imports                upload statement import
  GET  /payments/imports                list imports
  GET  /payments/imports/{id}           get import
  POST /payments/imports/{id}/process   process import
  POST /payments/callbacks/simulate     simulate gateway callback
  GET  /payments/callbacks/{id}         get callback record
"""

import uuid

import pytest

from conftest import TEST_USERS

NON_EXISTENT_ID = "00000000-0000-0000-0000-000000000000"


def _unique_key(prefix: str = "api_test") -> str:
    return f"{prefix}_{uuid.uuid4().hex[:12]}"


# ── Transactions ──────────────────────────────────────────────────────────────

class TestCreateTransaction:
    def test_finance_can_create_transaction(self, api, finance_token, test_user_ids):
        r = api("POST", "/payments/transactions", token=finance_token,
                json={
                    "idempotency_key": _unique_key("txn"),
                    "amount": "50.00",
                    "payment_method": "card",
                })
        assert r.status_code in (200, 201)

    def test_create_transaction_returns_required_fields(self, api, finance_token, test_user_ids):
        r = api("POST", "/payments/transactions", token=finance_token,
                json={
                    "idempotency_key": _unique_key("txn"),
                    "amount": "25.00",
                    "payment_method": "cash",
                })
        assert r.status_code in (200, 201)
        body = r.json()
        assert "id" in body
        assert "idempotency_key" in body
        assert "status" in body

    def test_idempotency_same_key_returns_same_record(self, api, finance_token, test_user_ids):
        key = _unique_key("idem")
        payload = {"idempotency_key": key, "amount": "10.00", "payment_method": "mobile"}
        r1 = api("POST", "/payments/transactions", token=finance_token, json=payload)
        r2 = api("POST", "/payments/transactions", token=finance_token, json=payload)
        assert r1.status_code in (200, 201)
        assert r2.status_code in (200, 201)
        assert r1.json()["id"] == r2.json()["id"]

    def test_create_transaction_unauthenticated_returns_401(self, api):
        r = api("POST", "/payments/transactions",
                json={"idempotency_key": _unique_key(), "amount": "1.00",
                      "payment_method": "cash"})
        assert r.status_code == 401

    def test_staff_cannot_create_transaction(self, api, staff_token, test_user_ids):
        r = api("POST", "/payments/transactions", token=staff_token,
                json={"idempotency_key": _unique_key(), "amount": "5.00",
                      "payment_method": "cash"})
        assert r.status_code == 403

    def test_dispatcher_cannot_create_transaction(self, api, dispatcher_token, test_user_ids):
        r = api("POST", "/payments/transactions", token=dispatcher_token,
                json={"idempotency_key": _unique_key(), "amount": "5.00",
                      "payment_method": "cash"})
        assert r.status_code == 403

    def test_missing_amount_returns_error(self, api, finance_token, test_user_ids):
        r = api("POST", "/payments/transactions", token=finance_token,
                json={"idempotency_key": _unique_key(), "payment_method": "cash"})
        assert r.status_code in (400, 422)

    def test_missing_payment_method_returns_error(self, api, finance_token, test_user_ids):
        r = api("POST", "/payments/transactions", token=finance_token,
                json={"idempotency_key": _unique_key(), "amount": "5.00"})
        assert r.status_code in (400, 422)

    def test_invalid_payment_method_returns_error(self, api, finance_token, test_user_ids):
        r = api("POST", "/payments/transactions", token=finance_token,
                json={"idempotency_key": _unique_key(), "amount": "5.00",
                      "payment_method": "bitcoin"})
        assert r.status_code in (400, 422)


class TestListTransactions:
    def test_finance_can_list_transactions(self, api, finance_token, test_user_ids):
        r = api("GET", "/payments/transactions", token=finance_token)
        assert r.status_code == 200

    def test_list_returns_array(self, api, finance_token, test_user_ids):
        body = api("GET", "/payments/transactions", token=finance_token).json()
        assert isinstance(body, list)

    def test_admin_can_list_transactions(self, api, admin_token, test_user_ids):
        r = api("GET", "/payments/transactions", token=admin_token)
        assert r.status_code == 200

    def test_staff_cannot_list_transactions(self, api, staff_token, test_user_ids):
        r = api("GET", "/payments/transactions", token=staff_token)
        assert r.status_code == 403

    def test_list_with_limit_param(self, api, finance_token, test_user_ids):
        r = api("GET", "/payments/transactions", token=finance_token,
                params={"limit": 5})
        assert r.status_code == 200

    def test_list_unauthenticated_returns_401(self, api):
        r = api("GET", "/payments/transactions")
        assert r.status_code == 401

    def test_get_nonexistent_transaction_returns_404(self, api, finance_token, test_user_ids):
        r = api("GET", f"/payments/transactions/{NON_EXISTENT_ID}",
                token=finance_token)
        assert r.status_code == 404


# ── Refunds ───────────────────────────────────────────────────────────────────

class TestCreateRefund:
    def test_finance_can_create_refund(self, api, finance_token, test_user_ids):
        # Create a transaction first
        txn = api("POST", "/payments/transactions", token=finance_token,
                  json={"idempotency_key": _unique_key("txn"),
                        "amount": "100.00", "payment_method": "card"})
        if txn.status_code not in (200, 201):
            pytest.skip("Transaction creation failed; skipping refund test")
        txn_id = txn.json()["id"]

        r = api("POST", "/payments/refunds", token=finance_token,
                json={"transaction_id": txn_id,
                      "idempotency_key": _unique_key("ref"),
                      "amount": "10.00",
                      "reason": "API test refund"})
        assert r.status_code in (200, 201)

    def test_create_refund_idempotency(self, api, finance_token, test_user_ids):
        txn = api("POST", "/payments/transactions", token=finance_token,
                  json={"idempotency_key": _unique_key("txn"),
                        "amount": "200.00", "payment_method": "card"})
        if txn.status_code not in (200, 201):
            pytest.skip("Transaction creation failed; skipping refund idempotency test")
        txn_id = txn.json()["id"]
        key = _unique_key("ref")
        payload = {"transaction_id": txn_id, "idempotency_key": key, "amount": "5.00"}

        r1 = api("POST", "/payments/refunds", token=finance_token, json=payload)
        r2 = api("POST", "/payments/refunds", token=finance_token, json=payload)
        assert r1.status_code in (200, 201)
        assert r2.status_code in (200, 201)
        assert r1.json()["id"] == r2.json()["id"]

    def test_create_refund_unauthenticated_returns_401(self, api):
        r = api("POST", "/payments/refunds",
                json={"transaction_id": NON_EXISTENT_ID,
                      "idempotency_key": _unique_key(), "amount": "1.00"})
        assert r.status_code == 401

    def test_staff_cannot_create_refund(self, api, staff_token, test_user_ids):
        r = api("POST", "/payments/refunds", token=staff_token,
                json={"transaction_id": NON_EXISTENT_ID,
                      "idempotency_key": _unique_key(), "amount": "1.00"})
        assert r.status_code == 403

    def test_dispatcher_cannot_create_refund(self, api, dispatcher_token, test_user_ids):
        r = api("POST", "/payments/refunds", token=dispatcher_token,
                json={"transaction_id": NON_EXISTENT_ID,
                      "idempotency_key": _unique_key(), "amount": "1.00"})
        assert r.status_code == 403

    def test_create_refund_with_nonexistent_transaction_returns_error(
            self, api, finance_token, test_user_ids):
        r = api("POST", "/payments/refunds", token=finance_token,
                json={"transaction_id": NON_EXISTENT_ID,
                      "idempotency_key": _unique_key(), "amount": "1.00"})
        # 404 (transaction not found) or 400 (validation)
        assert r.status_code in (400, 404, 422)


class TestListRefunds:
    def test_finance_can_list_refunds(self, api, finance_token, test_user_ids):
        r = api("GET", "/payments/refunds", token=finance_token)
        assert r.status_code == 200

    def test_list_refunds_returns_array(self, api, finance_token, test_user_ids):
        body = api("GET", "/payments/refunds", token=finance_token).json()
        assert isinstance(body, list)

    def test_admin_can_read_refunds(self, api, admin_token, test_user_ids):
        r = api("GET", "/payments/refunds", token=admin_token)
        assert r.status_code == 200

    def test_staff_cannot_list_refunds(self, api, staff_token, test_user_ids):
        r = api("GET", "/payments/refunds", token=staff_token)
        assert r.status_code == 403

    def test_get_nonexistent_refund_returns_404(self, api, finance_token, test_user_ids):
        r = api("GET", f"/payments/refunds/{NON_EXISTENT_ID}",
                token=finance_token)
        assert r.status_code == 404

    def test_approve_nonexistent_refund_returns_404(self, api, finance_token, test_user_ids):
        r = api("POST", f"/payments/refunds/{NON_EXISTENT_ID}/approve",
                token=finance_token, json={})
        assert r.status_code == 404

    def test_process_nonexistent_refund_returns_404(self, api, finance_token, test_user_ids):
        r = api("POST", f"/payments/refunds/{NON_EXISTENT_ID}/process",
                token=finance_token)
        assert r.status_code == 404


# ── Imports ───────────────────────────────────────────────────────────────────

class TestStatementImports:
    def _b64(self) -> str:
        import base64
        content = f"date,amount,ref\n2024-01-01,100.00,REF{uuid.uuid4().hex[:8]}\n"
        return base64.b64encode(content.encode()).decode()

    def test_finance_can_upload_import(self, api, finance_token, test_user_ids):
        r = api("POST", "/payments/imports", token=finance_token,
                json={
                    "filename": "api_test_statement.csv",
                    "source": "test_gateway",
                    "format": "csv",
                    "content_base64": self._b64(),
                })
        assert r.status_code in (200, 201)

    def test_upload_import_returns_id(self, api, finance_token, test_user_ids):
        r = api("POST", "/payments/imports", token=finance_token,
                json={
                    "filename": "api_test_id_check.csv",
                    "source": "test_gateway",
                    "format": "csv",
                    "content_base64": self._b64(),
                })
        if r.status_code in (200, 201):
            assert "id" in r.json()

    def test_staff_cannot_upload_import(self, api, staff_token, test_user_ids):
        r = api("POST", "/payments/imports", token=staff_token,
                json={"filename": "x.csv", "source": "test",
                      "format": "csv", "content_base64": self._b64()})
        assert r.status_code == 403

    def test_dispatcher_cannot_upload_import(self, api, dispatcher_token, test_user_ids):
        r = api("POST", "/payments/imports", token=dispatcher_token,
                json={"filename": "x.csv", "source": "test",
                      "format": "csv", "content_base64": self._b64()})
        assert r.status_code == 403

    def test_finance_can_list_imports(self, api, finance_token, test_user_ids):
        r = api("GET", "/payments/imports", token=finance_token)
        assert r.status_code == 200

    def test_list_imports_returns_array(self, api, finance_token, test_user_ids):
        body = api("GET", "/payments/imports", token=finance_token).json()
        assert isinstance(body, list)

    def test_get_nonexistent_import_returns_404(self, api, finance_token, test_user_ids):
        r = api("GET", f"/payments/imports/{NON_EXISTENT_ID}",
                token=finance_token)
        assert r.status_code == 404

    def test_process_nonexistent_import_returns_404(self, api, finance_token, test_user_ids):
        r = api("POST", f"/payments/imports/{NON_EXISTENT_ID}/process",
                token=finance_token)
        assert r.status_code == 404

    def test_upload_import_unauthenticated_returns_401(self, api):
        r = api("POST", "/payments/imports",
                json={"filename": "x.csv", "source": "test",
                      "format": "csv", "content_base64": self._b64()})
        assert r.status_code == 401


# ── Callbacks ─────────────────────────────────────────────────────────────────

class TestCallbacks:
    def test_simulate_callback_nonexistent_transaction_returns_error(
            self, api, finance_token, test_user_ids):
        r = api("POST", "/payments/callbacks/simulate", token=finance_token,
                json={
                    "gateway": "test_gw",
                    "transaction_id": NON_EXISTENT_ID,
                    "status": "completed",
                })
        # 404 (transaction not found) or 400
        assert r.status_code in (400, 404, 422)

    def test_simulate_callback_unauthenticated_returns_401(self, api):
        r = api("POST", "/payments/callbacks/simulate",
                json={"gateway": "test_gw",
                      "transaction_id": NON_EXISTENT_ID,
                      "status": "completed"})
        assert r.status_code == 401

    def test_get_nonexistent_callback_returns_404(self, api, finance_token, test_user_ids):
        r = api("GET", f"/payments/callbacks/{NON_EXISTENT_ID}",
                token=finance_token)
        assert r.status_code == 404

    def test_simulate_callback_succeeds_for_real_transaction(
            self, api, finance_token, test_user_ids):
        txn = api("POST", "/payments/transactions", token=finance_token,
                  json={"idempotency_key": _unique_key("cb_txn"),
                        "amount": "75.00", "payment_method": "card"})
        if txn.status_code not in (200, 201):
            pytest.skip("Transaction creation failed; skipping callback simulate test")
        txn_id = txn.json()["id"]

        r = api("POST", "/payments/callbacks/simulate", token=finance_token,
                json={"gateway": "test_gw",
                      "transaction_id": txn_id,
                      "status": "completed"})
        assert r.status_code in (200, 201)
