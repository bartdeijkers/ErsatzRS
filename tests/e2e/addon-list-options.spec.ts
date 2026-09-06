import { expect, Locator, Page, test } from '@playwright/test';
import { randomUUID } from 'crypto';
import { execFileSync } from 'child_process';
import { chmodSync, mkdirSync, writeFileSync } from 'fs';
import { dirname, join } from 'path';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

function sqlite(sql: string): string {
  if (!dbPath) throw new Error('The isolated E2E database path is required');
  const input = `.timeout 10000\n${sql}`;
  try {
    return execFileSync('sqlite3', [dbPath], { input, encoding: 'utf8' }).trim();
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
    // Match the existing metadata-editor fixture's Windows SQLite fallback.
    return execFileSync('python', ['-c', [
      'import sqlite3, sys',
      'connection = sqlite3.connect(sys.argv[1], timeout=10)',
      'statement = sys.stdin.read().split("\\n", 1)[1].strip()',
      'if statement.upper().startswith("SELECT") and statement.count(";") <= 1:',
      '    for row in connection.execute(statement).fetchall():',
      '        print("|".join("" if value is None else str(value) for value in row))',
      'else:',
      '    connection.executescript(statement)',
      'connection.commit()',
      'connection.close()',
    ].join('\n'), dbPath], { input, encoding: 'utf8' }).trim();
  }
}

function quote(value: string): string {
  return `'${value.replace(/'/g, "''")}'`;
}

function seedOptions() {
  if (!dbPath) throw new Error('The isolated E2E database path is required');
  // Every invocation has its own numeric and provider namespace. No reseeding
  // or deletion can collide with another spec's live background operation.
  const suffix = randomUUID().replace(/-/g, '');
  const base = 2_000_000_000 + Number.parseInt(suffix.slice(0, 8), 16) * 10;
  const listId = base + 1;
  const firstItemId = base + 2;
  const secondItemId = base + 3;
  const revisionId = base + 4;
  const addonId = `org.example.e2e.options.${suffix}`;
  const packageRoot = join(dirname(dbPath), `e2e-addon-options-${suffix}`);
  mkdirSync(packageRoot, { recursive: true });
  // Only the mandatory local readiness check succeeds. Provider operations fail
  // closed; this fixture never opens browser profiles or performs network work.
  writeFileSync(join(packageRoot, 'addon.sh'), `#!/bin/sh\nif [ "\${1:-}" = "check" ]; then\n  printf '%s\\n' '{"status":"ready","code":"ready","message":"Synthetic option fixture is ready."}'\n  exit 0\nfi\nexit 1\n`);
  chmodSync(join(packageRoot, 'addon.sh'), 0o755);
  writeFileSync(join(packageRoot, 'addon.cmd'), [
    '@echo off',
    'if /i "%~1"=="check" (',
    '  echo {"status":"ready","code":"ready","message":"Synthetic option fixture is ready."}',
    '  exit /b 0',
    ')',
    'exit /b 1',
    '',
  ].join('\r\n'));
  const manifest = {
    manifest_version: 5, id: addonId, version: '1.0.0',
    name: { 'en-US': 'Synthetic option provider' },
    summary: { 'en-US': 'Native draft option fixture' },
    license: 'MIT', source_url: 'https://example.invalid/options-fixture', host_version: '*',
    capabilities: [
      { id: 'addon.check.v1' },
      { id: 'media-list.list.v6' },
      { id: 'media-list.options.v1', options: [
        {
          key: 'browser', label: { 'en-US': 'Browser' },
          description: { 'en-US': 'Synthetic browser selection; no profiles are opened.' },
          scope: 'both', operation_targets: ['playback'], applicability: 'all',
          kind: 'enum', default: 'none',
          choices: ['none', 'firefox', 'chrome', 'chromium'].map((value) => ({
            value, label: { 'en-US': value === 'none' ? 'None' : value },
          })),
        },
        {
          key: 'sponsorblock_remove', label: { 'en-US': 'Remove SponsorBlock segments' },
          description: { 'en-US': 'Prepare finite media before playback.' },
          scope: 'both', operation_targets: ['preparation', 'playback'],
          applicability: 'finite_only', kind: 'boolean', default: true,
          requires_preparation: true,
        },
      ] },
    ],
    dependencies: [],
    entrypoints: {
      any: { path: 'addon.sh', kind: 'posix_shell' },
      'win-x64': { path: 'addon.cmd', kind: 'windows_batch' },
    },
    permissions: { network_hosts: [], media_read: false, managed_ffmpeg: false, external_commands: [] },
    settings: [], panels: [],
  };
  sqlite(`
INSERT INTO "AddonInstallation" (
  "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256", "ManifestJson",
  "InstallPath", "InstalledAt", "UpdatedAt", "TrustTier"
) VALUES (${quote(addonId)}, '1.0.0', 1, 'synthetic-unused-package', 'synthetic-unused-content',
  ${quote(JSON.stringify(manifest))}, ${quote(packageRoot)},
  '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z', 'verified');
INSERT INTO "AddonMediaList" (
  "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "ItemCount", "AutoRefresh",
  "RefreshCadence", "GeneratePlaylist", "ReviewState", "PresentationKind",
  "RequestedPresentationKind", "PublishedRevision"
) VALUES (${listId}, ${quote(addonId)}, ${quote(`options-${suffix}`)},
  'https://example.invalid/options-list', 'Synthetic options list', 2, 0, 4, 0, 1, 0, 0, 1);
INSERT INTO "AddonMediaListItem" (
  "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind", "Rank", "Title",
  "SourceUrl", "DurationSeconds", "Availability", "ContentKind", "Included", "Published",
  "ProviderLiveness", "EffectiveLiveness"
) VALUES
  (${firstItemId}, ${listId}, 'first', 'First option item', 4, 0, 'First option item',
   'https://example.invalid/options-first', 60, 0, 0, 1, 1, 1, 1),
  (${secondItemId}, ${listId}, 'second', 'Second option item', 4, 1, 'Second option item',
   'https://example.invalid/options-second', 60, 0, 0, 1, 1, 1, 1);
INSERT INTO "AddonMediaListRevision" (
  "Id", "AddonMediaListId", "Revision", "SourceDraftRevision", "OptionValuesJson"
) VALUES (${revisionId}, ${listId}, 1, 0, '{"browser":"none","sponsorblock_remove":false}');
INSERT INTO "AddonMediaListRevisionItemOption" (
  "AddonMediaListRevisionId", "ProviderId", "OptionOverridesJson"
) VALUES (${revisionId}, 'first', '{}'), (${revisionId}, 'second', '{}');
`);
  return { listId, firstItemId, secondItemId, revisionId };
}

