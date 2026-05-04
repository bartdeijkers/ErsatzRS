import { defineConfig, devices } from '@playwright/test';
import path from 'node:path';

const port = Number(process.env.E2E_PORT ?? '8409');
const baseURL = process.env.E2E_BASE_URL ?? `http://127.0.0.1:${port}`;
const dbPath = path.resolve(process.env.E2E_DB_PATH ?? 'tests/e2e/.tmp/ErsatzRS.db');

export default defineConfig({
  testDir: './tests/e2e',
  timeout: 30_000,
  expect: {
    timeout: 5_000,
  },
  fullyParallel: false,
  reporter: [['list'], ['html', { open: 'never', outputFolder: 'tests/e2e/.playwright-report' }]],
  use: {
    baseURL,
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
  },
  webServer: {
    command: 'mkdir -p tests/e2e/.tmp && ./run.sh build',
    url: `${baseURL}/healthz`,
    reuseExistingServer: true,
    timeout: 180_000,
    env: {
      ...process.env,
      DATABASE_URL: process.env.DATABASE_URL ?? `sqlite:${dbPath}`,
      ERSATZRS_DB_PATH: process.env.ERSATZRS_DB_PATH ?? dbPath,
    },
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
});
