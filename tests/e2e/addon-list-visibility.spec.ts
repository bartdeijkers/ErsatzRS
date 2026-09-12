import { expect, Locator, Page, test } from '@playwright/test';
import { randomUUID } from 'crypto';
import { execFileSync } from 'child_process';
import { mkdirSync, writeFileSync } from 'fs';
import { dirname, join } from 'path';
import { addonContentHash, sqlEscape, sqlite } from './fixtures/addon-metadata';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;
const visibilityEndpoint = '/api/addon-lists/visibility';

function seedTransaction(sql: string) {
  if (!dbPath) throw new Error('An isolated E2E database is required');
  const transaction = `BEGIN;\n${sql}\nCOMMIT;`;
  try {
    // Without -bail, the CLI continues after a failed INSERT and could COMMIT
    // partial setup. Exit closes and rolls back this transaction on any error.
    execFileSync('sqlite3', ['-bail', dbPath], {
      input: `.timeout 10000\n${transaction}`, encoding: 'utf8',
    });
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
    // The shared helper's Python fallback stops executescript at the first
    // exception, closing the connection without committing partial setup.
    sqlite(transaction);
  }
}

function seedVisibility() {
  if (!dbPath) throw new Error('An isolated E2E database is required');
  const suffix = randomUUID().replace(/-/g, '');
  const id = 4_000_000_000 + Number.parseInt(suffix.slice(0, 8), 16) * 10;
  const addonId = `org.example.e2e.visibility.${suffix}`;
  const folder = join(dirname(dbPath), `visibility-${suffix}`);
  mkdirSync(folder);
  writeFileSync(join(folder, 'saved-content.txt'), 'Synthetic saved list content\n');
  const manifest = JSON.stringify({
    manifest_version: 5, id: addonId, version: '1.0.0',
    name: { 'en-US': 'Synthetic visibility provider' },
    summary: { 'en-US': 'Admin visibility regression fixture' },
    license: 'MIT', source_url: 'https://example.invalid/visibility', host_version: '*',
    capabilities: [{ id: 'addon.check.v1' }, { id: 'media-list.list.v6' }], dependencies: [],
    entrypoints: { any: { path: 'unused.sh', kind: 'posix_shell' } },
    permissions: { network_hosts: [], media_read: false, managed_ffmpeg: false, external_commands: [] },
    settings: [], panels: [],
  });
  // Disabled provider and future refresh dates keep the fixture inert. Different
  // saved/published revisions catch an accidental Save or Publish during hiding.
  seedTransaction(`
INSERT INTO "AddonInstallation" (
  "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256", "ManifestJson",
  "InstallPath", "InstalledAt", "UpdatedAt", "TrustTier"
) VALUES ('${addonId}', '1.0.0', 0, 'synthetic-package', 'synthetic-content',
  '${sqlEscape(manifest)}', 'synthetic-unused-installation', '2026-09-01T00:00:00Z',
  '2026-09-01T00:00:00Z', 'verified');
INSERT INTO "MediaSource" ("Id") VALUES (${id});
INSERT INTO "LocalMediaSource" ("Id") VALUES (${id});
INSERT INTO "Library" ("Id", "MediaSourceId", "MediaKind", "Name", "LastScan")
  VALUES (${id}, ${id}, 7, 'Synthetic visibility library ${suffix}', '2099-01-01T00:00:00Z');
INSERT INTO "LocalLibrary" ("Id") VALUES (${id});
INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path")
  VALUES (${id}, ${id}, '${sqlEscape(folder)}');
INSERT INTO "PlaylistGroup" ("Id", "Name", "IsSystem")
  VALUES (${id}, 'Visibility ${suffix}', 1);
INSERT INTO "Playlist" ("Id", "Name", "IsSystem", "PlaylistGroupId")
  VALUES (${id}, 'Visibility ${suffix}', 1, ${id});
INSERT INTO "AddonMediaList" (
  "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "ItemCount", "AutoRefresh",
  "RefreshCadence", "RefreshPublicationPolicy", "NextRefreshAt", "RefreshRetryAt",
  "LastRefreshSucceededAt", "GeneratePlaylist", "PlaylistId", "ReviewState",
  "PublishedRevision", "DraftRevision", "MetadataOverrideJson", "AdminHidden"
) VALUES
  (${id}, '${addonId}', 'target', 'https://example.invalid/visibility/target',
   'Synthetic visibility target ${suffix}', 2, 1, 0, 0, '2099-01-01T00:00:00Z',
   '2099-01-02T00:00:00Z', '2026-09-01T00:00:00Z', 1, ${id}, 1, 3, 7,
   '{"title":"Saved visibility draft"}', 0),
  (${id + 1}, '${addonId}', 'sibling', 'https://example.invalid/visibility/sibling',
   'Synthetic visibility sibling ${suffix}', 0, 0, 4, 0, NULL, NULL, NULL, 0, NULL, 1, 0, 1, '{}', 1);
UPDATE "AddonMediaList" SET "LastMatch"='2099-01-01T00:00:00Z' WHERE "Id" IN (${id}, ${id + 1});
INSERT INTO "AddonMediaListLibrary" ("AddonMediaListId", "LibraryId", "FolderName", "ManifestPath")
  VALUES (${id}, ${id}, 'visibility-${suffix}', '${sqlEscape(folder)}');
INSERT INTO "AddonMediaListItem" (
  "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind", "Rank", "Title",
  "SourceUrl", "Included", "Published", "MetadataOverrideJson"
) VALUES
  (${id}, ${id}, 'published', 'Synthetic published item', 4, 0, 'Synthetic published item',
   'https://example.invalid/visibility/published', 1, 1, '{}'),
  (${id + 1}, ${id}, 'excluded', 'Synthetic excluded item', 4, 1, 'Synthetic excluded item',
   'https://example.invalid/visibility/excluded', 0, 0, '{"season":2,"episode":3}');
INSERT INTO "AddonMediaListRefreshState" (
  "AddonMediaListId", "Generation", "NextEligibleAt", "UpdatedAt"
) VALUES (${id}, 2, '2099-01-01T00:00:00Z', '2026-09-01T00:00:00Z');
`);
  return { id, folder };
}