type Seed = ReturnType<typeof seedOptions>;

function publishedSnapshot(seed: Seed): string {
  return sqlite(`SELECT "list"."PublishedRevision" || char(31) || "revision"."OptionValuesJson" || char(31) ||
    (SELECT group_concat("ProviderId" || ':' || "OptionOverridesJson", ';') FROM
      (SELECT * FROM "AddonMediaListRevisionItemOption" WHERE "AddonMediaListRevisionId" = ${seed.revisionId} ORDER BY "ProviderId"))
    FROM "AddonMediaList" AS "list" JOIN "AddonMediaListRevision" AS "revision"
      ON "revision"."Id" = ${seed.revisionId} WHERE "list"."Id" = ${seed.listId}`);
}

function defaults(seed: Seed): Record<string, unknown> {
  return JSON.parse(sqlite(`SELECT "DraftOptionValuesJson" FROM "AddonMediaList" WHERE "Id" = ${seed.listId}`));
}

function overrides(itemId: number): Record<string, unknown> {
  return JSON.parse(sqlite(`SELECT "DraftOptionOverridesJson" FROM "AddonMediaListItem" WHERE "Id" = ${itemId}`));
}

function option(page: Page, key: string): Locator {
  return page.getByTestId('addon-list-options').locator(`form[data-option-key="${key}"]`);
}

async function save(page: Page, form: Locator): Promise<void> {
  await form.locator('button[value="save"]').click();
  await expect(page.getByTestId('addon-options-saved')).toBeVisible();
}

async function reset(page: Page, form: Locator): Promise<void> {
  await form.locator('button[value="reset"]').click();
  await expect(page.getByTestId('addon-options-saved')).toBeVisible();
}

