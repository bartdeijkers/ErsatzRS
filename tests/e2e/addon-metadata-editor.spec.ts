import { MetadataSeed, projectBase, sqlEscape, sqlite, addonContentHash, seedMetadataEditor } from './fixtures/addon-metadata';
import { expect, Page, TestInfo, test } from '@playwright/test';
import { createHash } from 'crypto';
import { execFileSync } from 'child_process';
import { chmodSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'fs';
import { dirname, join, relative } from 'path';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

function revisionLogicalContent(listId: number, revision: number): string {
  return sqlite(`
SELECT COALESCE("revision"."Name", '') || char(30) ||
       COALESCE("revision"."Description", '') || char(30) ||
       COALESCE("revision"."GeneratePlaylist", 0) || char(30) ||
       COALESCE("revision"."ContentKindOverride", 0) || char(30) ||
       COALESCE("revision"."PresentationKind", 0) || char(30) ||
       COALESCE("revision"."RequestedPresentationKind", 0) || char(30) ||
       COALESCE("revision"."MetadataJson", '') || char(30) ||
       COALESCE((
         SELECT group_concat("item", char(29)) FROM (
           SELECT "ProviderId" || char(31) || "GuidsJson" || char(31) || "Rank" ||
                  char(31) || "Kind" || char(31) || "ContentKind" || char(31) ||
                  "Included" || char(31) || "Published" || char(31) || "DisplayTitle" ||
                  char(31) || "MetadataDigest" AS "item"
           FROM "AddonMediaListRevisionItem"
           WHERE "AddonMediaListRevisionId" = "revision"."Id"
           ORDER BY "ProviderId"
         )
       ), '') || char(30) ||
       COALESCE((
         SELECT group_concat("file", char(29)) FROM (
           SELECT "file"."PublishedRelativePath" || char(31) || "blob"."Sha256" AS "file"
           FROM "AddonMediaListRevisionFile" AS "file"
           JOIN "AddonMediaListRevisionFileBlob" AS "blob"
             ON "blob"."Id" = "file"."AddonMediaListRevisionFileBlobId"
           WHERE "file"."AddonMediaListRevisionId" = "revision"."Id"
           ORDER BY "file"."PublishedRelativePath"
         )
       ), '')
FROM "AddonMediaListRevision" AS "revision"
WHERE "revision"."AddonMediaListId" = ${listId}
  AND "revision"."Revision" = ${revision};
`);
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

async function contrastAgainstPageBackground(page: Page, selector: string): Promise<number> {
  return page.locator(selector).first().evaluate((element) => {
    const parseColor = (value: string): [number, number, number, number] => {
      const channels = value.match(/[\d.]+/g)?.map(Number) ?? [];
      if (channels.length < 3) {
        throw new Error(`Could not parse computed color: ${value}`);
      }
      return [channels[0], channels[1], channels[2], channels[3] ?? 1];
    };
    const probe = document.createElement('span');
    probe.style.color = 'var(--color-bg)';
    element.appendChild(probe);
    const background = parseColor(getComputedStyle(probe).color);
    probe.remove();
    const foreground = parseColor(getComputedStyle(element).color);
    const composited = foreground.slice(0, 3).map(
      (channel, index) => channel * foreground[3] + background[index] * (1 - foreground[3]),
    );
    const luminance = (channels: number[]): number => channels
      .map((channel) => channel / 255)
      .map((channel) => (channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4))
      .reduce((total, channel, index) => total + channel * [0.2126, 0.7152, 0.0722][index], 0);
    const foregroundLuminance = luminance(composited);
    const backgroundLuminance = luminance(background.slice(0, 3));
    return (
      (Math.max(foregroundLuminance, backgroundLuminance) + 0.05)
      / (Math.min(foregroundLuminance, backgroundLuminance) + 0.05)
    );
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

async function workspaceSubmit(page: Page, intent: string, hydrated = true): Promise<void> {
  if(hydrated) await waitUntilHydrated(page);
  const selector = `button[name="intent"][value="${intent}"]${intent === 'navigate' ? ':not([formaction])' : ''}`;
  await Promise.all([page.waitForEvent('framenavigated',f=>f===page.mainFrame()),page.locator(selector).first().click()]);
  await page.waitForLoadState('load');
  if(hydrated) await waitUntilHydrated(page);
}

function seedPublicationRecovery(
  seed: MetadataSeed,
  state: 'expired' | 'failed',
): { jobId: number; operationId: number; secretMarker: string } {
  const operationId = seed.listId * 10 + 7;
  const jobId = operationId + 1;
  const secretMarker = 'session_cookie=must-not-reach-operator-output';
  const operationState = state === 'expired' ? 1 : 4;
  const phase = state === 'expired' ? 3 : 4;
  sqlite(`
UPDATE "AddonMediaList"
SET "ReviewState" = 3, "PublicationJournalJson" = '{}',
    "PublicationStartedAt" = '2026-09-03T12:00:00Z'
WHERE "Id" = ${seed.listId};
INSERT INTO "ManagedLibraryOperation" (
  "Id", "LibraryId", "OwnerToken", "Kind", "State", "AttemptCount",
  "LeaseOwner", "LeaseExpiresAt", "DiagnosticCode", "Diagnostic", "CreatedAt", "UpdatedAt"
) VALUES (
  ${operationId},
  (SELECT "LibraryId" FROM "AddonMediaListLibrary" WHERE "AddonMediaListId" = ${seed.listId}),
  'e2e-recovery-owner', 1, ${operationState}, 2,
  'expired-worker', '2026-09-03T12:01:00Z', 'sharing_violation',
  '${secretMarker}', '2026-09-03T12:00:00Z', '2026-09-03T12:02:00Z'
);
INSERT INTO "AddonMediaListPublicationJob" (
  "Id", "OperationId", "AddonMediaListId", "Generation", "SavedRevision",
  "TargetPublishedRevision", "Phase", "OriginalFailureCode",
  "OriginalFailureDiagnostic", "RecoveryDiagnostic", "CreatedAt", "UpdatedAt"
) VALUES (
  ${jobId}, ${operationId}, ${seed.listId}, 7,
  (SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id" = ${seed.listId}),
  (SELECT "PublishedRevision" + 1 FROM "AddonMediaList" WHERE "Id" = ${seed.listId}),
  ${phase}, 'sharing_violation', '${secretMarker}', '${secretMarker}',
  '2026-09-03T12:00:00Z', '2026-09-03T12:02:00Z'
);
`);
  return { jobId, operationId, secretMarker };
}

function seedPaginationItems(seed: MetadataSeed, testInfo: TestInfo): void {
  const suffix = testInfo.project.name.toLowerCase().replace(/[^a-z0-9]+/g, '-');
  const values = Array.from({ length: 26 }, (_, offset) => {
    const rank = offset + 2;
    const itemId = seed.secondItemId + 100 + rank;
    const title = `E2E Metadata Extra ${rank} ${testInfo.project.name}`;
    const releaseDate = `2026-07-${String((rank % 28) + 1).padStart(2, '0')}`;
    const metadata = JSON.stringify({ title, release_date: releaseDate });
    return `
  (${itemId}, ${seed.listId}, 'item:extra-${suffix}-${rank}', '${sqlEscape(title)}', 4, ${rank},
   '${sqlEscape(title)}', 'https://example.invalid/e2e/${suffix}/extra-${rank}', 600, 0, 1, 1, 0,
   '${sqlEscape(metadata)}', '{}', 1, 1)`;
  }).join(',');

  sqlite(`
UPDATE "AddonMediaList" SET "ItemCount" = 28 WHERE "Id" = ${seed.listId};
INSERT INTO "AddonMediaListItem" (
  "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind", "Rank", "Title",
  "SourceUrl", "DurationSeconds", "Availability", "ContentKind", "Included", "Published",
  "ProviderMetadataJson", "MetadataOverrideJson", "ProviderLiveness", "EffectiveLiveness"
) VALUES${values};
`);
}

function seedHighCardinalityNumberingBlockers(seed: MetadataSeed, testInfo: TestInfo): void {
  const suffix = testInfo.project.name.toLowerCase().replace(/[^a-z0-9]+/g, '-');
  const values = Array.from({ length: 48 }, (_, rank) => {
    const itemId = seed.secondItemId + 1_000 + rank;
    const title = `Glamourland episode ${rank + 1}`;
    const metadata = JSON.stringify({ title, release_date: `1986-09-${String((rank % 28) + 1).padStart(2, '0')}` });
    return `
  (${itemId}, ${seed.listId}, 'item:numbering-${suffix}-${rank}', '${sqlEscape(title)}', 4, ${rank},
   '${sqlEscape(title)}', 'https://example.invalid/e2e/${suffix}/numbering-${rank}', 1800, 0, 1, 1, 0,
   '${sqlEscape(metadata)}', '{}', 1, 1)`;
  }).join(',');

  sqlite(`
DELETE FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ${seed.listId};
UPDATE "AddonMediaList"
SET "Name" = 'Glamourland', "ItemCount" = 48, "ReviewState" = 1,
    "PresentationKind" = 2, "RequestedPresentationKind" = 2,
    "ProviderMetadataJson" = '{"title":"Glamourland","plot":"Provider collection summary"}',
    "MetadataOverrideJson" = '{}', "MetadataConflictJson" = NULL
WHERE "Id" = ${seed.listId};
INSERT INTO "AddonMediaListItem" (
  "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind", "Rank", "Title",
  "SourceUrl", "DurationSeconds", "Availability", "ContentKind", "Included", "Published",
  "ProviderMetadataJson", "MetadataOverrideJson", "ProviderLiveness", "EffectiveLiveness"
) VALUES${values};
`);
}

function seedDetailProgress(seed: MetadataSeed, testInfo: TestInfo): void {
  const suffix = testInfo.project.name.toLowerCase().replace(/[^a-z0-9]+/g, '-');
  const providerHost = `e2e-${suffix}.invalid`;
  sqlite(`
UPDATE "AddonMediaList"
SET "ReviewState" = 2, "PublishedRevision" = 1, "DetailImportPolicy" = 1,
    "DetailImportAuthorizedAt" = '2026-08-21T00:02:00Z'
WHERE "Id" = ${seed.listId};

INSERT INTO "AddonMediaListImportJob" (
  "AddonMediaListId", "Generation", "State", "RecordCapability", "DiscoveredCount",
  "TotalHint", "OverviewComplete", "LastProgressAt", "CreatedAt", "UpdatedAt", "CompletedAt"
) VALUES (
  ${seed.listId}, 1, 3, 'media-list.list.v4', 2, 2, 1,
  '2026-08-21T00:02:00Z', '2026-08-21T00:00:00Z',
  '2026-08-21T00:02:00Z', '2026-08-21T00:02:00Z'
);

INSERT INTO "AddonMediaListDetailJob" (
  "AddonMediaListItemId", "AddonMediaListId", "ProviderHost", "State", "ManualPriority",
  "OverviewFingerprint", "OverviewRecordJson", "LeaseOwner", "LeaseExpiresAt",
  "StartedAt", "UpdatedAt"
) VALUES
  (${seed.firstItemId}, ${seed.listId}, '${providerHost}', 3, 0,
   'first-overview', '{}', 'e2e-browser-fixture', '2099-01-01T00:00:00Z',
   '2026-08-21T00:02:00Z', '2026-08-21T00:02:00Z'),
  (${seed.secondItemId}, ${seed.listId}, '${providerHost}', 0, 0,
   'second-overview', '{}', NULL, NULL, NULL, '2026-08-21T00:02:00Z');

DELETE FROM "AddonProviderHostCooldown" WHERE "ProviderHost" = '${providerHost}';
INSERT INTO "AddonProviderHostCooldown" (
  "ProviderHost", "NextEligibleAt", "SampledDelaySeconds", "LastAttemptAt",
  "LeaseOwner", "LeaseExpiresAt", "UpdatedAt"
) VALUES (
  '${providerHost}', '2099-01-01T00:00:00Z', 16, '2026-08-21T00:02:00Z',
  'e2e-browser-fixture', '2099-01-01T00:00:00Z', '2026-08-21T00:02:00Z'
);
`);
}

test.describe('shared add-on metadata editor', () => {
  test.skip(!dbPath, 'E2E_DB_PATH or ERSATZRS_DB_PATH is required');

  for (const javaScriptEnabled of [true, false]) {
    test(`preserves and renews the visible new-video cutoff (${javaScriptEnabled ? 'hydrated' : 'no-JS'})`, async ({ browser, baseURL }, testInfo) => {
      const seed = seedMetadataEditor(testInfo);
      const previousCutoff = '2026-01-01T12:30:00Z';
      sqlite(`
UPDATE "AddonInstallation"
SET "ManifestJson" = json_insert(
    "ManifestJson",
    '$.capabilities[#]', json('{"id":"media-list.prepare.v2"}'),
    '$.capabilities[#]', json('{"id":"media-list.item-runtime.v1"}')
)
WHERE "AddonId" = '${sqlEscape(seed.addonId)}';
UPDATE "AddonMediaList"
SET "DeliveryMode" = 2, "DeliveryCutoffAt" = '${previousCutoff}', "DeliveryCutoffIdentityId" = 0
WHERE "Id" = ${seed.listId};
`);
      const publishedRevision = sqlite(`SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id" = ${seed.listId};`);
      const context = await browser.newContext({ javaScriptEnabled, baseURL });
      const page = await context.newPage();
      const runtimeErrors = watchRuntimeErrors(page);
      const savedDelivery = () => sqlite(`SELECT "DeliveryMode" || '|' || COALESCE("DeliveryCutoffAt", '') FROM "AddonMediaList" WHERE "Id" = ${seed.listId};`);
      const openEditor = async () => {
        if (javaScriptEnabled) {
          await gotoHydrated(page, `/media/addon-lists/${seed.listId}/playback`);
        } else {
          await page.goto(`/media/addon-lists/${seed.listId}/playback`);
        }
      };
      const save = () => workspaceSubmit(page,'save',javaScriptEnabled);
      try {
        await forceEnglish(page);
        await openEditor();
        const delivery = page.locator('[name="delivery_mode"]:checked');
        await expect(delivery).toHaveValue('download_ahead_new_videos');
        await expect(page.locator('#addon-delivery-help')).toContainText('Live items and items whose duration is unknown continue streaming.');
        await expect(page.getByTestId('addon-delivery-cutoff')).toContainText('2026-01-01 12:30:00 UTC');
        expect(Date.parse((await page.getByTestId('addon-delivery-cutoff').locator('time').getAttribute('datetime'))!)).toBe(Date.parse(previousCutoff));

        // Saving another setting must preserve the new-only selection and its cutoff.
        await page.locator('button[formaction$="/settings"]').click();
        if(javaScriptEnabled)await waitUntilHydrated(page);
        await page.getByRole('combobox', { name: 'Automatic Refresh', exact: true }).selectOption('weekly');
        await save();
        await expect.poll(() => sqlite(`SELECT "RefreshCadence" FROM "AddonMediaList" WHERE "Id" = ${seed.listId};`)).toBe('1');
        expect(savedDelivery()).toBe(`2|${previousCutoff}`);
        await openEditor();

        await page.locator('[name="delivery_mode"][value="stream"]').check();
        await save();
        await expect.poll(savedDelivery).toBe('0|');
        await openEditor();
        await expect(page.getByTestId('addon-delivery-cutoff')).toBeHidden();

        await page.locator('[name="delivery_mode"][value="download_ahead_new_videos"]').check();
        await save();
        await expect.poll(savedDelivery).toMatch(/^2\|.+/);
        // SQLite stores UTC without an offset; format it explicitly for JS,
        // which otherwise interprets this value in the runner's local timezone.
        const freshCutoff = savedDelivery().slice(2).replace(' ', 'T') + 'Z';
        expect(Date.parse(freshCutoff)).toBeGreaterThan(Date.parse(previousCutoff));
        await openEditor();
        await expect(delivery).toHaveValue('download_ahead_new_videos');
        await expect(page.getByTestId('addon-delivery-cutoff')).toBeVisible();
        expect(Date.parse((await page.getByTestId('addon-delivery-cutoff').locator('time').getAttribute('datetime'))!)).toBe(Date.parse(freshCutoff));
        expect(sqlite(`SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id" = ${seed.listId};`)).toBe(publishedRevision);
        expect(runtimeErrors).toEqual([]);
      } finally {
        await context.close();
      }
    });
  }

  for (const javaScriptEnabled of [true,false]) {
    test(`confirms a compatible provider change while retaining task input (${javaScriptEnabled ? 'hydrated' : 'no JS'})`,async({browser,baseURL},info)=>{
      test.setTimeout(90000);
      const seed=seedMetadataEditor(info),alternate=seedMetadataEditor(info,'reset');
      const context=await browser.newContext({baseURL,javaScriptEnabled}),page=await context.newPage();
      try {
        await forceEnglish(page);await page.goto(`/media/addon-lists/${seed.listId}/settings`);
        if(javaScriptEnabled)await waitUntilHydrated(page);
        await page.locator('[name="metadata_title"]').fill('Retained through provider change');
        await page.locator('[name="addon_id"]').selectOption(alternate.addonId);
        await workspaceSubmit(page,'change-addon-preview',javaScriptEnabled);
        await workspaceSubmit(page,'cancel-confirmation',javaScriptEnabled);
        await expect(page.locator('[name="addon_id"]')).toHaveValue(alternate.addonId);
        expect(sqlite(`SELECT "AddonId" FROM "AddonMediaList" WHERE "Id"=${seed.listId}`)).toBe(seed.addonId);
        await workspaceSubmit(page,'change-addon-preview',javaScriptEnabled);
        await page.locator('[name="acknowledged"][type="checkbox"]').check();
        await workspaceSubmit(page,'confirm-operation',javaScriptEnabled);
        await expect(page.locator('#workspace-errors')).toHaveCount(0);
        await expect.poll(()=>sqlite(`SELECT "AddonId" FROM "AddonMediaList" WHERE "Id"=${seed.listId}`),{timeout:60000}).toBe(alternate.addonId);
        await expect(page.locator('[name="metadata_title"]')).toHaveValue('Retained through provider change');
        expect(sqlite(`SELECT json_extract("MetadataOverrideJson",'$.title') FROM "AddonMediaList" WHERE "Id"=${seed.listId}`)).not.toBe('Retained through provider change');
      } finally {await context.close();}
    });
  }

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
      page.getByRole('combobox', { name: 'Automatic Refresh', exact: true }),
    ).toHaveValue('daily');
    await expect(
      page.getByRole('checkbox', { name: 'Create playlist in original order' }),
    ).not.toBeChecked();
    const loadDetails = page.getByRole('checkbox', {
      name: 'Load item details automatically',
    });
    await expect(loadDetails).toBeChecked();
    await expect(page.getByText(/playback-required technical details.*before the first publication/i)).toBeVisible();
    await loadDetails.uncheck();
    await expect(
      page.getByText(/request descriptive details manually.*before publication/i),
    ).toBeVisible();
    await expect(page.getByRole('button', { name: 'Import overview', exact: true })).toBeEnabled();
    expect(runtimeErrors).toEqual([]);
  });

  test('keeps source tests non-mutating and marks changed inputs stale', async ({ page }, testInfo) => {
    test.setTimeout(90_000);
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedMetadataEditor(testInfo);
    const before = sqlite(`
SELECT "DraftRevision" || ',' || "PublishedRevision" || ',' ||
       (SELECT COUNT(*) FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ${seed.listId})
FROM "AddonMediaList" WHERE "Id" = ${seed.listId};
`);
    await forceEnglish(page);
    await gotoHydrated(page, `/media/addon-lists/${seed.listId}/settings`);

    await workspaceSubmit(page,'test-source');
    await expect.poll(()=>sqlite(`SELECT "State" FROM "AddonMediaListSourceProbeJob" WHERE "AddonMediaListId"=${seed.listId} ORDER BY "Id" DESC LIMIT 1`),{timeout:60000}).toBe('2');
    await workspaceSubmit(page,'navigate');
    const sourcePanel=page.locator('.settings-section[role="status"]');
    await expect(sourcePanel.getByText('Source is available')).toBeVisible();
    await expect(sourcePanel.getByText('Provider identity', { exact: true })).toBeVisible();
    await expect(sourcePanel.getByText('Title', { exact: true })).toBeVisible();
    await expect(
      sourcePanel.getByText('Items returned on the first page', { exact: true }),
    ).toBeVisible();
    await expect(
      sourcePanel.getByText('Provider total item count', { exact: true }),
    ).toBeVisible();
    await expect(sourcePanel.getByText('First page complete', { exact: true })).toBeVisible();
    await expect(sourcePanel.getByText('Yes', { exact: true })).toBeVisible();
    expect(sqlite(`
SELECT "DraftRevision" || ',' || "PublishedRevision" || ',' ||
       (SELECT COUNT(*) FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ${seed.listId})
FROM "AddonMediaList" WHERE "Id" = ${seed.listId};
`)).toBe(before);

    sqlite(`UPDATE "AddonMediaList" SET "DraftRevision" = "DraftRevision" + 1 WHERE "Id" = ${seed.listId};`);
    await gotoHydrated(page, `/media/addon-lists/${seed.listId}/settings`);
    await expect(page.getByText(/saved inputs no longer match the current list/i)).toBeVisible();
    expect(runtimeErrors).toEqual([]);
  });

  test('sorts the overview by honest Name and displayed Id headers', async ({ page }, testInfo) => {
    const runtimeErrors = watchRuntimeErrors(page);
    seedMetadataEditor(testInfo);
    await forceEnglish(page);
    await gotoHydrated(page, '/media/addon-lists');

    const nameHeader = page.getByRole('columnheader').filter({
      has: page.getByRole('button', { name: 'Sort by Name' }),
    });
    const idHeader = page.getByRole('columnheader').filter({
      has: page.getByRole('button', { name: 'Sort by Id' }),
    });
    await expect(nameHeader).toHaveAttribute('aria-sort', 'ascending');
    await expect(idHeader).toHaveAttribute('aria-sort', 'none');
    await idHeader.getByRole('button').click();
    await expect(idHeader).toHaveAttribute('aria-sort', 'ascending');
    await expect(nameHeader).toHaveAttribute('aria-sort', 'none');
    await idHeader.getByRole('button').click();
    await expect(idHeader).toHaveAttribute('aria-sort', 'descending');
    expect(runtimeErrors).toEqual([]);
  });

  test('materializes the exact committed overview when discovery becomes ready', async ({
    page,
  }, testInfo) => {
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedMetadataEditor(testInfo);
    const firstMetadata = JSON.stringify({
      title: seed.firstTitle,
      plot: 'Primary item provider summary',
      release_date: '2026-08-20',
    });
    const secondMetadata = JSON.stringify({
      title: seed.secondTitle,
      plot: 'Review item provider summary',
      release_date: '2026-08-21',
    });
    sqlite(`
DELETE FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ${seed.listId};
UPDATE "AddonMediaList" SET "ItemCount" = 0 WHERE "Id" = ${seed.listId};
INSERT INTO "AddonMediaListImportJob" (
  "AddonMediaListId", "Generation", "State", "RecordCapability", "DiscoveredCount",
  "OverviewComplete", "NextEligibleAt", "CreatedAt", "UpdatedAt", "LastProgressAt"
) VALUES (
  ${seed.listId}, 1, 1, 'media-list.list.v4', 0, 0, '2099-01-01T00:00:00Z',
  '2026-08-26T10:00:00Z', '2026-08-26T10:00:00Z', '2026-08-26T10:00:00Z'
);
`);
    await forceEnglish(page);
    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);
    await page.locator('.quiet-details > summary').click();
    await expect(page.locator('#addon-list-import-progress')).toHaveAttribute('data-import-state','Discovering');
    const collection = page.locator('.episode-editor');
    await expect(collection.locator('.episode-row')).toHaveCount(0);

    sqlite(`
INSERT INTO "AddonMediaListItem" (
  "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind", "Rank", "Title",
  "SourceUrl", "DurationSeconds", "Availability", "ContentKind", "Included", "Published",
  "ProviderMetadataJson", "MetadataOverrideJson", "ProviderLiveness", "EffectiveLiveness"
) VALUES
  (${seed.firstItemId}, ${seed.listId}, 'item:primary-ready', '${sqlEscape(seed.firstTitle)}', 4, 0,
   '${sqlEscape(seed.firstTitle)}', 'https://example.invalid/ready/primary', 1200, 0, 0, 1, 0,
   '${sqlEscape(firstMetadata)}', '{}', 1, 1),
  (${seed.secondItemId}, ${seed.listId}, 'item:review-ready', '${sqlEscape(seed.secondTitle)}', 4, 1,
   '${sqlEscape(seed.secondTitle)}', 'https://example.invalid/ready/review', 600, 2, 0, 1, 0,
   '${sqlEscape(secondMetadata)}', '{}', 1, 1);
UPDATE "AddonMediaList"
SET "ItemCount" = 2,
    "ProviderMetadataJson" = '{"title":"Glamourland","plot":"Imported collection summary"}',
    "MetadataOverrideJson" = '{}', "MetadataConflictJson" = NULL
WHERE "Id" = ${seed.listId};
UPDATE "AddonMediaListImportJob"
SET "State" = 3, "DiscoveredCount" = 2, "TotalHint" = 2, "OverviewComplete" = 1,
    "UpdatedAt" = '2026-08-26T10:00:04Z', "LastProgressAt" = '2026-08-26T10:00:04Z',
    "CompletedAt" = '2026-08-26T10:00:04Z'
WHERE "AddonMediaListId" = ${seed.listId};
`);

    await workspaceSubmit(page,'navigate');
    await expect(collection.locator('.episode-row')).toHaveCount(2);
    await expect(collection.getByRole('button',{name:seed.firstTitle,exact:true})).toBeVisible();
    await expect(collection.getByRole('button',{name:seed.secondTitle,exact:true})).toBeVisible();
    await expect(page.getByRole('heading',{level:1})).toHaveText('Glamourland');
    const ids = await collection
      .locator('.episode-row')
      .evaluateAll((items) => items.map((item) => item.id.replace('item-','')));
    expect(ids).toEqual([String(seed.firstItemId), String(seed.secondItemId)]);
    expect(runtimeErrors).toEqual([]);
  });

  test('saves the automatic publication policy and shows held refreshes', async ({ page }, testInfo) => {
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedMetadataEditor(testInfo);
    await forceEnglish(page);
    await gotoHydrated(page, `/media/addon-lists/${seed.listId}/settings`);
    const policy = page.getByRole('combobox', { name: 'Refresh publication', exact: true });
    await expect(policy).toHaveValue('review');
    await policy.selectOption('auto_publish_additions');
    await expect(page.getByText('Applies only to scheduled refreshes after your first Publish.', { exact: false })).toBeVisible();
    // Saving reloads this same URL; wait for the new document before another goto.
    const savedNavigation = page.waitForEvent('framenavigated', {
      predicate: (frame) => frame === page.mainFrame(),
    });
    await page.locator('button[name="intent"][value="save"]').click();
    await savedNavigation;
    await page.waitForLoadState('domcontentloaded');
    await waitUntilHydrated(page);
    await expect.poll(() => sqlite(`SELECT "RefreshPublicationPolicy" FROM "AddonMediaList" WHERE "Id" = ${seed.listId};`)).toBe('1');
    sqlite(`INSERT INTO "AddonMediaListAutoPublication" (
      "AddonMediaListId", "DraftRevision", "PublishedRevision", "State", "Reason", "RecordedAt"
    ) SELECT "Id", "DraftRevision", "PublishedRevision", 'held', '"existing_changes"', '2026-09-04T12:00:00Z'
      FROM "AddonMediaList" WHERE "Id" = ${seed.listId};`);
    try {
      await gotoHydrated(page, `/media/addon-lists/${seed.listId}/settings`);
      await expect(policy).toHaveValue('auto_publish_additions');
      await gotoHydrated(page, `/media/addon-lists/${seed.listId}/activity`);
      await expect(page.getByTestId('automatic-publication-hold')).toContainText('Automatic publication needs review');
      expect(runtimeErrors).toEqual([]);
    } finally {
      sqlite(`DELETE FROM "AddonMediaListAutoPublication" WHERE "AddonMediaListId" = ${seed.listId};`);
    }
  });

  test('requires the persisted list name before reset and reload', async ({page}, testInfo) => {
    const errors=watchRuntimeErrors(page),seed=seedMetadataEditor(testInfo,'reset');
    await forceEnglish(page);
    await gotoHydrated(page,`/media/addon-lists/${seed.listId}/activity`);
    await expect(page.getByRole('heading',{name:'Revision history'})).toBeVisible();
    await expect(page.getByText('No published revisions are retained yet.')).toBeVisible();
    await expect(page.locator('button[value="compare"]')).toHaveCount(0);
    await gotoHydrated(page,`/media/addon-lists/${seed.listId}/settings`);
    await page.locator('[name="metadata_title"]').fill('Unsaved retained title');
    await workspaceSubmit(page,'reset-preview');
    const confirmation=page.getByRole('region',{name:'Reset and reload'});
    await expect(confirmation).toBeFocused();
    await expect(confirmation).toContainText('Operator draft');
    await expect(confirmation).toContainText('Published items');
    await expect(confirmation).toContainText('Active or future playouts');
    await expect(confirmation).not.toContainText('/e2e/');
    await workspaceSubmit(page,'cancel-confirmation');
    await expect(confirmation).toHaveCount(0);
    await expect(page.locator('[name="metadata_title"]')).toHaveValue('Unsaved retained title');
    await workspaceSubmit(page,'reset-preview');
    await page.locator('[name="confirmed_name"]').fill('operator draft');
    await page.locator('[name="acknowledged"][type="checkbox"]').check();
    await workspaceSubmit(page,'confirm-operation');
    await expect(page.locator('#workspace-errors')).toBeVisible();
    expect(sqlite(`SELECT COUNT(*) FROM "AddonMediaListLifecycleJob" WHERE "AddonMediaListId"=${seed.listId}`)).toBe('0');
    await page.setViewportSize({width:320,height:800});
    expect(await page.evaluate(()=>document.documentElement.scrollWidth<=document.documentElement.clientWidth+1)).toBe(true);
    await page.setViewportSize({width:640,height:900});
    await page.evaluate(()=>{document.body.style.zoom='2';});
    expect(await page.evaluate(()=>document.documentElement.scrollWidth<=document.documentElement.clientWidth+1)).toBe(true);
    await page.evaluate(()=>{document.body.style.zoom='';});
    await page.locator('[name="confirmed_name"]').fill('  Operator draft  ');
    await page.locator('[name="acknowledged"][type="checkbox"]').check();
    await workspaceSubmit(page,'confirm-operation');
    await expect(page.locator('#workspace-errors')).toHaveCount(0);
    expect(sqlite(`SELECT COUNT(*) FROM "AddonMediaListLifecycleJob" WHERE "AddonMediaListId"=${seed.listId}`)).toBe('1');
    expect(errors).toEqual([]);
  });

  test('keeps import, save, item review, and explicit publication usable without JavaScript', async ({
    browser,
    baseURL,
  }, testInfo) => {
    test.setTimeout(120_000);
    const seed = seedMetadataEditor(testInfo, 'no-js');
    const invalidCollectionMetadata = JSON.stringify({
      title: '',
      artwork: [{ url: 'http://invalid.example/poster.jpg', role: 'poster' }],
    });
    sqlite(`
UPDATE "AddonMediaList"
SET "ProviderMetadataJson" = '${sqlEscape(invalidCollectionMetadata)}',
    "MetadataOverrideJson" = '{}'
WHERE "Id" = ${seed.listId};
UPDATE "AddonMediaListItem" SET "Included" = 0 WHERE "Id" = ${seed.secondItemId};
`);
    const context = await browser.newContext({ javaScriptEnabled: false, baseURL });
    const noJsPage = await context.newPage();
    try {
      await noJsPage.goto(`/media/addon-lists/add?addon_id=${encodeURIComponent(seed.addonId)}`);
      await expect(noJsPage.getByRole('heading', { name: 'Add add-on list' })).toBeVisible();
      await expect(noJsPage.getByLabel('Add-on', { exact: true })).toHaveValue(seed.addonId);
      await noJsPage.getByLabel('Add-on list URL').fill(seed.noJsSourceUrl);
      await noJsPage.getByRole('combobox', { name: 'Automatic Refresh' }).selectOption('monthly');
      await noJsPage.getByRole('button', { name: 'Import overview' }).click();
      await expect(noJsPage).toHaveURL(/\/media\/addon-lists\/\d+\?import=1$/, {
        timeout: 60_000,
      });
      await expect
        .poll(() => sqlite(`
SELECT COUNT(*) FROM "AddonMediaList"
WHERE "AddonId" = '${sqlEscape(seed.addonId)}'
  AND "ProviderId" = '${sqlEscape(seed.noJsProviderId)}'
  AND "Name" = '${sqlEscape(seed.noJsListName)}';
`), { timeout: 60_000 })
        .toBe('1');
      expect(sqlite(`
SELECT "RefreshCadence" FROM "AddonMediaList"
WHERE "AddonId" = '${sqlEscape(seed.addonId)}'
  AND "ProviderId" = '${sqlEscape(seed.noJsProviderId)}';
`)).toBe('2');

      await noJsPage.goto(`/media/addon-lists/${seed.listId}/settings`);
      await workspaceSubmit(noJsPage,'test-source',false);
      await expect
        .poll(() => sqlite(`
SELECT COUNT(*) FROM "AddonMediaListSourceProbeJob"
WHERE "AddonMediaListId" = ${seed.listId};
`))
        .toBe('1');
      await noJsPage.goto('/media/addon-lists');
      await noJsPage.getByRole('button', { name: 'Sort by Id' }).click();
      await expect(noJsPage).toHaveURL(/sort=id&direction=ascending$/);
      await expect(
        noJsPage.getByRole('columnheader').filter({
          has: noJsPage.getByRole('button', { name: 'Sort by Id' }),
        }),
      ).toHaveAttribute('aria-sort', 'ascending');
      await noJsPage.goto(`/media/addon-lists/${seed.listId}`);
      await expect(noJsPage.getByText(/collection needs a title/i)).toBeVisible();
      await expect(
        noJsPage.getByText(/artwork must use a valid bounded HTTPS URL/i),
      ).toBeVisible();
      await noJsPage.locator('button[formaction$="/settings"]').click();
      await noJsPage.getByLabel('Title', { exact: true }).fill('No-JS collection title');
      await noJsPage.getByRole('combobox', { name: 'Refresh publication', exact: true }).selectOption('auto_publish_additions');
      await workspaceSubmit(noJsPage,'save',false);
      await expect(noJsPage.locator('#workspace-errors')).toHaveCount(0);
      await expect(noJsPage.getByRole('combobox', { name: 'Refresh publication', exact: true })).toHaveValue('auto_publish_additions');
      expect(sqlite(`SELECT "RefreshPublicationPolicy" FROM "AddonMediaList" WHERE "Id" = ${seed.listId};`)).toBe('1');
      await noJsPage.locator(`button[formaction="/media/addon-lists/${seed.listId}/artwork"]`).click();
      await noJsPage.getByLabel('Poster', {exact:true}).fill('https://example.invalid/poster.jpg');
      await workspaceSubmit(noJsPage,'save',false);
      await expect(noJsPage.locator('#workspace-errors')).toHaveCount(0);
      expect(
        sqlite(`
SELECT json_extract("MetadataOverrideJson", '$.title') || ',' ||
       json_extract("MetadataOverrideJson", '$.artwork[0].url')
FROM "AddonMediaList" WHERE "Id" = ${seed.listId};
`),
      ).toBe('No-JS collection title,https://example.invalid/poster.jpg');

      await noJsPage.goto(`/media/addon-lists/${seed.listId}/items/${seed.firstItemId}`);
      await noJsPage.locator('#item-metadata > summary').click();
      await noJsPage.getByLabel('Title', { exact: true }).fill('No-JS reviewed item');
      await noJsPage.getByLabel('Tags', { exact: true }).fill('native\nreviewed');
      await workspaceSubmit(noJsPage,'save',false);
      await expect(noJsPage.locator('#workspace-errors')).toHaveCount(0);
      expect(
        sqlite(`
SELECT json_extract("MetadataOverrideJson", '$.title') || ',' ||
       json_extract("MetadataOverrideJson", '$.tags[1]')
FROM "AddonMediaListItem" WHERE "Id" = ${seed.firstItemId};
`),
      ).toBe('No-JS reviewed item,reviewed');
      expect(
        sqlite(`
SELECT json_extract("MetadataOverrideJson", '$.title')
FROM "AddonMediaList" WHERE "Id" = ${seed.listId};
`),
      ).toBe('No-JS collection title');

      await noJsPage.goto(`/media/addon-lists/${seed.listId}/settings`);
      await noJsPage
        .getByRole('combobox', { name: 'Automatic Refresh', exact: true })
        .selectOption('daily');
      expect(
        await noJsPage
          .locator('[form="addon-list-workspace"]:invalid')
          .evaluateAll((controls) =>
            controls.map((control) => ({
              name: control.getAttribute('name'),
              value: (control as HTMLInputElement).value,
              validationMessage: (control as HTMLInputElement).validationMessage,
            })),
          ),
      ).toEqual([]);
      await workspaceSubmit(noJsPage,'save',false);
      await expect(noJsPage.locator('#workspace-errors')).toHaveCount(0);
      expect(
        sqlite(`SELECT "AutoRefresh" FROM "AddonMediaList" WHERE "Id" = ${seed.listId};`),
      ).toBe('1');

      await noJsPage.locator(`button[formaction="/media/addon-lists/${seed.listId}"]`).click();
      await workspaceSubmit(noJsPage,'publish',false);
      await expect(noJsPage.locator('#workspace-errors')).toHaveCount(0);
      await expect
        .poll(
          () =>
            sqlite(`
SELECT "ReviewState" || ',' || "PublishedRevision" || ',' || (
  SELECT "operation"."State"
  FROM "AddonMediaListPublicationJob" AS "job"
  JOIN "ManagedLibraryOperation" AS "operation" ON "operation"."Id" = "job"."OperationId"
  WHERE "job"."AddonMediaListId" = ${seed.listId}
  ORDER BY "job"."Generation" DESC
  LIMIT 1
)
FROM "AddonMediaList" WHERE "Id" = ${seed.listId};
`),
          { timeout: 30_000 },
        )
        .toBe('2,1,3');

      await noJsPage.goto(`/media/addon-lists/${seed.listId}/items/${seed.firstItemId}`);
      const fullDetail = noJsPage.locator('a[href^="/media/remote/streams/"]');
      await expect(fullDetail).toHaveAttribute('href', `/media/remote/streams/${sqlite(`SELECT "MediaItemId" FROM "AddonMediaListItem" WHERE "Id"=${seed.firstItemId};`)}`);
      await noJsPage.locator('#item-information > summary').click();
      await fullDetail.click();
      await expect(noJsPage.locator('h1')).not.toContainText('Not found');
      await noJsPage.goto(`/media/addon-lists/${seed.listId}/activity`);
      await expect(noJsPage.locator('button[value="compare"]')).toHaveCount(0);
      await noJsPage.goto(`/media/addon-lists/${seed.listId}`);

      const revisionOneContent = revisionLogicalContent(seed.listId, 1);
      expect(revisionOneContent).not.toBe('');
      sqlite(`
UPDATE "AddonMediaListItem"
SET "MetadataOverrideJson" = json_set(
      "MetadataOverrideJson",
      '$.title',
      'No-JS revision two item'
    )
WHERE "Id" = ${seed.firstItemId};
UPDATE "AddonMediaList"
SET "ReviewState" = 1, "DraftRevision" = "DraftRevision" + 1
WHERE "Id" = ${seed.listId};
`);

      await noJsPage.goto(`/media/addon-lists/${seed.listId}`);
      await noJsPage.locator(`button[formaction="/media/addon-lists/${seed.listId}"]`).click();
      await workspaceSubmit(noJsPage,'publish',false);
      await expect(noJsPage.locator('#workspace-errors')).toHaveCount(0);
      await expect
        .poll(
          () =>
            sqlite(`
SELECT "PublishedRevision" || ',' || (
  SELECT "operation"."State"
  FROM "AddonMediaListPublicationJob" AS "job"
  JOIN "ManagedLibraryOperation" AS "operation" ON "operation"."Id" = "job"."OperationId"
  WHERE "job"."AddonMediaListId" = ${seed.listId}
  ORDER BY "job"."Generation" DESC
  LIMIT 1
)
FROM "AddonMediaList" WHERE "Id" = ${seed.listId};
`),
          { timeout: 30_000 },
        )
        .toBe('2,3');

      await noJsPage.goto(`/media/addon-lists/${seed.listId}`);
      await noJsPage.goto(`/media/addon-lists/${seed.listId}/activity`);
      await expect(noJsPage.locator('button[value="restore-preview:2"]')).toBeDisabled();
      await expect(noJsPage.locator('button[value="restore-preview:1"]')).toBeEnabled();
      await noJsPage.locator('[name="compare_left"]').selectOption('2');
      await noJsPage.locator('[name="compare_right"]').selectOption('1');
      await workspaceSubmit(noJsPage,'compare',false);
      await expect(noJsPage.getByTestId('revision-diff')).toBeVisible();
      await workspaceSubmit(noJsPage,'restore-preview:1',false);
      await noJsPage.locator('[name="acknowledged"][type="checkbox"]').check();
      await workspaceSubmit(noJsPage,'confirm-operation',false);
      await expect(noJsPage.locator('#workspace-errors')).toHaveCount(0);
      await expect
        .poll(
          () =>
            sqlite(`
SELECT "list"."PublishedRevision" || ',' || "job"."TargetPublishedRevision" || ',' ||
       "job"."SourcePublishedRevision" || ',' || "operation"."State"
FROM "AddonMediaList" AS "list"
JOIN "AddonMediaListPublicationJob" AS "job"
  ON "job"."AddonMediaListId" = "list"."Id"
JOIN "ManagedLibraryOperation" AS "operation"
  ON "operation"."Id" = "job"."OperationId"
WHERE "list"."Id" = ${seed.listId}
ORDER BY "job"."Generation" DESC
LIMIT 1;
`),
          { timeout: 30_000 },
        )
        .toBe('3,3,1,3');
      expect(revisionLogicalContent(seed.listId, 3)).toBe(revisionOneContent);

      await noJsPage.goto(`/media/addon-lists/${seed.listId}`);
      await noJsPage.goto(`/media/addon-lists/${seed.listId}/settings`);
      await workspaceSubmit(noJsPage,'reset-preview',false);
      await noJsPage.locator('[name="confirmed_name"]').fill('Operator draft');
      await noJsPage.locator('[name="acknowledged"][type="checkbox"]').check();
      await workspaceSubmit(noJsPage,'confirm-operation',false);
      await expect(noJsPage.locator('#workspace-errors')).toHaveCount(0);
      await expect
        .poll(() => sqlite(`
SELECT COUNT(*) FROM "AddonMediaListLifecycleJob"
WHERE "AddonMediaListId" = ${seed.listId};
`))
        .toBe('1');
    } finally {
      await context.close();
    }
  });

  test('shows expired publication status and retries the exact generation', async ({
    page,
  }, testInfo) => {
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedMetadataEditor(testInfo);
    const recovery = seedPublicationRecovery(seed, 'expired');
    await forceEnglish(page);
    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);

    const alert = page.getByTestId('addon-publication-recovery');
    await expect(page.locator('button[name="intent"][value="publish"]')).toBeDisabled();
    await expect(alert).toContainText('Committing');
    await expect(alert).toContainText('2');
    await expect(alert).toContainText('sharing_violation');
    await expect(alert).not.toContainText(recovery.secretMarker);
    await expect(alert.locator('button[value="recover-retry"]')).toBeVisible();
    await expect(
      alert.getByRole('button', { name: 'Restore previous revision' }),
    ).toBeVisible();

    await workspaceSubmit(page,'recover-retry');
    await expect
      .poll(() =>
        sqlite(
          `SELECT "Kind" FROM "ManagedLibraryOperation" WHERE "Id" = ${recovery.operationId};`,
        ),
      )
      .toBe('6');
    expect(
      sqlite(`SELECT COUNT(*) || ',' || MAX("Generation")
FROM "AddonMediaListPublicationJob" WHERE "AddonMediaListId" = ${seed.listId};`),
    ).toBe('1,7');
    expect(runtimeErrors).toEqual([]);
  });

  test('restores the exact failed generation without JavaScript', async ({
    browser,
    baseURL,
  }, testInfo) => {
    const seed = seedMetadataEditor(testInfo);
    const recovery = seedPublicationRecovery(seed, 'failed');
    const context = await browser.newContext({ javaScriptEnabled: false, baseURL });
    const noJsPage = await context.newPage();
    try {
      await noJsPage.goto(`/media/addon-lists/${seed.listId}`);
      const alert = noJsPage.getByTestId('addon-publication-recovery');
      await expect(alert).toContainText('Restoring files');
      await expect(alert).not.toContainText(recovery.secretMarker);
      await workspaceSubmit(noJsPage,'recover-restore',false);
      await expect(noJsPage.locator('#workspace-errors')).toHaveCount(0);
      expect(
        sqlite(`SELECT COUNT(*) || ',' || MAX("Generation")
FROM "AddonMediaListPublicationJob" WHERE "AddonMediaListId" = ${seed.listId};`),
      ).toBe('1,7');
      expect(
        sqlite(
          `SELECT "Kind" FROM "ManagedLibraryOperation" WHERE "Id" = ${recovery.operationId};`,
        ),
      ).toBe('6');
    } finally {
      await context.close();
    }
  });

  test('keeps numbering optional and validates paired fields in the locked layout',async({page},info)=>{
    const seed=seedMetadataEditor(info);seedHighCardinalityNumberingBlockers(seed,info);
    await forceEnglish(page);await gotoHydrated(page,`/media/addon-lists/${seed.listId}`);
    const bulk=page.locator('.bulk-tools');await expect(bulk).not.toHaveAttribute('open','');
    const first=page.locator('.episode-row').first();await expect(first).toContainText('Unnumbered');
    const season=first.locator('[name^="season_"]');await season.fill('1');
    await workspaceSubmit(page,'save');await expect(page.locator('#workspace-errors')).toBeFocused();
    await expect(season).toHaveAttribute('aria-invalid','true');
    await expect(first.locator('[name^="episode_"]')).toHaveAttribute('aria-invalid','true');
    await workspaceSubmit(page,'discard');await expect(season).toHaveValue('');
    await bulk.locator('summary').first().focus();await page.keyboard.press('Enter');
    await expect(bulk).toHaveAttribute('open','');
    for(const width of [320,1180,3840]){await page.setViewportSize({width,height:900});expect(await page.evaluate(()=>document.documentElement.scrollWidth<=document.documentElement.clientWidth+1)).toBe(true);}
  });

  test('reflows review cards and keeps the mobile navigation keyboard-safe', async ({
    page,
  }, testInfo) => {
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedMetadataEditor(testInfo);
    const longDescription = Array.from(
      { length: 16 },
      (_, index) => `Localized provider description sentence ${index + 1}.`,
    ).join(' ');
    sqlite(`
UPDATE "AddonMediaList"
SET "ProviderMetadataJson" = json_set("ProviderMetadataJson", '$.plot', '${sqlEscape(longDescription)}')
WHERE "Id" = ${seed.listId};
`);
    await page.setViewportSize({ width: 320, height: 800 });
    await forceEnglish(page);
    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);

    const toggle = page.getByRole('button', { name: 'Toggle navigation' });
    const drawer = page.locator('[data-nav-drawer]');
    await expect(toggle).toHaveAttribute('aria-expanded', 'false');
    await expect(drawer).toHaveAttribute('aria-hidden', 'true');
    await expect(drawer).toHaveAttribute('inert', '');
    await toggle.focus();
    await page.keyboard.press('Enter');
    await expect(toggle).toHaveAttribute('aria-expanded', 'true');
    await expect(drawer).toHaveAttribute('aria-hidden', 'false');
    await expect(drawer).not.toHaveAttribute('inert', '');
    expect(await drawer.evaluate((element) => element.contains(document.activeElement))).toBe(true);
    await page.keyboard.press('Escape');
    await expect(toggle).toBeFocused();
    await expect(drawer).toHaveAttribute('inert', '');

    const collection=page.locator('.episode-editor');
    const first=collection.locator('.episode-row').first();
    for(const label of ['First aired','Duration'])await expect(first.getByText(label,{exact:true})).toBeVisible();
    const review=first.locator('button.row-title');await expect(review).toBeVisible();
    await review.focus();await page.keyboard.press('Enter');await waitUntilHydrated(page);
    await expect(page.locator('.item-adjacent')).toHaveCount(2);
    await expect(page.locator('.item-adjacent').first()).toBeDisabled();
    for(const width of [320,390,900,1440,1920,2560,3840]) {
      await page.setViewportSize({width,height:900});
      expect(await page.evaluate(()=>document.documentElement.scrollWidth<=document.documentElement.clientWidth+1)).toBe(true);
    }
    await page.locator('button[formaction$="/settings"]').click();await waitUntilHydrated(page);
    await expect(page.locator('[name="metadata_plot"]')).toHaveValue(longDescription);
    await expect(page.locator('section[aria-label="Metadata conflicts"]')).toBeVisible();
    expect(await contrastAgainstPageBackground(page,'section[aria-label="Metadata conflicts"]')).toBeGreaterThanOrEqual(4.5);
    await page.setViewportSize({width:640,height:900});await page.evaluate(()=>{document.body.style.zoom='2';});
    expect(await page.evaluate(()=>document.documentElement.scrollWidth<=document.documentElement.clientWidth+1)).toBe(true);
    await page.evaluate(()=>{document.body.style.zoom='';});
    await forceEnglish(page);
    await gotoHydrated(page, `/settings/addons/${encodeURIComponent(seed.addonId)}`);
    await expect(page.getByRole('link', { name: 'Import a list' })).toHaveAttribute(
      'href',
      `/media/addon-lists/add?addon_id=${encodeURIComponent(seed.addonId)}`,
    );
    expect(runtimeErrors).toEqual([]);
  });

  test('shows durable detail progress and accessible per-item actions', async ({
    page,
  }, testInfo) => {
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedMetadataEditor(testInfo);
    seedDetailProgress(seed, testInfo);
    await forceEnglish(page);
    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);

    await page.locator('.quiet-details > summary').click();
    const progress=page.locator('#addon-list-import-progress');
    await expect(progress).toContainText('Loading item details');
    await expect(progress.getByRole('progressbar')).toHaveAttribute('aria-valuemax','2');
    await expect(progress.getByRole('progressbar')).toHaveAttribute('aria-valuenow','0');
    await expect(progress.locator('[aria-live="polite"]')).toContainText('Loading item details');
    await gotoHydrated(page,`/media/addon-lists/${seed.listId}/items/${seed.firstItemId}`);
    await expect(page.locator('button[value="fetch-details"]')).toBeDisabled();
    await expect(page.locator('button[value="fetch-details"]')).toHaveAttribute('aria-busy','true');
    await page.locator('.item-adjacent').last().click();await waitUntilHydrated(page);
    await page.locator('#item-information > summary').click();
    await workspaceSubmit(page,'fetch-details');
    await expect.poll(()=>sqlite(`SELECT "State" || ',' || "ManualPriority" FROM "AddonMediaListDetailJob" WHERE "AddonMediaListItemId"=${seed.secondItemId}`)).toMatch(/^[12],1$/);
    await expect(page.locator('button[value="fetch-details"]')).toHaveAttribute('aria-busy','true');
    expect(runtimeErrors).toEqual([]);
  });

  test('queues a missing finite duration before publication instead of failing save', async ({
    page,
  }, testInfo) => {
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedMetadataEditor(testInfo);
    const suffix = testInfo.project.name.toLowerCase().replace(/[^a-z0-9]+/g, '-');
    const providerHost = `e2e-duration-${suffix}.invalid`;
    sqlite(`
UPDATE "AddonMediaListItem" SET "DurationSeconds" = NULL WHERE "Id" = ${seed.firstItemId};
UPDATE "AddonMediaListItem" SET "Included" = 0 WHERE "Id" = ${seed.secondItemId};
INSERT INTO "AddonMediaListImportJob" (
  "AddonMediaListId", "Generation", "State", "RecordCapability", "DiscoveredCount",
  "TotalHint", "OverviewComplete", "LastProgressAt", "CreatedAt", "UpdatedAt", "CompletedAt"
) VALUES (
  ${seed.listId}, 1, 3, 'media-list.list.v4', 2, 2, 1,
  '2026-08-26T10:00:00Z', '2026-08-26T10:00:00Z',
  '2026-08-26T10:00:00Z', '2026-08-26T10:00:00Z'
);
INSERT INTO "AddonMediaListDetailJob" (
  "AddonMediaListItemId", "AddonMediaListId", "ProviderHost", "State", "ManualPriority",
  "OverviewFingerprint", "OverviewRecordJson", "UpdatedAt"
) VALUES (
  ${seed.firstItemId}, ${seed.listId}, '${providerHost}', 0, 0,
  'duration-overview', '{}', '2026-08-26T10:00:00Z'
);
INSERT INTO "AddonProviderHostCooldown" (
  "ProviderHost", "NextEligibleAt", "SampledDelaySeconds", "LastAttemptAt", "UpdatedAt"
) VALUES (
  '${providerHost}', '2099-01-01T00:00:00Z', 16,
  '2026-08-26T10:00:00Z', '2026-08-26T10:00:00Z'
);
`);
    await forceEnglish(page);
    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);

    await workspaceSubmit(page,'publish');
    await expect(page.locator('#workspace-errors')).toHaveCount(0);
    await expect(page.getByText('Preparing publication', {exact:true})).toBeVisible();
    await expect
      .poll(() =>
        sqlite(`
SELECT "State" || ',' || "ManualPriority"
FROM "AddonMediaListDetailJob" WHERE "AddonMediaListItemId" = ${seed.firstItemId};
`),
      )
      .toMatch(/^[12],1$/);
    expect(
      sqlite(`SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id" = ${seed.listId};`),
    ).toBe('0');
    expect(runtimeErrors).toEqual([]);
  });

  test('previews exact numbering across pages with mobile and reduced-motion cues', async ({
    page,
  }, testInfo) => {
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedMetadataEditor(testInfo);
    sqlite(`
UPDATE "AddonMediaList"
SET "PresentationKind" = 1, "RequestedPresentationKind" = 1
WHERE "Id" = ${seed.listId};
UPDATE "AddonMediaListItem" SET "ContentKind" = 1 WHERE "AddonMediaListId" = ${seed.listId};
UPDATE "AddonMediaListItem"
SET "ProviderMetadataJson" = json_set(
      "ProviderMetadataJson", '$.season', 4, '$.episode', 9
    ), "Season" = 4, "Episode" = 9
WHERE "Id" = ${seed.firstItemId};
`);
    seedPaginationItems(seed, testInfo);
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await forceEnglish(page);
    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);

    const items=page.locator('.episode-editor'),first=items.locator('.episode-row').first();
    await expect(first.locator('[name^="season_"]')).toHaveValue('4');
    await expect(first.locator('[name^="episode_"]')).toHaveValue('9');
    await page.locator('.bulk-tools > summary').click();
    await page.locator('[name="bulk_scope"]').selectOption('all');
    await workspaceSubmit(page,'number');
    await expect(page.getByTestId('bulk-preview').locator('tbody tr')).toHaveCount(27);
    const firstIds=await items.locator('.episode-row').evaluateAll(rows=>rows.map(r=>r.id));
    await workspaceSubmit(page,'page:1');
    const secondIds=await items.locator('.episode-row').evaluateAll(rows=>rows.map(r=>r.id));
    expect(new Set([...firstIds,...secondIds]).size).toBe(firstIds.length+secondIds.length);
    await expect(items.locator('.episode-row').first().locator('[name^="episode_"]')).toHaveValue('10');
    await page.setViewportSize({width:390,height:844});
    await expect(page.locator('button[value="page:0"]')).toBeVisible();
    expect(await items.locator('.episode-row').first().evaluate(e=>getComputedStyle(e).transitionDuration)).toBe('0s');
    await workspaceSubmit(page,'save');await expect(page.locator('#workspace-errors')).toHaveCount(0);
    expect(sqlite(`SELECT COUNT(*) FROM "AddonMediaListItem" WHERE "AddonMediaListId"=${seed.listId} AND json_extract("MetadataOverrideJson",'$.season')=1`)).toBe('27');
    expect(runtimeErrors).toEqual([]);
  });

  test('edits and undoes media kind and playback behavior in explicit bulk scopes', async ({
    page,
  }, testInfo) => {
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedMetadataEditor(testInfo);
    sqlite(`
UPDATE "AddonMediaListItem"
SET "ProviderLiveness" = 2, "EffectiveLiveness" = 2,
    "LivenessOverride" = NULL, "LivenessConflict" = 0
WHERE "Id" = ${seed.firstItemId};
UPDATE "AddonMediaListItem"
SET "ProviderLiveness" = 2, "EffectiveLiveness" = 1,
    "LivenessOverride" = 1, "LivenessConflict" = 1
WHERE "Id" = ${seed.secondItemId};
`);
    await forceEnglish(page);
    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);

    const original=()=>sqlite(`SELECT group_concat("ContentKind" || ',' || COALESCE("LivenessOverride",-1) || ',' || "EffectiveLiveness",';') FROM (SELECT * FROM "AddonMediaListItem" WHERE "AddonMediaListId"=${seed.listId} ORDER BY "Rank")`);
    const before=original();
    await page.locator('.bulk-tools > summary').click();
    await page.locator(`[name="selected_${seed.firstItemId}"][type="checkbox"]`).check();
    await page.locator('[name="bulk_liveness"]').selectOption('finite');
    await workspaceSubmit(page,'classify');
    await expect(page.getByTestId('bulk-preview').locator('tbody tr')).toHaveCount(1);
    expect(original()).toBe(before);
    await workspaceSubmit(page,'undo');expect(original()).toBe(before);
    await page.locator('[name="bulk_scope"]').selectOption('all');
    await page.locator('[name="bulk_kind"]').selectOption('television_episode');
    await page.locator('[name="bulk_liveness"]').selectOption('live');
    await workspaceSubmit(page,'classify');
    await expect(page.getByTestId('bulk-preview').locator('tbody tr')).toHaveCount(2);
    await workspaceSubmit(page,'save');await expect(page.locator('#workspace-errors')).toHaveCount(0);
    expect(original()).toBe('1,2,2;1,2,2');
    await workspaceSubmit(page,'undo-saved-review');await expect(page.locator('#workspace-errors')).toHaveCount(0);
    expect(original()).toBe(before);
    await page.setViewportSize({width:390,height:844});
    await page.locator('.bulk-tools > summary').click();
    expect(await page.evaluate(()=>document.documentElement.scrollWidth<=document.documentElement.clientWidth+1)).toBe(true);
    expect(runtimeErrors).toEqual([]);
  });

  test('keeps row persistence explicit during latency and restores failed edits', async ({
    page,
  }, testInfo) => {
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedMetadataEditor(testInfo);
    sqlite(`
UPDATE "AddonMediaListItem"
SET "Included" = 0
WHERE "Id" = ${seed.secondItemId};
UPDATE "AddonMediaListItem"
SET "ProviderLiveness" = 2, "EffectiveLiveness" = 2,
    "LivenessOverride" = NULL, "LivenessConflict" = 0
WHERE "Id" = ${seed.firstItemId};
`);

    await forceEnglish(page);await gotoHydrated(page,`/media/addon-lists/${seed.listId}/items/${seed.firstItemId}`);
    await page.locator('[name="liveness"]').selectOption('finite');
    expect(sqlite(`SELECT COALESCE("LivenessOverride",-1) FROM "AddonMediaListItem" WHERE "Id"=${seed.firstItemId}`)).toBe('-1');
    await page.route(`**/media/addon-lists/${seed.listId}/items/${seed.firstItemId}`,async route=>{
      if(route.request().method()==='POST')await new Promise(resolve=>setTimeout(resolve,300));
      await route.continue();
    });
    await workspaceSubmit(page,'save');
    expect(sqlite(`SELECT "LivenessOverride" FROM "AddonMediaListItem" WHERE "Id"=${seed.firstItemId}`)).toBe('1');
    const trigger=`e2e_reject_item_${seed.firstItemId}`;
    sqlite(`CREATE TRIGGER "${trigger}" BEFORE UPDATE ON "AddonMediaListItem" WHEN NEW."Id"=${seed.firstItemId} BEGIN SELECT RAISE(ABORT,'injected native save failure'); END;`);
    try {
      await page.locator('[name="liveness"]').selectOption('live');
      await workspaceSubmit(page,'save');
      await expect(page.locator('#workspace-errors')).toBeFocused();
      await expect(page.locator('[name="liveness"]')).toHaveValue('live');
      expect(sqlite(`SELECT "LivenessOverride" FROM "AddonMediaListItem" WHERE "Id"=${seed.firstItemId}`)).toBe('1');
    }finally{sqlite(`DROP TRIGGER "${trigger}"`);}
    expect(runtimeErrors).toEqual([]);
  });

  test('assigns missing episode numbers in original order and allows a bounded overwrite', async ({
    page,
  }, testInfo) => {
    test.setTimeout(60_000);
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
UPDATE "AddonMediaListItem"
SET "ProviderMetadataJson" = json_set(
      "ProviderMetadataJson", '$.release_date', '2027-01-20'
    )
WHERE "Id" = ${seed.secondItemId};
`);
    await forceEnglish(page);
    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);

    const first=page.locator(`#item-${seed.firstItemId}`),second=page.locator(`#item-${seed.secondItemId}`);
    await second.locator('[name^="season_"]').fill('3');await second.locator('[name^="episode_"]').fill('7');
    await workspaceSubmit(page,'save');await expect(page.locator('#workspace-errors')).toHaveCount(0);
    await workspaceSubmit(page,'undo-saved-review');
    await expect(second.locator('[name^="season_"]')).toHaveValue('');
    await page.locator('.bulk-tools > summary').click();
    await page.locator('[name="bulk_scope"]').selectOption('all');
    await workspaceSubmit(page,'number');
    await expect(page.getByTestId('bulk-preview').locator('tbody tr')).toHaveCount(1);
    await workspaceSubmit(page,'save');
    await expect(first.locator('[name^="season_"]')).toHaveValue('4');
    await expect(second.locator('[name^="season_"]')).toHaveValue('1');
    await page.locator('[name="bulk_count"]').fill('1');
    await page.locator('[name="replace_existing"][type="checkbox"]').check();
    await workspaceSubmit(page,'number');
    await expect(page.locator('.numbering-save-bar button[value^="undo"]')).toHaveCount(1);
    await workspaceSubmit(page,'save');
    await expect(first.locator('[name^="season_"]')).toHaveValue('1');
    await expect(first.locator('[name^="episode_"]')).toHaveValue('2');
    await expect(second.locator('[name^="episode_"]')).toHaveValue('1');
    await page.locator('[name="bulk_count"]').fill('');
    await page.locator('[name="gap_days"]').fill('90');
    await page.locator('[name="start_item"]').selectOption(String(seed.secondItemId));
    await page.locator('[name="bulk_season"]').fill('3');
    await workspaceSubmit(page,'auto-number');await expect(page.getByTestId('bulk-preview').locator('tbody tr')).toHaveCount(1);
    await workspaceSubmit(page,'undo');
    await page.locator('[name="start_item"]').selectOption('');
    await page.locator('[name="bulk_season"]').fill('1');
    await workspaceSubmit(page,'auto-number');await workspaceSubmit(page,'save');
    expect(sqlite(`SELECT group_concat(json_extract("MetadataOverrideJson",'$.season') || ',' || json_extract("MetadataOverrideJson",'$.episode'),';') FROM (SELECT * FROM "AddonMediaListItem" WHERE "AddonMediaListId"=${seed.listId} ORDER BY "Rank")`)).toBe('1,1;2,1');
    expect(runtimeErrors).toEqual([]);
  });

  test('reviews, edits, uploads, rejects stale ETags, publishes, and reincludes', async ({
    page,
  }, testInfo) => {
    test.setTimeout(180_000);
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedMetadataEditor(testInfo);
    await forceEnglish(page);
    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);

    await gotoHydrated(page,`/media/addon-lists/${seed.listId}/settings`);
    await expect(page.locator('[name="metadata_title"]')).toHaveValue('Operator draft');
    await expect(page.getByText('Provider original')).toBeVisible();
    await expect(page.getByText('Provider refreshed')).toBeVisible();
    await page.locator(`button[formaction="/media/addon-lists/${seed.listId}"]`).click();await waitUntilHydrated(page);
    await page.locator('[name="category"]').selectOption('review');await workspaceSubmit(page,'filter');
    const itemsTable=page.locator('.episode-editor');
    await expect(itemsTable).toContainText(seed.secondTitle);
    await expect(itemsTable.getByText(seed.firstTitle)).toHaveCount(0);
    const itemConflict = JSON.stringify({
      title: {
        provider_before: 'Review item original',
        provider_after: 'Review item refreshed',
        operator_value: seed.secondTitle,
      },
    });
    sqlite(`
UPDATE "AddonMediaListItem"
SET "MetadataConflictJson" = '${sqlEscape(itemConflict)}'
WHERE "Id" = ${seed.secondItemId};
`);
    await page.locator(`button[formaction$="/items/${seed.secondItemId}"]`).click();await waitUntilHydrated(page);
    const itemEditor=page.locator('.item-review'),itemMetadata=itemEditor;
    const expandMetadata=async()=>{
      const outer=itemEditor.locator('#item-metadata');
      if(await outer.getAttribute('open')===null)await outer.locator('summary').first().click();
      await expect(outer.locator('.metadata-credits')).toBeVisible();
    };
    await expandMetadata();
    for(const label of ['Original title','Year','First aired','Artists','Studios','Directors','Writers','Languages'])await expect(itemMetadata.getByLabel(label,{exact:true})).toBeVisible();
    const castAndCredits=itemMetadata.getByRole('textbox',{name:'Cast and credits',exact:true});
    await expect(itemEditor.getByText('Review item original')).toBeVisible();
    await expect(itemEditor.getByText('Review item refreshed')).toBeVisible();
    const manyTags = Array.from({ length: 120 }, (_, index) => `tag-${index + 1}`).join('\n');
    await itemMetadata.getByLabel('Original title', { exact: true }).fill('Original review title');
    await itemMetadata.getByLabel('Year', { exact: true }).fill('2025');
    await itemMetadata.getByLabel('First aired', { exact: true }).fill('2025-08-21');
    await itemMetadata.getByLabel('Artists', { exact: true }).fill('Artist One, Artist Two');
    await itemMetadata.getByLabel('Studios', { exact: true }).fill('Review Studio');
    await itemMetadata.getByLabel('Directors', { exact: true }).fill('Review Director');
    await itemMetadata.getByLabel('Writers', { exact: true }).fill('Review Writer');
    await itemMetadata.getByLabel('Languages', { exact: true }).fill('en, nl');
    await castAndCredits.fill('Presenter | Host');
    await itemEditor.getByLabel(/^tags/i).fill(manyTags);
    await workspaceSubmit(page,'save');
    await expect
      .poll(() =>
        sqlite(`
SELECT json_extract("MetadataOverrideJson", '$.tags[119]') || ',' ||
       json_extract("MetadataOverrideJson", '$.original_title') || ',' ||
       json_extract("MetadataOverrideJson", '$.year') || ',' ||
       json_extract("MetadataOverrideJson", '$.people[0].role')
FROM "AddonMediaListItem" WHERE "Id" = ${seed.secondItemId};
`),
      )
      .toBe('tag-120,Original review title,2025,Host');

    // A second save from the same hydrated editor exercises the refreshed ETag.
    await expandMetadata();
    await itemMetadata.getByLabel('Original title', { exact: true }).fill('Second saved title');
    await workspaceSubmit(page,'save');
    await expect
      .poll(() =>
        sqlite(`
SELECT json_extract("MetadataOverrideJson", '$.original_title')
FROM "AddonMediaListItem" WHERE "Id" = ${seed.secondItemId};
`),
      )
      .toBe('Second saved title');

    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);

    sqlite(`
UPDATE "AddonMediaListItem"
SET "Availability" = 1, "AvailabilityReason" = 'not_playable',
    "AvailabilityReasonCode" = 1
WHERE "Id" = ${seed.secondItemId};
`);
    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);
    await expect(page.getByText(/Restricted items excluded from this revision/)).toContainText('1');
    expect(sqlite(`SELECT "Included" FROM "AddonMediaListItem" WHERE "Id"=${seed.secondItemId}`)).toBe('1');
    await gotoHydrated(page,`/media/addon-lists/${seed.listId}/artwork`);
    await page.locator('[name="upload_poster"]').setInputFiles({name:'poster.png',mimeType:'image/png',buffer:Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Y9Wl9sAAAAASUVORK5CYII=','base64')});
    await workspaceSubmit(page,'upload:poster');
    await expect(page.locator('[name="metadata_poster"]')).toHaveValue(/^ersatzrs-upload:\/\//);
    sqlite(`
UPDATE "AddonMediaList"
SET "MetadataOverrideJson" = json_set("MetadataOverrideJson", '$.sort_title', 'Concurrent writer')
WHERE "Id" = ${seed.listId};
`);
    await workspaceSubmit(page,'save');
    await expect(page.locator('#workspace-errors')).toBeVisible();

    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);
    await gotoHydrated(page,`/media/addon-lists/${seed.listId}/settings`);
    await page.locator('[name="metadata_title"]').fill('Reviewed collection');
    await workspaceSubmit(page,'save');
    await expect
      .poll(() =>
        sqlite(`
SELECT json_extract("MetadataOverrideJson", '$.title')
FROM "AddonMediaList" WHERE "Id" = ${seed.listId};
`),
      )
      .toBe('Reviewed collection');

    await gotoHydrated(page,`/media/addon-lists/${seed.listId}`);
    await workspaceSubmit(page,'publish');
    await expect
      .poll(
        () =>
          sqlite(`
SELECT "ReviewState" || ',' || "PublishedRevision" || ',' || (
  SELECT "operation"."State"
  FROM "AddonMediaListPublicationJob" AS "job"
  JOIN "ManagedLibraryOperation" AS "operation" ON "operation"."Id" = "job"."OperationId"
  WHERE "job"."AddonMediaListId" = ${seed.listId}
  ORDER BY "job"."Generation" DESC
  LIMIT 1
)
FROM "AddonMediaList" WHERE "Id" = ${seed.listId};
`),
        { timeout: 30_000 },
      )
      .toBe('2,1,3');
    expect(
      sqlite(`SELECT "Published" FROM "AddonMediaListItem" WHERE "Id" = ${seed.firstItemId};`),
    ).toBe('1');
    expect(
      sqlite(`SELECT "Published" FROM "AddonMediaListItem" WHERE "Id" = ${seed.secondItemId};`),
    ).toBe('0');
    expect(
      sqlite(`SELECT "Included" FROM "AddonMediaListItem" WHERE "Id" = ${seed.secondItemId};`),
    ).toBe('1');

    sqlite(`
UPDATE "AddonMediaListItem"
SET "Availability" = 0, "AvailabilityReason" = NULL, "AvailabilityReasonCode" = NULL
WHERE "Id" = ${seed.secondItemId};
UPDATE "AddonMediaList"
SET "ReviewState" = 1, "DraftRevision" = "DraftRevision" + 1
WHERE "Id" = ${seed.listId};
`);
    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);
    await page.locator('[name="category"]').selectOption('added');await workspaceSubmit(page,'filter');
    await expect(page.locator('.episode-row')).toHaveCount(1);
    await expect(page.locator('.episode-row')).toContainText(seed.secondTitle);
    await gotoHydrated(page,`/media/addon-lists/${seed.listId}`);
    await workspaceSubmit(page,'publish');
    await expect
      .poll(
        () =>
          sqlite(`
SELECT "PublishedRevision" || ',' || (
  SELECT "Published" FROM "AddonMediaListItem" WHERE "Id" = ${seed.secondItemId}
) || ',' || (
  SELECT "operation"."State"
  FROM "AddonMediaListPublicationJob" AS "job"
  JOIN "ManagedLibraryOperation" AS "operation" ON "operation"."Id" = "job"."OperationId"
  WHERE "job"."AddonMediaListId" = ${seed.listId}
  ORDER BY "job"."Generation" DESC
  LIMIT 1
)
FROM "AddonMediaList" WHERE "Id" = ${seed.listId};
`),
        { timeout: 30_000 },
      )
      .toBe('2,1,3');

    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);
    await gotoHydrated(page,`/media/addon-lists/${seed.listId}/activity`);
    await expect(page.locator('button[value="restore-preview:2"]')).toBeDisabled();
    await expect(page.locator('button[value="restore-preview:1"]')).toBeEnabled();
    await page.locator('[name="compare_left"]').selectOption('2');await page.locator('[name="compare_right"]').selectOption('1');
    await workspaceSubmit(page,'compare');await expect(page.getByTestId('revision-diff')).toBeVisible();
    const staleRestore = await page.request.post('/api/addon-lists/restore-revision', {
      form: {
        id: String(seed.listId),
        source_published_revision: '1',
        expected_draft_revision: '0',
        expected_published_revision: '1',
      },
      maxRedirects: 0,
    });
    expect(staleRestore.status()).toBe(303);
    expect(staleRestore.headers().location).toContain('?error=revision-restore');

    await gotoHydrated(page,`/media/addon-lists/${seed.listId}/settings`);
    await workspaceSubmit(page,'reset-preview');
    await expect(page.getByRole('region',{name:'Reset and reload'})).toContainText('Published items2');
    await workspaceSubmit(page,'cancel-confirmation');
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
    await workspaceSubmit(page,'save');
    await expect(page.getByLabel('Collection Type', { exact: true })).toHaveValue('other_videos');
    await expect(page.getByTestId('addon-media-type-change-help')).toContainText(
      'Remote Streams → Other Videos',
    );

    await gotoHydrated(page,`/media/addon-lists/${seed.listId}`);
    await workspaceSubmit(page,'publish');
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
       "PublishedRevision" || ',' ||
       (SELECT "operation"."State"
        FROM "AddonMediaListPublicationJob" AS "job"
        JOIN "ManagedLibraryOperation" AS "operation" ON "operation"."Id" = "job"."OperationId"
        WHERE "job"."AddonMediaListId" = ${seed.listId}
        ORDER BY "job"."Generation" DESC
        LIMIT 1)
