import { expect, Locator, Page, TestInfo, test } from '@playwright/test';
import { execFileSync } from 'child_process';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

type PreviewSeed = {
  blockId: number;
  blockName: string;
  collectionName: string;
  mediaTitle: string;
};

function projectBase(testInfo: TestInfo): number {
  const projectOffset = Array.from(testInfo.project.name).reduce(
    (total, char) => total + char.charCodeAt(0),
    0,
  ) * 1_000;
  return 850_000 + projectOffset;
}

function screenshotPath(testInfo: TestInfo, name: string): string {
  return `test-results/${name}-${testInfo.project.name}.png`;
}

function sqlEscape(value: string): string {
  return value.replace(/'/g, "''");
}

function sqlite(sql: string): string {
  if (!dbPath) {
    throw new Error('E2E_DB_PATH or ERSATZRS_DB_PATH is required for block-preview seeding');
  }
  return execFileSync('sqlite3', [dbPath], {
    input: `.timeout 10000\n${sql}`,
    encoding: 'utf8',
  }).trim();
}

function seedPreview(testInfo: TestInfo): PreviewSeed {
  const base = projectBase(testInfo);
  const blockGroupId = base + 1;
  const blockId = base + 2;
  const collectionId = base + 3;
  const mediaSourceId = base + 4;
  const libraryId = base + 5;
  const libraryPathId = base + 6;
  const mediaItemId = base + 7;
  const mediaVersionId = base + 8;
  const suffix = testInfo.project.name;
  const seed = {
    blockId,
    blockName: `E2E Draft Preview ${suffix}`,
    collectionName: `E2E Preview Collection ${suffix}`,
    mediaTitle: `E2E Real Scheduled Movie ${suffix}`,
  };

  sqlite(`
DELETE FROM "BlockItem" WHERE "BlockId" = ${blockId};
DELETE FROM "Block" WHERE "Id" = ${blockId};
DELETE FROM "BlockGroup" WHERE "Id" = ${blockGroupId};
DELETE FROM "CollectionItem" WHERE "CollectionId" = ${collectionId};
DELETE FROM "Collection" WHERE "Id" = ${collectionId};
DELETE FROM "MediaVersion" WHERE "Id" = ${mediaVersionId};
DELETE FROM "MovieMetadata" WHERE "MovieId" = ${mediaItemId};
DELETE FROM "Movie" WHERE "Id" = ${mediaItemId};
DELETE FROM "MediaItem" WHERE "Id" = ${mediaItemId};
DELETE FROM "LibraryPath" WHERE "Id" = ${libraryPathId};
DELETE FROM "LocalLibrary" WHERE "Id" = ${libraryId};
DELETE FROM "Library" WHERE "Id" = ${libraryId};
DELETE FROM "LocalMediaSource" WHERE "Id" = ${mediaSourceId};
DELETE FROM "MediaSource" WHERE "Id" = ${mediaSourceId};

INSERT INTO "BlockGroup" ("Id", "Name")
  VALUES (${blockGroupId}, 'E2E Preview Group ${suffix}');
INSERT INTO "Block" (
  "Id", "BlockGroupId", "DateUpdated", "Minutes", "Name", "StopScheduling"
) VALUES (
  ${blockId}, ${blockGroupId}, '2026-07-24T00:00:00Z', 30,
  '${sqlEscape(seed.blockName)}', 0
);

INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder")
  VALUES (${collectionId}, '${sqlEscape(seed.collectionName)}', 1);
INSERT INTO "MediaSource" ("Id") VALUES (${mediaSourceId});
INSERT INTO "LocalMediaSource" ("Id") VALUES (${mediaSourceId});
INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
  VALUES (${libraryId}, 'E2E Preview Movies ${suffix}', 1, ${mediaSourceId});
INSERT INTO "LocalLibrary" ("Id") VALUES (${libraryId});
INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId")
  VALUES (${libraryPathId}, '/e2e/block-preview/${suffix}', ${libraryId});
INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State")
  VALUES (${mediaItemId}, ${libraryPathId}, 0);
INSERT INTO "Movie" ("Id") VALUES (${mediaItemId});
INSERT INTO "MovieMetadata" (
  "MovieId", "MetadataKind", "Title", "OriginalTitle", "SortTitle",
  "DateAdded", "DateUpdated", "Year"
) VALUES (
  ${mediaItemId}, 0, '${sqlEscape(seed.mediaTitle)}', '${sqlEscape(seed.mediaTitle)}',
  '${sqlEscape(seed.mediaTitle.toLowerCase())}',
  '2026-07-24T00:00:00Z', '2026-07-24T00:00:00Z', 2026
);
INSERT INTO "MediaVersion" (
  "Id", "DateAdded", "DateUpdated", "DisplayAspectRatio", "Duration",
  "Height", "MovieId", "Name", "RFrameRate", "SampleAspectRatio",
  "VideoScanKind", "Width", "InterlacedRatio"
) VALUES (
  ${mediaVersionId}, '2026-07-24T00:00:00Z', '2026-07-24T00:00:00Z',
  '16:9', '00:05:00', 1080, ${mediaItemId}, 'Main', '24/1', '1:1', 0, 1920, NULL
);
INSERT INTO "CollectionItem" ("CollectionId", "MediaItemId", "CustomIndex")
  VALUES (${collectionId}, ${mediaItemId}, 0);
`);

  return seed;
}

async function forceEnglish(page: Page): Promise<void> {
  await page.request.post('/admin-api/settings/ui/save', {
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
            (window as Window & { __ersatzrs_hydrated?: boolean })
              .__ersatzrs_hydrated === true,
        ),
      { timeout: 15_000 },
    )
    .toBe(true);
}

function watchRuntimeErrors(page: Page): string[] {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.stack ?? error.message));
  page.on('console', (message) => {
    if (message.type() === 'error') {
      errors.push(message.text());
    }
  });
  return errors;
}

