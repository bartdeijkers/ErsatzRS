import { expect, Page, TestInfo, test } from '@playwright/test';
import { execFileSync } from 'child_process';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

type Seed = {
  collectionId: number;
  smartCollectionId: number;
  multiCollectionId: number;
  rerunCollectionId: number;
  playlistGroupId: number;
  playlistId: number;
  playlistItemId: number;
  fillerPresetId: number;
};

function projectBase(testInfo: TestInfo): number {
  const projectOffset = Array.from(testInfo.project.name).reduce(
    (total, char) => total + char.charCodeAt(0),
    0,
  ) * 100;
  return 150_000 + projectOffset;
}

function screenshotPath(testInfo: TestInfo, name: string): string {
  return `test-results/${name}-${testInfo.project.name}.png`;
}

function sqlEscape(value: string): string {
  return value.replace(/'/g, "''");
}

function sqlite(sql: string): string {
  if (!dbPath) {
    throw new Error('E2E_DB_PATH or ERSATZRS_DB_PATH is required for route-family seeding');
  }
  return execFileSync('sqlite3', [dbPath], {
    input: `.timeout 10000\n${sql}`,
    encoding: 'utf8',
  }).trim();
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

function seedRouteFamily(testInfo: TestInfo): Seed {
  const base = projectBase(testInfo);
  const seed = {
    collectionId: base + 1,
    smartCollectionId: base + 2,
    multiCollectionId: base + 3,
    rerunCollectionId: base + 4,
    playlistGroupId: base + 5,
    playlistId: base + 6,
    playlistItemId: base + 7,
    fillerPresetId: base + 8,
  };

  sqlite(`
DELETE FROM "FillerPreset" WHERE "Id" = ${seed.fillerPresetId};
DELETE FROM "PlaylistItem" WHERE "PlaylistId" = ${seed.playlistId};
DELETE FROM "Playlist" WHERE "Id" = ${seed.playlistId};
DELETE FROM "PlaylistGroup" WHERE "Id" = ${seed.playlistGroupId};
DELETE FROM "RerunCollection" WHERE "Id" = ${seed.rerunCollectionId};
DELETE FROM "MultiCollectionItem" WHERE "MultiCollectionId" = ${seed.multiCollectionId};
DELETE FROM "MultiCollectionSmartItem" WHERE "MultiCollectionId" = ${seed.multiCollectionId};
DELETE FROM "MultiCollection" WHERE "Id" = ${seed.multiCollectionId};
DELETE FROM "SmartCollection" WHERE "Id" = ${seed.smartCollectionId};
DELETE FROM "Collection" WHERE "Id" = ${seed.collectionId};

INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder")
  VALUES (${seed.collectionId}, 'E2E Manual Collection', 1);
INSERT INTO "SmartCollection" ("Id", "Name", "Query")
  VALUES (${seed.smartCollectionId}, 'E2E Smart Collection', 'media_kind:movie');
INSERT INTO "MultiCollection" ("Id", "Name")
  VALUES (${seed.multiCollectionId}, 'E2E Multi Collection');
INSERT INTO "MultiCollectionItem" (
  "MultiCollectionId", "CollectionId", "ScheduleAsGroup", "PlaybackOrder"
) VALUES (${seed.multiCollectionId}, ${seed.collectionId}, 1, 4);
INSERT INTO "RerunCollection" (
  "Id", "Name", "CollectionType", "CollectionId", "FirstRunPlaybackOrder", "RerunPlaybackOrder"
) VALUES (${seed.rerunCollectionId}, 'E2E Rerun Collection', 0, ${seed.collectionId}, 1, 3);
INSERT INTO "PlaylistGroup" ("Id", "Name", "IsSystem")
  VALUES (${seed.playlistGroupId}, 'E2E Playlist Group', 0);
INSERT INTO "Playlist" ("Id", "Name", "PlaylistGroupId", "IsSystem")
  VALUES (${seed.playlistId}, 'E2E Playlist', ${seed.playlistGroupId}, 0);
INSERT INTO "PlaylistItem" (
  "Id", "Index", "PlaylistId", "CollectionType", "CollectionId",
  "PlaybackOrder", "Count", "PlayAll", "IncludeInProgramGuide"
) VALUES (${seed.playlistItemId}, 0, ${seed.playlistId}, 0, ${seed.collectionId}, 1, 1, 0, 1);
INSERT INTO "FillerPreset" (
  "Id", "Name", "FillerKind", "FillerMode", "Duration", "Count",
  "PadToNearestMinute", "AllowWatermarks", "CollectionType", "CollectionId",
  "MediaItemId", "MultiCollectionId", "SmartCollectionId", "PlaylistId",
  "Expression", "UseChaptersAsMediaItems"
) VALUES (
  ${seed.fillerPresetId}, 'E2E Filler Preset', 1, 2, NULL, 1,
  NULL, 1, 0, ${seed.collectionId}, NULL, NULL, NULL, NULL, '', 0
);
`);

  return seed;
}

async function expectContentGeometry(page: Page, heading: string): Promise<void> {
  await expect(page.getByRole('heading', { name: heading, exact: true })).toBeVisible();
  const content = page
    .locator('section, form, div')
    .filter({ has: page.getByRole('heading', { name: heading, exact: true }) })
    .first();
  const box = await content.boundingBox();
  expect(box?.width).toBeGreaterThan(320);
  expect(box?.width).toBeLessThanOrEqual(1600);
}

async function expectCollectionPagerTypography(page: Page): Promise<void> {
  const styles = await page.locator('main').evaluate((main) => {
    const button = main.querySelector('button[class*="pager-btn"]');
    const select = main.querySelector('select[class*="page-size"]');
    if (!(button instanceof HTMLElement) || !(select instanceof HTMLElement)) {
      return null;
    }
    const buttonStyle = window.getComputedStyle(button);
    const selectStyle = window.getComputedStyle(select);
    return {
      buttonFontSize: buttonStyle.fontSize,
      buttonLineHeight: buttonStyle.lineHeight,
      selectFontSize: selectStyle.fontSize,
      selectLineHeight: selectStyle.lineHeight,
    };
  });

  expect(styles).not.toBeNull();
  expect(styles?.buttonFontSize).toBe('14px');
  expect(styles?.selectFontSize).toBe('14px');
  expect(styles?.buttonLineHeight).toBe(styles?.selectLineHeight);
}

async function expectMultiCollectionEditorGeometry(page: Page): Promise<void> {
  await expect(page.getByRole('button', { name: /save multi collection/i })).toHaveCount(1);

  const metrics = await page.locator('main').evaluate((main) => {
    const rect = (element: Element) => {
      const box = element.getBoundingClientRect();
      return {
        bottom: box.bottom,
        left: box.left,
        top: box.top,
        width: box.width,
      };
    };
    const text = (element: Element) => element.textContent?.replace(/\s+/g, ' ').trim() ?? '';
    const actionBar = main.querySelector('[class*="editor-action-bar"]');
    const heading = Array.from(main.querySelectorAll('h1')).find(
      (element) => text(element) === 'Multi Collection',
    );
    const editor = heading?.parentElement;
    const itemsSection = Array.from(main.querySelectorAll('section')).find((element) =>
      text(element).includes('E2E Multi Collection Items'),
    );
    const table = itemsSection?.querySelector('table');
    const collectionSelect = Array.from(main.querySelectorAll('select')).find((element) =>
      Array.from(element.options).some((option) => option.textContent === 'E2E Manual Collection'),
    );
    const collectionAddButton = Array.from(main.querySelectorAll('button')).find(
      (element) => text(element) === 'Add Collection',
    );
    const smartAddButton = Array.from(main.querySelectorAll('button')).find(
      (element) => text(element) === 'Add Smart Collection',
    );

    if (
      !actionBar ||
      !heading ||
      !editor ||
      !itemsSection ||
      !table ||
      !collectionSelect ||
      !collectionAddButton ||
      !smartAddButton
    ) {
      return null;
    }

    return {
      actionBar: rect(actionBar),
      addButton: rect(collectionAddButton),
      editor: rect(editor),
      heading: rect(heading),
      itemsSection: rect(itemsSection),
      select: rect(collectionSelect),
      smartAddButton: rect(smartAddButton),
      table: rect(table),
    };
  });

  expect(metrics).not.toBeNull();
  expect(metrics?.actionBar.width).toBeGreaterThan(metrics?.editor.width ?? 0);
  expect(metrics?.heading.top).toBeGreaterThan(metrics?.actionBar.bottom ?? 0);
  expect(metrics?.itemsSection.top).toBeGreaterThan(metrics?.smartAddButton.bottom ?? 0);
  expect(metrics?.itemsSection.width).toBeGreaterThan((metrics?.editor.width ?? 0) * 0.9);
  expect(metrics?.table.width).toBeGreaterThan((metrics?.editor.width ?? 0) * 0.9);
  expect(metrics?.addButton.left).toBeGreaterThanOrEqual((metrics?.select.left ?? 0) - 1);
  expect(metrics?.addButton.width).toBeLessThan((metrics?.select.width ?? 0) * 0.9);
}

test.describe('media collection route-family parity', () => {
  test.skip(!dbPath, 'E2E_DB_PATH or ERSATZRS_DB_PATH is required to seed collection routes');

  test('multi-collections, rerun collections, playlists, and filler presets render route-family workflows', async ({
    browser,
    baseURL,
    page,
  }, testInfo) => {
    test.skip(
      testInfo.project.name !== 'chromium',
      'This route-family proof captures screenshots once to avoid cross-project app-state churn.',
    );

    const runtimeErrors: string[] = [];
    page.on('console', (message) => {
      if (message.type() === 'error') {
        runtimeErrors.push(message.text());
      }
    });
    page.on('pageerror', (error) => runtimeErrors.push(error.message));

    const seed = seedRouteFamily(testInfo);
    await forceEnglish(page);

    const noJsContext = await browser.newContext({ javaScriptEnabled: false, baseURL });
    const noJsPage = await noJsContext.newPage();
    const fallbackName = `E2E Multi Fallback ${Date.now()}`;
    await noJsPage.goto('/media/multi-collections/add');
    await expect(
      noJsPage.getByRole('heading', { name: 'Multi Collection', exact: true }),
    ).toBeVisible();
    const fallbackForm = noJsPage.locator(
      'noscript form[action="/api/multi-collections/save"]',
    );
    await fallbackForm.locator('input[name="name"]').fill(fallbackName);
    await fallbackForm.getByRole('button', { name: /add multi collection/i }).click();
    await expect(noJsPage).toHaveURL(/\/media\/multi-collections$/);
    await noJsContext.close();
    expect(sqlite(`
SELECT COUNT(*) FROM "MultiCollection" WHERE "Name" = '${sqlEscape(fallbackName)}';
`)).toBe('1');

    await page.goto('/media/multi-collections');
    await expectContentGeometry(page, 'Multi Collections');
    await expectCollectionPagerTypography(page);
    await expect(page.getByText('E2E Multi Collection').first()).toBeVisible();
    await expect(page.getByText(fallbackName).first()).toBeVisible();
    await page.screenshot({
      path: screenshotPath(testInfo, 'media-route-family-multi-collections'),
      fullPage: true,
    });
    await page.goto(`/media/multi-collections/${seed.multiCollectionId}/edit`);
    await expect(page.getByRole('heading', { name: 'Multi Collection', exact: true })).toBeVisible();
    await expect(page.getByRole('cell', { name: 'E2E Manual Collection' })).toBeVisible();
    await expect(page.getByText('Schedule As Group').first()).toBeVisible();
    await expectMultiCollectionEditorGeometry(page);
    await page.screenshot({
      path: screenshotPath(testInfo, 'media-route-family-multi-collection-editor'),
      fullPage: true,
    });

    await page.goto('/media/rerun-collections');
    await expectContentGeometry(page, 'Rerun Collections');
    await expect(page.getByText('E2E Rerun Collection').first()).toBeVisible();
    await page.goto(`/media/rerun-collections/${seed.rerunCollectionId}/edit`);
    await expect(page.getByRole('heading', { name: 'Rerun Collection', exact: true })).toBeVisible();
    await expect(page.locator('select[name="collection_type"]')).toHaveValue('0');
    await expect(page.locator('select[name="collection_id"]')).toHaveValue(
      seed.collectionId.toString(),
    );

    await page.goto('/media/playlists');
    await expectContentGeometry(page, 'Playlists');
    await expect(
      page.locator('div[class*="playlist-group-header"]').filter({ hasText: 'E2E Playlist Group' }),
    ).toBeVisible();
    await expect(page.locator('li').filter({ hasText: 'E2E Playlist' })).toBeVisible();
    await page.goto(`/media/playlists/${seed.playlistId}`);
    await expect(page.getByRole('heading', { name: 'Playlist', exact: true })).toBeVisible();
    await expect(page.getByText('E2E Manual Collection').first()).toBeVisible();
    await expect(page.getByRole('button', { name: /^copy$/i }).first()).toBeVisible();
    await expect(page.getByRole('button', { name: /^move down$/i }).first()).toBeDisabled();
    await expect(page.getByRole('button', { name: /^delete$/i }).first()).toBeVisible();

    await page.goto('/media/filler/presets');
    await expectContentGeometry(page, 'Filler Presets');
    await expect(page.getByText('E2E Filler Preset').first()).toBeVisible();
    await page.goto(`/media/filler/presets/${seed.fillerPresetId}/edit`);
    await expect(page.getByRole('heading', { name: 'Filler Preset', exact: true })).toBeVisible();
    await expect(page.locator('input[name="name"]')).toHaveValue('E2E Filler Preset');
    await expect(page.locator('select[name="collection_type"]')).toHaveValue('0');

    await expect(page.locator('[role="alert"]')).toHaveCount(0);
    expect(runtimeErrors).toEqual([]);
  });
});
