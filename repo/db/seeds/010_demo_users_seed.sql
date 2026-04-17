-- =============================================================================
-- Seed 001: Demo users for all four roles.
--
-- Passwords (DEVELOPMENT ONLY — never use in production):
--   admin      / AdminPass123!       → operations_admin
--   dispatcher / DispatcherPass123!  → dispatcher
--   finance    / FinancePass123!     → finance_analyst
--   staff      / StaffPass123!       → staff_user
--
-- Hashes are argon2id (m=19456, t=2, p=1) matching the Rust backend defaults.
-- Emails are AES-256-GCM encrypted with the dev ENCRYPTION_KEY.
-- Upsert on username so re-running the seed is safe.
-- =============================================================================

INSERT INTO auth.users (id, username, email_encrypted, password_hash, role_id, is_active)
VALUES
    ('c0000001-0000-4000-8000-000000000001', 'admin', E'\\x1595842f7c79acd7d6a5c01edd5327fd4e5856daf6f671879cf9b5e90a7a48155d50b4b73c777b228397b55d379dcf9671cb', '$argon2id$v=19$m=19456,t=2,p=1$ZzTNCQNdqq/rwLl4wYMu4g$Z4VsKvotDJbTnfIvxI3OcfaMY69DfNbuFd0kJmB6WcA', (SELECT id FROM auth.roles WHERE name = 'operations_admin'), TRUE),
    ('c0000001-0000-4000-8000-000000000002', 'dispatcher', E'\\x3eb3328c6335a741d2361e54f3161f70313fc3befd1e1a70aacc8c3ec22037c2984f729aaeafed4d063f5e85ac2b6349079e7777a66441', '$argon2id$v=19$m=19456,t=2,p=1$UntcQ65k4ZgRxBZpTHK4WA$tRtw1uP/MzK5oYhnUZsGYOl/sbcbXzGj/iM2HRy3gm4', (SELECT id FROM auth.roles WHERE name = 'dispatcher'), TRUE),
    ('c0000001-0000-4000-8000-000000000003', 'finance', E'\\xb39f2ee0ff99d6ad32835b2e42fc27569e46e7090bcb14e233382a4ba1e9614e727e849fae06b38d9a1344b8f64fc34d169900fb', '$argon2id$v=19$m=19456,t=2,p=1$Sy1OIWhJyJ9P/WkpIb9tEg$o0TSW4blY/iRUNPHIcF863f9kSgulXTGofH+QzqIFjM', (SELECT id FROM auth.roles WHERE name = 'finance_analyst'), TRUE),
    ('c0000001-0000-4000-8000-000000000004', 'staff', E'\\xec6ee363c1d08c4d36c831c5c39c9041f520ea003993eca2517e5802097f618ae6867e7e3185c295712cd1d9c8181f85d163', '$argon2id$v=19$m=19456,t=2,p=1$/KyA2Uhn3N53LbNERkLgmg$wwtixJ+69I8JwvO47RKrRrnuqmXoAwBayBV1+aNnuq0', (SELECT id FROM auth.roles WHERE name = 'staff_user'), TRUE)
ON CONFLICT (username) DO UPDATE
    SET password_hash   = EXCLUDED.password_hash,
        email_encrypted = EXCLUDED.email_encrypted,
        role_id         = EXCLUDED.role_id,
        is_active       = TRUE,
        deleted_at      = NULL,
        updated_at      = now();
