import { expect, Page, TestInfo, test } from '@playwright/test';
import { execFileSync } from 'node:child_process';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

type Seed = {
  collectionId: number;
  firstMovieId: number;
  secondMovieId: number;
  firstTitle: string;
  secondTitle: string;
};

function sqlite(sql: string): string {
  if (!dbPath) {
    throw new Error('E2E_DB_PATH or ERSATZRS_DB_PATH is required for collection-detail seeding');
  }

  return execFileSync('sqlite3', [dbPath], {
    input: `.timeout 10000\n${sql}`,
    encoding: 'utf8',
  }).trim();
}

function seedForProject(testInfo: TestInfo): Seed {
  const projectOffset = Array.from(testInfo.project.name).reduce(
    (total, char) => total + char.charCodeAt(0),
    0,
  ) * 100;
  const base = 470_000 + projectOffset;
  const collectionId = base + 1;
  const mediaSourceId = base + 2;
  const libraryId = base + 3;
  const libraryPathId = base + 4;
  const firstMovieId = base + 5;
  const secondMovieId = base + 6;
  const suffix = testInfo.project.name;
  const seed = {
    collectionId,
    firstMovieId,
    secondMovieId,
    firstTitle: `E2E Collection Zulu ${suffix}`,
    secondTitle: `E2E Collection Alpha ${suffix}`,
  };

  sqlite(`
PRAGMA foreign_keys=OFF;
DELETE FROM "CollectionItem" WHERE "CollectionId" = ${collectionId};
DELETE FROM "Collection" WHERE "Id" = ${collectionId};
DELETE FROM "MovieMetadata" WHERE "MovieId" IN (${firstMovieId}, ${secondMovieId});
DELETE FROM "Movie" WHERE "Id" IN (${firstMovieId}, ${secondMovieId});
DELETE FROM "MediaItem" WHERE "Id" IN (${firstMovieId}, ${secondMovieId});
DELETE FROM "LibraryPath" WHERE "Id" = ${libraryPathId};
DELETE FROM "LocalLibrary" WHERE "Id" = ${libraryId};
DELETE FROM "Library" WHERE "Id" = ${libraryId};
DELETE FROM "LocalMediaSource" WHERE "Id" = ${mediaSourceId};
DELETE FROM "MediaSource" WHERE "Id" = ${mediaSourceId};

INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder")
  VALUES (${collectionId}, 'E2E Collection Detail ${suffix}', 0);
INSERT INTO "MediaSource" ("Id") VALUES (${mediaSourceId});
INSERT INTO "LocalMediaSource" ("Id") VALUES (${mediaSourceId});
INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
  VALUES (${libraryId}, 'E2E Collection Detail ${suffix}', 1, ${mediaSourceId});
INSERT INTO "LocalLibrary" ("Id") VALUES (${libraryId});
INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId")
  VALUES (${libraryPathId}, '/e2e/collection-detail/${suffix}', ${libraryId});
INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State")
  VALUES (${firstMovieId}, ${libraryPathId}, 0), (${secondMovieId}, ${libraryPathId}, 0);
INSERT INTO "Movie" ("Id") VALUES (${firstMovieId}), (${secondMovieId});
INSERT INTO "MovieMetadata" (
  "MovieId", "MetadataKind", "Title", "OriginalTitle", "SortTitle", "DateAdded", "DateUpdated", "Year"
) VALUES
  (${firstMovieId}, 0, '${seed.firstTitle}', '${seed.firstTitle}', '${seed.firstTitle.toLowerCase()}', '2026-07-26T00:00:00Z', '2026-07-26T00:00:00Z', 2026),
  (${secondMovieId}, 0, '${seed.secondTitle}', '${seed.secondTitle}', '${seed.secondTitle.toLowerCase()}', '2026-07-26T00:00:00Z', '2026-07-26T00:00:00Z', 2026);
INSERT INTO "Artwork" ("MovieMetadataId", "ArtworkKind", "Path", "DateAdded", "DateUpdated")
  SELECT "Id", 0, 'synthetic-row160.png', '2026-09-30', '2026-09-30'
  FROM "MovieMetadata" WHERE "MovieId" IN (${firstMovieId}, ${secondMovieId});
INSERT INTO "CollectionItem" ("CollectionId", "MediaItemId", "CustomIndex")
  VALUES (${collectionId}, ${firstMovieId}, 0), (${collectionId}, ${secondMovieId}, 1);
PRAGMA foreign_keys=ON;
`);

  return seed;
}

