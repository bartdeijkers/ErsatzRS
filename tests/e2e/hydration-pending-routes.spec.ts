import { expect, Page, TestInfo, test } from '@playwright/test';
import { execFileSync } from 'child_process';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

type ProviderSeed = {
  plexSourceId: number;
  plexLibraryId: number;
  jellyfinSourceId: number;
  jellyfinLibraryId: number;
  embySourceId: number;
  embyLibraryId: number;
};

type MediaSeed = {
  collectionId: number;
  collectionName: string;
  normalMovieId: number;
  normalMovieTitle: string;
  trashMovieId: number;
  trashMovieTitle: string;
};

type AddonListSeed = {
  editorListId: number;
  editorListName: string;
  addonId: string;
  editorProviderId: string;
  editorSourceUrl: string;
  deleteListId: number;
  deleteListName: string;
};

function projectBase(testInfo: TestInfo): number {
  const projectOffset = Array.from(testInfo.project.name).reduce(
    (total, char) => total + char.charCodeAt(0),
    0,
  ) * 1_000;
  return 250_000 + projectOffset;
}

function sqlEscape(value: string): string {
  return value.replace(/'/g, "''");
}

function sqlite(sql: string): string {
  if (!dbPath) {
    throw new Error('E2E_DB_PATH or ERSATZRS_DB_PATH is required for hydration route seeding');
  }
  return execFileSync('sqlite3', [dbPath], {
    input: `.timeout 10000\n${sql}`,
    encoding: 'utf8',
  }).trim();
}

function isBenignAbortError(text: string): boolean {
  return (
    (text.includes('AbortError') && text.toLowerCase().includes('operation was aborted')) ||
    text.includes('NetworkError when attempting to fetch resource')
  );
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
  const response = await page.goto(route, { waitUntil: 'domcontentloaded' });
  expect(response?.status(), `GET ${route}`).toBe(200);
  await expect(page.locator('[data-not-found]')).toHaveCount(0);
  await waitForHydration(page);
}

function seedProviders(testInfo: TestInfo): ProviderSeed {
  const base = projectBase(testInfo);
  const suffix = testInfo.project.name;
  const seed = {
    plexSourceId: base + 101,
    plexLibraryId: base + 102,
    jellyfinSourceId: base + 201,
    jellyfinLibraryId: base + 202,
    embySourceId: base + 301,
    embyLibraryId: base + 302,
  };

  sqlite(`
DELETE FROM "PlexPathReplacement" WHERE "PlexMediaSourceId" = ${seed.plexSourceId};
DELETE FROM "JellyfinPathReplacement" WHERE "JellyfinMediaSourceId" = ${seed.jellyfinSourceId};
DELETE FROM "EmbyPathReplacement" WHERE "EmbyMediaSourceId" = ${seed.embySourceId};
DELETE FROM "PlexLibrary" WHERE "Id" = ${seed.plexLibraryId};
DELETE FROM "JellyfinLibrary" WHERE "Id" = ${seed.jellyfinLibraryId};
DELETE FROM "EmbyLibrary" WHERE "Id" = ${seed.embyLibraryId};
DELETE FROM "Library" WHERE "Id" IN (${seed.plexLibraryId}, ${seed.jellyfinLibraryId}, ${seed.embyLibraryId});
DELETE FROM "PlexConnection" WHERE "PlexMediaSourceId" = ${seed.plexSourceId};
DELETE FROM "JellyfinConnection" WHERE "JellyfinMediaSourceId" = ${seed.jellyfinSourceId};
DELETE FROM "EmbyConnection" WHERE "EmbyMediaSourceId" = ${seed.embySourceId};
DELETE FROM "PlexMediaSource" WHERE "Id" = ${seed.plexSourceId};
DELETE FROM "JellyfinMediaSource" WHERE "Id" = ${seed.jellyfinSourceId};
DELETE FROM "EmbyMediaSource" WHERE "Id" = ${seed.embySourceId};
DELETE FROM "MediaSource" WHERE "Id" IN (${seed.plexSourceId}, ${seed.jellyfinSourceId}, ${seed.embySourceId});

INSERT INTO "MediaSource" ("Id") VALUES (${seed.plexSourceId});
INSERT INTO "PlexMediaSource" ("Id", "ServerName", "ClientIdentifier", "ProductVersion")
  VALUES (${seed.plexSourceId}, 'E2E Plex ${suffix}', 'e2e-client-${suffix}', '1.0');
INSERT INTO "PlexConnection" ("IsActive", "Uri", "PlexMediaSourceId")
  VALUES (1, 'http://plex-e2e.test:32400', ${seed.plexSourceId});
INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
  VALUES (${seed.plexLibraryId}, 'E2E Plex Movies ${suffix}', 1, ${seed.plexSourceId});
INSERT INTO "PlexLibrary" ("Id", "Key", "ShouldSyncItems")
  VALUES (${seed.plexLibraryId}, 'plex-movies', 1);
REPLACE INTO "ConfigElement" ("Key", "Value")
  VALUES ('plex.current_server_auth_token', 'e2e-token');

INSERT INTO "MediaSource" ("Id") VALUES (${seed.jellyfinSourceId});
INSERT INTO "JellyfinMediaSource" ("Id", "ServerName", "OperatingSystem")
  VALUES (${seed.jellyfinSourceId}, 'E2E Jellyfin ${suffix}', 'Linux');
INSERT INTO "JellyfinConnection" ("Address", "JellyfinMediaSourceId")
  VALUES ('http://jellyfin-e2e.test:8096', ${seed.jellyfinSourceId});
INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
  VALUES (${seed.jellyfinLibraryId}, 'E2E Jellyfin Movies ${suffix}', 1, ${seed.jellyfinSourceId});
INSERT INTO "JellyfinLibrary" ("Id", "ItemId", "ShouldSyncItems")
  VALUES (${seed.jellyfinLibraryId}, 'jellyfin-movies', 1);
REPLACE INTO "ConfigElement" ("Key", "Value")
  VALUES ('jellyfin.current_server_api_key', 'jellyfin-key');

INSERT INTO "MediaSource" ("Id") VALUES (${seed.embySourceId});
INSERT INTO "EmbyMediaSource" ("Id", "ServerName", "OperatingSystem")
  VALUES (${seed.embySourceId}, 'E2E Emby ${suffix}', 'Linux');
INSERT INTO "EmbyConnection" ("Address", "EmbyMediaSourceId")
  VALUES ('http://emby-e2e.test:8096', ${seed.embySourceId});
INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
  VALUES (${seed.embyLibraryId}, 'E2E Emby Movies ${suffix}', 1, ${seed.embySourceId});
INSERT INTO "EmbyLibrary" ("Id", "ItemId", "ShouldSyncItems")
  VALUES (${seed.embyLibraryId}, 'emby-movies', 1);
REPLACE INTO "ConfigElement" ("Key", "Value")
  VALUES ('emby.current_server_api_key', 'emby-key');
`);

  return seed;
}

function seedMedia(testInfo: TestInfo): MediaSeed {
  const base = projectBase(testInfo);
  const suffix = testInfo.project.name;
  const seed = {
    collectionId: base + 401,
    collectionName: `E2E Hydration Collection ${suffix}`,
    normalMovieId: base + 402,
    normalMovieTitle: `Hydration Pending Movie ${suffix}`,
    trashMovieId: base + 403,
    trashMovieTitle: `Hydration Trash Movie ${suffix}`,
  };
  const mediaSourceId = base + 404;
  const libraryId = base + 405;
  const libraryPathId = base + 406;

  sqlite(`
DELETE FROM "CollectionItem" WHERE "MediaItemId" IN (${seed.normalMovieId}, ${seed.trashMovieId});
DELETE FROM "MovieMetadata" WHERE "MovieId" IN (${seed.normalMovieId}, ${seed.trashMovieId});
DELETE FROM "Movie" WHERE "Id" IN (${seed.normalMovieId}, ${seed.trashMovieId});
DELETE FROM "MediaItem" WHERE "Id" IN (${seed.normalMovieId}, ${seed.trashMovieId});
DELETE FROM "LibraryPath" WHERE "Id" = ${libraryPathId};
DELETE FROM "LocalLibrary" WHERE "Id" = ${libraryId};
DELETE FROM "Library" WHERE "Id" = ${libraryId};
DELETE FROM "LocalMediaSource" WHERE "Id" = ${mediaSourceId};
DELETE FROM "MediaSource" WHERE "Id" = ${mediaSourceId};
DELETE FROM "Collection" WHERE "Id" = ${seed.collectionId};

INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder")
  VALUES (${seed.collectionId}, '${sqlEscape(seed.collectionName)}', 1);
INSERT INTO "MediaSource" ("Id") VALUES (${mediaSourceId});
INSERT INTO "LocalMediaSource" ("Id") VALUES (${mediaSourceId});
INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
  VALUES (${libraryId}, 'E2E Hydration Movies', 1, ${mediaSourceId});
INSERT INTO "LocalLibrary" ("Id") VALUES (${libraryId});
INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId")
  VALUES (${libraryPathId}, '/e2e/hydration/movies/${testInfo.project.name}', ${libraryId});

INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State")
  VALUES (${seed.normalMovieId}, ${libraryPathId}, 0);
INSERT INTO "Movie" ("Id") VALUES (${seed.normalMovieId});
INSERT INTO "MovieMetadata" (
  "MovieId", "MetadataKind", "Title", "OriginalTitle", "SortTitle",
  "DateAdded", "DateUpdated", "Year"
) VALUES (
  ${seed.normalMovieId}, 0, '${sqlEscape(seed.normalMovieTitle)}', '${sqlEscape(seed.normalMovieTitle)}',
  '${sqlEscape(seed.normalMovieTitle.toLowerCase())}', '2026-06-28T00:00:00Z', '2026-06-28T00:00:00Z', 2026
);

INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State")
  VALUES (${seed.trashMovieId}, ${libraryPathId}, 1);
INSERT INTO "Movie" ("Id") VALUES (${seed.trashMovieId});
INSERT INTO "MovieMetadata" (
  "MovieId", "MetadataKind", "Title", "OriginalTitle", "SortTitle",
  "DateAdded", "DateUpdated", "Year"
) VALUES (
  ${seed.trashMovieId}, 0, '${sqlEscape(seed.trashMovieTitle)}', '${sqlEscape(seed.trashMovieTitle)}',
  '${sqlEscape(seed.trashMovieTitle.toLowerCase())}', '2026-06-28T00:00:00Z', '2026-06-28T00:00:00Z', 2026
);
`);

  return seed;
}

function seedAddonLists(testInfo: TestInfo): AddonListSeed {
  const base = projectBase(testInfo);
  const suffix = testInfo.project.name;
  const addonId = `org.example.e2e-lists-${suffix.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`;
  const manifest = JSON.stringify({
    manifest_version: 3,
    id: addonId,
    version: '1.0.0',
    name: { 'en-US': 'E2E List Provider' },
    summary: { 'en-US': 'E2E media-list provider' },
    icon: null,
    license: 'Zlib',
    source_url: 'https://example.invalid/e2e-addon',
    host_version: '*',
    capabilities: [{ id: 'addon.check.v1' }, { id: 'media-list.list.v1' }],
    dependencies: [],
    entrypoints: { any: { path: 'addon.sh', kind: 'posix_shell' } },
    permissions: {
      network_hosts: ['example.invalid'],
      media_read: false,
      managed_ffmpeg: false,
      external_commands: [],
    },
    settings: [],
    panels: [],
  });
  const seed = {
    editorListId: base + 501,
    editorListName: `E2E Add-on List Editor ${suffix}`,
    addonId,
    editorProviderId: `hydration-editor-${suffix}`,
    editorSourceUrl: `https://example.invalid/lists/hydration-editor-${suffix}`,
    deleteListId: base + 502,
    deleteListName: `E2E Add-on List Delete ${suffix}`,
  };

  sqlite(`
DELETE FROM "AddonMediaListItemGuid" WHERE "AddonMediaListItemId" IN (
  SELECT "Id" FROM "AddonMediaListItem" WHERE "AddonMediaListId" IN (${seed.editorListId}, ${seed.deleteListId})
);
DELETE FROM "AddonMediaListItem" WHERE "AddonMediaListId" IN (${seed.editorListId}, ${seed.deleteListId});
DELETE FROM "AddonMediaList" WHERE "Id" IN (${seed.editorListId}, ${seed.deleteListId});

INSERT INTO "AddonInstallation" (
  "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
  "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt", "TrustTier"
) VALUES (
  '${sqlEscape(addonId)}', '1.0.0', 1, 'e2e-package', 'e2e-content',
  '${sqlEscape(manifest)}', 'e2e/addon', '2026-08-14T00:00:00Z',
  '2026-08-14T00:00:00Z', 'verified'
) ON CONFLICT("AddonId") DO UPDATE SET
  "Version" = excluded."Version",
  "Enabled" = excluded."Enabled",
  "ManifestJson" = excluded."ManifestJson",
  "UpdatedAt" = excluded."UpdatedAt";

INSERT INTO "AddonMediaList" (
  "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "Description", "ItemCount",
  "AutoRefresh", "GeneratePlaylist"
) VALUES (
  ${seed.editorListId}, '${sqlEscape(addonId)}', '${sqlEscape(seed.editorProviderId)}',
  '${sqlEscape(seed.editorSourceUrl)}',
  '${sqlEscape(seed.editorListName)}', 'Editor proof', 1, 0, 1
);
INSERT INTO "AddonMediaListItem" (
  "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind", "Rank", "Title"
) VALUES (
  ${base + 503}, ${seed.editorListId}, 'movie:99003',
  'Hydration Pending Movie', 0, 1, 'Hydration Pending Movie'
);

INSERT INTO "AddonMediaList" (
  "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "Description", "ItemCount",
  "AutoRefresh", "GeneratePlaylist"
) VALUES (
  ${seed.deleteListId}, '${sqlEscape(addonId)}', 'hydration-delete-${suffix}',
  'https://example.invalid/lists/hydration-delete-${suffix}',
  '${sqlEscape(seed.deleteListName)}', 'Delete proof', 0, 0, 0
);
`);

  return seed;
}

test.describe('pending hydration route parity', () => {
  test.skip(!dbPath, 'E2E_DB_PATH or ERSATZRS_DB_PATH is required to seed pending hydration routes');

  test('provider source detail routes hydrate and persist library/path edits', async ({ page }, testInfo) => {
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedProviders(testInfo);
    await forceEnglish(page);

    await gotoHydrated(page, '/media/sources/jellyfin');
    const jellyfinRow = page
      .locator('tr')
      .filter({ hasText: `E2E Jellyfin ${testInfo.project.name}` });
    await expect(jellyfinRow).toBeVisible();
    await expect(jellyfinRow.getByRole('link', { name: /^libraries$/i })).toHaveAttribute(
      'href',
      `/media/sources/jellyfin/${seed.jellyfinSourceId}/libraries`,
    );
    await expect(jellyfinRow.getByRole('link', { name: /^paths$/i })).toHaveAttribute(
      'href',
      `/media/sources/jellyfin/${seed.jellyfinSourceId}/paths`,
    );

    await gotoHydrated(page, '/media/sources/jellyfin/edit');
    await page.locator('input[type="url"]').fill('');
    await page.getByRole('button', { name: /^save$/i }).click();
    await expect(page.getByText(/address.*required/i)).toBeVisible();

    await gotoHydrated(page, '/media/sources/emby');
    await expect(page.getByText(`E2E Emby ${testInfo.project.name}`)).toBeVisible();
    await gotoHydrated(page, '/media/sources/emby/edit');
    await page.locator('input[type="url"]').fill('http://emby-updated-e2e.test:8096');
    await page.locator('input[type="password"]').fill('updated-emby-key');
    await page.getByRole('button', { name: /^save$/i }).click();
    await expect(page).toHaveURL(/\/media\/sources\/emby$/);
    expect(sqlite(`
SELECT "Address" FROM "EmbyConnection" WHERE "EmbyMediaSourceId" = ${seed.embySourceId};
`)).toBe('http://emby-updated-e2e.test:8096');

    for (const provider of [
      ['plex', seed.plexSourceId, seed.plexLibraryId, 'Plex'] as const,
      ['jellyfin', seed.jellyfinSourceId, seed.jellyfinLibraryId, 'Jellyfin'] as const,
      ['emby', seed.embySourceId, seed.embyLibraryId, 'Emby'] as const,
    ]) {
      const [slug, sourceId, libraryId, label] = provider;
      await gotoHydrated(page, `/media/sources/${slug}/${sourceId}/libraries`);
      await expect(page.getByRole('heading', { name: `${label} Libraries` })).toBeVisible();
      await expect(page.getByText(`E2E ${label} Movies ${testInfo.project.name}`)).toBeVisible();
      const checkbox = page.locator('tbody input[type="checkbox"]').first();
      await checkbox.uncheck();
      await page.getByRole('button', { name: /^save$/i }).click();
      await expect(page.getByText('Saved').first()).toBeVisible();
      const table =
        slug === 'plex' ? 'PlexLibrary' : slug === 'jellyfin' ? 'JellyfinLibrary' : 'EmbyLibrary';
      expect(sqlite(`SELECT "ShouldSyncItems" FROM "${table}" WHERE "Id" = ${libraryId};`)).toBe(
        '0',
      );

      await gotoHydrated(page, `/media/sources/${slug}/${sourceId}/paths`);
      await expect(page.getByRole('heading', { name: `${label} Path Replacements` })).toBeVisible();
      await page.getByPlaceholder('/remote/path').fill(`/${slug}/remote/${testInfo.project.name}`);
      await page.getByPlaceholder('/local/path').fill(`/local/${slug}/${testInfo.project.name}`);
      await page.getByRole('button', { name: /^add$/i }).click();
      await expect(page.getByText(`/${slug}/remote/${testInfo.project.name}`)).toBeVisible();
      await page.getByRole('button', { name: /^save$/i }).click();
      await expect(page.getByText('Saved').first()).toBeVisible();
      const pathTable =
        slug === 'plex'
          ? ['PlexPathReplacement', 'PlexPath', 'PlexMediaSourceId']
          : slug === 'jellyfin'
            ? ['JellyfinPathReplacement', 'JellyfinPath', 'JellyfinMediaSourceId']
            : ['EmbyPathReplacement', 'EmbyPath', 'EmbyMediaSourceId'];
      expect(sqlite(`
SELECT COUNT(*) FROM "${pathTable[0]}"
WHERE "${pathTable[2]}" = ${sourceId}
  AND "${pathTable[1]}" = '/${slug}/remote/${testInfo.project.name}';
`)).toBe('1');
    }

    expect(runtimeErrors).toEqual([]);
  });

  test('movies overflow menu and trash actions hydrate with seeded media', async ({ page }, testInfo) => {
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedMedia(testInfo);
    await forceEnglish(page);

    await gotoHydrated(page, `/media/movies?query=${encodeURIComponent(seed.normalMovieTitle)}`);
    await expect(page.getByRole('heading', { name: 'Movies' })).toBeVisible();
    await expect(page.getByText(seed.normalMovieTitle)).toBeVisible();
    await page.getByRole('button', { name: /item actions/i }).first().click();
    await expect(page.getByRole('menuitem', { name: /^add to collection$/i })).toBeVisible();
    await expect(page.getByRole('menuitem', { name: /^show media info$/i })).toBeVisible();
    await expect(page.getByRole('menuitem', { name: /^troubleshoot playback$/i })).toHaveAttribute(
      'href',
      `/system/troubleshooting/playback?mediaItem=${seed.normalMovieId}`,
    );
    await page.getByRole('menuitem', { name: /^add to collection$/i }).click();
    const collectionDialog = page
      .locator('div[class*="panel"]')
      .filter({ has: page.getByRole('heading', { name: /^add to collection$/i }) })
      .last();
    await expect(collectionDialog).toBeVisible();
    await expect(collectionDialog.getByText(/1 selected items/i)).toBeVisible();
    await collectionDialog.getByRole('button', { name: /^cancel$/i }).click();

    await gotoHydrated(page, '/media/trash');
    await expect(page.getByText(seed.trashMovieTitle)).toBeVisible();
    await page.locator('section#movies input[type="checkbox"]').first().check();
    await page.getByRole('button', { name: /delete selected/i }).click();
    let deleteDialog = page
      .locator('div[class*="panel"]')
      .filter({ has: page.getByRole('heading', { name: 'Confirm Delete' }) })
      .last();
    await expect(deleteDialog).toBeVisible();
    await deleteDialog.getByRole('button', { name: /^cancel$/i }).click();
    await page.getByRole('button', { name: /^empty trash$/i }).click();
    const emptyDialog = page
      .locator('div[class*="panel"]')
      .filter({ has: page.getByRole('heading', { name: 'Empty Trash' }) })
      .last();
    await expect(emptyDialog).toBeVisible();
    await emptyDialog.getByRole('button', { name: /^delete all$/i }).click();
    await expect.poll(() => sqlite(`
SELECT COUNT(*) FROM "MediaItem" WHERE "Id" = ${seed.trashMovieId};
`)).toBe('0');

    expect(runtimeErrors).toEqual([]);
  });

  test('add-on list actions and editor fallback persist without hydration errors', async ({
    page,
  }, testInfo) => {
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedAddonLists(testInfo);
    await forceEnglish(page);

    await gotoHydrated(page, '/media/addon-lists');
    await expect(page.getByText(seed.editorListName)).toBeVisible();
    await page.getByRole('button', { name: /^add add-on list$/i }).click();
    const addDialog = page
      .locator('div[class*="panel"]')
      .filter({ has: page.getByRole('heading', { name: 'Add add-on list' }) })
      .last();
    await expect(addDialog).toBeVisible();
    await addDialog.getByRole('button', { name: /^cancel$/i }).click();
    const editorRow = page.locator('tr').filter({ hasText: seed.editorListName });
    await expect(editorRow.getByRole('link', { name: /^search add-on list$/i })).toHaveAttribute(
      'href',
      `/search?query=addon_list%3a${seed.editorListId}`,
    );
    await editorRow.getByRole('button', { name: /^match add-on list items$/i }).click();
    await expect.poll(() => sqlite(`
SELECT COALESCE("PlaylistId", '') FROM "AddonMediaList" WHERE "Id" = ${seed.editorListId};
`)).not.toBe('');
    const generatedPlaylistId = sqlite(`
SELECT "PlaylistId" FROM "AddonMediaList" WHERE "Id" = ${seed.editorListId};
`);
    const deleteRow = page.locator('tr').filter({ hasText: seed.deleteListName });
    await deleteRow.getByRole('button', { name: /^delete add-on list$/i }).click();
    const deleteDialog = page
      .locator('div[class*="panel"]')
      .filter({ has: page.getByRole('heading', { name: 'Confirm Delete' }) })
      .last();
    await expect(deleteDialog).toBeVisible();
    await deleteDialog.getByRole('button', { name: /^delete$/i }).click();
    await expect.poll(() => sqlite(`
SELECT COUNT(*) FROM "AddonMediaList" WHERE "Id" = ${seed.deleteListId};
`)).toBe('0');

    await gotoHydrated(page, `/media/addon-lists/${seed.editorListId}`);
    await expect(page.locator('form[action="/api/addon-lists/save"]')).toBeVisible();
    await page.request.post('/api/addon-lists/save', {
      form: {
        id: seed.editorListId.toString(),
        addon_id: seed.addonId,
        source_url: seed.editorSourceUrl,
        auto_refresh: 'true',
      },
      maxRedirects: 0,
    });

    expect(sqlite(`
SELECT "AutoRefresh" || ',' || "GeneratePlaylist"
FROM "AddonMediaList" WHERE "Id" = ${seed.editorListId};
`)).toBe('1,0');
    expect(sqlite(`
SELECT COALESCE("PlaylistId", '') FROM "AddonMediaList" WHERE "Id" = ${seed.editorListId};
`)).toBe('');
    expect(sqlite(`
SELECT COUNT(*) FROM "Playlist" WHERE "Id" = ${generatedPlaylistId};
`)).toBe('0');
    expect(runtimeErrors).toEqual([]);
  });

  test('filler presets and rerun collections keep POST save/delete fallbacks', async ({
    page,
  }, testInfo) => {
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedMedia(testInfo);
    await forceEnglish(page);

    const fillerName = `E2E Filler Fallback ${testInfo.project.name}`;
    await gotoHydrated(page, '/media/filler/presets/add');
    await expect(page.locator('form[action="/api/filler-presets/save"]')).toBeVisible();
    await page.request.post('/api/filler-presets/save', {
      form: {
        id: '0',
        name: fillerName,
        filler_kind: '1',
        filler_mode: '1',
        duration_hours: '0',
        duration_minutes: '0',
        duration_seconds: '1',
        count: '',
        pad_to_nearest_minute: '',
        allow_watermarks: 'true',
        collection_type: '0',
        collection_id: seed.collectionId.toString(),
        media_item_id: '',
        multi_collection_id: '',
        smart_collection_id: '',
        playlist_id: '',
        expression: '',
      },
      maxRedirects: 0,
    });
    expect(sqlite(`
SELECT COUNT(*) FROM "FillerPreset" WHERE "Name" = '${sqlEscape(fillerName)}';
`)).toBe('1');
    const fillerId = sqlite(`
SELECT "Id" FROM "FillerPreset" WHERE "Name" = '${sqlEscape(fillerName)}' LIMIT 1;
`);
    await page.request.post('/api/filler-presets/delete', {
      form: { id: fillerId },
      maxRedirects: 0,
    });
    await expect.poll(() => sqlite(`
SELECT COUNT(*) FROM "FillerPreset" WHERE "Name" = '${sqlEscape(fillerName)}';
`)).toBe('0');

    const rerunName = `E2E Rerun Fallback ${testInfo.project.name}`;
    await gotoHydrated(page, '/media/rerun-collections/add');
    await expect(page.locator('form[action="/api/rerun-collections/save"]')).toBeVisible();
    await page.request.post('/api/rerun-collections/save', {
      form: {
        id: '0',
        name: rerunName,
        collection_type: '0',
        collection_id: seed.collectionId.toString(),
        media_item_id: '',
        multi_collection_id: '',
        smart_collection_id: '',
        first_run_playback_order: '0',
        rerun_playback_order: '0',
      },
      maxRedirects: 0,
    });
    expect(sqlite(`
SELECT COUNT(*) FROM "RerunCollection" WHERE "Name" = '${sqlEscape(rerunName)}';
`)).toBe('1');
    const rerunId = sqlite(`
SELECT "Id" FROM "RerunCollection" WHERE "Name" = '${sqlEscape(rerunName)}' LIMIT 1;
`);
    await page.request.post('/api/rerun-collections/delete', {
      form: { id: rerunId },
      maxRedirects: 0,
    });
    await expect.poll(() => sqlite(`
SELECT COUNT(*) FROM "RerunCollection" WHERE "Name" = '${sqlEscape(rerunName)}';
`)).toBe('0');

    expect(runtimeErrors).toEqual([]);
  });
});
