import { expect, test, type Page, type TestInfo } from '@playwright/test';
import { DatabaseSync } from 'node:sqlite';

const DATABASE_PATH = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;
const NOW = '2026-08-24T00:00:00Z';
const TINY_PNG = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==',
  'base64',
);

const ERROR_SURFACE_SELECTOR =
  '[data-admin-resource][data-state="error"], [role="alert"]:not([data-toast-id]), [role="alert"][data-toast-severity="error"], [role="alert"][data-toast-severity="warning"]';

type SeasonSeed = {
  id: number;
  number: number;
  episodeIds: number[];
  episodeMetadataIds: number[];
  artworkIds: number[];
  thumbnailUrls: string[];
};

type CardinalitySeed = {
  mediaSourceId: number;
  libraryId: number;
  libraryPathId: number;
  showId: number;
  showMetadataId: number;
  seasonMetadataIds: number[];
  showTitle: string;
  seasons: SeasonSeed[];
};

function projectBase(testInfo: TestInfo): number {
  const projectOffset = Array.from(testInfo.project.name).reduce(
    (total, character) => total + character.charCodeAt(0),
    0,
  ) * 10_000;
  return 2_000_000 + projectOffset;
}

function fixtureForProject(testInfo: TestInfo): CardinalitySeed {
  const base = projectBase(testInfo);
  const suffix = testInfo.project.name.toLowerCase().replace(/[^a-z0-9]+/g, '-');
  const seasons: SeasonSeed[] = [];
  let globalEpisodeIndex = 0;

  for (const [seasonIndex, episodeCount] of [13, 14].entries()) {
    const seasonNumber = seasonIndex + 1;
    const episodeIds: number[] = [];
    const episodeMetadataIds: number[] = [];
    const artworkIds: number[] = [];
    const thumbnailUrls: string[] = [];

    for (let episodeIndex = 0; episodeIndex < episodeCount; episodeIndex += 1) {
      const lowestArtworkId = base + 1_000 + globalEpisodeIndex * 3;
      episodeIds.push(base + 100 + globalEpisodeIndex);
      episodeMetadataIds.push(base + 300 + globalEpisodeIndex);
      artworkIds.push(lowestArtworkId, lowestArtworkId + 1, lowestArtworkId + 2);
      thumbnailUrls.push(
        `https://amm-h-01.example.test/${suffix}/thumbnail-${lowestArtworkId}.png`,
      );
      globalEpisodeIndex += 1;
    }

    seasons.push({
      id: base + 20 + seasonIndex,
      number: seasonNumber,
      episodeIds,
      episodeMetadataIds,
      artworkIds,
      thumbnailUrls,
    });
  }

  return {
    mediaSourceId: base + 1,
    libraryId: base + 2,
    libraryPathId: base + 3,
    showId: base + 10,
    showMetadataId: base + 11,
    seasonMetadataIds: [base + 30, base + 31],
    showTitle: `AMM-H-01 Cardinality Show (${testInfo.project.name})`,
    seasons,
  };
}

function openDatabase(): DatabaseSync {
  if (!DATABASE_PATH) {
    throw new Error('E2E_DB_PATH or ERSATZRS_DB_PATH is required for TV cardinality seeding');
  }
  const database = new DatabaseSync(DATABASE_PATH);
  database.exec('PRAGMA busy_timeout=10000; PRAGMA foreign_keys=ON;');
  return database;
}

function idList(ids: number[]): string {
  return ids.join(', ');
}

function cleanupFixture(database: DatabaseSync, seed: CardinalitySeed): void {
  const episodeIds = seed.seasons.flatMap((season) => season.episodeIds);
  const episodeMetadataIds = seed.seasons.flatMap((season) => season.episodeMetadataIds);
  const artworkIds = seed.seasons.flatMap((season) => season.artworkIds);
  const mediaItemIds = [seed.showId, ...seed.seasons.map((season) => season.id), ...episodeIds];

  database.exec(`
    DELETE FROM "Artwork"
      WHERE "Id" IN (${idList(artworkIds)})
         OR "EpisodeMetadataId" IN (${idList(episodeMetadataIds)});
    DELETE FROM "EpisodeMetadata" WHERE "Id" IN (${idList(episodeMetadataIds)});
    DELETE FROM "SeasonMetadata" WHERE "Id" IN (${idList(seed.seasonMetadataIds)});
    DELETE FROM "ShowMetadata" WHERE "Id" = ${seed.showMetadataId};
    DELETE FROM "Episode" WHERE "Id" IN (${idList(episodeIds)});
    DELETE FROM "Season" WHERE "Id" IN (${idList(seed.seasons.map((season) => season.id))});
    DELETE FROM "Show" WHERE "Id" = ${seed.showId};
    DELETE FROM "MediaItem" WHERE "Id" IN (${idList(mediaItemIds)});
    DELETE FROM "LibraryPath" WHERE "Id" = ${seed.libraryPathId};
    DELETE FROM "LocalLibrary" WHERE "Id" = ${seed.libraryId};
    DELETE FROM "Library" WHERE "Id" = ${seed.libraryId};
    DELETE FROM "LocalMediaSource" WHERE "Id" = ${seed.mediaSourceId};
    DELETE FROM "MediaSource" WHERE "Id" = ${seed.mediaSourceId};
  `);
}

