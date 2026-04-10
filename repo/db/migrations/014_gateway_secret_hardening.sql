-- ============================================================
-- Migration 014 — Payment gateway secret hardening
-- ============================================================
-- Ensures that any gateway still carrying a placeholder secret
-- is deactivated.  This is idempotent: safe to re-apply.
--
-- Placeholder patterns treated as insecure:
--   'CHANGE_ME_IN_PRODUCTION'
--   Any secret shorter than 16 characters
--
-- To re-activate a gateway after this migration, supply a
-- strong secret:
--   UPDATE payments.gateway_configs
--   SET hmac_secret = '<your-secure-secret>', is_active = TRUE
--   WHERE name = '<gateway-name>';
-- ============================================================

UPDATE payments.gateway_configs
SET    is_active  = FALSE,
       updated_at = now()
WHERE  is_active = TRUE
  AND  (
    hmac_secret IN (
        'CHANGE_ME_IN_PRODUCTION',
        'changeme',
        'secret',
        'test_secret',
        'placeholder',
        'password',
        'default'
    )
    OR length(hmac_secret) < 16
  );