function expectNoUnexpectedRuntimeErrors(errors: string[]): void {
  expect(
    errors.filter(
      (error) =>
        !error.includes(
          'Failed to load resource: the server responded with a status of 500',
        ),
    ),
  ).toEqual([]);
}

async function previewSection(page: Page): Promise<Locator> {
  const preview = page.locator('[data-block-preview]');
  await expect(preview).toHaveCount(1);
  return preview;
}

function blockNameInput(page: Page): Locator {
  return page.getByText('Block Name', { exact: true }).locator('..').locator('input');
}

async function assertPreviewGeometry(page: Page): Promise<void> {
  const metrics = await page.locator('main').evaluate((main) => {
    const preview = main.querySelector('[data-block-preview]');
    const itemTable = main.querySelector('table');
    const previewTable = preview?.querySelector('table');
    if (!preview || !itemTable || !previewTable) {
      return null;
    }
    const previewBox = preview.getBoundingClientRect();
    const itemBox = itemTable.getBoundingClientRect();
    const previewTableBox = previewTable.getBoundingClientRect();
    return {
      itemBottom: itemBox.bottom,
      previewTop: previewBox.top,
      previewWidth: previewBox.width,
      previewTableWidth: previewTableBox.width,
      viewportWidth: window.innerWidth,
      documentWidth: document.documentElement.scrollWidth,
    };
  });

  expect(metrics).not.toBeNull();
  expect(metrics?.previewTop).toBeGreaterThan(metrics?.itemBottom ?? 0);
  expect(metrics?.previewWidth).toBeGreaterThan(300);
  expect(metrics?.previewTableWidth).toBeGreaterThanOrEqual(
    (metrics?.previewWidth ?? 0) - 40,
  );
  expect(metrics?.documentWidth).toBeLessThanOrEqual((metrics?.viewportWidth ?? 0) + 1);
}

async function exerciseDraftPreview(
  page: Page,
  seed: PreviewSeed,
  testInfo: TestInfo,
  viewportName: 'desktop' | 'mobile',
): Promise<void> {
  const runtimeErrors = watchRuntimeErrors(page);
  await forceEnglish(page);
  await page.goto(`/blocks/${seed.blockId}`);
  await waitForHydration(page);

  const preview = await previewSection(page);
  await expect(preview).toHaveAttribute('data-state', 'idle');
  await expect(preview).toBeHidden();

  await page.getByRole('button', { name: /preview block playout/i }).click();
  await expect(preview).toHaveAttribute('data-state', 'empty');
  await expect(page.locator('[data-preview-empty]')).toBeVisible();

  await page.getByRole('button', { name: /add block item/i }).click();
  await page.getByRole('button', { name: /preview block playout/i }).click();
  await expect(preview).toHaveAttribute('data-state', 'error');
  await expect(page.locator('[data-preview-error]')).toContainText(
    'Could not generate the block preview.',
  );
  expectNoUnexpectedRuntimeErrors(runtimeErrors);
  runtimeErrors.length = 0;

  await page.locator('main table').first().locator('tbody tr').first().click();
  const collectionInput = page.getByPlaceholder('Type to search...');
  await collectionInput.fill(seed.collectionName);
  const suggestion = page.locator('li').filter({ hasText: seed.collectionName });
  await expect(suggestion).toBeVisible();
  await suggestion.click();
  await expect(collectionInput).toHaveValue(seed.collectionName);

  const unsavedName = `${seed.blockName} unsaved`;
  await blockNameInput(page).fill(unsavedName);
  await page.route('**/admin-api/*', async (route) => {
    if (route.request().method() === 'POST') {
      await new Promise((resolve) => setTimeout(resolve, 400));
    }
    await route.continue();
  });
  await page.getByRole('button', { name: /preview block playout/i }).click();
  await expect(preview).toHaveAttribute('data-state', 'loading');
  await expect(page.locator('[data-preview-loading]')).toBeVisible();
  await expect(preview).toHaveAttribute('data-state', 'ready', { timeout: 15_000 });
  await expect(page.locator('[data-preview-ready]')).toBeVisible();
  await expect(page.locator('[data-preview-ready] tbody tr')).toHaveCount(1);
  await expect(page.locator('[data-preview-ready] tbody tr').first()).toContainText(
    seed.mediaTitle,
  );
  await expect(page.locator('[data-preview-ready] tbody tr').first()).toContainText(
    '00:05:00',
  );
  await assertPreviewGeometry(page);
  await page.screenshot({
    path: screenshotPath(testInfo, `block-preview-${viewportName}`),
    fullPage: true,
  });

  expect(runtimeErrors).toEqual([]);
}

test.describe('real block preview parity', () => {
  test.skip(!dbPath, 'E2E_DB_PATH or ERSATZRS_DB_PATH is required to seed block previews');

  test('previews unsaved draft state through the real scheduler in responsive layouts', async ({
    browser,
    baseURL,
    page,
  }, testInfo) => {
    test.setTimeout(90_000);
    const seed = seedPreview(testInfo);

    await exerciseDraftPreview(page, seed, testInfo, 'desktop');
    await page.reload();
    await waitForHydration(page);
    await expect(page.locator('tbody tr')).toHaveCount(0);
    await expect(blockNameInput(page)).toHaveValue(seed.blockName);

    const mobileContext = await browser.newContext({
      baseURL,
      viewport: { width: 390, height: 844 },
    });
    const mobilePage = await mobileContext.newPage();
    await exerciseDraftPreview(mobilePage, seed, testInfo, 'mobile');
    await mobileContext.close();
  });
});