function retainedState(id: number, folder: string) {
  // Unrelated scans call match_all_lists and update LastMatch despite the future
  // timestamp. Native tests cover that timestamp's full visibility invariant;
  // this browser proof compares every other field except the visibility bit.
  const listColumns = sqlite('PRAGMA table_info("AddonMediaList");').split('\n')
    .map(line => line.split('|')[1]).filter(name => !['AdminHidden', 'LastMatch'].includes(name))
    .map(name => `"${name}"`).join(', ');
  return {
    list: sqlite(`SELECT ${listColumns} FROM "AddonMediaList" WHERE "Id"=${id};`),
    items: sqlite(`SELECT * FROM "AddonMediaListItem" WHERE "AddonMediaListId"=${id} ORDER BY "Id";`),
    library: sqlite(`SELECT * FROM "Library" WHERE "Id"=${id};`),
    path: sqlite(`SELECT * FROM "LibraryPath" WHERE "LibraryId"=${id};`),
    mapping: sqlite(`SELECT * FROM "AddonMediaListLibrary" WHERE "AddonMediaListId"=${id};`),
    playlist: sqlite(`SELECT * FROM "Playlist" WHERE "Id"=${id};`),
    refresh: sqlite(`SELECT * FROM "AddonMediaListRefreshState" WHERE "AddonMediaListId"=${id};`),
    operations: sqlite(`SELECT * FROM "ManagedLibraryOperation" WHERE "LibraryId"=${id} ORDER BY "Id";`),
    files: addonContentHash(folder),
  };
}

