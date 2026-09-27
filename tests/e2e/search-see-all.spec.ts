import { expect, Page, TestInfo, test } from '@playwright/test';
import { execFileSync } from 'child_process';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;
const TINY_PNG = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==',
  'base64',
);

type Seed = {
  query: string;
  seasonId: number;
  episodeId: number;
  musicVideoId: number;
  showArtworkId: number;
  artistArtworkId: number;
};

function sqlite(sql: string): string {
  if (!dbPath) {
    throw new Error('E2E_DB_PATH or ERSATZRS_DB_PATH is required for search seeding');
  }
  return execFileSync('sqlite3', [dbPath], {
    input: `.timeout 10000\n${sql}`,
    encoding: 'utf8',
  }).trim();
}

function projectBase(testInfo: TestInfo): number {
  const projectOffset = Array.from(testInfo.project.name).reduce(
    (total, char) => total + char.charCodeAt(0),
    0,
  ) * 1_000;
  return 1_200_000 + projectOffset;
}

function screenshotPath(testInfo: TestInfo, name: string): string {
  return `test-results/${name}-${testInfo.project.name}.png`;
}

function seedSearchResults(testInfo: TestInfo): Seed {
  const base = projectBase(testInfo);
  const query = `p7c03${testInfo.project.name.replace(/[^a-z0-9]/gi, '')}`;
  const mediaSourceId = base + 1;
  const libraryId = base + 2;
  const libraryPathId = base + 3;
  const showId = base + 10;
  const showMetadataId = base + 11;
  const artistId = base + 20;
  const artistMetadataId = base + 21;
  const now = '2026-07-25T00:00:00Z';

  const mediaItems = [
    `(${showId}, ${libraryPathId}, 0, 0, NULL, NULL)`,
    `(${artistId}, ${libraryPathId}, 0, 0, NULL, NULL)`,
  ];
  const seasons: string[] = [];
  const seasonMetadata: string[] = [];
  const episodes: string[] = [];
  const episodeMetadata: string[] = [];
  const musicVideos: string[] = [];
  const musicVideoMetadata: string[] = [];

  for (let index = 0; index < 51; index += 1) {
    const seasonId = base + 100 + index;
    const seasonMetadataId = base + 200 + index;
    const episodeId = base + 300 + index;
    const episodeMetadataId = base + 400 + index;
    const musicVideoId = base + 500 + index;
    const musicVideoMetadataId = base + 600 + index;
    const prefix = index === 50 ? 'z' : 'a';

    mediaItems.push(
      `(${seasonId}, ${libraryPathId}, 0, 0, NULL, NULL)`,
      `(${episodeId}, ${libraryPathId}, 0, 0, NULL, NULL)`,
      `(${musicVideoId}, ${libraryPathId}, 0, 0, NULL, NULL)`,
    );
    seasons.push(`(${seasonId}, ${showId}, ${index + 1})`);
    seasonMetadata.push(
      `(${seasonMetadataId}, ${seasonId}, 0, '${query} season ${index + 1}', '${prefix} ${query} season ${index + 1}', '${now}', '${now}')`,
    );
    episodes.push(`(${episodeId}, ${seasonId})`);
    episodeMetadata.push(
      `(${episodeMetadataId}, ${episodeId}, 0, '${query} episode ${index + 1}', '${prefix} ${query} episode ${index + 1}', ${index + 1}, '${now}', '${now}')`,
    );
    musicVideos.push(`(${musicVideoId}, ${artistId})`);
    musicVideoMetadata.push(
      `(${musicVideoMetadataId}, ${musicVideoId}, 0, '${query} music video ${index + 1}', '${prefix} ${query} music video ${index + 1}', 'P7C-03', '${now}', '${now}')`,
    );
  }

  sqlite(`
PRAGMA foreign_keys=OFF;
INSERT OR IGNORE INTO "MediaSource" ("Id") VALUES (${mediaSourceId});
INSERT OR IGNORE INTO "LocalMediaSource" ("Id") VALUES (${mediaSourceId});
INSERT OR IGNORE INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
  VALUES (${libraryId}, 'P7C-03 Search', 2, ${mediaSourceId});
INSERT OR IGNORE INTO "LocalLibrary" ("Id") VALUES (${libraryId});
INSERT OR IGNORE INTO "LibraryPath" ("Id", "Path", "LibraryId")
  VALUES (${libraryPathId}, '/e2e/p7c-03/${testInfo.project.name}', ${libraryId});

INSERT OR REPLACE INTO "MediaItem"
  ("Id", "LibraryPathId", "State", "ContentKind", "SubKind", "ParentMediaItemId")
  VALUES ${mediaItems.join(',\n')};
INSERT OR REPLACE INTO "Show" ("Id") VALUES (${showId});
INSERT OR REPLACE INTO "ShowMetadata"
  ("Id", "ShowId", "MetadataKind", "Title", "SortTitle", "DateAdded", "DateUpdated")
  VALUES (${showMetadataId}, ${showId}, 0, '${query} show', '${query} show', '${now}', '${now}');
INSERT OR REPLACE INTO "Artist" ("Id") VALUES (${artistId});
INSERT OR REPLACE INTO "ArtistMetadata"
  ("Id", "ArtistId", "MetadataKind", "Title", "SortTitle", "DateAdded", "DateUpdated")
  VALUES (${artistMetadataId}, ${artistId}, 0, '${query} artist', '${query} artist', '${now}', '${now}');
INSERT OR REPLACE INTO "Season" ("Id", "ShowId", "SeasonNumber")
  VALUES ${seasons.join(',\n')};
INSERT OR REPLACE INTO "SeasonMetadata"
  ("Id", "SeasonId", "MetadataKind", "Title", "SortTitle", "DateAdded", "DateUpdated")
  VALUES ${seasonMetadata.join(',\n')};
INSERT OR REPLACE INTO "Episode" ("Id", "SeasonId")
  VALUES ${episodes.join(',\n')};
INSERT OR REPLACE INTO "EpisodeMetadata"
  ("Id", "EpisodeId", "MetadataKind", "Title", "SortTitle", "EpisodeNumber", "DateAdded", "DateUpdated")
  VALUES ${episodeMetadata.join(',\n')};
INSERT OR REPLACE INTO "MusicVideo" ("Id", "ArtistId")
  VALUES ${musicVideos.join(',\n')};
INSERT OR REPLACE INTO "MusicVideoMetadata"
  ("Id", "MusicVideoId", "MetadataKind", "Title", "SortTitle", "Album", "DateAdded", "DateUpdated")
  VALUES ${musicVideoMetadata.join(',\n')};

DELETE FROM "Artwork"
 WHERE "ShowMetadataId" = ${showMetadataId}
    OR "ArtistMetadataId" = ${artistMetadataId}
    OR "SeasonMetadataId" = ${base + 200}
    OR "EpisodeMetadataId" = ${base + 400}
    OR "MusicVideoMetadataId" = ${base + 600};
INSERT INTO "Artwork" ("ShowMetadataId", "ArtworkKind", "Path", "DateAdded", "DateUpdated")
  VALUES (${showMetadataId}, 0, 'jellyfin://Items/${query}-show/Images/Primary?tag=poster', '${now}', '${now}');
INSERT INTO "Artwork" ("ArtistMetadataId", "ArtworkKind", "Path", "DateAdded", "DateUpdated")
  VALUES (${artistMetadataId}, 1, 'emby://Items/${query}-artist/Images/Primary?tag=thumbnail', '${now}', '${now}');
INSERT INTO "Artwork" ("SeasonMetadataId", "ArtworkKind", "Path", "DateAdded", "DateUpdated")
  VALUES (${base + 200}, 0, 'jellyfin://Items/${query}-season/Images/Primary?tag=poster', '${now}', '${now}');
INSERT INTO "Artwork" ("EpisodeMetadataId", "ArtworkKind", "Path", "DateAdded", "DateUpdated")
  VALUES (${base + 400}, 0, 'jellyfin://Items/${query}-episode/Images/Primary?tag=poster', '${now}', '${now}');
INSERT INTO "Artwork" ("MusicVideoMetadataId", "ArtworkKind", "Path", "DateAdded", "DateUpdated")
  VALUES (${base + 600}, 1, 'emby://Items/${query}-music-video/Images/Primary?tag=thumbnail', '${now}', '${now}');
PRAGMA foreign_keys=ON;
`);

  return {
    query,
    seasonId: base + 100,
    episodeId: base + 300,
    musicVideoId: base + 500,
    showArtworkId: Number(sqlite(`SELECT "Id" FROM "Artwork" WHERE "ShowMetadataId" = ${showMetadataId} AND "ArtworkKind" = 0 ORDER BY "Id" LIMIT 1;`)),
    artistArtworkId: Number(sqlite(`SELECT "Id" FROM "Artwork" WHERE "ArtistMetadataId" = ${artistMetadataId} AND "ArtworkKind" = 1 ORDER BY "Id" LIMIT 1;`)),
  };
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
    if (
      message.type() === 'error' &&
      /hydration|failed_to_cast_marker_node|rust_panic|unreachable executed/i.test(message.text())
    ) {
      runtimeErrors.push(message.text());
    }
  });
  return runtimeErrors;
}

