import { test, expect } from '@playwright/test';

/**
 * E2E: KPI metrics reporting flow.
 *
 * Stack exercised (no mocks):
 *   Playwright → nginx → WASM Yew → fetch('/reporting/metrics') → API → Postgres
 *
 * Verifies:
 *   1. Admin can reach /reporting/metrics.
 *   2. The GET /reporting/metrics call returns 200 with an array of metric
 *      definitions (seeded in db/seeds/005_metric_definitions_seed.sql).
 *   3. The KPI Metrics heading is visible.
 *   4. At least one seeded metric (on_time_departure_rate) renders on screen.
 */

const ADMIN = { username: 'admin', password: 'AdminPass123!' };

async function login(page: import('@playwright/test').Page): Promise<void> {
  await page.goto('/login');
  await page.getByPlaceholder('username').fill(ADMIN.username);
  await page.getByPlaceholder('••••••••').fill(ADMIN.password);
  await page.getByRole('button', { name: 'Sign in' }).click();
  await page.waitForURL('**/notifications');
}

test.describe('Reporting / KPI metrics', () => {
  test('admin can list seeded KPI metrics', async ({ page }) => {
    await login(page);

    const listResp = page.waitForResponse(
      (resp) =>
        resp.url().endsWith('/api/reporting/metrics') &&
        resp.request().method() === 'GET',
    );
    await page.goto('/reporting/metrics');
    const resp = await listResp;
    expect(resp.status()).toBe(200);
    const body = await resp.json();
    expect(Array.isArray(body)).toBeTruthy();
    // At least one seeded metric definition must be present.
    expect(body.length).toBeGreaterThan(0);
    const keys = body.map((m: { metric_key: string }) => m.metric_key);
    // Seed 005 installs at minimum the on-time-departure metric.
    expect(keys).toContain('on_time_departure_rate');

    await expect(page.getByRole('heading', { name: 'KPI Metrics' })).toBeVisible();
    // The metric's display name should appear in the rendered list.
    await expect(page.getByText('On-Time Departure Rate').first()).toBeVisible();
  });
});
