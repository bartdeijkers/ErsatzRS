import { expect, Locator, Page, TestInfo, test } from '@playwright/test';
import { execFileSync } from 'child_process';
import { createHash } from 'crypto';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

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
  collectionId: number;
  playlistGroupId: number;
  playlistId: number;
  addonListId: number;
  addonListItemId: number;
  optionSchemaId: number;
  revisionId: number;
  channelId: number;
  playoutId: number;
  playoutItemId: number;
  mediaVersionId: number;
  mediaStreamId: number;
  mediaChapterId: number;
  tag: string;
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
    collectionId: base + 9_001,
    playlistGroupId: base + 9_002,
    playlistId: base + 9_003,
    addonListId: base + 9_101,
    addonListItemId: base + 9_102,
    optionSchemaId: base + 9_103,
    revisionId: base + 9_104,
    channelId: base + 9_201,
    playoutId: base + 9_202,
    playoutItemId: base + 9_203,
    mediaVersionId: base + 9_301,
    mediaStreamId: base + 9_302,
    mediaChapterId: base + 9_303,
    tag: `E2E playlist facet ${testInfo.project.name}`,
  };
}

function seedRemoteStreams(testInfo: TestInfo): RemoteStreamSeed {
  const seed = seedForProject(testInfo);
  const addonId = `org.ersatzrs.addon.remote-fixture.${testInfo.project.name}`;
  const addonManifest = JSON.stringify({
    manifest_version: 1,
    id: addonId,
    version: '1.0.0',
    name: { 'en-US': 'Remote Stream fixture' },
    summary: { 'en-US': 'Network-free browser fixture.' },
    license: 'MIT',
    source_url: 'https://example.test/remote-stream-fixture',
    host_version: '*',
    capabilities: [
      { id: 'addon.check.v1' },
      { id: 'remote-stream.list.v1' },
      { id: 'remote-stream.play.v1' },
    ],
    entrypoints: { any: { path: 'addon.sh', kind: 'posix_shell' } },
    permissions: {
      network_hosts: ['example.test'],
      media_read: false,
      managed_ffmpeg: false,
      external_commands: [],
    },
    settings: [],
    panels: [],
  });
  sqlite(`
DELETE FROM "AddonMediaList" WHERE "Id" = ${seed.addonListId};
DELETE FROM "AddonMediaListOptionSchema" WHERE "Id" = ${seed.optionSchemaId};
DELETE FROM "PlayoutItem" WHERE "Id" = ${seed.playoutItemId};
DELETE FROM "Playout" WHERE "Id" = ${seed.playoutId};
DELETE FROM "Channel" WHERE "Id" = ${seed.channelId};
DELETE FROM "MediaChapter" WHERE "Id" = ${seed.mediaChapterId};
DELETE FROM "MediaStream" WHERE "Id" = ${seed.mediaStreamId};
DELETE FROM "MediaVersion" WHERE "Id" = ${seed.mediaVersionId};
DELETE FROM "AddonInstallation" WHERE "AddonId" = '${addonId}';
INSERT INTO "AddonInstallation" (
  "AddonId", "RepositoryId", "Version", "Enabled", "PackageSha256",
  "ContentSha256", "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
) VALUES (
  '${addonId}', NULL, '1.0.0', 1,
  '${'0'.repeat(64)}', '${'1'.repeat(64)}',
  '${sqlEscape(addonManifest)}', '/tmp/ersatzrs-remote-stream-addon-fixture',
  '2026-08-11T00:00:00Z', '2026-08-11T00:00:00Z'
);
DELETE FROM "CollectionItem" WHERE "MediaItemId" IN (${seed.mediaItemId}, ${seed.otherMediaItemId});
DELETE FROM "CollectionItem" WHERE "CollectionId" = ${seed.collectionId};
DELETE FROM "Collection" WHERE "Id" = ${seed.collectionId};
DELETE FROM "PlaylistItem" WHERE "PlaylistId" = ${seed.playlistId}
  OR "MediaItemId" IN (${seed.mediaItemId}, ${seed.otherMediaItemId});
DELETE FROM "Playlist" WHERE "Id" = ${seed.playlistId};
DELETE FROM "PlaylistGroup" WHERE "Id" = ${seed.playlistGroupId};
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
INSERT INTO "Tag" ("Name", "RemoteStreamMetadataId")
  SELECT '${sqlEscape(seed.tag)}', "Id" FROM "RemoteStreamMetadata"
  WHERE "RemoteStreamId" = ${seed.mediaItemId};

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

INSERT OR IGNORE INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder")
  VALUES (${seed.collectionId}, 'E2E Existing Remote Stream Collection ${testInfo.project.name}', 0);
INSERT OR IGNORE INTO "PlaylistGroup" ("Id", "Name", "IsSystem")
  VALUES (${seed.playlistGroupId}, 'E2E Remote Stream Playlist Group ${testInfo.project.name}', 0);
INSERT OR IGNORE INTO "Playlist" ("Id", "Name", "PlaylistGroupId", "IsSystem")
  VALUES (
    ${seed.playlistId},
    'E2E Remote Stream Playlist ${testInfo.project.name}',
    ${seed.playlistGroupId},
    0
  );
`);
  return seed;
}