async function forceEnglish(page: Page): Promise<void> {
  await page.request.post('/admin-api/settings/ui/save', {
    form: { is_dark_mode: 'true', language: 'en-us' },
    maxRedirects: 0,
  });
}

async function gotoHydrated(page: Page, route: string): Promise<void> {
  const response = await page.goto(route, { waitUntil: 'domcontentloaded' });
  expect(response?.status(), `GET ${route}`).toBe(200);
  await expect(page.locator('[data-not-found]')).toHaveCount(0);
  await waitForHydration(page);
}

async function waitForHydration(page: Page): Promise<void> {
  await expect
    .poll(
      () =>
        page.evaluate(
          () =>
            (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated ===
            true,
        ),
      { timeout: 15_000 },
    )
    .toBe(true);
}

test.describe('collection detail parity', () => {
  test.skip(!dbPath, 'E2E_DB_PATH or ERSATZRS_DB_PATH is required to seed collection detail');

  test('hydrates, persists custom ordering, and removes selected items', async ({ page }, testInfo) => {
    const seed = seedForProject(testInfo);
    const runtimeErrors: string[] = [];
    page.on('pageerror', (error) => runtimeErrors.push(error.stack ?? error.message));
    page.on('console', (message) => {
      if (
        message.type() === 'error' &&
        /failed_to_cast_marker_node|rust_panic|unreachable executed|hydration/i.test(message.text())
      ) {
        runtimeErrors.push(message.text());
      }
    });

    await forceEnglish(page);
    await gotoHydrated(page, `/media/collections/${seed.collectionId}`);

    await expect(page.getByText(seed.firstTitle, { exact: true })).toBeVisible();
    await expect(page.getByText(seed.secondTitle, { exact: true })).toBeVisible();

    const customOrder = page.getByLabel('Use Custom Playback Order');
    await customOrder.check();
    await expect(customOrder).toBeChecked();
    await expect(page.getByRole('button', { name: 'Move Down' }).first()).toBeEnabled();
    await page.getByRole('button', { name: 'Move Down' }).first().click();
    await expect
      .poll(() =>
        sqlite(
          `SELECT "MediaItemId" FROM "CollectionItem" WHERE "CollectionId" = ${seed.collectionId} ORDER BY "CustomIndex" LIMIT 1;`,
        ),
      )
      .toBe(String(seed.secondMovieId));

    await page.getByLabel('Select', { exact: true }).first().click();
    const selectionCount = page.getByText('1 Item Selected', { exact: true });
    await expect(selectionCount).toBeVisible();
    await selectionCount
      .locator('xpath=ancestor::div[1]')
      .getByRole('button', { name: 'Remove From Collection', exact: true })
      .click();
    await expect
      .poll(() =>
        sqlite(
          `SELECT COUNT(*) FROM "CollectionItem" WHERE "CollectionId" = ${seed.collectionId};`,
        ),
      )
      .toBe('1');
    await expect(selectionCount).toHaveCount(0);
    expect(runtimeErrors).toEqual([]);
  });

  test('drags posters, excludes overlapping saves and preserves selection mode', async ({ page }, testInfo) => {
    const seed = seedForProject(testInfo);
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.route('**/artwork/**', route => route.fulfill({
      contentType: 'image/svg+xml',
      body: '<svg xmlns="http://www.w3.org/2000/svg" width="120" height="180"><rect width="120" height="180" fill="teal"/></svg>',
    }));
    await forceEnglish(page);
    await gotoHydrated(page, `/media/collections/${seed.collectionId}`);
    const toggle = page.getByLabel('Use Custom Playback Order');
    await toggle.check();
    await expect(page.getByRole('button', { name: 'Move Down' }).first()).toBeEnabled();
    const firstPoster = page.getByRole('img', { name: seed.firstTitle, exact: true });
    const secondPoster = page.getByRole('img', { name: seed.secondTitle, exact: true });
    await expect(firstPoster).toBeVisible();
    let releaseSave!: () => void;
    const gate = new Promise<void>(resolve => { releaseSave = resolve; });
    let saveRequests = 0;
    await page.route('**/admin-api/update_collection_custom_order*', async route => {
      saveRequests += 1;
      await gate;
      await route.continue();
    });
    try {
      await firstPoster.dragTo(secondPoster);
      await expect.poll(() => saveRequests).toBe(1);
      await expect(toggle).toBeDisabled();
      await expect(page.locator('[draggable="true"]')).toHaveCount(0);
      await expect(page.getByRole('button', { name: 'Move Down' }).first()).toBeDisabled();
    } finally {
      releaseSave();
    }
    const currentOrder = () => sqlite(`SELECT "MediaItemId" FROM "CollectionItem" WHERE "CollectionId" = ${seed.collectionId} ORDER BY "CustomIndex";`);
    await expect.poll(currentOrder).toBe(`${seed.secondMovieId}\n${seed.firstMovieId}`);
    await expect(toggle).toBeEnabled();
    await page.reload();
    await waitForHydration(page);
    await expect(page.locator('[data-collection-item-id] img').first()).toHaveAttribute('alt', seed.secondTitle);
    await page.getByRole('button', { name: 'Move Down' }).first().click();
    await expect.poll(currentOrder).toBe(`${seed.firstMovieId}\n${seed.secondMovieId}`);
    await expect(page.locator('[data-collection-item-id] img').first()).toHaveAttribute('alt', seed.firstTitle);
    await expect(toggle).toBeEnabled();
    await page.getByRole('button', { name: 'Move Down' }).first().click();
    await expect.poll(currentOrder).toBe(`${seed.secondMovieId}\n${seed.firstMovieId}`);
    await page.getByLabel('Select', { exact: true }).first().click();
    await expect(page.locator('[draggable="true"]')).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Move Down' }).first()).toBeDisabled();
    expect(errors).toEqual([]);
  });

  for (const culture of [
    { code: 'en-us', toggle: 'Use Custom Playback Order', move: 'Move Down', failed: 'Reorder failed' },
    { code: 'nl', toggle: 'Aangepaste afspeelvolgorde gebruiken', move: 'Omlaag verplaatsen', failed: 'Volgorde wijzigen mislukt' },
  ]) {
    test(`restores the persisted poster order after a rejected save (${culture.code})`, async ({ page }, testInfo) => {
      const seed = seedForProject(testInfo);
      const errors: string[] = [];
      page.on('pageerror', error => errors.push(error.message));
      await page.request.post('/admin-api/settings/ui/save', {
        form: { is_dark_mode: 'true', language: culture.code }, maxRedirects: 0,
      });
      await gotoHydrated(page, `/media/collections/${seed.collectionId}`);
      await page.getByLabel(culture.toggle).check();
      await expect(page.getByRole('button', { name: culture.move }).first()).toBeEnabled();
      sqlite(`CREATE TRIGGER "FailCollectionOrder" BEFORE UPDATE OF "CustomIndex" ON "CollectionItem" WHEN NEW."CollectionId" = ${seed.collectionId} BEGIN SELECT RAISE(ABORT, 'synthetic order failure'); END;`);
      try {
        await page.getByRole('button', { name: culture.move }).first().click();
        await expect(page.getByText(new RegExp(culture.failed)).last()).toBeVisible();
        await expect(page.getByLabel(culture.toggle)).toBeEnabled();
        const cards = page.locator('[data-collection-item-id]');
        await expect(cards.first()).toContainText(seed.firstTitle);
        expect(sqlite(`SELECT "MediaItemId" FROM "CollectionItem" WHERE "CollectionId" = ${seed.collectionId} ORDER BY "CustomIndex";`)).toBe(`${seed.firstMovieId}\n${seed.secondMovieId}`);
      } finally {
        sqlite('DROP TRIGGER "FailCollectionOrder";');
      }
      await page.getByRole('button', { name: culture.move }).first().click();
      await expect.poll(() => sqlite(`SELECT "MediaItemId" FROM "CollectionItem" WHERE "CollectionId" = ${seed.collectionId} ORDER BY "CustomIndex" LIMIT 1;`)).toBe(String(seed.secondMovieId));
      expect(errors).toEqual([]);
    });
  }
});
