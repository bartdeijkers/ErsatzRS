import { test as base, expect, type Page } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import path from 'node:path';

type Fixtures = {
  seedAdmin: () => void;
  mediaFixturePath: string;
};

const repoRoot = path.resolve(__dirname, '../..');
const mediaFixturePath = path.join(repoRoot, 'tests/fixtures/media');
const dbPath = path.resolve(process.env.E2E_DB_PATH ?? 'tests/e2e/.tmp/ErsatzRS.db');

function seedAdmin() {
  execFileSync('./run.sh', ['seed'], {
    cwd: repoRoot,
    env: {
      ...process.env,
      DATABASE_URL: process.env.DATABASE_URL ?? `sqlite:${dbPath}`,
      ERSATZRS_DB_PATH: process.env.ERSATZRS_DB_PATH ?? dbPath,
      ERSATZRS_SEED_FIXTURE: 'tests/fixtures/seed_admin.sql',
    },
    stdio: 'inherit',
  });
}

async function installConsoleGuards(page: Page) {
  const failures: string[] = [];

  page.on('console', (message) => {
    if (message.type() !== 'error') {
      return;
    }

    const text = message.text();
    if (
      text.includes('favicon') ||
      text.includes('ResizeObserver loop completed')
    ) {
      return;
    }

    failures.push(text);
  });

  page.on('pageerror', (error) => {
    failures.push(error.message);
  });

  return failures;
}

export const test = base.extend<Fixtures>({
  seedAdmin: async ({}, use) => {
    await use(seedAdmin);
  },
  mediaFixturePath: async ({}, use) => {
    await use(mediaFixturePath);
  },
  page: async ({ page }, use) => {
    const failures = await installConsoleGuards(page);
    await use(page);

    const hydrationFailures = failures.filter(
      (failure) =>
        failure.includes('failed_to_cast_marker_node') ||
        failure.includes('hydration') ||
        failure.includes('Uncaught') ||
        failure.includes('panicked'),
    );

    expect(hydrationFailures, 'browser console/page errors').toEqual([]);
  },
});

export { expect };