function seedRemoteStreamDetails(testInfo: TestInfo): RemoteStreamSeed {
  const seed = seedRemoteStreams(testInfo);
  const addonId = `org.ersatzrs.addon.remote-fixture.${testInfo.project.name}`;
  const profileId = sqlite('SELECT "Id" FROM "FFmpegProfile" ORDER BY "Id" LIMIT 1;');
  if (!profileId) {
    throw new Error('E2E database has no FFmpegProfile row for Remote Stream detail seeding');
  }
  const capabilityId = 'media-list.options.v1';
  const optionDefinition = JSON.stringify({
    id: capabilityId,
    options: [{
      key: 'stream_quality',
      label: { 'en-US': 'Stream quality', 'nl-NL': 'Streamkwaliteit' },
      description: { 'en-US': 'Published playback preference.' },
      scope: 'both',
      operation_targets: ['playback'],
      applicability: 'all',
      kind: 'enum',
      default: 'medium',
      choices: [
        { value: 'medium', label: { 'en-US': 'Medium' } },
        { value: 'high', label: { 'en-US': 'High' } },
      ],
    }],
  });
  const schemaDigest = createHash('sha256').update(optionDefinition).digest('hex');
  const channelName = `E2E Remote Stream Channel ${testInfo.project.name}`;
  const listName = `E2E Published Remote Streams ${testInfo.project.name}`;

  sqlite(`
UPDATE "RemoteStream"
SET "AddonId" = '${addonId}', "Liveness" = 2, "Availability" = 0,
    "ContentKind" = 2, "Duration" = '00:30:00'
WHERE "Id" = ${seed.mediaItemId};
UPDATE "RemoteStreamMetadata"
SET "Year" = 2026, "Plot" = 'A live motorsport fixture.', "CollectionName" = 'Race day',
    "Broadcasters" = '["E2E Sports"]'
WHERE "RemoteStreamId" = ${seed.mediaItemId};

INSERT INTO "Artwork" (
  "ArtworkKind", "DateAdded", "DateUpdated", "Path", "RemoteStreamMetadataId"
)
SELECT 0, '2026-09-04T00:00:00Z', '2026-09-04T00:00:00Z',
       '/e2e/remote-stream-poster.jpg', "Id"
FROM "RemoteStreamMetadata" WHERE "RemoteStreamId" = ${seed.mediaItemId};

INSERT INTO "MediaVersion" (
  "Id", "DateAdded", "DateUpdated", "Duration", "Height", "Name",
  "RemoteStreamId", "VideoScanKind", "Width"
) VALUES (
  ${seed.mediaVersionId}, '2026-09-04T00:00:00Z', '2026-09-04T00:00:00Z',
  '00:30:00', 1080, 'E2E Remote Stream source', ${seed.mediaItemId}, 1, 1920
);
INSERT INTO "MediaStream" (
  "Id", "Index", "Codec", "Profile", "MediaStreamKind", "Language", "Channels",
  "Title", "Default", "Forced", "MediaVersionId"
) VALUES (
  ${seed.mediaStreamId}, 1, 'aac', 'LC', 2, 'eng', 2,
  'English stereo', 1, 0, ${seed.mediaVersionId}
);
INSERT INTO "MediaChapter" (
  "Id", "MediaVersionId", "ChapterId", "StartTime", "EndTime", "Title"
) VALUES (
  ${seed.mediaChapterId}, ${seed.mediaVersionId}, 0, '00:00:00', '00:05:00', 'Opening lap'
);

INSERT INTO "AddonMediaListOptionSchema" (
  "Id", "AddonId", "CapabilityId", "SchemaDigest", "DefinitionJson", "CreatedAt"
) VALUES (
  ${seed.optionSchemaId}, '${addonId}', '${capabilityId}', '${schemaDigest}',
  '${sqlEscape(optionDefinition)}', '2026-09-04T00:00:00Z'
);
INSERT INTO "AddonMediaList" (
  "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "PublishedRevision"
) VALUES (
  ${seed.addonListId}, '${addonId}', 'e2e-detail-${testInfo.project.name}',
  'https://example.test/detail-list', '${sqlEscape(listName)}', 1
);
INSERT INTO "AddonMediaListItem" (
  "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind", "MediaItemId",
  "Rank", "Title", "Included", "Published"
) VALUES (
  ${seed.addonListItemId}, ${seed.addonListId}, 'e2e-detail-item',
  'Nurburgring broadcast', 4, ${seed.mediaItemId}, 0, 'Nurburgring broadcast', 1, 1
);
INSERT INTO "AddonMediaListRevision" (
  "Id", "AddonMediaListId", "Revision", "OptionSchemaId", "OptionValuesJson", "CompletedAt"
) VALUES (
  ${seed.revisionId}, ${seed.addonListId}, 1, ${seed.optionSchemaId},
  '{"stream_quality":"medium"}', '2026-09-04T00:00:00Z'
);
INSERT INTO "AddonMediaListRevisionItemOption" (
  "AddonMediaListRevisionId", "ProviderId", "OptionOverridesJson"
) VALUES (${seed.revisionId}, 'e2e-detail-item', '{"stream_quality":"high"}');

INSERT INTO "Channel" (
  "Id", "UniqueId", "Number", "SortNumber", "Name", "Group",
  "FFmpegProfileId", "StreamingMode", "StreamSelectorMode", "PlayoutSource",
  "PlayoutMode", "TranscodeMode", "IdleBehavior", "MusicVideoCreditsMode",
  "SongVideoMode", "SubtitleMode", "IsEnabled", "ShowInEpg"
) VALUES (
  ${seed.channelId}, '40000000-0000-0000-0000-${seed.channelId.toString().padStart(12, '0')}',
  '903', -903, '${sqlEscape(channelName)}', 'ErsatzRS', ${profileId}, 5, 0, 0,
  0, 0, 0, 0, 0, 0, 1, 1
);
INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed")
VALUES (${seed.playoutId}, ${seed.channelId}, 1, 1);
INSERT INTO "PlayoutItem" (
  "Id", "DisableWatermarks", "FillerKind", "Finish", "GuideGroup", "InPoint",
  "MediaItemId", "OutPoint", "PlayoutId", "Start"
) VALUES (
  ${seed.playoutItemId}, 0, 0, '2099-01-01 00:30:00', 0, '00:00:00',
  ${seed.mediaItemId}, '00:30:00', ${seed.playoutId}, '2099-01-01 00:00:00'
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

async function clickFirstCardAddToPlaylist(page: Page): Promise<void> {
  const itemMenu = page.locator('details[class*="item-menu"]').first();
  const isOpen = await itemMenu.evaluate((node) => (node as HTMLDetailsElement).open);
  if (!isOpen) {
    await itemMenu.locator('summary[aria-label="Item actions"]').click();
  }
  await itemMenu.getByRole('menuitem', { name: /^add to playlist$/i }).click();
}

async function selectNewCollection(
  collectionSelect: Locator,
  collectionNameInput: Locator,
  existingCollectionId: number,
): Promise<void> {
  await expect(collectionSelect.locator(`option[value="${existingCollectionId}"]`)).toHaveCount(1, {
    timeout: 10_000,
  });
  await expect.poll(async () => collectionSelect.inputValue(), { timeout: 10_000 }).not.toBe('-1');
  await collectionSelect.selectOption('-1');
  await collectionSelect.dispatchEvent('change');
  await expect(collectionNameInput).toBeEnabled({ timeout: 10_000 });
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
    await expect(page.getByRole('link', { name: seed.tag })).toBeVisible();

    const actionBar = page.locator('div[class*="action-bar"]').first();
    const actionBox = await actionBar.boundingBox();
    expect(actionBox?.height).toBeGreaterThanOrEqual(60);
    expect(actionBox?.height).toBeLessThanOrEqual(80);

    // Pin the measurement to the max-width content container. `admin_result_view`
    // wraps page content in an unconstrained `<section data-admin-resource>`, so a
    // bare `section` locator would silently retarget to that wrapper and measure
    // the full main-content box instead of the 96rem grid.
    const content = page
      .locator('section[class*="content"]:not([data-admin-resource])')
      .filter({ has: page.getByRole('heading', { name: 'Remote Streams' }) })
      .first();
    const contentBox = await content.boundingBox();
    expect(contentBox?.width).toBeGreaterThan(700);
    expect(contentBox?.width).toBeLessThanOrEqual(1540);

    await page.screenshot({ path: screenshotPath(testInfo, 'remote-streams-list-desktop'), fullPage: true });
  });

  test('add-on-list search opens the SSR Remote Stream detail with native actions', async ({ page, browser, baseURL }, testInfo) => {
    const seed = seedRemoteStreamDetails(testInfo);
    const query = encodeURIComponent(`addon_list:${seed.addonListId}`);
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto(`/search?query=${query}`);
    await expect.poll(
      () => page.evaluate(
        () => (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true,
      ),
      { timeout: 15_000 },
    ).toBe(true);

    const detailLink = page.getByRole('link', { name: 'Nurburgring broadcast' }).first();
    await expect(detailLink).toHaveAttribute('href', `/media/remote/streams/${seed.mediaItemId}`);
    await detailLink.focus();
    await Promise.all([
      page.waitForURL(`/media/remote/streams/${seed.mediaItemId}`),
      detailLink.press('Enter'),
    ]);

    await expect(page.getByRole('heading', { name: 'Nurburgring broadcast' })).toBeVisible();
    await expect(page.getByText('A live motorsport fixture.')).toBeVisible();
    await expect(page.getByText('Live source')).toBeVisible();
    await expect(page.getByText('00:30:00')).toBeVisible();
    await expect(page.getByRole('img', { name: 'Nurburgring broadcast' })).toHaveCount(1);
    await expect(page.getByRole('heading', { name: 'Playback health' })).toBeVisible();
    await expect(page.getByText('English stereo')).toBeVisible();
    await expect(page.getByText('Opening lap')).toBeVisible();

    const owner = page.locator(`[data-addon-list-id="${seed.addonListId}"]`);
    await expect(owner).toContainText(`E2E Published Remote Streams ${testInfo.project.name}`);
    await expect(owner.getByRole('heading', { name: 'Published options' })).toBeVisible();
    await expect(owner).toContainText('Stream quality');
    await expect(owner).toContainText('High');

    const channel = page.locator(`[data-channel-id="${seed.channelId}"]`);
    await expect(channel).toContainText(`E2E Remote Stream Channel ${testInfo.project.name}`);
    await expect(channel.getByRole('link', { name: 'Configure' }))
      .toHaveAttribute('href', `/channels/${seed.channelId}`);
    await expect(channel.getByRole('link', { name: 'Watch' }))
      .toHaveAttribute('href', `/channels/${seed.channelId}/watch`);
    await expect(page.getByText('https://example.test/nurburgring.m3u8')).toHaveCount(0);
    expect(await page.evaluate(
      () => document.documentElement.scrollWidth <= document.documentElement.clientWidth,
    )).toBe(true);

    const noJsContext = await browser.newContext({ javaScriptEnabled: false, baseURL });
    const noJsPage = await noJsContext.newPage();
    const response = await noJsPage.goto(`/media/remote/streams/${seed.mediaItemId}`);
    const html = await response?.text();
    expect(html).toContain('Nurburgring broadcast');
    expect(html).toContain(`E2E Published Remote Streams ${testInfo.project.name}`);
    expect(html).toContain(`E2E Remote Stream Channel ${testInfo.project.name}`);
    await expect(noJsPage.getByRole('link', { name: 'Watch' }))
      .toHaveAttribute('href', `/channels/${seed.channelId}/watch`);
    await noJsContext.close();
  });

  test('playlist management hydrates and preserves hand-authored manifests', async ({ page }, testInfo) => {
    const seed = seedRemoteStreams(testInfo);
    const root = mkdtempSync(join(tmpdir(), 'ersatzrs-playlist-e2e-'));
    try {
      sqlite(`UPDATE "LibraryPath" SET "Path" = '${sqlEscape(root)}' WHERE "Id" = ${seed.libraryPathId};`);
      writeFileSync(join(root, 'Hand.playlist.yml'), [
        'name: Hand-authored E2E',
        'url: https://example.test/hand',
        `addon: org.ersatzrs.addon.remote-fixture.${testInfo.project.name}`,
        'folder: Hand',
        'create_playlist: true',
        '',
      ].join('\n'));
      mkdirSync(join(root, 'Hand'));
      writeFileSync(join(root, 'Hand', '.ersatzrs-playlist.json'), JSON.stringify({
        version: 1,
        manifest_etag: 'manifest',
        playlist_etag: 'playlist',
        last_sync: '2026-08-10T12:00:00Z',
        last_result: { kind: 'ok', entries: 1, truncated: false },
        managed_files: { one: 'one.yml' },
        entries: [{ id: 'one', file: 'one.yml', title: 'Episode one', duration_seconds: 60, active: true }],
        overrides: { excluded: [] },
      }));

      await page.goto(`/media/remote/streams/playlists/${seed.libraryId}`);
      await expect(page.getByTestId('remote-stream-playlists-page')).toBeVisible();
      const handRow = page.getByTestId('remote-playlist-row').filter({ hasText: 'Hand-authored E2E' });
      await expect(handRow).toBeVisible();
      await expect(handRow.getByRole('button', { name: /^edit$/i })).toBeDisabled();
      await expect(handRow.getByText('Episode one')).toBeVisible();

      await page.getByTestId('remote-playlist-add').click();
      await expect(page.getByTestId('remote-playlist-editor')).toBeVisible();
      await page.locator('#remote-playlist-name').fill('Managed E2E');
      await page.locator('#remote-playlist-url').fill('https://example.test/managed');
      await page.locator('#remote-playlist-addon').selectOption(`org.ersatzrs.addon.remote-fixture.${testInfo.project.name}`);
      await page.locator('#remote-playlist-folder').fill('Managed E2E');
      await page.getByTestId('remote-playlist-save').click();
      await expect(page.getByTestId('remote-playlist-row').filter({ hasText: 'Managed E2E' })).toBeVisible();
      await expect(page.locator('#remote-playlist-name')).toBeHidden();
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
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
    await selectNewCollection(collectionSelect, collectionNameInput, seed.collectionId);

    await collectionModal.getByRole('button', { name: /^cancel$/i }).click();
    await expect(collectionModal.getByRole('heading', { name: /^add to collection$/i })).toBeHidden();

    await clickFirstCardAddToCollection(page);
    await expect(collectionModal.getByRole('heading', { name: /^add to collection$/i })).toBeVisible();
    await expect(collectionModal.getByText('1 selected items')).toBeVisible();
    await selectNewCollection(collectionSelect, collectionNameInput, seed.collectionId);
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

  test('hydrated card adds a remote stream directly to a playlist', async ({ page }, testInfo) => {
    const seed = seedRemoteStreams(testInfo);
    await gotoRemoteStreams(page, seed);

    await clickFirstCardAddToPlaylist(page);
    const playlistModal = page
      .locator('div[class*="panel"]')
      .filter({ has: page.getByRole('heading', { name: /^add to playlist$/i }) })
      .last();
    await expect(playlistModal).toBeVisible();
    await expect(playlistModal.getByText('1 selected items')).toBeVisible();
    await playlistModal
      .locator('select')
      .first()
      .selectOption({ label: `E2E Remote Stream Playlist Group ${testInfo.project.name}` });
    await expect(playlistModal.locator('select').nth(1)).toContainText(
      `E2E Remote Stream Playlist ${testInfo.project.name}`,
    );
    await playlistModal
      .locator('select')
      .nth(1)
      .selectOption({ label: `E2E Remote Stream Playlist ${testInfo.project.name}` });
    await playlistModal.getByRole('button', { name: /^add to playlist$/i }).click();

    await expect(playlistModal).toBeHidden();
    await expect.poll(() => sqlite(`
SELECT "CollectionType"
FROM "PlaylistItem"
WHERE "PlaylistId" = ${seed.playlistId}
  AND "MediaItemId" = ${seed.mediaItemId}
ORDER BY "Id" DESC
LIMIT 1;
`)).toBe('70');
  });

  test('no-JS route still server-renders the list and pager links', async ({ browser, baseURL }, testInfo) => {
    const seed = seedRemoteStreams(testInfo);
    const context = await browser.newContext({ javaScriptEnabled: false, baseURL });
    const noJsPage = await context.newPage();

    const response = await noJsPage.goto(`/media/remote/streams?query=library_id:${seed.libraryId}`);
    expect(await response?.text()).toContain('Nurburgring broadcast');
    await expect(noJsPage.getByRole('heading', { name: 'Remote Streams' }).first()).toBeVisible();
    await noJsPage.getByRole('combobox', { name: 'Rows per page' }).selectOption('50');
    const changedPageResponse = noJsPage.waitForResponse(
      (candidate) => candidate.request().resourceType() === 'document' && candidate.url().includes('page_size=50'),
    );
    await noJsPage.getByRole('button', { name: 'Apply' }).click();
    const changedPageHtml = await (await changedPageResponse).text();
    await expect(noJsPage).toHaveURL(/page_size=50/);
    expect(changedPageHtml).toContain('Nurburgring broadcast');
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