function seedFixture(testInfo: TestInfo): CardinalitySeed {
  const seed = fixtureForProject(testInfo);
  const suffix = testInfo.project.name.toLowerCase().replace(/[^a-z0-9]+/g, '-');
  const database = openDatabase();

  database.exec('BEGIN IMMEDIATE');
  try {
    cleanupFixture(database, seed);

    database.prepare('INSERT INTO "MediaSource" ("Id") VALUES (?)').run(seed.mediaSourceId);
    database
      .prepare('INSERT INTO "LocalMediaSource" ("Id") VALUES (?)')
      .run(seed.mediaSourceId);
    database
      .prepare(
        'INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId") VALUES (?, ?, 2, ?)',
      )
      .run(seed.libraryId, `AMM-H-01 TV (${testInfo.project.name})`, seed.mediaSourceId);
    database.prepare('INSERT INTO "LocalLibrary" ("Id") VALUES (?)').run(seed.libraryId);
    database
      .prepare('INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId") VALUES (?, ?, ?)')
      .run(seed.libraryPathId, `/e2e/amm-h-01/${suffix}`, seed.libraryId);

    const insertMediaItem = database.prepare(
      'INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (?, ?, 0)',
    );
    insertMediaItem.run(seed.showId, seed.libraryPathId);
    for (const season of seed.seasons) {
      insertMediaItem.run(season.id, seed.libraryPathId);
      for (const episodeId of season.episodeIds) {
        insertMediaItem.run(episodeId, seed.libraryPathId);
      }
    }

    database.prepare('INSERT INTO "Show" ("Id") VALUES (?)').run(seed.showId);
    database
      .prepare(
        `INSERT INTO "ShowMetadata" (
          "Id", "ShowId", "MetadataKind", "Title", "SortTitle", "Year", "Plot",
          "DateAdded", "DateUpdated"
        ) VALUES (?, ?, 0, ?, ?, 2026, ?, ?, ?)`,
      )
      .run(
        seed.showMetadataId,
        seed.showId,
        seed.showTitle,
        seed.showTitle.toLowerCase(),
        'A deterministic 27-episode cardinality fixture.',
        NOW,
        NOW,
      );

    const insertSeason = database.prepare(
      'INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber") VALUES (?, ?, ?)',
    );
    const insertSeasonMetadata = database.prepare(
      `INSERT INTO "SeasonMetadata" (
        "Id", "SeasonId", "MetadataKind", "Title", "SortTitle", "DateAdded", "DateUpdated"
      ) VALUES (?, ?, 0, ?, ?, ?, ?)`,
    );
    const insertEpisode = database.prepare(
      'INSERT INTO "Episode" ("Id", "SeasonId") VALUES (?, ?)',
    );
    const insertEpisodeMetadata = database.prepare(
      `INSERT INTO "EpisodeMetadata" (
        "Id", "EpisodeId", "MetadataKind", "Title", "SortTitle", "ReleaseDate",
        "DateAdded", "DateUpdated", "EpisodeNumber", "Plot"
      ) VALUES (?, ?, 0, ?, ?, ?, ?, ?, ?, ?)`,
    );
    const insertArtwork = database.prepare(
      `INSERT INTO "Artwork" (
        "Id", "ArtworkKind", "Path", "EpisodeMetadataId", "DateAdded", "DateUpdated"
      ) VALUES (?, 1, ?, ?, ?, ?)`,
    );

    for (const [seasonIndex, season] of seed.seasons.entries()) {
      insertSeason.run(season.id, seed.showId, season.number);
      insertSeasonMetadata.run(
        seed.seasonMetadataIds[seasonIndex],
        season.id,
        `Season ${season.number}`,
        `season ${season.number}`,
        NOW,
        NOW,
      );

      for (const [episodeIndex, episodeId] of season.episodeIds.entries()) {
        const episodeNumber = episodeIndex + 1;
        const metadataId = season.episodeMetadataIds[episodeIndex];
        const thumbnailUrl = season.thumbnailUrls[episodeIndex];
        const lowestArtworkId = season.artworkIds[episodeIndex * 3];
        const title = `Season ${season.number} Episode ${episodeNumber}`;

        insertEpisode.run(episodeId, season.id);
        insertEpisodeMetadata.run(
          metadataId,
          episodeId,
          title,
          title.toLowerCase(),
          `2026-${String(season.number).padStart(2, '0')}-${String(episodeNumber).padStart(2, '0')}T00:00:00Z`,
          NOW,
          NOW,
          episodeNumber,
          `Cardinality fixture episode ${episodeNumber}.`,
        );

        // Migrations have already run before Playwright starts. Reintroduce the
        // exact legacy duplicate shape here so the SSR read remains defensive
        // even when an old producer writes the same thumbnail twice.
        insertArtwork.run(lowestArtworkId, thumbnailUrl, metadataId, NOW, NOW);
        insertArtwork.run(lowestArtworkId + 1, thumbnailUrl, metadataId, NOW, NOW);
        insertArtwork.run(
          lowestArtworkId + 2,
          `https://amm-h-01.example.test/${suffix}/variant-${lowestArtworkId + 2}.png`,
          metadataId,
          NOW,
          NOW,
        );
      }
    }

    database.exec('COMMIT');
    return seed;
  } catch (error) {
    database.exec('ROLLBACK');
    throw error;
  } finally {
    database.close();
  }
}

