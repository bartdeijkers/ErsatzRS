import { expect, Page, TestInfo, test } from '@playwright/test';
import { execFileSync } from 'child_process';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

function uniqueName(prefix: string): string {
  return `${prefix} ${Date.now()} ${Math.floor(Math.random() * 10_000)}`;
}

function sqlEscape(value: string): string {
  return value.replace(/'/g, "''");
}

function sqlite(sql: string): string {
  if (!dbPath) {
    throw new Error('E2E_DB_PATH or ERSATZRS_DB_PATH is required for remote-stream seeding');
  }
  return execFileSync('sqlite3', [dbPath], {
    input: sql,
    encoding: 'utf8',
  }).trim();
}

function seedRemoteStreams(): void {
  sqlite(`
DELETE FROM "CollectionItem" WHERE "MediaItemId" IN (90701, 90801);
DELETE FROM "RemoteStreamMetadata" WHERE "RemoteStreamId" IN (90701, 90801);
DELETE FROM "RemoteStream" WHERE "Id" IN (90701, 90801);
DELETE FROM "MediaItem" WHERE "Id" IN (90701, 90801);
DELETE FROM "LibraryPath" WHERE "Id" IN (9070, 9080);
DELETE FROM "LocalLibrary" WHERE "Id" IN (907, 908);
DELETE FROM "Library" WHERE "Id" IN (907, 908);
DELETE FROM "LocalMediaSource" WHERE "Id" IN (9070, 9080);
DELETE FROM "MediaSource" WHERE "Id" IN (9070, 9080);

INSERT OR IGNORE INTO "MediaSource" ("Id") VALUES (9070);
INSERT OR IGNORE INTO "LocalMediaSource" ("Id") VALUES (9070);
INSERT OR IGNORE INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
  VALUES (907, 'E2E Remote Streams', 7, 9070);
INSERT OR IGNORE INTO "LocalLibrary" ("Id") VALUES (907);
INSERT OR IGNORE INTO "LibraryPath" ("Id", "Path", "LibraryId")
  VALUES (9070, '/e2e/remote-streams', 907);
INSERT OR IGNORE INTO "MediaItem" ("Id", "LibraryPathId", "State")
  VALUES (90701, 9070, 0);
INSERT OR IGNORE INTO "RemoteStream" ("Id", "Url", "Script", "IsLive")
  VALUES (90701, 'https://example.test/nurburgring.m3u8', NULL, 1);
INSERT OR IGNORE INTO "RemoteStreamMetadata" (
  "RemoteStreamId", "MetadataKind", "Title", "OriginalTitle",
  "SortTitle", "DateAdded", "DateUpdated"
) VALUES (
  90701, 0, 'Nurburgring broadcast', 'Racing',
  'nurburgring broadcast', '2026-06-14T00:00:00Z', '2026-06-14T00:00:00Z'
);

INSERT OR IGNORE INTO "MediaSource" ("Id") VALUES (9080);
INSERT OR IGNORE INTO "LocalMediaSource" ("Id") VALUES (9080);
INSERT OR IGNORE INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
  VALUES (908, 'E2E Other Remote Streams', 7, 9080);
INSERT OR IGNORE INTO "LocalLibrary" ("Id") VALUES (908);
INSERT OR IGNORE INTO "LibraryPath" ("Id", "Path", "LibraryId")
  VALUES (9080, '/e2e/other-remote-streams', 908);
INSERT OR IGNORE INTO "MediaItem" ("Id", "LibraryPathId", "State")
  VALUES (90801, 9080, 0);
INSERT OR IGNORE INTO "RemoteStream" ("Id", "Url", "Script", "IsLive")
  VALUES (90801, 'https://example.test/visdeurbel.m3u8', NULL, 1);
INSERT OR IGNORE INTO "RemoteStreamMetadata" (
  "RemoteStreamId", "MetadataKind", "Title", "OriginalTitle",
  "SortTitle", "DateAdded", "DateUpdated"
) VALUES (
  90801, 0, 'Visdeurbel', 'Vis',
  'visdeurbel', '2026-06-14T00:00:00Z', '2026-06-14T00:00:00Z'
);
`);
}

function screenshotPath(testInfo: TestInfo, name: string): string {
  return `test-results/${name}-${testInfo.project.name}.png`;
}

async function gotoRemoteStreams(page: Page): Promise<void> {
  await page.goto('/media/remote/streams?query=library_id:907');
  await expect(page.getByRole('heading', { name: 'Remote Streams' })).toBeVisible();
  await expect(page.getByText('Nurburgring broadcast').first()).toBeVisible();
}

async function clickFirstCardAddToCollection(page: Page): Promise<void> {
  const itemMenu = page.locator('details[class*="item-menu"]').first();
  const isOpen = await itemMenu.evaluate((node) => (node as HTMLDetailsElement).open);
  if (!isOpen) {
    await itemMenu.locator('summary[aria-label="Item actions"]').click();
  }
  await itemMenu.getByRole('menuitem', { name: /^add to collection$/i }).click();
}

test.describe('remote streams browser parity', () => {
  test.skip(!dbPath, 'E2E_DB_PATH or ERSATZRS_DB_PATH is required to seed remote streams');

  test.beforeEach(() => {
    seedRemoteStreams();
  });

  test('list route keeps the ErsatzTV action-bar and card-grid geometry', async ({ page }, testInfo) => {
    await page.setViewportSize({ width: 2048, height: 1024 });
    await gotoRemoteStreams(page);
    await expect(page.getByText('library_id:907')).toBeVisible();
    await expect(page.getByText('1 item')).toBeVisible();
    await expect(page.getByText('Visdeurbel')).toHaveCount(0);

    const actionBar = page.locator('div[class*="action-bar"]').first();
    const actionBox = await actionBar.boundingBox();
    expect(actionBox?.height).toBeGreaterThanOrEqual(60);
    expect(actionBox?.height).toBeLessThanOrEqual(80);

    const content = page.locator('section').filter({ has: page.getByRole('heading', { name: 'Remote Streams' }) }).first();
    const contentBox = await content.boundingBox();
    expect(contentBox?.width).toBeGreaterThan(700);
    expect(contentBox?.width).toBeLessThanOrEqual(1540);

    await page.screenshot({ path: screenshotPath(testInfo, 'remote-streams-list-desktop'), fullPage: true });
  });

  test('hydrated card add-to-collection action cancels and persists', async ({ page }) => {
    await gotoRemoteStreams(page);

    await clickFirstCardAddToCollection(page);
    const collectionModal = page
      .locator('div[class*="panel"]')
      .filter({ has: page.getByRole('heading', { name: /^add to collection$/i }) })
      .last();
    await expect(collectionModal.getByRole('heading', { name: /^add to collection$/i })).toBeVisible();
    await expect(collectionModal.getByText('1 selected items')).toBeVisible();
    const collectionSelect = collectionModal.locator('select').first();
    const collectionNameInput = collectionModal.locator('input[type="text"]').last();
    await expect.poll(async () => collectionSelect.locator('option').count()).toBeGreaterThan(0);
    await collectionSelect.selectOption('-1');
    await expect(collectionNameInput).toBeEnabled();

    await collectionModal.getByRole('button', { name: /^cancel$/i }).click();
    await expect(collectionModal.getByRole('heading', { name: /^add to collection$/i })).toBeHidden();

    await clickFirstCardAddToCollection(page);
    await expect(collectionModal.getByRole('heading', { name: /^add to collection$/i })).toBeVisible();
    await expect(collectionModal.getByText('1 selected items')).toBeVisible();
    await expect.poll(async () => collectionSelect.locator('option').count()).toBeGreaterThan(0);
    await collectionSelect.selectOption('-1');
    await expect(collectionNameInput).toBeEnabled();
    const collectionName = uniqueName('Remote Stream Collection');
    await collectionNameInput.fill(collectionName);
    await collectionModal.getByRole('button', { name: /^add to collection$/i }).click();

    await expect(collectionModal.getByRole('heading', { name: /^add to collection$/i })).toBeHidden();
    await expect.poll(() => sqlite(`
SELECT COUNT(*)
FROM "CollectionItem" ci
JOIN "Collection" c ON c."Id" = ci."CollectionId"
WHERE ci."MediaItemId" = 90701 AND c."Name" = '${sqlEscape(collectionName)}';
`)).toBe('1');
  });

  test('no-JS route still server-renders the list and pager links', async ({ browser, baseURL }) => {
    const context = await browser.newContext({ javaScriptEnabled: false, baseURL });
    const noJsPage = await context.newPage();

    const response = await noJsPage.goto('/media/remote/streams?query=library_id:907');
    expect(await response?.text()).toContain('Nurburgring broadcast');
    await expect(noJsPage.getByRole('heading', { name: 'Remote Streams' }).first()).toBeVisible();
    await expect(noJsPage.getByRole('link', { name: /next/i })).toHaveCount(0);

    await context.close();
  });

  test('mobile action menu remains reachable', async ({ page }, testInfo) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await gotoRemoteStreams(page);

    const menu = page.getByRole('button', { name: /item actions/i }).first();
    await expect(menu).toBeVisible();
    const menuBox = await menu.boundingBox();
    expect(menuBox?.width).toBeGreaterThan(32);
    expect(menuBox?.height).toBeGreaterThan(32);

    await page.screenshot({ path: screenshotPath(testInfo, 'remote-streams-list-mobile'), fullPage: true });
  });
});