FROM "AddonMediaList" WHERE "Id" = ${seed.listId};
`),
        { timeout: 30_000 },
      )
      .toBe('2,2,3,3');
    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);
    await gotoHydrated(page,`/media/addon-lists/${seed.listId}/settings`);
    await expect(page.getByTestId('addon-media-type-change-help')).toContainText(
      'Current library type: Other Videos',
    );
    await expect(page.getByTestId('addon-media-type-change-route')).not.toBeVisible();
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

    const revisionOneContent = revisionLogicalContent(seed.listId, 1);
    expect(revisionOneContent).not.toBe('');
    await gotoHydrated(page,`/media/addon-lists/${seed.listId}/activity`);
    await workspaceSubmit(page,'restore-preview:1');
    await page.locator('[name="acknowledged"][type="checkbox"]').check();
    await workspaceSubmit(page,'confirm-operation');
    await expect(page.locator('#workspace-errors')).toHaveCount(0);
    await expect
      .poll(
        () => sqlite(`
SELECT "list"."PublishedRevision" || ',' || "job"."TargetPublishedRevision" || ',' ||
       "job"."SourcePublishedRevision" || ',' || "operation"."State"
FROM "AddonMediaList" AS "list"
JOIN "AddonMediaListPublicationJob" AS "job"
  ON "job"."AddonMediaListId" = "list"."Id"
JOIN "ManagedLibraryOperation" AS "operation"
  ON "operation"."Id" = "job"."OperationId"
WHERE "list"."Id" = ${seed.listId}
ORDER BY "job"."Generation" DESC
LIMIT 1;
`),
        { timeout: 30_000 },
      )
      .toBe('4,4,1,3');
    expect(revisionLogicalContent(seed.listId, 4)).toBe(revisionOneContent);
    // Older unpinned revisions are pruned by hourly maintenance, not publication.
    // Repository retention tests cover that cutoff; all three latest snapshots must exist now.
    expect(sqlite(`
SELECT group_concat("Revision", ',') FROM (
  SELECT "Revision" FROM "AddonMediaListRevision"
  WHERE "AddonMediaListId" = ${seed.listId}
  ORDER BY "Revision" DESC
  LIMIT 3
);
`)).toBe('4,3,2');

    expect(runtimeErrors).toEqual([]);
  });
});
