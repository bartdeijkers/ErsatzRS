import { expect, Page, TestInfo, test } from '@playwright/test';
import { execFileSync } from 'child_process';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

type Seed = {
  mediaSourceId: number;
  libraryId: number;
  libraryPathId: number;
  movieId: number;
  movieMetadataId: number;
  movieExtraId: number;
  movieExtraMetadataId: number;
  showId: number;
  seasonId: number;
  showMetadataId: number;
  seasonMetadataId: number;
  seasonExtraId: number;
  seasonExtraMetadataId: number;
  artistId: number;
  artistMetadataId: number;
  musicVideoId: number;
  musicVideoVersionId: number;
  musicVideoMetadataId: number;
  musicVideoVersionMetadataId: number;
};

function sqlite(sql: string): string {
  if (!dbPath) {
    throw new Error('E2E_DB_PATH or ERSATZRS_DB_PATH is required for content-variant seeding');
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
  const base = 250_000 + projectOffset;
  return {
    mediaSourceId: base + 1,
    libraryId: base + 2,
    libraryPathId: base + 3,
    movieId: base + 10,
    movieMetadataId: base + 11,
    movieExtraId: base + 12,
    movieExtraMetadataId: base + 13,
    showId: base + 20,
    seasonId: base + 21,
    showMetadataId: base + 22,
    seasonMetadataId: base + 23,
    seasonExtraId: base + 24,
    seasonExtraMetadataId: base + 25,
    artistId: base + 30,
    artistMetadataId: base + 31,
    musicVideoId: base + 32,
    musicVideoVersionId: base + 33,
    musicVideoMetadataId: base + 34,
    musicVideoVersionMetadataId: base + 35,
  };
}

function seedContentVariants(testInfo: TestInfo): Seed {
  const seed = seedForProject(testInfo);
  const now = '2026-06-24T00:00:00Z';
  const itemIds = [
    seed.movieId,
    seed.movieExtraId,
    seed.showId,
    seed.seasonId,
    seed.seasonExtraId,
    seed.artistId,
    seed.musicVideoId,
    seed.musicVideoVersionId,
  ].join(', ');

  sqlite(`
PRAGMA foreign_keys=OFF;
DELETE FROM "ConfigElement" WHERE "Key" = 'pages.media.show_content_variants';
DELETE FROM "MusicVideoMetadata" WHERE "Id" IN (${seed.musicVideoMetadataId}, ${seed.musicVideoVersionMetadataId});
DELETE FROM "MusicVideo" WHERE "Id" IN (${seed.musicVideoId}, ${seed.musicVideoVersionId});
DELETE FROM "ArtistMetadata" WHERE "Id" = ${seed.artistMetadataId};
DELETE FROM "Artist" WHERE "Id" = ${seed.artistId};
DELETE FROM "OtherVideoMetadata" WHERE "Id" IN (${seed.movieExtraMetadataId}, ${seed.seasonExtraMetadataId});
DELETE FROM "OtherVideo" WHERE "Id" IN (${seed.movieExtraId}, ${seed.seasonExtraId});
DELETE FROM "SeasonMetadata" WHERE "Id" = ${seed.seasonMetadataId};
DELETE FROM "ShowMetadata" WHERE "Id" = ${seed.showMetadataId};
DELETE FROM "Season" WHERE "Id" = ${seed.seasonId};
DELETE FROM "Show" WHERE "Id" = ${seed.showId};
DELETE FROM "MovieMetadata" WHERE "Id" = ${seed.movieMetadataId};
DELETE FROM "Movie" WHERE "Id" = ${seed.movieId};
DELETE FROM "MediaItem" WHERE "Id" IN (${itemIds});
DELETE FROM "LibraryPath" WHERE "Id" = ${seed.libraryPathId};
DELETE FROM "LocalLibrary" WHERE "Id" = ${seed.libraryId};
DELETE FROM "Library" WHERE "Id" = ${seed.libraryId};
DELETE FROM "LocalMediaSource" WHERE "Id" = ${seed.mediaSourceId};
DELETE FROM "MediaSource" WHERE "Id" = ${seed.mediaSourceId};

INSERT INTO "MediaSource" ("Id") VALUES (${seed.mediaSourceId});
INSERT INTO "LocalMediaSource" ("Id") VALUES (${seed.mediaSourceId});
INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
  VALUES (${seed.libraryId}, 'E2E Content Variants', 4, ${seed.mediaSourceId});
INSERT INTO "LocalLibrary" ("Id") VALUES (${seed.libraryId});
INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId")
  VALUES (${seed.libraryPathId}, '/tmp/ersatzrs-e2e-content-variants/${testInfo.project.name}', ${seed.libraryId});

INSERT INTO "MediaItem" (
  "Id", "LibraryPathId", "State", "ContentKind", "SubKind", "ParentMediaItemId"
) VALUES
  (${seed.movieId}, ${seed.libraryPathId}, 0, 0, NULL, NULL),
  (${seed.movieExtraId}, ${seed.libraryPathId}, 0, 1, 'featurette', ${seed.movieId}),
  (${seed.showId}, ${seed.libraryPathId}, 0, 0, NULL, NULL),
  (${seed.seasonId}, ${seed.libraryPathId}, 0, 0, NULL, NULL),
  (${seed.seasonExtraId}, ${seed.libraryPathId}, 0, 1, 'behind_the_scenes', ${seed.seasonId}),
  (${seed.artistId}, ${seed.libraryPathId}, 0, 0, NULL, NULL),
  (${seed.musicVideoId}, ${seed.libraryPathId}, 0, 0, NULL, NULL),
  (${seed.musicVideoVersionId}, ${seed.libraryPathId}, 0, 2, 'unplugged', ${seed.musicVideoId});

INSERT INTO "Movie" ("Id") VALUES (${seed.movieId});
INSERT INTO "MovieMetadata" (
  "Id", "MovieId", "MetadataKind", "Title", "SortTitle", "Year", "Plot", "DateAdded", "DateUpdated"
) VALUES (
  ${seed.movieMetadataId}, ${seed.movieId}, 0, 'E2E Variant Movie', 'e2e variant movie',
  2026, 'Movie with special features.', '${now}', '${now}'
);

INSERT INTO "OtherVideo" ("Id") VALUES (${seed.movieExtraId}), (${seed.seasonExtraId});
INSERT INTO "OtherVideoMetadata" (
  "Id", "OtherVideoId", "MetadataKind", "Title", "OriginalTitle", "SortTitle", "Year", "DateAdded", "DateUpdated"
) VALUES
  (${seed.movieExtraMetadataId}, ${seed.movieExtraId}, 0, 'E2E Variant Featurette', 'Movie extra', 'e2e variant featurette', 2026, '${now}', '${now}'),
  (${seed.seasonExtraMetadataId}, ${seed.seasonExtraId}, 0, 'E2E Variant Season Extra', 'Season extra', 'e2e variant season extra', 2026, '${now}', '${now}');

INSERT INTO "Show" ("Id") VALUES (${seed.showId});
INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber")
  VALUES (${seed.seasonId}, ${seed.showId}, 1);
INSERT INTO "ShowMetadata" (
  "Id", "ShowId", "MetadataKind", "Title", "SortTitle", "Year", "Plot", "DateAdded", "DateUpdated"
) VALUES (
  ${seed.showMetadataId}, ${seed.showId}, 0, 'E2E Variant Show', 'e2e variant show',
  2026, 'Show with special features.', '${now}', '${now}'
);
INSERT INTO "SeasonMetadata" (
  "Id", "SeasonId", "MetadataKind", "Title", "SortTitle", "DateAdded", "DateUpdated"
) VALUES (
  ${seed.seasonMetadataId}, ${seed.seasonId}, 0, 'Season 1', 'season 1', '${now}', '${now}'
);

INSERT INTO "Artist" ("Id") VALUES (${seed.artistId});
INSERT INTO "ArtistMetadata" (
  "Id", "ArtistId", "MetadataKind", "Title", "SortTitle", "Biography", "DateAdded", "DateUpdated"
) VALUES (
  ${seed.artistMetadataId}, ${seed.artistId}, 0, 'E2E Variant Artist',
  'e2e variant artist', 'Artist with alternate versions.', '${now}', '${now}'
);
INSERT INTO "MusicVideo" ("Id", "ArtistId")
  VALUES (${seed.musicVideoId}, ${seed.artistId}), (${seed.musicVideoVersionId}, ${seed.artistId});
INSERT INTO "MusicVideoMetadata" (
  "Id", "MusicVideoId", "MetadataKind", "Title", "SortTitle", "Album", "Plot", "DateAdded", "DateUpdated"
) VALUES
  (${seed.musicVideoMetadataId}, ${seed.musicVideoId}, 0, 'E2E Variant Song', 'e2e variant song', 'E2E Album', 'Primary cut.', '${now}', '${now}'),
  (${seed.musicVideoVersionMetadataId}, ${seed.musicVideoVersionId}, 0, 'E2E Variant Song Unplugged', 'e2e variant song unplugged', 'E2E Album', 'Alternate cut.', '${now}', '${now}');
PRAGMA foreign_keys=ON;
`);

  return seed;
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

function screenshotPath(testInfo: TestInfo, name: string): string {
  return `test-results/${name}-${testInfo.project.name}.png`;
}

test.describe('content variants UI surfacing', () => {
  test.skip(!dbPath, 'E2E_DB_PATH or ERSATZRS_DB_PATH is required to seed content variants');

  test('browse opt-in, detail sections, search help, and no-JS rendering stay usable', async ({
    browser,
    baseURL,
    page,
  }, testInfo) => {
    const seed = seedContentVariants(testInfo);
    await forceEnglish(page);

    await page.setViewportSize({ width: 2048, height: 1024 });
    await page.goto('/media/other/videos?query=E2E%20Variant%20Featurette');
    await expect(page.getByRole('heading', { name: 'Other Videos' })).toBeVisible();
    await expect(page.getByLabel('Show special features / versions')).toBeVisible();
    await expect(page.getByText('E2E Variant Featurette')).toHaveCount(0);

    await page.goto('/media/other/videos?query=E2E%20Variant%20Featurette&show_variants=1');
    await expect(page.getByText('E2E Variant Featurette').first()).toBeVisible();
    await expect(page.getByText('Extra: Featurette').first()).toBeVisible();
    await page.screenshot({
      path: screenshotPath(testInfo, 'content-variants-browse-desktop'),
      fullPage: true,
    });

    await page.goto(`/media/movies/${seed.movieId}`);
    await expect(page.getByRole('heading', { name: 'Extras / Special Features' })).toBeVisible();
    await expect(page.getByText('E2E Variant Featurette').first()).toBeVisible();
    await expect(page.getByText('Extra: Featurette').first()).toBeVisible();

    await page.goto(`/media/tv/seasons/${seed.seasonId}`);
    await expect(page.getByRole('heading', { name: 'Extras / Special Features' })).toBeVisible();
    await expect(page.getByText('E2E Variant Season Extra').first()).toBeVisible();
    await expect(page.getByText('Extra: Behind the Scenes').first()).toBeVisible();

    await page.goto(`/media/music/artists/${seed.artistId}`);
    await expect(page.getByRole('heading', { name: 'Versions' })).toBeVisible();
    await expect(page.getByText('E2E Variant Song Unplugged').first()).toBeVisible();

    await page.goto(`/search?query=${encodeURIComponent('content_kind:extra AND sub_kind:featurette')}`);
    await expect(page.getByText('Variant tokens: content_kind:extra')).toBeVisible();
    await expect(page.getByText('E2E Variant Featurette').first()).toBeVisible();
    await expect(page.getByText('Extra: Featurette').first()).toBeVisible();

    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto('/media/other/videos?query=E2E%20Variant%20Featurette&show_variants=1');
    await expect(page.getByText('E2E Variant Featurette').first()).toBeVisible();
    await page.screenshot({
      path: screenshotPath(testInfo, 'content-variants-browse-mobile'),
      fullPage: true,
    });

    const context = await browser.newContext({ javaScriptEnabled: false, baseURL });
    const noJsPage = await context.newPage();
    const response = await noJsPage.goto('/media/other/videos?query=E2E%20Variant%20Featurette&show_variants=1');
    expect(await response?.text()).toContain('E2E Variant Featurette');
    await context.close();
  });
});
