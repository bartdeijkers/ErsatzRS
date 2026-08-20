import { expect, test, type Page } from '@playwright/test';
import { DatabaseSync } from 'node:sqlite';

const DATABASE_PATH = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;
const FIXTURE_ADDON_ID = 'org.example.addon.fixture';
const FIXTURE_REPOSITORY_ID = 'org.example.addons';

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
    database.prepare('DELETE FROM "AddonRepository" WHERE "Id" = ?').run(FIXTURE_REPOSITORY_ID);
    database
      .prepare('DELETE FROM "ConfigElement" WHERE "Key" = ?')
      .run('addons.allow_unverified_sources');
  } finally {
    database.close();
  }
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
  await request.post('/api/settings/ui/save', {
    form: { is_dark_mode: 'true', language: 'en-us' },
    maxRedirects: 0,
  });
});

test.afterEach(() => resetFixtures());

test('manages trusted repositories through the hydrated settings page', async ({ page }) => {
  await page.goto('/settings/addons?view=sources');
  await waitForHydration(page);
  await expect(page.getByRole('heading', { name: 'Add-ons', exact: true })).toBeVisible();
  await expect(page.getByTestId('addon-repository-row').filter({ hasText: 'Official ErsatzRS add-ons' })).toBeVisible();

  await page.locator('button').filter({ hasText: 'Add repository' }).click();
  const editor = page.getByTestId('addon-repository-editor');
  await editor.locator('#addon-repository-name').fill('Example add-ons');
  await editor.locator('#addon-repository-url').fill('https://example.test/addons/index-v1.json');
  await editor.locator('#addon-repository-id').fill(FIXTURE_REPOSITORY_ID);
  await editor
    .locator('#addon-repository-key')
    .fill('11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo=');
  // The editor is a dialog now, so Save lives in the dialog footer beside
  // Cancel rather than inside the field group.
  await page.getByRole('dialog').getByRole('button', { name: 'Save', exact: true }).click();

  const row = page.getByTestId('addon-repository-row').filter({ hasText: 'Example add-ons' });
  await expect(row).toBeVisible();
  await expect(row).toContainText('Never');

  // Editing an existing repository keeps its identity: the id is shown but not
  // editable, and the dialog says which flow the operator is in.
  await row.getByRole('button', { name: 'Edit' }).click();
  const editDialog = page.getByRole('dialog');
  await expect(editDialog.getByRole('heading', { name: 'Edit repository' })).toBeVisible();
  await expect(editor.locator('#addon-repository-id')).toHaveAttribute('readonly', '');
  await editDialog.getByRole('button', { name: 'Cancel', exact: true }).click();

  await row.getByRole('button', { name: 'Delete' }).click();
  const deleteDialog = page.getByRole('dialog');
  await expect(deleteDialog).toContainText('Example add-ons');
  await deleteDialog.getByRole('button', { name: 'Confirm' }).click();
  await expect(row).toHaveCount(0);
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
