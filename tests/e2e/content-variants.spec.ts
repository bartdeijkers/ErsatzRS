import { expect, Page, TestInfo, test } from '@playwright/test';
import { execFileSync } from 'child_process';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

type Seed = {
  mediaSourceId: number;
  libraryId: number;
  libraryPathId: number;
  movieId: number;
  movieMetadataId: number;
  movieVersionId: number;
  movieVersionMetadataId: number;
  movieMediaVersionId: number;
  movieCommentarySubtitleId: number;
  movieExtraId: number;
  movieExtraMetadataId: number;
  collectionId: number;
  playlistGroupId: number;
  playlistId: number;
  showId: number;
  seasonId: number;
  showExtraId: number;
  showExtraMetadataId: number;
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
    movieVersionId: base + 12,
    movieVersionMetadataId: base + 13,
    movieMediaVersionId: base + 14,
    movieCommentarySubtitleId: base + 15,
    movieExtraId: base + 16,
    movieExtraMetadataId: base + 17,
    collectionId: base + 36,
    playlistGroupId: base + 37,
    playlistId: base + 38,
    showId: base + 20,
    seasonId: base + 21,
    showExtraId: base + 26,
    showExtraMetadataId: base + 27,
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
    seed.movieVersionId,
    seed.movieExtraId,
    seed.showId,
    seed.seasonId,
    seed.showExtraId,
    seed.seasonExtraId,
    seed.artistId,
    seed.musicVideoId,
    seed.musicVideoVersionId,
  ].join(', ');

  sqlite(`
PRAGMA foreign_keys=OFF;
DELETE FROM "MusicVideoMetadata" WHERE "Id" IN (${seed.musicVideoMetadataId}, ${seed.musicVideoVersionMetadataId});
DELETE FROM "MusicVideo" WHERE "Id" IN (${seed.musicVideoId}, ${seed.musicVideoVersionId});
DELETE FROM "ArtistMetadata" WHERE "Id" = ${seed.artistMetadataId};
DELETE FROM "Artist" WHERE "Id" = ${seed.artistId};
DELETE FROM "PlaylistItem" WHERE "PlaylistId" = ${seed.playlistId} OR "MediaItemId" IN (${itemIds});
DELETE FROM "Playlist" WHERE "Id" = ${seed.playlistId};
DELETE FROM "PlaylistGroup" WHERE "Id" = ${seed.playlistGroupId};
DELETE FROM "CollectionItem" WHERE "CollectionId" = ${seed.collectionId} OR "MediaItemId" IN (${itemIds});
DELETE FROM "Collection" WHERE "Id" = ${seed.collectionId};
DELETE FROM "Subtitle" WHERE "Id" = ${seed.movieCommentarySubtitleId};
DELETE FROM "MediaStream" WHERE "MediaVersionId" = ${seed.movieMediaVersionId};
DELETE FROM "MediaVersion" WHERE "Id" = ${seed.movieMediaVersionId};
DELETE FROM "OtherVideoMetadata" WHERE "Id" IN (${seed.movieExtraMetadataId}, ${seed.showExtraMetadataId}, ${seed.seasonExtraMetadataId});
DELETE FROM "OtherVideo" WHERE "Id" IN (${seed.movieExtraId}, ${seed.showExtraId}, ${seed.seasonExtraId});
DELETE FROM "SeasonMetadata" WHERE "Id" = ${seed.seasonMetadataId};
DELETE FROM "ShowMetadata" WHERE "Id" = ${seed.showMetadataId};
DELETE FROM "Season" WHERE "Id" = ${seed.seasonId};
DELETE FROM "Show" WHERE "Id" = ${seed.showId};
DELETE FROM "MovieMetadata" WHERE "Id" IN (${seed.movieMetadataId}, ${seed.movieVersionMetadataId});
DELETE FROM "Movie" WHERE "Id" IN (${seed.movieId}, ${seed.movieVersionId});
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
  (${seed.movieVersionId}, ${seed.libraryPathId}, 0, 2, 'directors_cut', ${seed.movieId}),
  (${seed.movieExtraId}, ${seed.libraryPathId}, 0, 1, 'featurette', ${seed.movieId}),
  (${seed.showId}, ${seed.libraryPathId}, 0, 0, NULL, NULL),
  (${seed.seasonId}, ${seed.libraryPathId}, 0, 0, NULL, NULL),
  (${seed.showExtraId}, ${seed.libraryPathId}, 0, 1, 'trailer', ${seed.showId}),
  (${seed.seasonExtraId}, ${seed.libraryPathId}, 0, 1, 'behind_the_scenes', ${seed.seasonId}),
  (${seed.artistId}, ${seed.libraryPathId}, 0, 0, NULL, NULL),
  (${seed.musicVideoId}, ${seed.libraryPathId}, 0, 0, NULL, NULL),
  (${seed.musicVideoVersionId}, ${seed.libraryPathId}, 0, 2, 'unplugged', ${seed.musicVideoId});

INSERT INTO "Movie" ("Id") VALUES (${seed.movieId}), (${seed.movieVersionId});
INSERT INTO "MovieMetadata" (
  "Id", "MovieId", "MetadataKind", "Title", "SortTitle", "Year", "Plot", "DateAdded", "DateUpdated"
) VALUES
(
  ${seed.movieMetadataId}, ${seed.movieId}, 0, 'E2E Variant Movie', 'e2e variant movie',
  2026, 'Movie with special features.', '${now}', '${now}'
),
(
  ${seed.movieVersionMetadataId}, ${seed.movieVersionId}, 0, 'E2E Variant Movie Director''s Cut',
  'e2e variant movie directors cut', 2026, 'Alternate edition.', '${now}', '${now}'
);

INSERT INTO "MediaVersion" (
  "Id", "DateAdded", "DateUpdated", "Duration", "MovieId",
  "Height", "Width", "VideoScanKind", "SampleAspectRatio", "DisplayAspectRatio", "RFrameRate"
) VALUES (
  ${seed.movieMediaVersionId}, '${now}', '${now}', '02:00:00', ${seed.movieId},
  1080, 1920, 1, '1:1', '16:9', '24000/1001'
);
INSERT INTO "MediaStream" (
  "Index", "Codec", "Profile", "MediaStreamKind", "Language",
  "Channels", "Title", "Default", "Forced", "MediaVersionId"
) VALUES
  (0, 'h264', 'High', 1, NULL, 0, NULL, 1, 0, ${seed.movieMediaVersionId}),
  (1, 'aac', NULL, 2, 'eng', 2, 'Main Audio', 1, 0, ${seed.movieMediaVersionId}),
  (2, 'aac', NULL, 2, 'eng', 2, 'Director Commentary', 0, 0, ${seed.movieMediaVersionId}),
  (3, 'srt', NULL, 3, 'eng', 0, 'Director Commentary Subtitles', 0, 0, ${seed.movieMediaVersionId});
INSERT INTO "Subtitle" (
  "Id", "MovieMetadataId", "StreamIndex", "SubtitleKind", "Title",
  "Codec", "Language", "Default", "Forced", "Path", "IsExtracted",
  "DateAdded", "DateUpdated"
) VALUES (
  ${seed.movieCommentarySubtitleId}, ${seed.movieMetadataId}, 3, 0,
  'Director Commentary Subtitles', 'srt', 'eng', 0, 0,
  '/cache/e2e-movie-commentary.srt', 1, '${now}', '${now}'
);

INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder")
  VALUES (${seed.collectionId}, 'E2E Variant Collection', 0);
INSERT INTO "PlaylistGroup" ("Id", "Name", "IsSystem")
  VALUES (${seed.playlistGroupId}, 'E2E Variant Playlists', 0);
INSERT INTO "Playlist" ("Id", "Name", "PlaylistGroupId", "IsSystem")
  VALUES (${seed.playlistId}, 'E2E Variant Playlist', ${seed.playlistGroupId}, 0);

INSERT INTO "OtherVideo" ("Id") VALUES (${seed.movieExtraId}), (${seed.showExtraId}), (${seed.seasonExtraId});
INSERT INTO "OtherVideoMetadata" (
  "Id", "OtherVideoId", "MetadataKind", "Title", "OriginalTitle", "SortTitle", "Year", "DateAdded", "DateUpdated"
) VALUES
  (${seed.movieExtraMetadataId}, ${seed.movieExtraId}, 0, 'E2E Variant Featurette', 'Movie extra', 'e2e variant featurette', 2026, '${now}', '${now}'),
  (${seed.showExtraMetadataId}, ${seed.showExtraId}, 0, 'E2E Variant Show Extra', 'Show extra', 'e2e variant show extra', 2026, '${now}', '${now}'),
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

function isBenignAbortError(text: string): boolean {
  return text.includes('AbortError') && text.toLowerCase().includes('operation was aborted');
}

function watchRuntimeErrors(page: Page): string[] {
  const runtimeErrors: string[] = [];
  page.on('pageerror', (error) => {
    const text = error.stack ?? error.message;
    if (!isBenignAbortError(text)) {
      runtimeErrors.push(text);
    }
  });
  page.on('console', (message) => {
    if (message.type() !== 'error') {
      return;
    }
    const text = message.text();
    if (
      text.includes('failed_to_cast_marker_node') ||
      text.includes('rust_panic') ||
      text.includes('unreachable executed') ||
      text.toLowerCase().includes('hydration')
    ) {
      runtimeErrors.push(text);
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
      { timeout: 15_000 },
    )
    .toBe(true);
}

async function gotoHydrated(page: Page, route: string): Promise<void> {
  const response = await page.goto(route);
  expect(response?.status(), `GET ${route}`).toBe(200);
  await expect(page.locator('[data-not-found]')).toHaveCount(0);
  await waitForHydration(page);
}

test.describe('content variants UI surfacing', () => {
  test.skip(!dbPath, 'E2E_DB_PATH or ERSATZRS_DB_PATH is required to seed content variants');

  test('browse visibility, detail sections, search help, and no-JS rendering stay usable', async ({
    browser,
    baseURL,
    page,
  }, testInfo) => {
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedContentVariants(testInfo);
    await forceEnglish(page);

    await page.setViewportSize({ width: 2048, height: 1024 });
    await gotoHydrated(page, '/media/other/videos?query=E2E%20Variant%20Featurette');
    await expect(page.getByRole('heading', { name: 'Other Videos' })).toBeVisible();
    await expect(page.getByLabel('Show special features / versions')).toHaveCount(0);
    await expect(page.getByText('E2E Variant Featurette').first()).toBeVisible();
    await expect(page.getByText('Extra: Featurette').first()).toBeVisible();
    await page.screenshot({
      path: screenshotPath(testInfo, 'content-variants-browse-desktop'),
      fullPage: true,
    });

    await gotoHydrated(page, `/media/movies/${seed.movieId}`);
    await expect(page.getByRole('button', { name: 'Add To Collection' }).first()).toBeVisible();
    await expect(page.getByRole('button', { name: 'Add To Playlist' }).first()).toBeVisible();
    await expect(page.getByRole('button', { name: 'Show Media Info' }).first()).toBeVisible();
    await expect(page.getByRole('link', { name: 'Troubleshoot Playback' }).first()).toHaveAttribute(
      'href',
      `/system/troubleshooting/playback?mediaItem=${seed.movieId}`,
    );
    await expect(page.getByRole('heading', { name: 'Extras / Special Features' })).toBeVisible();
    const movieSpecialRow = page.locator('[data-media-special-row]', {
      hasText: 'E2E Variant Featurette',
    });
    await expect(movieSpecialRow).toBeVisible();
    await expect(movieSpecialRow.getByText('Extra: Featurette')).toBeVisible();
    await expect(movieSpecialRow.getByRole('button', { name: 'Add To Collection' })).toBeVisible();
    await expect(movieSpecialRow.getByRole('button', { name: 'Add To Playlist' })).toBeVisible();
    await expect(movieSpecialRow.getByRole('button', { name: 'Show Media Info' })).toBeVisible();
    await expect(movieSpecialRow.getByRole('link', { name: 'Troubleshoot Playback' })).toHaveAttribute(
      'href',
      `/system/troubleshooting/playback?mediaItem=${seed.movieExtraId}`,
    );
    await expect(page.getByRole('heading', { name: 'Versions' })).toBeVisible();
    const movieVersionCard = page.locator('[data-movie-version-card]', {
      hasText: "E2E Variant Movie Director's Cut",
    });
    await expect(movieVersionCard).toBeVisible();
    await expect(movieVersionCard.getByText("Version: Director's Cut")).toBeVisible();
    await expect(movieVersionCard.getByRole('link', { name: 'Details' })).toHaveAttribute(
      'href',
      `/media/movies/${seed.movieVersionId}`,
    );
    await expect(page.getByRole('heading', { name: 'Playback' })).toBeVisible();
    const commentaryCard = page.locator('[data-commentary-track-card]', {
      hasText: 'Director Commentary',
    });
    await expect(commentaryCard).toBeVisible();
    await expect(commentaryCard.getByText('Subtitle: Director Commentary Subtitles')).toBeVisible();
    await expect(commentaryCard.getByRole('button', { name: 'Add To Collection' })).toBeVisible();
    await expect(commentaryCard.getByRole('button', { name: 'Add To Playlist' })).toBeVisible();
    await expect(commentaryCard.getByRole('link', { name: 'Play' })).toHaveAttribute(
      'href',
      new RegExp(`/system/troubleshooting/playback\\?mediaItem=${seed.movieId}&preferredAudioTitle=Director%20Commentary&subtitleId=${seed.movieCommentarySubtitleId}`),
    );
    const commentarySubtitleCard = page.locator('[data-commentary-subtitle-card]', {
      hasText: 'Director Commentary Subtitles',
    });
    await expect(commentarySubtitleCard).toBeVisible();
    await expect(commentarySubtitleCard.getByRole('link', { name: 'Play' })).toHaveAttribute(
      'href',
      `/system/troubleshooting/playback?mediaItem=${seed.movieId}&subtitleId=${seed.movieCommentarySubtitleId}`,
    );
    await commentaryCard.getByRole('link', { name: 'Play' }).click();
    await expect(page.locator('input[name="preferredAudioTitle"]')).toHaveValue('Director Commentary');
    await expect(page.locator('select[name="subtitleId"]')).toHaveValue(
      String(seed.movieCommentarySubtitleId),
    );

    await gotoHydrated(page, `/media/tv/shows/${seed.showId}`);
    await expect(page.getByRole('button', { name: 'Add To Collection' }).first()).toBeVisible();
    await expect(page.getByRole('button', { name: 'Add To Playlist' }).first()).toBeVisible();
    await expect(page.getByRole('button', { name: 'Add To Schedule' }).first()).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Extras / Special Features' })).toBeVisible();
    const showSpecialRow = page.locator('[data-media-special-row]', {
      hasText: 'E2E Variant Show Extra',
    });
    await expect(showSpecialRow).toBeVisible();
    await expect(showSpecialRow.getByText('Extra: Trailer')).toBeVisible();
    await expect(showSpecialRow.getByRole('button', { name: 'Add To Collection' })).toBeVisible();
    await expect(showSpecialRow.getByRole('button', { name: 'Add To Playlist' })).toBeVisible();
    await expect(showSpecialRow.getByRole('button', { name: 'Show Media Info' })).toBeVisible();
    await expect(showSpecialRow.getByRole('link', { name: 'Troubleshoot Playback' })).toHaveAttribute(
      'href',
      `/system/troubleshooting/playback?mediaItem=${seed.showExtraId}`,
    );

    await gotoHydrated(page, `/media/tv/seasons/${seed.seasonId}`);
    await expect(page.getByRole('button', { name: 'Add To Collection' }).first()).toBeVisible();
    await expect(page.getByRole('button', { name: 'Add To Playlist' }).first()).toBeVisible();
    await expect(page.getByRole('button', { name: 'Add To Schedule' }).first()).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Extras / Special Features' })).toBeVisible();
    const seasonSpecialRow = page.locator('[data-media-special-row]', {
      hasText: 'E2E Variant Season Extra',
    });
    await expect(seasonSpecialRow).toBeVisible();
    await expect(seasonSpecialRow.getByText('Extra: Behind the Scenes')).toBeVisible();
    await expect(seasonSpecialRow.getByRole('button', { name: 'Add To Collection' })).toBeVisible();
    await expect(seasonSpecialRow.getByRole('button', { name: 'Add To Playlist' })).toBeVisible();
    await expect(seasonSpecialRow.getByRole('button', { name: 'Show Media Info' })).toBeVisible();
    await expect(seasonSpecialRow.getByRole('link', { name: 'Troubleshoot Playback' })).toHaveAttribute(
      'href',
      `/system/troubleshooting/playback?mediaItem=${seed.seasonExtraId}`,
    );

    await gotoHydrated(page, `/media/music/artists/${seed.artistId}`);
    await expect(page.getByRole('button', { name: 'Add To Collection' }).first()).toBeVisible();
    await expect(page.getByRole('button', { name: 'Add To Schedule' }).first()).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Versions' })).toBeVisible();
    await expect(page.getByText('E2E Variant Song Unplugged').first()).toBeVisible();

    await gotoHydrated(page, `/search?query=${encodeURIComponent('content_kind:extra AND sub_kind:featurette')}`);
    await expect(page.getByText('Variant tokens: content_kind:extra')).toBeVisible();
    await expect(page.getByText('E2E Variant Featurette').first()).toBeVisible();
    await expect(page.getByText('Extra: Featurette').first()).toBeVisible();

    await page.setViewportSize({ width: 390, height: 844 });
    await gotoHydrated(page, '/media/other/videos?query=E2E%20Variant%20Featurette');
    await expect(page.getByText('E2E Variant Featurette').first()).toBeVisible();
    await page.screenshot({
      path: screenshotPath(testInfo, 'content-variants-browse-mobile'),
      fullPage: true,
    });
    expect(runtimeErrors).toEqual([]);

    const context = await browser.newContext({ javaScriptEnabled: false, baseURL });
    const noJsPage = await context.newPage();
    const response = await noJsPage.goto('/media/other/videos?query=E2E%20Variant%20Featurette');
    expect(await response?.text()).toContain('E2E Variant Featurette');
    await context.close();
  });
});