async function waitForHydration(page: Page): Promise<void> {
  await expect
    .poll(
      () =>
        page.evaluate(
          () => (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true,
        ),
      { timeout: 20_000 },
    )
    .toBe(true);
}

async function gotoHydrated(page: Page, route: string): Promise<void> {
  const response = await page.goto(route);
  expect(response?.status(), `GET ${route}`).toBe(200);
  await expect(page.locator('[data-not-found]')).toHaveCount(0);
  await waitForHydration(page);
}

test.describe('P7C-03 search and See All parity', () => {
  test.skip(!dbPath, 'E2E_DB_PATH or ERSATZRS_DB_PATH is required to seed search rows');

  test('preserves artwork identities, kinds, valid navigation, hydration, and mobile containment', async ({
    page,
  }, testInfo) => {
    const runtimeErrors = watchRuntimeErrors(page);
    const failedRequests: string[] = [];
    page.on('requestfailed', (request) => {
      const failure = request.failure()?.errorText ?? '';
      if (!/ERR_ABORTED|NS_BINDING_ABORTED/i.test(failure)) {
        failedRequests.push(`${request.url()} (${failure})`);
      }
    });
    const seed = seedSearchResults(testInfo);
    const encodedQuery = encodeURIComponent(seed.query);

    await page.route(/\/artwork\/(?:posters|thumbnails)\/(?:jellyfin|emby)\/\d+$/, (route) =>
      route.fulfill({ status: 200, contentType: 'image/png', body: TINY_PNG }),
    );

    await gotoHydrated(page, `/search?query=${encodedQuery}`);
    const poster = page.locator(
      `img[src="/artwork/posters/jellyfin/${seed.showArtworkId}"]`,
    );
    const thumbnail = page.locator(
      `img[src="/artwork/thumbnails/emby/${seed.artistArtworkId}"]`,
    );
    await expect(poster.first()).toHaveAttribute('src', `/artwork/posters/jellyfin/${seed.showArtworkId}`);
    await expect(thumbnail.first()).toHaveAttribute('src', `/artwork/thumbnails/emby/${seed.artistArtworkId}`);
    await poster.first().scrollIntoViewIfNeeded();
    await expect(poster.first()).toHaveJSProperty('naturalWidth', 1);
    await thumbnail.first().scrollIntoViewIfNeeded();
    await expect(thumbnail.first()).toHaveJSProperty('naturalWidth', 1);
    await page.evaluate(() => window.scrollTo(0, 0));
    await expect(page.getByRole('button', { name: 'Add All to Collection' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Add All to Playlist' })).toBeVisible();
    await page.screenshot({
      path: screenshotPath(testInfo, 'search-results-desktop'),
    });

    const allSeeAllHrefs = await page.getByRole('link', { name: /see all/i }).evaluateAll((links) =>
      links.map((link) => link.getAttribute('href') ?? ''),
    );
    expect(allSeeAllHrefs).toContain(`/media/tv/seasons?query=${encodedQuery}&page=1`);
    expect(allSeeAllHrefs).toContain(`/media/tv/episodes?query=${encodedQuery}&page=1`);
    expect(allSeeAllHrefs).toContain(`/media/music/videos?query=${encodedQuery}&page=1`);

    await gotoHydrated(page, `/media/tv/seasons?query=${encodedQuery}&page=1`);
    const seasonCard = page.getByText('Season 1', { exact: true }).first();
    await expect(seasonCard).toBeVisible();
    await expect(seasonCard.locator('xpath=ancestor::a[1]')).toHaveAttribute(
      'href',
      `/media/tv/seasons/${seed.seasonId}`,
    );
    const nextPage = page.locator(`a[href*="query=${encodedQuery}"][href*="page=2"]`).first();
    await expect(nextPage).toBeVisible();
    await nextPage.click();
    await expect(page).toHaveURL(new RegExp(`query=${encodedQuery}.*page=2`));
    const letterLink = page.getByRole('link', { name: 'A', exact: true }).first();
    await expect(letterLink).toHaveAttribute('href', new RegExp(`query=${encodedQuery}`));

    await gotoHydrated(page, `/media/tv/episodes?query=${encodedQuery}&page=1`);
    const episodeCard = page.getByText(`${seed.query} episode 1`, { exact: true }).first();
    await expect(episodeCard.locator('xpath=ancestor::a[1]')).toHaveAttribute(
      'href',
      `/media/tv/seasons/${seed.seasonId}#episode-${seed.episodeId}`,
    );

    await gotoHydrated(page, `/media/music/videos?query=${encodedQuery}&page=1`);
    const musicVideoCard = page
      .getByText(`${seed.query} music video 1`, { exact: true })
      .first()
      .locator('xpath=ancestor::div[contains(@class, "card-wrap")][1]');
    await expect(musicVideoCard).toBeVisible();
    await expect(
      musicVideoCard.locator(`a[href="/media/music/videos/${seed.musicVideoId}"]`),
    ).toHaveCount(1);
    await musicVideoCard.getByLabel('Select').click();
    await expect(page.getByText(/1 item selected/i)).toBeVisible();

    await gotoHydrated(page, `/search?query=${encodedQuery}`);
    await page.setViewportSize({ width: 390, height: 844 });
    await expect(page.getByRole('button', { name: 'Toggle navigation' })).toHaveAttribute(
      'aria-expanded',
      'false',
    );
    await expect
      .poll(() =>
        page.locator('[data-nav-drawer]').evaluate((drawer) => drawer.getBoundingClientRect().right),
      )
      .toBeLessThanOrEqual(0);
    const hasOverflow = await page.evaluate(
      () => document.documentElement.scrollWidth > document.documentElement.clientWidth + 1,
    );
    expect(hasOverflow).toBe(false);
    await page.screenshot({
      path: screenshotPath(testInfo, 'search-results-mobile'),
    });
    expect(runtimeErrors).toEqual([]);
    expect(failedRequests).toEqual([]);
  });
});