async function ready(page: Page, hydrated: boolean) {
  await expect(page.locator('#addon-list-visibility')).toBeVisible();
  await expect(page.locator('[data-not-found]')).toHaveCount(0);
  if (hydrated) {
    await expect.poll(() => page.evaluate(() =>
      (window as unknown as { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true,
    ), { timeout: 15_000 }).toBe(true);
  }
}

async function nativeSubmit(page: Page, button: Locator) {
  await Promise.all([
    page.waitForEvent('framenavigated', frame => frame === page.mainFrame()),
    button.click(),
  ]);
}

test.describe('add-on list admin visibility only', () => {
  test.describe.configure({ mode: 'serial' });
  test.skip(!dbPath, 'E2E_DB_PATH or ERSATZRS_DB_PATH is required');

  for (const javaScriptEnabled of [true, false]) {
    test(`hide and reveal preserve saved content (${javaScriptEnabled ? 'hydrated' : 'no JS'})`, async ({ browser, baseURL }) => {
      test.setTimeout(120_000);
      const { id, folder } = seedVisibility();
      const before = retainedState(id, folder);
      const context = await browser.newContext({ baseURL, javaScriptEnabled });
      const page = await context.newPage();
      const errors: string[] = [];
      page.on('pageerror', error => errors.push(error.message));
      page.on('console', message => {
        if (message.type() === 'error' && /hydration|failed_to_cast_marker_node|rust_panic/i.test(message.text())) {
          errors.push(message.text());
        }
      });
      const row = (listId: number) => page.locator('tbody tr').filter({
        has: page.locator(`a[href="/media/addon-lists/${listId}"]`),
      });
      const action = () => row(id).locator(`form[action="${visibilityEndpoint}"] button[type="submit"]`);
      const hidden = () => sqlite(`SELECT "AdminHidden" FROM "AddonMediaList" WHERE "Id"=${id};`);
      const filter = async (value: string) => {
        await page.locator('#addon-list-visibility').selectOption(value);
        const form = page.locator('form').filter({ has: page.locator('#addon-list-visibility') });
        await nativeSubmit(page, form.locator('button[type="submit"]'));
        await ready(page, javaScriptEnabled);
        await expect(page.locator('#addon-list-visibility')).toHaveValue(value);
      };
      const changeVisibility = async () => {
        if (javaScriptEnabled) {
          // Require the server-function request to prove the handler attached;
          // native form success alone could mask dead hydration.
          const response = page.waitForResponse(response =>
            response.request().method() === 'POST'
            && response.url().includes('/api/')
            && response.url().includes('set_addon_media_list_admin_hidden'),
          );
          await action().click();
          expect((await response).ok()).toBe(true);
        } else {
          await nativeSubmit(page, action());
          await ready(page, false);
        }
      };
      const trigger = `visibility_failure_${id}`;
      try {
        await page.goto('/media/addon-lists?page_size=0');
        await ready(page, javaScriptEnabled);
        await expect(row(id)).toBeVisible();
        await expect(row(id + 1)).toHaveCount(0);
        await changeVisibility();
        await expect.poll(hidden).toBe('1');
        await expect(row(id)).toHaveCount(0);
        // The first isolated scenario hides the final visible list. A full-suite
        // run may also contain other specs' lists, which this test leaves alone.
        if (sqlite('SELECT COUNT(*) FROM "AddonMediaList" WHERE "AdminHidden"=0;') === '0') {
          await expect(page.getByText('No visible lists.', { exact: true })).toBeVisible();
        }
        expect(retainedState(id, folder)).toEqual(before);

        await page.reload();
        await ready(page, javaScriptEnabled);
        await expect(row(id)).toHaveCount(0);
        await filter('true');
        await expect(row(id)).toBeVisible();
        await expect(row(id + 1)).toBeVisible();
        await page.reload();
        await ready(page, javaScriptEnabled);
        await expect(row(id)).toBeVisible();

        const idSort = page.locator('th form').filter({ has: page.locator('input[name="sort"][value="id"]') });
        if (javaScriptEnabled) await idSort.locator('button').click();
        else await nativeSubmit(page, idSort.locator('button'));
        await ready(page, javaScriptEnabled);
        await expect(page.locator('#addon-list-visibility')).toHaveValue('true');
        await expect(page.locator('th[aria-sort="ascending"]').filter({
          has: page.locator('input[name="sort"][value="id"]'),
        })).toBeVisible();
        await expect(row(id)).toBeVisible();
        const pageSize = page.locator('[data-pagination-page-size]:visible');
        await pageSize.selectOption('100');
        if (!javaScriptEnabled) {
          const sizeForm = page.locator('form').filter({ has: pageSize });
          await nativeSubmit(page, sizeForm.locator('button[type="submit"]'));
        }
        await ready(page, javaScriptEnabled);
        await expect(pageSize).toHaveValue('100');
        await expect(page.locator('#addon-list-visibility')).toHaveValue('true');
        await expect(row(id)).toBeVisible();

        // Desktop first: a preceding mobile focus must not scroll the actions
        // into view and accidentally conceal a clipped desktop action.
        for (const width of [1440, 390]) {
          await page.setViewportSize({ width, height: 900 });
          const visibilityFilter = page.locator('#addon-list-visibility');
          await visibilityFilter.scrollIntoViewIfNeeded();
          await expect(visibilityFilter).toBeInViewport({ ratio: 1 });
          const geometry = await page.evaluate(() => ({
            width: document.documentElement.clientWidth,
            scroll: document.documentElement.scrollWidth,
          }));
          expect(geometry.scroll, `hidden lists at ${width}px: page overflow`)
            .toBeLessThanOrEqual(geometry.width + 1);
          if (width === 1440) {
            await expect(action()).toBeInViewport({ ratio: 1 });
          }
          await page.screenshot({
            path: test.info().outputPath(`hidden-lists-${width}.png`),
            animations: 'disabled',
          });
          if (width === 390) {
            const rowBounds = await row(id).boundingBox();
            expect(rowBounds).not.toBeNull();
            expect(rowBounds!.height, 'provider identity must not wrap into a one-character column')
              .toBeLessThan(180);
            await action().focus();
            await action().scrollIntoViewIfNeeded();
            await expect(action()).toBeFocused();
            await expect(action()).toBeInViewport({ ratio: 1 });
            await page.screenshot({
              path: test.info().outputPath('hidden-lists-390-actions.png'),
              animations: 'disabled',
            });
          }
        }

        if (!javaScriptEnabled) {
          sqlite(`CREATE TRIGGER "${trigger}" BEFORE UPDATE OF "AdminHidden" ON "AddonMediaList"
WHEN OLD."Id"=${id} BEGIN SELECT RAISE(ABORT, 'synthetic visibility failure'); END;`);
          await changeVisibility();
          await expect(page).toHaveURL(/hidden=true.*error=visibility/);
          await expect(page.locator('[role="alert"]:visible')).toContainText(/\S/);
          await expect(row(id)).toBeVisible();
          expect(hidden()).toBe('1');
          expect(retainedState(id, folder)).toEqual(before);
          sqlite(`DROP TRIGGER "${trigger}";`);
        }

        await changeVisibility();
        await expect.poll(hidden).toBe('0');
        await expect(row(id)).toHaveCount(0);
        expect(retainedState(id, folder)).toEqual(before);
        await filter('false');
        // Native visibility redirects may use the saved default page size;
        // other specs can have enough normal lists to put this fixture off-page.
        await page.goto('/media/addon-lists?page_size=0');
        await ready(page, javaScriptEnabled);
        await expect(row(id)).toBeVisible();
        await expect(row(id + 1)).toHaveCount(0);
        await page.reload();
        await ready(page, javaScriptEnabled);
        await expect(row(id)).toBeVisible();
        expect(retainedState(id, folder)).toEqual(before);
        if (!javaScriptEnabled) {
          const legacy = await page.request.post('/api/addon-lists/delete', {
            form: { id: String(id) }, maxRedirects: 0,
          });
          expect(legacy.status()).toBe(303);
          expect(legacy.headers().location).toBe('/media/addon-lists');
          expect(hidden()).toBe('1');
          expect(retainedState(id, folder)).toEqual(before);
          await page.goto('/media/addon-lists?page_size=0');
          await ready(page, false);
          await expect(row(id)).toHaveCount(0);
          await filter('true');
          await expect(row(id)).toBeVisible();
        }
        expect(errors).toEqual([]);
      } finally {
        sqlite(`DROP TRIGGER IF EXISTS "${trigger}";`);
        await context.close();
      }
    });
  }
});
