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
    input: `.timeout 10000\n${sql}`,
    encoding: 'utf8',
  }).trim();
}

type RemoteStreamSeed = {
  libraryId: number;
  otherLibraryId: number;
  libraryPathId: number;
  otherLibraryPathId: number;
  mediaSourceId: number;
  otherMediaSourceId: number;
  mediaItemId: number;
  otherMediaItemId: number;
};

function seedForProject(testInfo: TestInfo): RemoteStreamSeed {
  const projectOffset = Array.from(testInfo.project.name).reduce(
    (total, char) => total + char.charCodeAt(0),
    0,
  ) * 100;
  const base = 90_000 + projectOffset;
  return {
    libraryId: base + 7,
    otherLibraryId: base + 8,
    libraryPathId: base + 70,
    otherLibraryPathId: base + 80,
    mediaSourceId: base + 700,
    otherMediaSourceId: base + 800,
    mediaItemId: base + 7_001,
    otherMediaItemId: base + 8_001,
  };
}

function seedRemoteStreams(testInfo: TestInfo): RemoteStreamSeed {
  const seed = seedForProject(testInfo);
  sqlite(`
DELETE FROM "CollectionItem" WHERE "MediaItemId" IN (${seed.mediaItemId}, ${seed.otherMediaItemId});
DELETE FROM "RemoteStreamMetadata" WHERE "RemoteStreamId" IN (${seed.mediaItemId}, ${seed.otherMediaItemId});
DELETE FROM "RemoteStream" WHERE "Id" IN (${seed.mediaItemId}, ${seed.otherMediaItemId});
DELETE FROM "MediaItem" WHERE "Id" IN (${seed.mediaItemId}, ${seed.otherMediaItemId});
DELETE FROM "LibraryPath" WHERE "Id" IN (${seed.libraryPathId}, ${seed.otherLibraryPathId});
DELETE FROM "LocalLibrary" WHERE "Id" IN (${seed.libraryId}, ${seed.otherLibraryId});
DELETE FROM "Library" WHERE "Id" IN (${seed.libraryId}, ${seed.otherLibraryId});
DELETE FROM "LocalMediaSource" WHERE "Id" IN (${seed.mediaSourceId}, ${seed.otherMediaSourceId});
DELETE FROM "MediaSource" WHERE "Id" IN (${seed.mediaSourceId}, ${seed.otherMediaSourceId});

INSERT OR IGNORE INTO "MediaSource" ("Id") VALUES (${seed.mediaSourceId});
INSERT OR IGNORE INTO "LocalMediaSource" ("Id") VALUES (${seed.mediaSourceId});
INSERT OR IGNORE INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
  VALUES (${seed.libraryId}, 'E2E Remote Streams', 7, ${seed.mediaSourceId});
INSERT OR IGNORE INTO "LocalLibrary" ("Id") VALUES (${seed.libraryId});
INSERT OR IGNORE INTO "LibraryPath" ("Id", "Path", "LibraryId")
  VALUES (${seed.libraryPathId}, '/e2e/remote-streams/${testInfo.project.name}', ${seed.libraryId});
INSERT OR IGNORE INTO "MediaItem" ("Id", "LibraryPathId", "State")
  VALUES (${seed.mediaItemId}, ${seed.libraryPathId}, 0);
INSERT OR IGNORE INTO "RemoteStream" ("Id", "Url", "Script", "IsLive")
  VALUES (${seed.mediaItemId}, 'https://example.test/nurburgring.m3u8', NULL, 1);
INSERT OR IGNORE INTO "RemoteStreamMetadata" (
  "RemoteStreamId", "MetadataKind", "Title", "OriginalTitle",
  "SortTitle", "DateAdded", "DateUpdated"
) VALUES (
  ${seed.mediaItemId}, 0, 'Nurburgring broadcast', 'Racing',
  'nurburgring broadcast', '2026-06-14T00:00:00Z', '2026-06-14T00:00:00Z'
);

INSERT OR IGNORE INTO "MediaSource" ("Id") VALUES (${seed.otherMediaSourceId});
INSERT OR IGNORE INTO "LocalMediaSource" ("Id") VALUES (${seed.otherMediaSourceId});
INSERT OR IGNORE INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
  VALUES (${seed.otherLibraryId}, 'E2E Other Remote Streams', 7, ${seed.otherMediaSourceId});
INSERT OR IGNORE INTO "LocalLibrary" ("Id") VALUES (${seed.otherLibraryId});
INSERT OR IGNORE INTO "LibraryPath" ("Id", "Path", "LibraryId")
  VALUES (${seed.otherLibraryPathId}, '/e2e/other-remote-streams/${testInfo.project.name}', ${seed.otherLibraryId});
INSERT OR IGNORE INTO "MediaItem" ("Id", "LibraryPathId", "State")
  VALUES (${seed.otherMediaItemId}, ${seed.otherLibraryPathId}, 0);
INSERT OR IGNORE INTO "RemoteStream" ("Id", "Url", "Script", "IsLive")
  VALUES (${seed.otherMediaItemId}, 'https://example.test/visdeurbel.m3u8', NULL, 1);
INSERT OR IGNORE INTO "RemoteStreamMetadata" (
  "RemoteStreamId", "MetadataKind", "Title", "OriginalTitle",
  "SortTitle", "DateAdded", "DateUpdated"
) VALUES (
  ${seed.otherMediaItemId}, 0, 'Visdeurbel', 'Vis',
  'visdeurbel', '2026-06-14T00:00:00Z', '2026-06-14T00:00:00Z'
);
`);
  return seed;
}