function removeFixture(seed: CardinalitySeed): void {
  const database = openDatabase();
  database.exec('BEGIN IMMEDIATE');
  try {
    cleanupFixture(database, seed);
    database.exec('COMMIT');
  } catch (error) {
    database.exec('ROLLBACK');
    throw error;
  } finally {
    database.close();
  }
}

function assertLegacyDuplicateShape(seed: CardinalitySeed): void {
  const database = openDatabase();
  try {
    const episodeMetadataIds = seed.seasons.flatMap((season) => season.episodeMetadataIds);
    const rows = database
      .prepare(
        `SELECT "EpisodeMetadataId" AS "metadataId",
                COUNT(*) AS "artworkCount",
                COUNT(DISTINCT "Path") AS "distinctPaths",
                MIN("Id") AS "lowestArtworkId"
         FROM "Artwork"
         WHERE "EpisodeMetadataId" IN (${idList(episodeMetadataIds)})
           AND "ArtworkKind" = 1
         GROUP BY "EpisodeMetadataId"
         ORDER BY "EpisodeMetadataId"`,
      )
      .all() as Array<{
      metadataId: number;
      artworkCount: number;
      distinctPaths: number;
      lowestArtworkId: number;
    }>;

    expect(rows).toHaveLength(27);
    expect(rows.every((row) => row.artworkCount === 3 && row.distinctPaths === 2)).toBe(true);
    expect(rows.map((row) => row.lowestArtworkId)).toEqual(
      seed.seasons.flatMap((season) => season.artworkIds.filter((_, index) => index % 3 === 0)),
    );
  } finally {
    database.close();
  }
}

function watchRuntimeErrors(page: Page): string[] {
  const runtimeErrors: string[] = [];
  page.on('pageerror', (error) => {
    const text = error.stack ?? error.message;
    const isBenignNavigationAbort =
      (text.includes('AbortError') && text.toLowerCase().includes('operation was aborted')) ||
      text.includes('NetworkError when attempting to fetch resource');
    if (!isBenignNavigationAbort) {
      runtimeErrors.push(text);
    }
  });
  page.on('console', (message) => {
    if (message.type() === 'error') {
      runtimeErrors.push(message.text());
    }
  });
  return runtimeErrors;
}

async function forceEnglish(page: Page): Promise<void> {
  await page.request.post('/api/settings/ui/save', {
    form: {
      is_dark_mode: 'true',
      language: 'en-us',
    },
    maxRedirects: 0,
  });
}

