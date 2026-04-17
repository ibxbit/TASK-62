import { test, expect } from '@playwright/test';

/**
 * Critical-flow E2E: log in as the seeded operations admin and reach the
 * inbox page.
 *
 * Stack exercised end-to-end (no mocks):
 *   Playwright (Chromium)
 *     → nginx (frontend container)        — serves the WASM SPA shell
 *     → WASM Yew app                      — renders LoginPage, drives fetch
 *     → nginx (reverse proxy)             — proxies fetch → API
 *     → Rust/Actix API (api-test)         — verifies argon2 password hash
 *     → PostgreSQL (db-test)              — looks up the seeded admin row
 *
 * The seeded admin (`admin` / `AdminPass123!`) is created by db/seeds during
 * the db-test container's first start, so no test fixture needs to insert it.
 */

const ADMIN_USERNAME = process.env.E2E_ADMIN_USERNAME ?? 'admin';
const ADMIN_PASSWORD = process.env.E2E_ADMIN_PASSWORD ?? 'AdminPass123!';

test.describe('Authentication flow', () => {
  test('seeded admin can log in and lands on the inbox', async ({ page }) => {
    // ── Step 1: SPA shell loads ────────────────────────────────────────────
    await page.goto('/login');
    await expect(page.getByRole('heading', { name: 'TransitOps' })).toBeVisible();

    // ── Step 2: Fill the form using accessible selectors ───────────────────
    await page.getByPlaceholder('username').fill(ADMIN_USERNAME);
    await page.getByPlaceholder('••••••••').fill(ADMIN_PASSWORD);

    // ── Step 3: Submit and wait for the real /auth/login round-trip ───────
    const loginResponsePromise = page.waitForResponse(
      (resp) => resp.url().endsWith('/auth/login') && resp.request().method() === 'POST',
    );
    await page.getByRole('button', { name: 'Sign in' }).click();
    const loginResponse = await loginResponsePromise;
    expect(loginResponse.status()).toBe(200);

    const loginBody = await loginResponse.json();
    expect(loginBody).toHaveProperty('token');
    expect(loginBody.username).toBe(ADMIN_USERNAME);
    expect(loginBody.role).toBe('operations_admin');

    // ── Step 4: Yew router redirects to /notifications, inbox renders ─────
    await page.waitForURL('**/notifications', { timeout: 15_000 });
    await expect(page.getByRole('heading', { name: 'Inbox' })).toBeVisible();

    // ── Step 5: Token is persisted so subsequent reloads stay authed ──────
    const token = await page.evaluate(() => localStorage.getItem('transitops_token'));
    expect(token).toBeTruthy();
  });

  test('rejected credentials surface the API error', async ({ page }) => {
    await page.goto('/login');

    await page.getByPlaceholder('username').fill(ADMIN_USERNAME);
    await page.getByPlaceholder('••••••••').fill('definitely-wrong-password');

    const loginResponsePromise = page.waitForResponse(
      (resp) => resp.url().endsWith('/auth/login') && resp.request().method() === 'POST',
    );
    await page.getByRole('button', { name: 'Sign in' }).click();
    const loginResponse = await loginResponsePromise;
    expect(loginResponse.status()).toBeGreaterThanOrEqual(400);

    // Error text is rendered into the form (LoginState::Error variant).
    await expect(page.locator('.form-error')).toBeVisible();
    // The router should not have advanced past /login.
    expect(new URL(page.url()).pathname).toBe('/login');
  });
});
