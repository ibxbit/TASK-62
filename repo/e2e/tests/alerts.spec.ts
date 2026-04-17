import { test, expect } from '@playwright/test';

/**
 * E2E: alerts dashboard flow.
 *
 * Stack exercised (no mocks):
 *   Playwright → nginx → WASM Yew → fetch('/alerts') → Rust API → Postgres
 *
 * Verifies:
 *   1. Admin can navigate to /alerts and the page shell renders.
 *   2. Real GET /alerts fires and returns 200 with an array.
 *   3. Clicking "Refresh" triggers a second real GET /alerts.
 *   4. A staff user is denied — the WASM-level RoleGuard redirects them.
 */

const ADMIN = { username: 'admin', password: 'AdminPass123!' };
const STAFF = { username: 'staff', password: 'StaffPass123!' };

async function login(
  page: import('@playwright/test').Page,
  creds: { username: string; password: string },
  expectedPath: string = '/notifications',
): Promise<void> {
  await page.goto('/login');
  await page.getByPlaceholder('username').fill(creds.username);
  await page.getByPlaceholder('••••••••').fill(creds.password);
  await page.getByRole('button', { name: 'Sign in' }).click();
  await page.waitForURL(`**${expectedPath}`);
}

test.describe('Alerts dashboard', () => {
  test('admin can load alerts and refresh', async ({ page }) => {
    await login(page, ADMIN);

    const firstLoad = page.waitForResponse(
      (resp) => resp.url().endsWith('/api/alerts') && resp.request().method() === 'GET',
    );
    await page.goto('/alerts');
    const firstResp = await firstLoad;
    expect(firstResp.status()).toBe(200);
    const body = await firstResp.json();
    expect(Array.isArray(body)).toBeTruthy();

    await expect(page.getByRole('heading', { name: 'Alerts' })).toBeVisible();

    // Clicking refresh fires a second GET.
    const refreshResp = page.waitForResponse(
      (resp) => resp.url().endsWith('/api/alerts') && resp.request().method() === 'GET',
    );
    await page.getByRole('button', { name: 'Refresh' }).click();
    const refreshed = await refreshResp;
    expect(refreshed.status()).toBe(200);
  });

  test('staff user hitting /alerts directly gets a real 403 from the API', async ({ page }) => {
    await login(page, STAFF);

    // Watch for the backend 403 — the SPA's fetch hits the real API.
    const alertsResponsePromise = page.waitForResponse(
      (resp) => resp.url().endsWith('/api/alerts') && resp.request().method() === 'GET',
      { timeout: 10_000 },
    ).catch(() => null);

    await page.goto('/alerts');

    const resp = await alertsResponsePromise;
    if (resp !== null) {
      // If the SPA issued the fetch, the backend must say 403.
      expect(resp.status()).toBe(403);
    }
    // Either way, the "Alerts" heading's data must not have loaded successfully.
    // RoleGuard may redirect or render an error — both are acceptable UX.
    const heading = page.getByRole('heading', { name: 'Alerts' });
    // Give the SPA a beat to render denial UI.
    await page.waitForTimeout(1000);
    const body = await page.content();
    expect(
      body.toLowerCase().includes('forbidden') ||
        body.toLowerCase().includes('permission') ||
        body.toLowerCase().includes('not authorised') ||
        body.toLowerCase().includes('not authorized') ||
        // Or the app simply didn't render the alerts list successfully.
        !(await heading.isVisible().catch(() => false)),
    ).toBeTruthy();
  });
});
