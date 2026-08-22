import { expect, Page, TestInfo, test } from '@playwright/test';
import { execFileSync } from 'child_process';
import { mkdirSync, writeFileSync } from 'fs';
import { dirname, join } from 'path';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

type MetadataSeed = {
  addonId: string;
  listId: number;
  firstItemId: number;
  secondItemId: number;
  firstTitle: string;
  secondTitle: string;
};

function projectBase(testInfo: TestInfo): number {
  const projectOffset = Array.from(testInfo.project.name).reduce(
    (total, character) => total + character.charCodeAt(0),
    0,
  ) * 1_000;
  return 760_000 + projectOffset;
}

function sqlEscape(value: string): string {
  return value.replace(/'/g, "''");
}

function sqlite(sql: string): string {
  if (!dbPath) {
    throw new Error('E2E_DB_PATH or ERSATZRS_DB_PATH is required for metadata editor seeding');
  }
  return execFileSync('sqlite3', [dbPath], {
    input: `.timeout 10000\n${sql}`,
    encoding: 'utf8',
  }).trim();
}

function watchRuntimeErrors(page: Page): string[] {
  const runtimeErrors: string[] = [];
  page.on('pageerror', (error) => {
    const text = error.stack ?? error.message;
    if (
      !(
        text.includes('AbortError') &&
        text.toLowerCase().includes('operation was aborted')
      ) &&
      !text.includes('NetworkError when attempting to fetch resource')
    ) {
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

async function gotoHydrated(page: Page, route: string): Promise<void> {
  const response = await page.goto(route, { waitUntil: 'domcontentloaded' });
  expect(response?.status(), `GET ${route}`).toBe(200);
  await expect(page.locator('[data-not-found]')).toHaveCount(0);
  await waitUntilHydrated(page);
}

async function waitUntilHydrated(page: Page): Promise<void> {
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

function seedMetadataEditor(testInfo: TestInfo): MetadataSeed {
  if (!dbPath) {
    throw new Error('E2E database path is required');
  }
  const base = projectBase(testInfo);
  const suffix = testInfo.project.name.toLowerCase().replace(/[^a-z0-9]+/g, '-');
  const addonId = `org.example.e2e.metadata.${suffix}`;
  const listId = base + 1;
  const firstItemId = base + 2;
  const secondItemId = base + 3;
  const sourceId = base + 4;
  const libraryId = base + 5;
  const libraryPathId = base + 6;
  const firstTitle = `E2E Metadata Primary ${testInfo.project.name}`;
  const secondTitle = `E2E Metadata Review ${testInfo.project.name}`;
  const folder = join(dirname(dbPath), `e2e-addon-metadata-${suffix}`);
  const packageRoot = join(dirname(dbPath), `e2e-addon-package-${suffix}`);
  mkdirSync(folder, { recursive: true });
  mkdirSync(packageRoot, { recursive: true });
  writeFileSync(join(packageRoot, 'unused'), 'not invoked\n');
  writeFileSync(
    join(folder, 'primary.yml'),
    [
      `provider_id: item:primary-${suffix}`,
      `url: https://example.invalid/e2e/${suffix}/primary`,
      `title: ${firstTitle}`,
      'duration: "00:20:00"',
      'is_live: false',
      'content_kind: auto',
      '',
    ].join('\n'),
  );
  writeFileSync(
    join(folder, 'review.yml'),
    [
      `provider_id: item:review-${suffix}`,
      `url: https://example.invalid/e2e/${suffix}/review`,
      `title: ${secondTitle}`,
      'duration: "00:10:00"',
      'is_live: false',
      'content_kind: auto',
      '',
    ].join('\n'),
  );

  const manifest = JSON.stringify({
    manifest_version: 3,
    id: addonId,
    version: '1.0.0',
    name: { 'en-US': `E2E Metadata Provider ${testInfo.project.name}` },
    summary: { 'en-US': 'Browser metadata editor proof' },
    icon: null,
    license: 'Zlib',
    source_url: 'https://example.invalid/e2e-metadata-addon',
    host_version: '*',
    capabilities: [{ id: 'addon.check.v1' }, { id: 'media-list.list.v4' }],
    dependencies: [],
    entrypoints: { any: { path: 'unused', kind: 'native' } },
    permissions: {
      network_hosts: [],
      media_read: false,
      managed_ffmpeg: false,
      external_commands: [],
    },
    settings: [],
    panels: [],
  });
  const providerMetadata = JSON.stringify({
    title: 'Provider refreshed',
    plot: 'Provider collection summary',
    genres: ['Documentary'],
    tags: ['archive', 'television'],
  });
  const metadataOverride = JSON.stringify({ title: 'Operator draft' });
  const metadataConflict = JSON.stringify({
    title: {
      provider_before: 'Provider original',
      provider_after: 'Provider refreshed',
      operator_value: 'Operator draft',
    },
  });
  const firstProviderMetadata = JSON.stringify({
    title: firstTitle,
    plot: 'Primary item provider summary',
  });
  const secondProviderMetadata = JSON.stringify({
    title: secondTitle,
    plot: 'Review item provider summary',
  });

  sqlite(`
DELETE FROM "AddonMediaListArtworkMaterialization" WHERE "AddonMediaListId" = ${listId};
DELETE FROM "AddonMediaListArtworkUpload" WHERE "AddonMediaListId" = ${listId};
DELETE FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ${listId};
DELETE FROM "AddonMediaList" WHERE "Id" = ${listId};
DELETE FROM "LibraryPath" WHERE "Id" = ${libraryPathId};
DELETE FROM "LocalLibrary" WHERE "Id" = ${libraryId};
DELETE FROM "Library" WHERE "Id" = ${libraryId};
DELETE FROM "LocalMediaSource" WHERE "Id" = ${sourceId};
DELETE FROM "MediaSource" WHERE "Id" = ${sourceId};

INSERT INTO "AddonInstallation" (
  "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
  "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt", "TrustTier"
) VALUES (
  '${sqlEscape(addonId)}', '1.0.0', 1, 'e2e-package', 'e2e-content',
  '${sqlEscape(manifest)}', '${sqlEscape(packageRoot)}',
  '2026-08-21T00:00:00Z', '2026-08-21T00:00:00Z', 'verified'
) ON CONFLICT("AddonId") DO UPDATE SET
  "Enabled" = 1,
  "ManifestJson" = excluded."ManifestJson",
  "InstallPath" = excluded."InstallPath",
  "UpdatedAt" = excluded."UpdatedAt";

INSERT INTO "MediaSource" ("Id") VALUES (${sourceId});
INSERT INTO "LocalMediaSource" ("Id") VALUES (${sourceId});
INSERT INTO "Library" ("Id", "MediaSourceId", "MediaKind", "Name")
  VALUES (${libraryId}, ${sourceId}, 7, 'E2E metadata remote streams');
INSERT INTO "LocalLibrary" ("Id") VALUES (${libraryId});
INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path")
  VALUES (${libraryPathId}, ${libraryId}, '${sqlEscape(folder)}');

INSERT INTO "AddonMediaList" (
  "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "Description", "ItemCount",
  "AutoRefresh", "GeneratePlaylist", "ProviderMetadataJson", "MetadataOverrideJson",
  "MetadataConflictJson", "ReviewState", "PresentationKind", "RequestedPresentationKind"
) VALUES (
  ${listId}, '${sqlEscape(addonId)}', 'e2e/metadata-${suffix}',
  'https://example.invalid/e2e/${suffix}/list', 'Operator draft',
  'Provider collection summary', 2, 0, 0,
  '${sqlEscape(providerMetadata)}', '${sqlEscape(metadataOverride)}',
  '${sqlEscape(metadataConflict)}', 1, 0, 0
);
INSERT INTO "AddonMediaListLibrary" (
  "AddonMediaListId", "LibraryId", "FolderName", "ManifestPath"
) VALUES (${listId}, ${libraryId}, 'e2e-addon-metadata-${suffix}', '${sqlEscape(folder)}');

INSERT INTO "AddonMediaListItem" (
  "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind", "Rank", "Title",
  "SourceUrl", "Availability", "ContentKind", "Included", "Published",
  "ProviderMetadataJson", "MetadataOverrideJson"
) VALUES
  (${firstItemId}, ${listId}, 'item:primary-${suffix}', '${sqlEscape(firstTitle)}', 4, 0,
   '${sqlEscape(firstTitle)}', 'https://example.invalid/e2e/${suffix}/primary', 0, 0, 1, 0,
   '${sqlEscape(firstProviderMetadata)}', '{}'),
  (${secondItemId}, ${listId}, 'item:review-${suffix}', '${sqlEscape(secondTitle)}', 4, 1,
   '${sqlEscape(secondTitle)}', 'https://example.invalid/e2e/${suffix}/review', 2, 0, 1, 0,
   '${sqlEscape(secondProviderMetadata)}', '{}');
`);

  return { addonId, listId, firstItemId, secondItemId, firstTitle, secondTitle };
}

test.describe('shared add-on metadata editor', () => {
  test.skip(!dbPath, 'E2E_DB_PATH or ERSATZRS_DB_PATH is required');

  test('keeps source and import metadata together on the add page', async ({ page }) => {
    const runtimeErrors = watchRuntimeErrors(page);
    await forceEnglish(page);
    await gotoHydrated(page, '/media/addon-lists/add');

    await expect(page.getByRole('heading', { name: 'Add add-on list' })).toBeVisible();
    await expect(page.getByLabel('Add-on list URL')).toBeVisible();
    await expect(page.getByLabel('Add-on', { exact: true })).toHaveValue('');

    const collectionType = page.getByLabel('Collection Type', { exact: true });
    await expect(collectionType).toHaveValue('television_show');
    await collectionType.selectOption('movies');
    await expect(collectionType).toHaveValue('movies');

    await expect(page.getByLabel('Storage folder')).toBeVisible();
    await expect(
      page.getByRole('checkbox', { name: 'Update list from the provider once each day' }),
    ).toBeChecked();
    await expect(
      page.getByRole('checkbox', { name: 'Create playlist in original order' }),
    ).not.toBeChecked();
    await expect(page.getByRole('button', { name: 'Import' })).toBeDisabled();
    expect(runtimeErrors).toEqual([]);
  });

  test('assigns missing episode numbers in original order and allows a bounded overwrite', async ({
    page,
  }, testInfo) => {
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedMetadataEditor(testInfo);
    sqlite(`
UPDATE "AddonMediaList"
SET "PresentationKind" = 1, "RequestedPresentationKind" = 1
WHERE "Id" = ${seed.listId};
UPDATE "AddonMediaListItem"
SET "ContentKind" = 1
WHERE "AddonMediaListId" = ${seed.listId};
UPDATE "AddonMediaListItem"
SET "ProviderMetadataJson" = json_set(
      "ProviderMetadataJson", '$.season', 4, '$.episode', 9
    ),
    "Season" = 4,
    "Episode" = 9
WHERE "Id" = ${seed.firstItemId};
`);
    await forceEnglish(page);
    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);

    const numberingButton = page.getByRole('button', { name: /^number episodes$/i });
    await expect(numberingButton).toBeVisible();
    await numberingButton.click();
    const panel = page.locator('#episode-numbering-panel');
    await expect(panel).toBeVisible();
    await expect(panel.getByText(/1 episodes · s01e01–s01e01/i)).toBeVisible();
    await panel.getByRole('button', { name: /^apply$/i }).click();
    await expect
      .poll(() =>
        sqlite(`
SELECT json_extract("MetadataOverrideJson", '$.season') || ',' ||
       json_extract("MetadataOverrideJson", '$.episode')
FROM "AddonMediaListItem" WHERE "Id" = ${seed.secondItemId};
`),
      )
      .toBe('1,1');
    await expect(page.getByText('S04E09', { exact: true })).toBeVisible();
    await expect(page.getByText('S01E01', { exact: true })).toBeVisible();

    await numberingButton.click();
    await panel.getByLabel(/^season$/i).fill('1');
    await panel.getByLabel(/^first episode$/i).fill('1');
    await panel.getByLabel(/^number to assign$/i).fill('1');
    await panel.getByRole('checkbox', { name: /replace existing episode numbers/i }).check();
    await expect(panel.getByText(/1 episodes · s01e02–s01e02/i)).toBeVisible();
    await panel.getByRole('button', { name: /^apply$/i }).click();
    await expect
      .poll(() =>
        sqlite(`
SELECT json_extract("MetadataOverrideJson", '$.season') || ',' ||
       json_extract("MetadataOverrideJson", '$.episode')
FROM "AddonMediaListItem" WHERE "Id" = ${seed.firstItemId};
`),
      )
      .toBe('1,2');
    await expect(page.getByText('S01E02', { exact: true })).toBeVisible();
    expect(
      sqlite(`
SELECT json_extract("MetadataOverrideJson", '$.season') || ',' ||
       json_extract("MetadataOverrideJson", '$.episode')
FROM "AddonMediaListItem" WHERE "Id" = ${seed.secondItemId};
`),
    ).toBe('1,1');
    expect(runtimeErrors).toEqual([]);
  });

  test('reviews, edits, uploads, rejects stale ETags, publishes, and reincludes', async ({
    page,
  }, testInfo) => {
    test.setTimeout(90_000);
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedMetadataEditor(testInfo);
    await forceEnglish(page);
    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);

    const editorForm = page.locator('#addon_media_lists-list-editor-form');
    const hero = editorForm.locator('header[aria-expanded]');
    await expect(hero.getByRole('heading', { name: 'Operator draft' })).toBeVisible();
    await expect(hero.getByText(/1 metadata conflicts/i)).toBeVisible();
    await hero.getByRole('button', { name: /^edit metadata$/i }).click();
    await expect(
      editorForm.getByRole('heading', { name: /^metadata conflicts$/i }),
    ).toBeVisible();
    await expect(editorForm.getByText('Provider original')).toBeVisible();
    await expect(editorForm.getByText('Provider refreshed')).toBeVisible();

    const reviewFilter = page.getByRole('button', { name: /1 need review/i });
    await reviewFilter.click();
    await expect(reviewFilter).toHaveAttribute('aria-pressed', 'true');
    const itemsTable = page.getByTestId('addon-metadata-items');
    await expect(itemsTable.getByText(seed.secondTitle)).toBeVisible();
    await expect(itemsTable.getByText(seed.firstTitle)).toHaveCount(0);

    let reviewRow = itemsTable.locator('tbody tr').filter({ hasText: seed.secondTitle }).first();
    await reviewRow.getByRole('button', { name: seed.secondTitle }).click();
    const inlineEditor = itemsTable
      .getByRole('row')
      .filter({ has: page.getByLabel(/^tags/i) })
      .first();
    await expect(inlineEditor).toBeVisible();
    const manyTags = Array.from({ length: 120 }, (_, index) => `tag-${index + 1}`).join('\n');
    await inlineEditor.getByLabel(/^tags/i).fill(manyTags);
    await inlineEditor.getByRole('button', { name: /^save$/i }).click();
    await expect
      .poll(() =>
        sqlite(`
SELECT json_extract("MetadataOverrideJson", '$.tags[119]')
FROM "AddonMediaListItem" WHERE "Id" = ${seed.secondItemId};
`),
      )
      .toBe('tag-120');

    reviewRow = itemsTable.locator('tbody tr').filter({ hasText: seed.secondTitle }).first();
    await reviewRow.getByRole('checkbox', { name: /^include$/i }).uncheck();
    await expect
      .poll(() =>
        sqlite(`SELECT "Included" FROM "AddonMediaListItem" WHERE "Id" = ${seed.secondItemId};`),
      )
      .toBe('0');

    const generalPanel = editorForm.getByTestId('addon-general-metadata');
    const posterUpload = generalPanel.getByLabel(/^upload image: poster$/i);
    await posterUpload.setInputFiles({
      name: 'poster.png',
      mimeType: 'image/png',
      buffer: Buffer.from(
        'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Y9Wl9sAAAAASUVORK5CYII=',
        'base64',
      ),
    });
    const posterValue = generalPanel.getByLabel(/^poster$/i);
    await expect(posterValue).toHaveValue(/^ersatzrs-upload:\/\//);

    sqlite(`
UPDATE "AddonMediaList"
SET "MetadataOverrideJson" = json_set("MetadataOverrideJson", '$.sort_title', 'Concurrent writer')
WHERE "Id" = ${seed.listId};
`);
    await page.getByRole('button', { name: /^save add-on list$/i }).click();
    await expect(page.getByText(/metadata changed since the editor was loaded/i)).toBeVisible();

    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);
    const refreshedForm = page.locator('#addon_media_lists-list-editor-form');
    const refreshedHero = refreshedForm.locator('header[aria-expanded]');
    await refreshedHero.getByRole('button', { name: /^edit metadata$/i }).click();
    const refreshedPanel = refreshedForm.getByTestId('addon-general-metadata');
    await refreshedPanel.getByRole('textbox', { name: /^title$/i }).fill('Reviewed collection');
    const saveNavigation = page.waitForEvent('framenavigated');
    await page.getByRole('button', { name: /^save add-on list$/i }).click();
    await saveNavigation;
    await waitUntilHydrated(page);
    await expect
      .poll(() =>
        sqlite(`
SELECT json_extract("MetadataOverrideJson", '$.title')
FROM "AddonMediaList" WHERE "Id" = ${seed.listId};
`),
      )
      .toBe('Reviewed collection');

    const firstPublishNavigation = page.waitForEvent('framenavigated');
    await page.getByRole('button', { name: /^publish$/i }).click();
    await firstPublishNavigation;
    await waitUntilHydrated(page);
    await expect
      .poll(
        () =>
          sqlite(`
SELECT "ReviewState" || ',' || "PublishedRevision"
FROM "AddonMediaList" WHERE "Id" = ${seed.listId};
`),
        { timeout: 30_000 },
      )
      .toBe('2,1');
    expect(
      sqlite(`SELECT "Published" FROM "AddonMediaListItem" WHERE "Id" = ${seed.firstItemId};`),
    ).toBe('1');
    expect(
      sqlite(`SELECT "Published" FROM "AddonMediaListItem" WHERE "Id" = ${seed.secondItemId};`),
    ).toBe('0');

    sqlite(`
UPDATE "AddonMediaListItem" SET "Availability" = 0, "AvailabilityReason" = NULL
WHERE "Id" = ${seed.secondItemId};
`);
    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);
    const reincludeRow = page
      .getByTestId('addon-metadata-items')
      .locator('tbody tr')
      .filter({ hasText: seed.secondTitle })
      .first();
    await reincludeRow.getByRole('checkbox', { name: /^include$/i }).check();
    await expect
      .poll(() =>
        sqlite(`SELECT "Included" FROM "AddonMediaListItem" WHERE "Id" = ${seed.secondItemId};`),
      )
      .toBe('1');
    const secondPublishNavigation = page.waitForEvent('framenavigated');
    await page.getByRole('button', { name: /^publish$/i }).click();
    await secondPublishNavigation;
    await waitUntilHydrated(page);
    await expect
      .poll(
        () =>
          sqlite(`
SELECT "PublishedRevision" || ',' || (
  SELECT "Published" FROM "AddonMediaListItem" WHERE "Id" = ${seed.secondItemId}
)
FROM "AddonMediaList" WHERE "Id" = ${seed.listId};
`),
        { timeout: 30_000 },
      )
      .toBe('2,1');

    const providerStateBeforeReclassification = sqlite(`
SELECT group_concat(
  "ProviderId" || '|' || "SourceUrl" || '|' || "Rank" || '|' ||
  "Included" || '|' || "ProviderMetadataJson" || '|' || "MetadataOverrideJson",
  char(10)
)
FROM (
  SELECT * FROM "AddonMediaListItem"
  WHERE "AddonMediaListId" = ${seed.listId}
  ORDER BY "Id"
);
`);
    const mediaType = page.getByLabel('Collection Type', { exact: true });
    await expect(mediaType.locator('option')).toHaveText([
      'Determine from the items',
      'TV Shows',
      'Movies',
      'Music Video',
      'Other Videos',
      'Songs',
      'Images',
      'Remote Streams',
    ]);
    const typeChangeHelp = page.getByTestId('addon-media-type-change-help');
    await expect(typeChangeHelp).toContainText('Current library type: Remote Streams');
    await expect(typeChangeHelp).toContainText('without importing the add-on again');

    await mediaType.selectOption('other_videos');
    await expect(typeChangeHelp).toContainText('Remote Streams → Other Videos');
    await expect(page.getByTestId('addon-media-type-change-route')).toBeVisible();
    const typeSaveNavigation = page.waitForEvent('framenavigated');
    await page.getByRole('button', { name: /^save add-on list$/i }).click();
    await typeSaveNavigation;
    await waitUntilHydrated(page);
    await expect(page.getByLabel('Collection Type', { exact: true })).toHaveValue('other_videos');
    await expect(page.getByTestId('addon-media-type-change-help')).toContainText(
      'Remote Streams → Other Videos',
    );

    const typePublishNavigation = page.waitForEvent('framenavigated');
    await page.getByRole('button', { name: /^publish$/i }).click();
    await typePublishNavigation;
    await waitUntilHydrated(page);
    await expect(page.getByTestId('addon-media-type-change-help')).toContainText(
      'Current library type: Other Videos',
    );
    await expect(page.getByTestId('addon-media-type-change-route')).not.toBeVisible();
    await expect
      .poll(
        () =>
          sqlite(`
SELECT (SELECT COUNT(*) FROM "AddonMediaListItemOtherVideo" AS "link"
        JOIN "AddonMediaListItem" AS "item" ON "item"."Id" = "link"."AddonMediaListItemId"
        WHERE "item"."AddonMediaListId" = ${seed.listId}) || ',' ||
       (SELECT COUNT(*) FROM "RemoteStream" AS "stream"
        JOIN "AddonMediaListItem" AS "item" ON "item"."MediaItemId" = "stream"."Id"
        WHERE "item"."AddonMediaListId" = ${seed.listId}) || ',' ||
       "PublishedRevision"
FROM "AddonMediaList" WHERE "Id" = ${seed.listId};
`),
        { timeout: 30_000 },
      )
      .toBe('2,2,3');
    expect(
      sqlite(`
SELECT group_concat(
  "ProviderId" || '|' || "SourceUrl" || '|' || "Rank" || '|' ||
  "Included" || '|' || "ProviderMetadataJson" || '|' || "MetadataOverrideJson",
  char(10)
)
FROM (
  SELECT * FROM "AddonMediaListItem"
  WHERE "AddonMediaListId" = ${seed.listId}
  ORDER BY "Id"
);
`),
    ).toBe(providerStateBeforeReclassification);

    expect(runtimeErrors).toEqual([]);
  });
});
