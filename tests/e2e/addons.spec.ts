import { expect, test, type Page } from '@playwright/test';
import { DatabaseSync } from 'node:sqlite';
import { existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const DATABASE_PATH = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;
const APP_STATE_DIR = process.env.E2E_APP_STATE_DIR;
const FIXTURE_ADDON_ID = 'org.example.addon.fixture';
const FIXTURE_REPOSITORY_ID = 'org.example.addons';
const BUNDLE_REPOSITORY_ID = 'org.example.addons.bundle';

test.skip(!DATABASE_PATH, 'E2E_DB_PATH or ERSATZRS_DB_PATH is required.');
test.describe.configure({ mode: 'serial' });

const manifest = {
  manifest_version: 1,
  id: FIXTURE_ADDON_ID,
  version: '1.0.0',
  name: { 'en-US': 'Fixture add-on', nl: 'Test-add-on' },
  summary: { 'en-US': 'Exercises host-rendered add-on settings.' },
  license: 'MIT',
  source_url: 'https://example.test/addon-source',
  host_version: '*',
  capabilities: [
    { id: 'addon.check.v1' },
    { id: 'declarative-panel.v1' },
  ],
  entrypoints: {
    any: { path: 'addon.sh', kind: 'posix_shell' },
  },
  permissions: {
    network_hosts: ['example.test'],
    media_read: false,
    managed_ffmpeg: false,
    external_commands: [],
  },
  settings: [
    {
      key: 'DISPLAY_LABEL',
      label: { 'en-US': 'Display label' },
      kind: 'string',
      required: true,
      default: 'Default value',
      options: [],
    },
    {
      key: 'API_TOKEN',
      label: { 'en-US': 'API token reference' },
      kind: 'secret_reference',
      required: false,
      default: null,
      options: [],
    },
  ],
  panels: [
    {
      id: 'about',
      title: { 'en-US': 'About this add-on' },
      blocks: [
        { kind: 'text', text: { 'en-US': 'Rendered by the ErsatzRS host.' } },
      ],
    },
  ],
};

function openDatabase(): DatabaseSync {
  const database = new DatabaseSync(DATABASE_PATH!);
  database.exec('PRAGMA busy_timeout=5000');
  return database;
}

async function waitForHydration(page: Page): Promise<void> {
  await expect
    .poll(() => page.evaluate(() => window.__ersatzrs_hydrated === true))
    .toBe(true);
}

function resetFixtures(): void {
  const database = openDatabase();
  try {
    database
      .prepare('DELETE FROM "AddonInstallation" WHERE "AddonId" = ?')
      .run(FIXTURE_ADDON_ID);
    database.prepare('DELETE FROM "AddonSourceOffer" WHERE "SourceId" = ?').run(FIXTURE_REPOSITORY_ID);
    database.prepare('DELETE FROM "AddonSourceOffer" WHERE "SourceId" = ?').run(BUNDLE_REPOSITORY_ID);
    database.prepare('DELETE FROM "AddonSource" WHERE "Id" = ?').run(FIXTURE_REPOSITORY_ID);
    database.prepare('DELETE FROM "AddonSource" WHERE "Id" = ?').run(BUNDLE_REPOSITORY_ID);
    database
      .prepare('DELETE FROM "ConfigElement" WHERE "Key" = ?')
      .run('addons.allow_unverified_sources');
    database
      .prepare('DELETE FROM "ConfigElement" WHERE "Key" IN (?, ?)')
      .run('addons.detail_wait.minimum_seconds', 'addons.detail_wait.maximum_seconds');
  } finally {
    database.close();
  }
  if (APP_STATE_DIR) {
    rmSync(join(APP_STATE_DIR, 'addons', 'repository-bundles', BUNDLE_REPOSITORY_ID), {
      recursive: true,
      force: true,
    });
  }
}

function seedLocalRepository(): void {
  const database = openDatabase();
  try {
    database
      .prepare(`
        INSERT INTO "AddonSource" (
          "Id", "Name", "Kind", "RootPath", "ManifestJson", "ContentSha256",
          "LastScan", "LastError", "StateRevision"
        ) VALUES (?, 'Example add-ons', 'checkout', 'e2e-addon-repository', ?, ?, ?, NULL, 1)
      `)
      .run(
        FIXTURE_REPOSITORY_ID,
        JSON.stringify({
          schema_version: 1,
          id: FIXTURE_REPOSITORY_ID,
          name: 'Example add-ons',
          description: { 'en-US': 'Local repository fixture' },
        }),
        '2'.repeat(64),
        '2026-08-28T10:00:00Z',
      );
  } finally {
    database.close();
  }
}

function seedBundleRepository(): string {
  if (!APP_STATE_DIR) {
    throw new Error('E2E_APP_STATE_DIR is required for repository-bundle removal tests');
  }
  const sourceRoot = join(
    APP_STATE_DIR,
    'addons',
    'repository-bundles',
    BUNDLE_REPOSITORY_ID,
    '0123456789abcdef',
  );
  mkdirSync(sourceRoot, { recursive: true });
  writeFileSync(join(sourceRoot, 'repository.toml'), 'schema_version = 1\n');

  const database = openDatabase();
  try {
    database
      .prepare(`
        INSERT INTO "AddonSource" (
          "Id", "Name", "Kind", "RootPath", "ManifestJson", "ContentSha256",
          "LastScan", "LastError", "StateRevision"
        ) VALUES (?, 'Uploaded fixture repository', 'bundle', ?, ?, ?, ?, NULL, 1)
      `)
      .run(
        BUNDLE_REPOSITORY_ID,
        sourceRoot,
        JSON.stringify({
          schema_version: 1,
          id: BUNDLE_REPOSITORY_ID,
          name: 'Uploaded fixture repository',
        }),
        '3'.repeat(64),
        '2026-08-31T10:00:00Z',
      );
    database
      .prepare(`
        INSERT INTO "AddonSourceOffer" (
          "SourceId", "AddonId", "Version", "ManifestJson", "RelativePath", "ContentSha256"
        ) VALUES (?, ?, '1.0.0', ?, 'addons/org.example.addon.fixture', ?)
      `)
      .run(BUNDLE_REPOSITORY_ID, FIXTURE_ADDON_ID, JSON.stringify(manifest), '4'.repeat(64));
    database
      .prepare('UPDATE "AddonInstallation" SET "SourceId" = ? WHERE "AddonId" = ?')
      .run(BUNDLE_REPOSITORY_ID, FIXTURE_ADDON_ID);
  } finally {
    database.close();
  }
  return sourceRoot;
}

function seedInstalledAddon(): void {
  const database = openDatabase();
  try {
    database
      .prepare(`
        INSERT INTO "AddonInstallation" (
          "AddonId", "RepositoryId", "TrustTier", "Version", "Enabled", "PackageSha256",
          "ContentSha256", "ManifestJson", "InstallPath", "DataNamespace", "InstalledAt", "UpdatedAt"
        ) VALUES (?, NULL, 'sideloaded', ?, 0, ?, ?, ?, ?, 'sideload', ?, ?)
      `)
      .run(
        FIXTURE_ADDON_ID,
        '1.0.0',
        '0'.repeat(64),
        '1'.repeat(64),
        JSON.stringify(manifest),
        '/tmp/ersatzrs-addon-fixture',
        '2026-08-11T00:00:00Z',
        '2026-08-11T00:00:00Z',
      );
  } finally {
    database.close();
  }
}

test.beforeEach(async ({ request }) => {
  resetFixtures();
  seedInstalledAddon();
  seedLocalRepository();
  await request.post('/api/settings/ui/save', {
    form: { is_dark_mode: 'true', language: 'en-us' },
    maxRedirects: 0,
  });
});

test.afterEach(() => resetFixtures());

test('validates and round-trips the global item-detail pacing range', async ({ page }) => {
  await page.goto('/settings/addons');
  await waitForHydration(page);

  const settings = page.getByTestId('addon-detail-wait-settings');
  const minimum = settings.getByTestId('addon-detail-wait-minimum');
  const maximum = settings.getByTestId('addon-detail-wait-maximum');
  await expect(minimum).toHaveValue('2');
  await expect(maximum).toHaveValue('16');
  await expect(settings).toContainText('same provider host');

  await minimum.fill('12');
  await maximum.fill('11');
  await settings.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(
    page.locator('[data-toast-severity="error"]').filter({
      hasText: 'minimum does not exceed the maximum',
    }),
  ).toBeVisible();

  await minimum.fill('7');
  await maximum.fill('11');
  await settings.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(
    page.locator('[data-toast-severity="success"]').filter({ hasText: 'Saved.' }),
  ).toBeVisible();
  await expect
    .poll(() => {
      const database = openDatabase();
      try {
        return database
          .prepare(`
            SELECT "Key", "Value" FROM "ConfigElement"
            WHERE "Key" IN (?, ?) ORDER BY "Key"
          `)
          .all(
            'addons.detail_wait.minimum_seconds',
            'addons.detail_wait.maximum_seconds',
          ) as Array<{ Key: string; Value: string }>;
      } finally {
        database.close();
      }
    })
    .toEqual([
      { Key: 'addons.detail_wait.maximum_seconds', Value: '11' },
      { Key: 'addons.detail_wait.minimum_seconds', Value: '7' },
    ]);

  await page.reload();
  await waitForHydration(page);
  await expect(page.getByTestId('addon-detail-wait-minimum')).toHaveValue('7');
  await expect(page.getByTestId('addon-detail-wait-maximum')).toHaveValue('11');
});

test('shows local repository sources without remote catalog controls', async ({ page }) => {
  await page.goto('/settings/addons?view=sources');
  await waitForHydration(page);
  await expect(page.getByRole('heading', { name: 'Add-ons', exact: true })).toBeVisible();
  const row = page.getByTestId('addon-repository-row').filter({ hasText: 'Example add-ons' });
  await expect(row).toBeVisible();
  await expect(row).toContainText(FIXTURE_REPOSITORY_ID);
  await expect(row).toContainText('e2e-addon-repository');
  await expect(row).toContainText('Git checkout');
  await expect(page.getByRole('button', { name: 'Add repository' })).toHaveCount(0);
  await expect(page.getByTestId('addon-repository-editor')).toHaveCount(0);
  await expect(page.getByText('Add-on package', { exact: true })).toBeVisible();
  await expect(page.getByText('Repository package', { exact: true })).toBeVisible();
  await expect(page.locator('input[type="file"]')).toHaveCount(2);
  await expect(page.locator('input[type="file"]').first()).toBeDisabled();
  await expect(page.locator('input[type="file"]').last()).toBeDisabled();
});

test('removes an uploaded repository bundle while preserving its installed add-on', async ({ page }) => {
  test.skip(!APP_STATE_DIR, 'E2E_APP_STATE_DIR is required.');
  const sourceRoot = seedBundleRepository();

  await page.goto('/settings/addons?view=sources');
  await waitForHydration(page);
  const row = page
    .getByTestId('addon-repository-row')
    .filter({ hasText: 'Uploaded fixture repository' });
  await row.getByTestId('remove-addon-repository').click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toContainText('Uploaded fixture repository');
  await dialog.getByRole('button', { name: 'Delete', exact: true }).click();

  await expect(row).toHaveCount(0);
  await expect.poll(() => {
    const database = openDatabase();
    try {
      return {
        installation: database
          .prepare('SELECT "SourceId" FROM "AddonInstallation" WHERE "AddonId" = ?')
          .get(FIXTURE_ADDON_ID),
        offers: database
          .prepare('SELECT COUNT(*) AS count FROM "AddonSourceOffer" WHERE "SourceId" = ?')
          .get(BUNDLE_REPOSITORY_ID),
        source: database
          .prepare('SELECT COUNT(*) AS count FROM "AddonSource" WHERE "Id" = ?')
          .get(BUNDLE_REPOSITORY_ID),
      };
    } finally {
      database.close();
    }
  }).toEqual({
    installation: { SourceId: null },
    offers: { count: 0 },
    source: { count: 0 },
  });
  expect(existsSync(sourceRoot)).toBe(false);
});

test('removes an uploaded repository bundle without JavaScript', async ({ browser, baseURL }) => {
  test.skip(!APP_STATE_DIR, 'E2E_APP_STATE_DIR is required.');
  seedBundleRepository();
  const context = await browser.newContext({ javaScriptEnabled: false, baseURL });
  const page = await context.newPage();

  await page.goto('/settings/addons?view=sources');
  const row = page
    .getByTestId('addon-repository-row')
    .filter({ hasText: 'Uploaded fixture repository' });
  await row.getByTestId('remove-addon-repository').click();
  await expect(page).toHaveURL(/\/settings\/addons\?view=sources$/);
  await expect(row).toHaveCount(0);
  await context.close();

  const database = openDatabase();
  try {
    expect(
      database.prepare('SELECT COUNT(*) AS count FROM "AddonSource" WHERE "Id" = ?')
        .get(BUNDLE_REPOSITORY_ID),
    ).toEqual({ count: 0 });
    expect(
      database.prepare('SELECT "SourceId" FROM "AddonInstallation" WHERE "AddonId" = ?')
        .get(FIXTURE_ADDON_ID),
    ).toEqual({ SourceId: null });
  } finally {
    database.close();
  }
});

test('renders declarative panels and persists normal and secret-reference settings', async ({
  page,
}) => {
  await page.goto(`/settings/addons/${FIXTURE_ADDON_ID}`);
  await waitForHydration(page);
  await expect(page).toHaveURL(new RegExp(`/settings/addons/${FIXTURE_ADDON_ID}$`));
  await expect(page.getByTestId('addon-detail-page')).toContainText('Fixture add-on');
  await expect(page.getByText('Rendered by the ErsatzRS host.')).toBeVisible();

  const labelSetting = page
    .getByTestId('addon-setting-row')
    .filter({ hasText: 'Display label' });
  await labelSetting.locator('input').fill('Configured value');
  await Promise.all([
    page.waitForEvent('load'),
    labelSetting.getByRole('button', { name: 'Save' }).click(),
  ]);
  await waitForHydration(page);

  await expect
    .poll(() => {
      const database = openDatabase();
      try {
        return database
          .prepare('SELECT "Value" FROM "AddonSetting" WHERE "AddonId" = ? AND "Key" = ?')
          .get(FIXTURE_ADDON_ID, 'DISPLAY_LABEL') as { Value: string } | undefined;
      } finally {
        database.close();
      }
    })
    .toEqual({ Value: 'Configured value' });

  const secretSetting = page
    .getByTestId('addon-setting-row')
    .filter({ hasText: 'API token reference' });
  await secretSetting.getByRole('combobox').selectOption('environment');
  await secretSetting.locator('input').fill('EXAMPLE_ADDON_TOKEN');
  await Promise.all([
    page.waitForEvent('load'),
    secretSetting.getByRole('button', { name: 'Save' }).click(),
  ]);

  await expect
    .poll(() => {
      const database = openDatabase();
      try {
        return database
          .prepare(`
            SELECT "Value", "ReferenceKind", "Reference"
            FROM "AddonSetting" WHERE "AddonId" = ? AND "Key" = ?
          `)
          .get(FIXTURE_ADDON_ID, 'API_TOKEN') as
          | { Value: string | null; ReferenceKind: string; Reference: string }
          | undefined;
      } finally {
        database.close();
      }
    })
    .toEqual({ Value: null, ReferenceKind: 'environment', Reference: 'EXAMPLE_ADDON_TOKEN' });
});

test('opens the canonical detail page and confirms unknown sources', async ({ page }) => {
  await page.goto('/settings/addons');
  await waitForHydration(page);
  const card = page.getByTestId('installed-addon-row').filter({ hasText: 'Fixture add-on' });
  await card.getByRole('link', { name: 'Settings' }).click();
  await expect(page).toHaveURL(new RegExp(`/settings/addons/${FIXTURE_ADDON_ID}$`));
  await expect(page.getByRole('heading', { name: 'Fixture add-on' })).toBeVisible();
  await waitForHydration(page);

  await page.goto('/settings/addons?view=sources');
  await waitForHydration(page);
  await page.getByRole('button', { name: 'Enable', exact: true }).click();
  await expect(page.getByRole('dialog')).toContainText('Unknown sources');
  await page.getByRole('dialog').getByRole('button', { name: 'Confirm' }).click();
  await expect
    .poll(() => {
      const database = openDatabase();
      try {
        return database
          .prepare('SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?')
          .get('addons.allow_unverified_sources') as { Value: string } | undefined;
      } finally {
        database.close();
      }
    })
    .toEqual({ Value: 'true' });
});
