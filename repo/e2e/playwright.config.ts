import { defineConfig, devices } from '@playwright/test';

/**
 * TransitOps Playwright config.
 *
 * `BASE_URL` points at the nginx-fronted SPA inside the docker-compose test
 * network. The frontend container reverse-proxies API calls to the api-test
 * service, so the test exercises the real WASM bundle, the real HTTP API,
 * and the real PostgreSQL instance — no mocks at any layer.
 */
const BASE_URL = process.env.BASE_URL ?? 'http://frontend-test:80';

export default defineConfig({
  testDir: './tests',
  // The Yew app + WASM bundle takes a moment to boot on first paint, so give
  // each spec generous time before deciding it has hung.
  timeout: 60_000,
  expect: { timeout: 15_000 },
  fullyParallel: false,
  retries: process.env.CI ? 1 : 0,
  workers: 1,
  reporter: [
    ['list'],
    ['html', { outputFolder: 'playwright-report', open: 'never' }],
    ['junit', { outputFile: 'playwright-report/junit.xml' }],
  ],
  use: {
    baseURL: BASE_URL,
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    video: 'retain-on-failure',
    actionTimeout: 15_000,
    navigationTimeout: 30_000,
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
});