function screenshotPath(testInfo: TestInfo, name: string): string {
  return `test-results/${name}-${testInfo.project.name}.png`;
}

async function gotoRemoteStreams(page: Page, seed: RemoteStreamSeed): Promise<void> {
  await page.goto(`/media/remote/streams?query=library_id:${seed.libraryId}`);
  await expect(page.getByRole('heading', { name: 'Remote Streams' })).toBeVisible();
  await expect(page.getByText('Nurburgring broadcast').first()).toBeVisible();
  await expect(page.locator('[data-remote-streams-hydrated="true"]')).toBeVisible();
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

  test('list route keeps the ErsatzTV action-bar and card-grid geometry', async ({ page }, testInfo) => {
    const seed = seedRemoteStreams(testInfo);
    await page.setViewportSize({ width: 2048, height: 1024 });
    await gotoRemoteStreams(page, seed);
    await expect(page.getByText(`library_id:${seed.libraryId}`)).toBeVisible();
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

  test('hydrated card add-to-collection action cancels and persists', async ({ page }, testInfo) => {
    const seed = seedRemoteStreams(testInfo);
    await gotoRemoteStreams(page, seed);

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
WHERE ci."MediaItemId" = ${seed.mediaItemId} AND c."Name" = '${sqlEscape(collectionName)}';
`)).toBe('1');
  });

  test('no-JS route still server-renders the list and pager links', async ({ browser, baseURL }, testInfo) => {
    const seed = seedRemoteStreams(testInfo);
    const context = await browser.newContext({ javaScriptEnabled: false, baseURL });
    const noJsPage = await context.newPage();

    const response = await noJsPage.goto(`/media/remote/streams?query=library_id:${seed.libraryId}`);
    expect(await response?.text()).toContain('Nurburgring broadcast');
    await expect(noJsPage.getByRole('heading', { name: 'Remote Streams' }).first()).toBeVisible();
    await expect(noJsPage.getByRole('link', { name: /next/i })).toHaveCount(0);

    await context.close();
  });

  test('mobile action menu remains reachable', async ({ page }, testInfo) => {
    const seed = seedRemoteStreams(testInfo);
    await page.setViewportSize({ width: 390, height: 844 });
    await gotoRemoteStreams(page, seed);

    const menu = page.getByRole('button', { name: /item actions/i }).first();
    await expect(menu).toBeVisible();
    const menuBox = await menu.boundingBox();
    expect(menuBox?.width).toBeGreaterThan(32);
    expect(menuBox?.height).toBeGreaterThan(32);

    await page.screenshot({ path: screenshotPath(testInfo, 'remote-streams-list-mobile'), fullPage: true });
  });
});
