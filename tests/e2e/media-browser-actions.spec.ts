import { expect, Locator, Page, TestInfo, test } from '@playwright/test';
import { execFileSync } from 'child_process';
import { mkdirSync, writeFileSync } from 'fs';
import path from 'path';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

type Seed = {
  mediaSourceId: number;
  libraryId: number;
  libraryPathId: number;
  collectionId: number;
  collectionName: string;
  playlistGroupId: number;
  playlistGroupName: string;
  playlistId: number;
  playlistName: string;
  artistId: number;
  artistMetadataId: number;
  musicVideoId: number;
  musicVideoMetadataId: number;
  songId: number;
  songMetadataId: number;
  imageId: number;
  imageMetadataId: number;
  artistTitle: string;
  musicVideoTitle: string;
  songTitle: string;
  imageTitle: string;
};

const TINY_PNG = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==',
  'base64',
);

function projectBase(testInfo: TestInfo): number {
  const projectOffset = Array.from(testInfo.project.name).reduce(
    (total, char) => total + char.charCodeAt(0),
    0,
  ) * 100;
  return 360_000 + projectOffset;
}

function sqlEscape(value: string): string {
  return value.replace(/'/g, "''");
}

function sqlite(sql: string): string {
  if (!dbPath) {
    throw new Error('E2E_DB_PATH or ERSATZRS_DB_PATH is required for media-browser seeding');
  }
  return execFileSync('sqlite3', [dbPath], {
    input: `.timeout 10000\n${sql}`,
    encoding: 'utf8',
  }).trim();
}

function writeArtworkFixture(fileName: string): void {
  if (!dbPath) {
    throw new Error('E2E_DB_PATH or ERSATZRS_DB_PATH is required for artwork fixtures');
  }
  const root = path.join(path.dirname(dbPath), 'home', '.ersatzrs', 'data', 'artwork');
  for (const kind of ['thumbnails', 'fanart', 'posters']) {
    mkdirSync(path.join(root, kind), { recursive: true });
    writeFileSync(path.join(root, kind, fileName), TINY_PNG);
  }
}

function seedForProject(testInfo: TestInfo): Seed {
  const base = projectBase(testInfo);
  const suffix = testInfo.project.name;
  return {
    mediaSourceId: base + 1,
    libraryId: base + 2,
    libraryPathId: base + 3,
    collectionId: base + 4,
    collectionName: `E2E Browser Collection ${suffix}`,
    playlistGroupId: base + 5,
    playlistGroupName: `E2E Browser Playlist Group ${suffix}`,
    playlistId: base + 6,
    playlistName: `E2E Browser Playlist ${suffix}`,
    artistId: base + 10,
    artistMetadataId: base + 11,
    musicVideoId: base + 12,
    musicVideoMetadataId: base + 13,
    songId: base + 14,
    songMetadataId: base + 15,
    imageId: base + 16,
    imageMetadataId: base + 17,
    artistTitle: `E2E Browser Artist ${suffix}`,
    musicVideoTitle: `E2E Browser Music Video ${suffix}`,
    songTitle: `E2E Browser Song ${suffix}`,
    imageTitle: `E2E Browser Image ${suffix}`,
  };
}

function seedMediaBrowserActions(testInfo: TestInfo): Seed {
  const seed = seedForProject(testInfo);
  const now = '2026-07-01T00:00:00Z';
  const itemIds = [seed.artistId, seed.musicVideoId, seed.songId, seed.imageId].join(', ');
  const artwork = {
    artistThumb: `e2e-artist-thumb-${seed.artistId}.png`,
    artistFanart: `e2e-artist-fanart-${seed.artistId}.png`,
    musicVideoThumb: `e2e-music-video-thumb-${seed.musicVideoId}.png`,
    songThumb: `e2e-song-thumb-${seed.songId}.png`,
    imageThumb: `e2e-image-thumb-${seed.imageId}.png`,
  };

  Object.values(artwork).forEach(writeArtworkFixture);

  sqlite(`
PRAGMA foreign_keys=OFF;
DELETE FROM "PlaylistItem" WHERE "PlaylistId" = ${seed.playlistId} OR "MediaItemId" IN (${itemIds});
DELETE FROM "Playlist" WHERE "Id" = ${seed.playlistId};
DELETE FROM "PlaylistGroup" WHERE "Id" = ${seed.playlistGroupId};
DELETE FROM "CollectionItem" WHERE "CollectionId" = ${seed.collectionId} OR "MediaItemId" IN (${itemIds});
DELETE FROM "Collection" WHERE "Id" = ${seed.collectionId};
DELETE FROM "Artwork" WHERE "ArtistMetadataId" = ${seed.artistMetadataId}
   OR "MusicVideoMetadataId" = ${seed.musicVideoMetadataId}
   OR "SongMetadataId" = ${seed.songMetadataId}
   OR "ImageMetadataId" = ${seed.imageMetadataId};
DELETE FROM "MusicVideoMetadata" WHERE "Id" = ${seed.musicVideoMetadataId};
DELETE FROM "MusicVideo" WHERE "Id" = ${seed.musicVideoId};
DELETE FROM "SongMetadata" WHERE "Id" = ${seed.songMetadataId};
DELETE FROM "Song" WHERE "Id" = ${seed.songId};
DELETE FROM "ImageMetadata" WHERE "Id" = ${seed.imageMetadataId};
DELETE FROM "Image" WHERE "Id" = ${seed.imageId};
DELETE FROM "ArtistMetadata" WHERE "Id" = ${seed.artistMetadataId};
DELETE FROM "Artist" WHERE "Id" = ${seed.artistId};
DELETE FROM "MediaItem" WHERE "Id" IN (${itemIds});
DELETE FROM "LibraryPath" WHERE "Id" = ${seed.libraryPathId};
DELETE FROM "LocalLibrary" WHERE "Id" = ${seed.libraryId};
DELETE FROM "Library" WHERE "Id" = ${seed.libraryId};
DELETE FROM "LocalMediaSource" WHERE "Id" = ${seed.mediaSourceId};
DELETE FROM "MediaSource" WHERE "Id" = ${seed.mediaSourceId};

INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder")
  VALUES (${seed.collectionId}, '${sqlEscape(seed.collectionName)}', 0);
INSERT INTO "PlaylistGroup" ("Id", "Name", "IsSystem")
  VALUES (${seed.playlistGroupId}, '${sqlEscape(seed.playlistGroupName)}', 0);
INSERT INTO "Playlist" ("Id", "Name", "PlaylistGroupId", "IsSystem")
  VALUES (${seed.playlistId}, '${sqlEscape(seed.playlistName)}', ${seed.playlistGroupId}, 0);

INSERT INTO "MediaSource" ("Id") VALUES (${seed.mediaSourceId});
INSERT INTO "LocalMediaSource" ("Id") VALUES (${seed.mediaSourceId});
INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
  VALUES (${seed.libraryId}, 'E2E Browser Actions', 4, ${seed.mediaSourceId});
INSERT INTO "LocalLibrary" ("Id") VALUES (${seed.libraryId});
INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId")
  VALUES (${seed.libraryPathId}, '/e2e/media-browser-actions/${testInfo.project.name}', ${seed.libraryId});

INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State", "ContentKind", "SubKind", "ParentMediaItemId")
  VALUES
  (${seed.artistId}, ${seed.libraryPathId}, 0, 0, NULL, NULL),
  (${seed.musicVideoId}, ${seed.libraryPathId}, 0, 0, NULL, NULL),
  (${seed.songId}, ${seed.libraryPathId}, 0, 0, NULL, NULL),
  (${seed.imageId}, ${seed.libraryPathId}, 0, 0, NULL, NULL);

INSERT INTO "Artist" ("Id") VALUES (${seed.artistId});
INSERT INTO "ArtistMetadata" (
  "Id", "ArtistId", "MetadataKind", "Title", "SortTitle", "Biography", "DateAdded", "DateUpdated"
) VALUES (
  ${seed.artistMetadataId}, ${seed.artistId}, 0, '${sqlEscape(seed.artistTitle)}',
  '${sqlEscape(seed.artistTitle.toLowerCase())}', 'Artist detail action proof.', '${now}', '${now}'
);

INSERT INTO "MusicVideo" ("Id", "ArtistId") VALUES (${seed.musicVideoId}, ${seed.artistId});
INSERT INTO "MusicVideoMetadata" (
  "Id", "MusicVideoId", "MetadataKind", "Title", "SortTitle", "Album", "Plot", "DateAdded", "DateUpdated"
) VALUES (
  ${seed.musicVideoMetadataId}, ${seed.musicVideoId}, 0, '${sqlEscape(seed.musicVideoTitle)}',
  '${sqlEscape(seed.musicVideoTitle.toLowerCase())}', 'Browser Proof', 'Music-video artwork proof.',
  '${now}', '${now}'
);

INSERT INTO "Song" ("Id") VALUES (${seed.songId});
INSERT INTO "SongMetadata" (
  "Id", "SongId", "MetadataKind", "Title", "SortTitle", "Album", "Artists", "DateAdded", "DateUpdated"
) VALUES (
  ${seed.songMetadataId}, ${seed.songId}, 0, '${sqlEscape(seed.songTitle)}',
  '${sqlEscape(seed.songTitle.toLowerCase())}', 'Browser Proof', '${sqlEscape(seed.artistTitle)}',
  '${now}', '${now}'
);

INSERT INTO "Image" ("Id") VALUES (${seed.imageId});
INSERT INTO "ImageMetadata" (
  "Id", "ImageId", "MetadataKind", "Title", "SortTitle", "ReleaseDate", "DateAdded", "DateUpdated"
) VALUES (
  ${seed.imageMetadataId}, ${seed.imageId}, 0, '${sqlEscape(seed.imageTitle)}',
  '${sqlEscape(seed.imageTitle.toLowerCase())}', '2026-07-01', '${now}', '${now}'
);

INSERT INTO "Artwork" (
  "ArtistMetadataId", "ArtworkKind", "Path", "DateAdded", "DateUpdated"
) VALUES
  (${seed.artistMetadataId}, 1, '${artwork.artistThumb}', '${now}', '${now}'),
  (${seed.artistMetadataId}, 3, '${artwork.artistFanart}', '${now}', '${now}');
INSERT INTO "Artwork" (
  "MusicVideoMetadataId", "ArtworkKind", "Path", "DateAdded", "DateUpdated"
) VALUES (${seed.musicVideoMetadataId}, 1, '${artwork.musicVideoThumb}', '${now}', '${now}');
INSERT INTO "Artwork" (
  "SongMetadataId", "ArtworkKind", "Path", "DateAdded", "DateUpdated"
) VALUES (${seed.songMetadataId}, 1, '${artwork.songThumb}', '${now}', '${now}');
INSERT INTO "Artwork" (
  "ImageMetadataId", "ArtworkKind", "Path", "DateAdded", "DateUpdated"
) VALUES (${seed.imageMetadataId}, 1, '${artwork.imageThumb}', '${now}', '${now}');
PRAGMA foreign_keys=ON;
`);

  return seed;
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

async function expectLoadedImage(locator: Locator): Promise<void> {
  await expect(locator).toBeVisible();
  await expect
    .poll(
      () =>
        locator.evaluate(
          (element) =>
            element instanceof HTMLImageElement && element.complete && element.naturalWidth > 0,
        ),
      { timeout: 10_000 },
    )
    .toBe(true);
}

function dialogWithHeading(page: Page, heading: RegExp | string): Locator {
  return page
    .locator('div[class*="panel"]')
    .filter({ has: page.getByRole('heading', { name: heading }) })
    .last();
}

async function chooseExistingCollection(page: Page, seed: Seed): Promise<void> {
  const dialog = dialogWithHeading(page, /^add to collection$/i);
  await expect(dialog).toBeVisible();
  await expect(dialog.locator('select').first()).toContainText(seed.collectionName);
  await dialog.locator('select').first().selectOption({ label: seed.collectionName });
  await dialog.getByRole('button', { name: /^add to collection$/i }).click();
}

async function chooseExistingPlaylist(page: Page, seed: Seed): Promise<void> {
  const dialog = dialogWithHeading(page, /^add to playlist$/i);
  await expect(dialog).toBeVisible();
  await expect(dialog.locator('select').first()).toContainText(seed.playlistGroupName);
  await dialog.locator('select').first().selectOption({ label: seed.playlistGroupName });
  await expect(dialog.locator('select').nth(1)).toContainText(seed.playlistName);
  await dialog.locator('select').nth(1).selectOption({ label: seed.playlistName });
  await dialog.getByRole('button', { name: /^add to playlist$/i }).click();
}

async function expectSelectAllRightAligned(page: Page): Promise<void> {
  const metrics = await page.locator('main').evaluate((main) => {
    const buttons = Array.from(main.querySelectorAll('button'));
    const selectAll = buttons.find((button) => button.textContent?.includes('Select All'));
    const heading = main.querySelector('h1');
    if (!selectAll || !heading) {
      return null;
    }
    const selectAllBox = selectAll.getBoundingClientRect();
    const headingBox = heading.getBoundingClientRect();
    const mainBox = main.getBoundingClientRect();
    return {
      headingRight: headingBox.right,
      mainLeft: mainBox.left,
      mainWidth: mainBox.width,
      selectAllLeft: selectAllBox.left,
    };
  });
  expect(metrics).not.toBeNull();
  expect(metrics?.selectAllLeft).toBeGreaterThan(metrics?.headingRight ?? 0);
  expect(metrics?.selectAllLeft).toBeGreaterThan(
    (metrics?.mainLeft ?? 0) + (metrics?.mainWidth ?? 0) * 0.55,
  );
}

test.describe('media browser action parity', () => {
  test.skip(!dbPath, 'E2E_DB_PATH or ERSATZRS_DB_PATH is required to seed media-browser rows');

  test('songs and images expose movie-browser style selection and item actions', async ({
    page,
  }, testInfo) => {
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedMediaBrowserActions(testInfo);
    await forceEnglish(page);

    await gotoHydrated(page, `/media/music/songs?query=${encodeURIComponent(seed.songTitle)}`);
    await expect(page.getByRole('heading', { name: 'Songs' })).toBeVisible();
    await expect(page.getByText(seed.songTitle)).toBeVisible();
    await expectSelectAllRightAligned(page);
    await page.getByRole('button', { name: /item actions/i }).first().click();
    await expect(page.getByRole('menuitem', { name: /^add to collection$/i })).toBeVisible();
    await expect(page.getByRole('menuitem', { name: /^show media info$/i })).toBeVisible();
    await expect(page.getByRole('menuitem', { name: /^troubleshoot playback$/i })).toHaveAttribute(
      'href',
      `/system/troubleshooting/playback?mediaItem=${seed.songId}`,
    );
    await page.getByRole('menuitem', { name: /^add to collection$/i }).click();
    await chooseExistingCollection(page, seed);
    await expect.poll(() => sqlite(`
SELECT COUNT(*) FROM "CollectionItem"
WHERE "CollectionId" = ${seed.collectionId} AND "MediaItemId" = ${seed.songId};
`)).toBe('1');

    await gotoHydrated(page, `/media/music/songs?query=${encodeURIComponent(seed.songTitle)}`);
    await page.locator('main input[type="checkbox"]').first().check({ force: true });
    await expect(page.locator('main [class*="selection-count"]')).toHaveText(/1 item selected/i);
    await page.getByRole('button', { name: /add to playlist/i }).click();
    await chooseExistingPlaylist(page, seed);
    await expect.poll(() => sqlite(`
SELECT "CollectionType" FROM "PlaylistItem"
WHERE "PlaylistId" = ${seed.playlistId} AND "MediaItemId" = ${seed.songId}
ORDER BY "Id" DESC LIMIT 1;
`)).toBe('50');

    await gotoHydrated(page, `/media/browser/images?query=${encodeURIComponent(seed.imageTitle)}`);
    await expect(page.getByRole('heading', { name: 'Images' })).toBeVisible();
    await expect(page.getByText(seed.imageTitle)).toBeVisible();
    await expectSelectAllRightAligned(page);
    await page.getByRole('button', { name: /item actions/i }).first().click();
    await expect(page.getByRole('menuitem', { name: /^add to collection$/i })).toBeVisible();
    await expect(page.getByRole('menuitem', { name: /^show media info$/i })).toBeVisible();
    await expect(page.getByRole('menuitem', { name: /^troubleshoot playback$/i })).toHaveAttribute(
      'href',
      `/system/troubleshooting/playback?mediaItem=${seed.imageId}`,
    );
    await page.locator('main input[type="checkbox"]').first().check({ force: true });
    await expect(page.locator('main [class*="selection-count"]')).toHaveText(/1 item selected/i);
    await page.getByRole('button', { name: /add to playlist/i }).click();
    await chooseExistingPlaylist(page, seed);
    await expect.poll(() => sqlite(`
SELECT "CollectionType" FROM "PlaylistItem"
WHERE "PlaylistId" = ${seed.playlistId} AND "MediaItemId" = ${seed.imageId}
ORDER BY "Id" DESC LIMIT 1;
`)).toBe('60');

    expect(runtimeErrors).toEqual([]);
  });

  test('artist detail collection action and preview images hydrate', async ({ page }, testInfo) => {
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedMediaBrowserActions(testInfo);
    await forceEnglish(page);

    await gotoHydrated(page, `/media/music/artists/${seed.artistId}`);
    await expect(page.getByRole('heading', { name: seed.artistTitle })).toBeVisible();
    await expectLoadedImage(page.locator(`img[alt="${seed.artistTitle}"]`).first());
    await expectLoadedImage(page.locator('img[alt="fan art"]').first());
    await expectLoadedImage(
      page.locator(`img[alt="${seed.musicVideoTitle} thumbnail"]`).first(),
    );

    await page.getByRole('button', { name: /^add to collection$/i }).first().click();
    await chooseExistingCollection(page, seed);
    await expect(page).toHaveURL(new RegExp(`/media/collections/${seed.collectionId}$`));
    await expect.poll(() => sqlite(`
SELECT COUNT(*) FROM "CollectionItem"
WHERE "CollectionId" = ${seed.collectionId} AND "MediaItemId" = ${seed.artistId};
`)).toBe('1');

    expect(runtimeErrors).toEqual([]);
  });
});
