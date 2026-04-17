import { test, expect } from '@playwright/test';

/**
 * E2E: notifications inbox flow.
 *
 * Exercises the full stack:
 *   Playwright (Chromium)
 *     → nginx (frontend container)
 *     → WASM Yew app — renders InboxPage
 *     → fetch('/notifications') — proxied through nginx
 *     → Rust/Actix API (api-test) — RBAC'd by session
 *     → PostgreSQL (db-test) — reads notifications.inbox_items
 *
 * The test verifies:
 *   1. After login, the user lands on /notifications.
 *   2. The inbox heading renders.
 *   3. A real GET /notifications HTTP call fires and returns 200 with a
 *      JSON array (not an HTML document — proving API proxying works).
 *   4. The "Mark all read" button emits a POST /notifications/read-all.
 */

const ADMIN = { username: 'admin', password: 'AdminPass123!' };

async function login(page: import('@playwright/test').Page): Promise<void> {
  await page.goto('/login');
  await page.getByPlaceholder('username').fill(ADMIN.username);
  await page.getByPlaceholder('••••••••').fill(ADMIN.password);
  await page.getByRole('button', { name: 'Sign in' }).click();
  await page.waitForURL('**/notifications');
}

test.describe('Notifications inbox flow', () => {
  test('inbox fetches real data through the proxy', async ({ page }) => {
    await login(page);

    // Intercept the real fetch that Yew fires after the page mounts.
    const inboxResponsePromise = page.waitForResponse(
      (resp) =>
        resp.url().includes('/notifications') &&
        resp.request().method() === 'GET' &&
        !resp.url().endsWith('/notifications/unread-count'),
    );

    await expect(page.getByRole('heading', { name: 'Inbox' })).toBeVisible();

    const inboxResponse = await inboxResponsePromise;
    expect(inboxResponse.status()).toBe(200);
    const body = await inboxResponse.json();
    expect(Array.isArray(body)).toBeTruthy();
  });

  test('mark-all-read triggers a real POST against the API', async ({ page }) => {
    await login(page);
    await expect(page.getByRole('heading', { name: 'Inbox' })).toBeVisible();

    const ackResponsePromise = page.waitForResponse(
      (resp) =>
        resp.url().endsWith('/notifications/read-all') &&
        resp.request().method() === 'POST',
      { timeout: 15_000 },
    );
    await page.getByRole('button', { name: 'Mark all read' }).click();
    const ackResponse = await ackResponsePromise;
    expect(ackResponse.status()).toBe(200);
  });
});
