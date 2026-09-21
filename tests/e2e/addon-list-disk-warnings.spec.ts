import { expect, Page, test } from '@playwright/test';
import { randomUUID } from 'crypto';
import { execFileSync } from 'child_process';
import { waitForHydration } from './hydration';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

// Run through scripts/e2e/playwright-e2e.sh (one worker): both browser projects
// edit the same combined-list setting in the isolated test database.

function sqlite(sql: string): string {
  if (!dbPath) throw new Error('The isolated E2E database path is required');
  const input = `.timeout 10000\n${sql}`;
  try {
    return execFileSync('sqlite3', [dbPath], { input, encoding: 'utf8' }).trim();
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
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

function seedLists(): [number, number] {
  const suffix = randomUUID().replace(/-/g, '');
  const first = 3_000_000_000 + Number.parseInt(suffix.slice(0, 8), 16) * 10;
  // Disabled synthetic provider: this settings fixture never invokes an add-on,
  // imports a list, downloads media, or starts playback.
  const addonId = `org.example.e2e.disk-warning.${suffix}`;
  const manifest = JSON.stringify({
    manifest_version: 5, id: addonId, version: '1.0.0',
    name: { 'en-US': 'Synthetic disk warning provider' },
    summary: { 'en-US': 'Advisory settings fixture' },
    license: 'MIT', source_url: 'https://example.invalid/disk-warning-fixture', host_version: '*',
    capabilities: [{ id: 'addon.check.v1' }, { id: 'media-list.list.v6' }], dependencies: [],
    entrypoints: { any: { path: 'unused.sh', kind: 'posix_shell' } },
    permissions: { network_hosts: [], media_read: false, managed_ffmpeg: false, external_commands: [] },
    settings: [], panels: [],
  });
  sqlite(`
INSERT INTO "AddonInstallation" (
  "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256", "ManifestJson",
  "InstallPath", "InstalledAt", "UpdatedAt", "TrustTier"
) VALUES ('${addonId}', '1.0.0', 0, 'synthetic-unused-package', 'synthetic-unused-content',
  '${manifest}', 'synthetic-unused-installation', '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z', 'verified');
INSERT INTO "AddonMediaList" (
  "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "ItemCount", "AutoRefresh",
  "RefreshCadence", "GeneratePlaylist", "ReviewState", "PresentationKind",
  "RequestedPresentationKind", "PublishedRevision", "DraftRevision"
) VALUES
  (${first}, '${addonId}', 'first', 'https://example.invalid/disk-warning-first',
   'Synthetic disk warning first', 0, 0, 4, 0, 1, 0, 0, 0, 7),
  (${first + 1}, '${addonId}', 'second', 'https://example.invalid/disk-warning-second',
   'Synthetic disk warning second', 0, 0, 4, 0, 1, 0, 0, 0, 9);
`);
  return [first, first + 1];
}

async function save(page: Page, value: string, javaScriptEnabled: boolean): Promise<void> {
  const workspace=await page.locator('.addon-workspace').count()>0;
  const form=workspace?page.locator('.storage-settings'):page.getByTestId('addon-list-disk-warning-settings');
  await form.locator('input[type="number"]').fill(value);
  await Promise.all([page.waitForEvent('framenavigated',f=>f===page.mainFrame()),workspace?page.locator('button[name="intent"][value="save"]').click():form.locator('button[type="submit"]').click()]);
  if (javaScriptEnabled) await waitForHydration(page);
  await expect(page.locator('#workspace-errors')).toHaveCount(0);
  await page.reload();
  if (javaScriptEnabled) await waitForHydration(page);
  await expect(form.locator('input[type="number"]')).toHaveValue(value);
}

test.describe('advisory add-on list disk warnings', () => {
  test.describe.configure({ mode: 'serial' });
  test.skip(!dbPath, 'E2E_DB_PATH or ERSATZRS_DB_PATH is required');
  for (const javaScriptEnabled of [true, false]) {
    test(`threshold save and independent disable (${javaScriptEnabled ? 'hydrated' : 'no JS'})`, async ({ browser, baseURL }) => {
      test.setTimeout(90_000);
      const [first, second] = seedLists();
      const revisionSql = `SELECT "Id", "DraftRevision", "PublishedRevision" FROM "AddonMediaList" WHERE "Id" IN (${first}, ${second}) ORDER BY "Id"`;
      const before = sqlite(revisionSql);
      const context = await browser.newContext({ baseURL, javaScriptEnabled });
      const page = await context.newPage();
      const runtimeErrors: string[] = [];
      page.on('pageerror', (error) => {
        const text = error.stack || error.message;
        // Native form navigations cancel the surrounding editor's pending reads.
        if (!(text.includes('AbortError') && text.toLowerCase().includes('operation was aborted'))
          && !text.includes('NetworkError when attempting to fetch resource')) {
          runtimeErrors.push(text);
        }
      });
      page.on('console', (message) => {
        if (message.type() === 'error' && /hydration|failed_to_cast_marker_node|rust_panic/i.test(message.text())) {
          runtimeErrors.push(message.text());
        }
      });
      const input = page.locator('input[name="warning_gb"], input[name="disk_warning_gb"]');
      const visit = async (id?: number) => {
        await page.goto(id ? `/media/addon-lists/${id}/playback` : '/media/addon-lists');
        await expect(page.locator('[data-not-found]')).toHaveCount(0);
        await expect(input).toBeVisible();
        if (javaScriptEnabled) {
          await expect.poll(() => page.evaluate(() =>
            (window as unknown as { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true,
          )).toBe(true);
        }
      };
      let combinedBefore: string | undefined;
      try {
        await visit();
        combinedBefore = await input.inputValue();
        expect(combinedBefore).toBe('250');
        await expect(input).toHaveAttribute('min', '0');
        await expect(input).toHaveAttribute('max', '4294967295');
        await expect(input).toHaveAttribute('step', '1');
        for (const width of [390, 1440]) {
          await page.setViewportSize({ width, height: 900 });
          const bounds = await page.getByTestId('addon-list-disk-warning-settings').boundingBox();
          expect(bounds).not.toBeNull();
          expect(bounds!.height).toBeLessThan(600);
          expect(bounds!.x).toBeGreaterThanOrEqual(0);
          expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(width + 1);
          await page.getByTestId('addon-list-disk-warning-settings').screenshot({
            path: test.info().outputPath(`disk-warning-${width}.png`),
          });
        }
        await save(page, '300', javaScriptEnabled);
        await visit(first);
        await expect(input).toHaveValue('25');
        await save(page, '30', javaScriptEnabled);
        await visit(second);
        await expect(input).toHaveValue('25');
        await visit(first);
        await save(page, '0', javaScriptEnabled);
        await visit();
        await expect(input).toHaveValue('300');
        await save(page, '0', javaScriptEnabled);
        await visit(second);
        await expect(input).toHaveValue('25');
        await save(page, '40', javaScriptEnabled);
        await visit();
        await expect(input).toHaveValue('0');
        await visit(first);
        await expect(input).toHaveValue('0');
        const invalid = await page.request.post('/api/addon-lists/disk-warning', {
          form: { list_id: String(first), warning_gb: '-1' }, maxRedirects: 0,
        });
        expect(invalid.status()).toBe(303);
        expect(invalid.headers().location).toContain('disk_warning=error');
        await page.goto(invalid.headers().location);
        if (javaScriptEnabled) await waitForHydration(page);
        await expect(page.locator('#workspace-errors')).toBeVisible();
        await expect(input).toHaveValue('0');
        expect(sqlite(revisionSql)).toBe(before);
        expect(runtimeErrors).toEqual([]);
      } finally {
        if (combinedBefore !== undefined) {
          await page.request.post('/api/addon-lists/disk-warning', {
            form: { list_id: '', warning_gb: combinedBefore },
          });
        }
        await context.close();
      }
    });
  }
});