test.describe('add-on list draft options', () => {
  test.skip(!dbPath, 'E2E_DB_PATH or ERSATZRS_DB_PATH is required');

  for (const javaScriptEnabled of [true, false]) {
    test(`inheritance, published isolation and bulk undo (${javaScriptEnabled ? 'hydrated' : 'no JS'})`, async ({ browser, baseURL }) => {
      test.setTimeout(90_000);
      const seed = seedOptions();
      const published = publishedSnapshot(seed);
      expect(published).toContain('"sponsorblock_remove":false');
      const context = await browser.newContext({ baseURL, javaScriptEnabled });
      const page = await context.newPage();
      const runtimeErrors: string[] = [];
      page.on('pageerror', (error) => {
        const message = error.message;
        // Native form navigations abort the surrounding editor's in-flight reads.
        if (!(message.includes('AbortError') && message.toLowerCase().includes('operation was aborted'))
          && !message.includes('NetworkError when attempting to fetch resource')) {
          runtimeErrors.push(message);
        }
      });
      page.on('console', (message) => {
        if (message.type() === 'error' && /hydration|failed_to_cast_marker_node|rust_panic/i.test(message.text())) {
          runtimeErrors.push(message.text());
        }
      });
      const listUrl = `/media/addon-lists/${seed.listId}`;
      const firstUrl = `${listUrl}/items/${seed.firstItemId}`;
      const visit = async (url: string) => {
        await page.goto(url);
        await expect(page.locator('[data-not-found]')).toHaveCount(0);
        await expect(page.getByTestId('addon-list-options')).toBeVisible();
        if (javaScriptEnabled) {
          await expect.poll(() => page.evaluate(() =>
            (window as unknown as { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true,
          )).toBe(true);
        }
      };
      try {
        await page.request.post('/api/settings/ui/save', {
          form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0,
        });
        await visit(listUrl);
        await expect(option(page, 'browser').locator('select')).toHaveValue('"none"');
        await expect(option(page, 'sponsorblock_remove').locator('input[type="checkbox"]')).toBeChecked();
        expect(defaults(seed)).toEqual({}); // Displaying checked package defaults is read-only.
        expect(sqlite(`SELECT COALESCE("DraftOptionSchemaId", 0) FROM "AddonMediaList" WHERE "Id" = ${seed.listId}`)).toBe('0');
        await option(page, 'browser').locator('select').selectOption('"firefox"');
        await save(page, option(page, 'browser'));
        expect(defaults(seed)).toEqual({ browser: 'firefox' });
        expect(publishedSnapshot(seed)).toBe(published);

        await visit(firstUrl);
        await expect(option(page, 'browser')).toContainText('Inherited');
        await expect(option(page, 'browser').locator('select')).toHaveValue('"firefox"');
        await expect(option(page, 'sponsorblock_remove').locator('input[type="checkbox"]')).toBeChecked();
        await option(page, 'browser').locator('select').selectOption('"chrome"');
        await save(page, option(page, 'browser'));
        expect(overrides(seed.firstItemId)).toEqual({ browser: 'chrome' });
        await visit(firstUrl);
        await expect(option(page, 'browser')).toContainText('Override');
        await reset(page, option(page, 'browser'));
        expect(overrides(seed.firstItemId)).toEqual({});
        await visit(firstUrl);
        await expect(option(page, 'browser').locator('select')).toHaveValue('"firefox"');
        await expect(option(page, 'browser')).toContainText('Inherited');

        await option(page, 'sponsorblock_remove').locator('input[type="checkbox"]').uncheck();
        await save(page, option(page, 'sponsorblock_remove'));
        expect(overrides(seed.firstItemId)).toEqual({ sponsorblock_remove: false });
        await visit(firstUrl);
        await expect(option(page, 'sponsorblock_remove').locator('input[type="checkbox"]')).not.toBeChecked();
        await reset(page, option(page, 'sponsorblock_remove'));
        expect(overrides(seed.firstItemId)).toEqual({});
        await visit(firstUrl);
        await expect(option(page, 'sponsorblock_remove').locator('input[type="checkbox"]')).toBeChecked();
        await option(page, 'browser').locator('select').selectOption('"chrome"');
        await save(page, option(page, 'browser'));

        await visit(listUrl);
        const bulk = page.locator('form[action="/api/addon-lists/options/edit"]');
        await bulk.locator('select[name="scope"]').selectOption({ label: 'All items' });
        await bulk.locator('button[type="submit"]').click();
        await expect(page.getByTestId('addon-list-options')).toContainText('Second option item');
        await expect(option(page, 'browser').locator('select')).toHaveValue('');
        await option(page, 'browser').locator('select').selectOption('"chromium"');
        await option(page, 'browser').locator('button[value="save"]').click();
        const preview = page.getByTestId('addon-options-preview');
        await expect(preview).toBeVisible();
        await expect(preview.locator('tbody tr')).toHaveCount(2);
        expect(overrides(seed.firstItemId)).toEqual({ browser: 'chrome' });
        expect(overrides(seed.secondItemId)).toEqual({});
        await preview.getByRole('button', { name: 'Apply', exact: true }).click();
        await expect(page.getByTestId('addon-options-saved')).toBeVisible();
        expect(overrides(seed.firstItemId)).toEqual({ browser: 'chromium' });
        expect(overrides(seed.secondItemId)).toEqual({ browser: 'chromium' });
        expect(publishedSnapshot(seed)).toBe(published);
        await page.getByRole('button', { name: 'Undo last change', exact: true }).click();
        await expect(page.getByRole('status')).toHaveText('Change undone');
        expect(overrides(seed.firstItemId)).toEqual({ browser: 'chrome' });
        expect(overrides(seed.secondItemId)).toEqual({});
        expect(defaults(seed)).toEqual({ browser: 'firefox' });
        await visit(listUrl);
        await reset(page, option(page, 'browser'));
        expect(defaults(seed)).toEqual({});
        await visit(firstUrl);
        await expect(option(page, 'browser').locator('select')).toHaveValue('"chrome"');
        await visit(`${listUrl}/items/${seed.secondItemId}`);
        await expect(option(page, 'browser').locator('select')).toHaveValue('"none"');
        expect(publishedSnapshot(seed)).toBe(published);
        expect(runtimeErrors).toEqual([]);
      } finally {
        await context.close();
      }
    });
  }
});