async function waitForHydration(page: Page): Promise<void> {
  await expect
    .poll(
      () =>
        page.evaluate(
          () =>
            (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true,
        ),
      { timeout: 20_000 },
    )
    .toBe(true);
}

async function gotoHydrated(page: Page, route: string): Promise<void> {
  const response = await page.goto(route, { waitUntil: 'domcontentloaded' });
  expect(response?.status(), `GET ${route}`).toBe(200);
  await expect(page.locator('[data-not-found]')).toHaveCount(0);
  await waitForHydration(page);
  await expect(page.locator(ERROR_SURFACE_SELECTOR)).toHaveCount(0);
}

function expectUnique(values: string[], label: string): void {
  expect(values.every((value) => value.length > 0), `${label} must be non-empty`).toBe(true);
  expect(new Set(values).size, `${label} must be unique`).toBe(values.length);
}

async function seasonSnapshot(page: Page): Promise<{
  articleIds: string[];
  troubleshootingHrefs: string[];
  thumbnailUrls: string[];
}> {
  const articles = page.locator('article[id^="episode-"]');
  const articleIds = await articles.evaluateAll((elements) =>
    elements.map((element) => element.getAttribute('id') ?? ''),
  );
  const troubleshootingHrefs = await articles
    .locator('a[href^="/system/troubleshooting/playback?mediaItem="]')
    .evaluateAll((elements) => elements.map((element) => element.getAttribute('href') ?? ''));
  const thumbnailUrls = await articles
    .locator('img')
    .evaluateAll((elements) => elements.map((element) => element.getAttribute('src') ?? ''));

  return { articleIds, troubleshootingHrefs, thumbnailUrls };
}

async function assertSeasonPage(page: Page, seed: CardinalitySeed, season: SeasonSeed): Promise<void> {
  const route = `/media/tv/seasons/${season.id}`;
  await gotoHydrated(page, route);
  await expect(page.getByRole('heading', { name: seed.showTitle, exact: true })).toBeVisible();
  await expect(page.getByText(`Season ${season.number}`, { exact: true }).first()).toBeVisible();

  const articles = page.locator('article[id^="episode-"]');
  await expect(articles).toHaveCount(season.episodeIds.length);
  const firstSnapshot = await seasonSnapshot(page);
  const expectedArticleIds = season.episodeIds.map((episodeId) => `episode-${episodeId}`);
  const expectedTroubleshootingHrefs = season.episodeIds.map(
    (episodeId) => `/system/troubleshooting/playback?mediaItem=${episodeId}`,
  );

  expectUnique(firstSnapshot.articleIds, `season ${season.number} episode article IDs`);
  expectUnique(firstSnapshot.troubleshootingHrefs, `season ${season.number} episode anchors`);
  expect(firstSnapshot.articleIds).toEqual(expectedArticleIds);
  expect(firstSnapshot.troubleshootingHrefs).toEqual(expectedTroubleshootingHrefs);
  expect(firstSnapshot.thumbnailUrls).toEqual(season.thumbnailUrls);

  await page.reload({ waitUntil: 'domcontentloaded' });
  await waitForHydration(page);
  await expect(page.locator(ERROR_SURFACE_SELECTOR)).toHaveCount(0);
  await expect(page.locator('article[id^="episode-"]')).toHaveCount(season.episodeIds.length);
  expect(await seasonSnapshot(page)).toEqual(firstSnapshot);
}

test.describe('AMM-H-01 add-on television cardinality', () => {
  test.skip(!DATABASE_PATH, 'E2E_DB_PATH or ERSATZRS_DB_PATH is required to seed TV rows');

  test('keeps 13 and 14 episode seasons unique with legacy duplicate thumbnails', async ({
    page,
  }, testInfo) => {
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedFixture(testInfo);

    try {
      assertLegacyDuplicateShape(seed);
      await page.route('https://amm-h-01.example.test/**', (route) =>
        route.fulfill({ status: 200, contentType: 'image/png', body: TINY_PNG }),
      );
      await forceEnglish(page);

      await gotoHydrated(page, `/media/tv/shows/${seed.showId}`);
      await expect(page.getByRole('heading', { name: seed.showTitle, exact: true })).toBeVisible();

      const seasonLinks = page.locator('a[href^="/media/tv/seasons/"]');
      await expect(seasonLinks).toHaveCount(2);
      const seasonHrefs = await seasonLinks.evaluateAll((elements) =>
        elements.map((element) => element.getAttribute('href') ?? ''),
      );
      expectUnique(seasonHrefs, 'show season links');
      expect(seasonHrefs).toEqual(
        seed.seasons.map((season) => `/media/tv/seasons/${season.id}`),
      );

      for (const season of seed.seasons) {
        await assertSeasonPage(page, seed, season);
      }

      expect(runtimeErrors).toEqual([]);
    } finally {
      removeFixture(seed);
    }
  });
});
