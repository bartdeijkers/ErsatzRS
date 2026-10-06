import { test, expect, Page } from '@playwright/test';
import { existsSync, readFileSync } from 'fs';
import { execFileSync } from 'child_process';
import { join } from 'path';
import { createHash } from 'crypto';
import { installPreparedFixture, preparedSourceUrl, hydrated, waitForPreparedImport } from './addon-prepared-fixture';
import { seedMetadataEditor, sqlite, sqlEscape } from './fixtures/addon-metadata';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

async function submit(page: Page, intent: string, hydrated: boolean) {
  await Promise.all([
    page.waitForEvent('framenavigated', frame => frame === page.mainFrame()),
    page.locator(`button[name="intent"][value="${intent}"]:visible`).first().click(),
  ]);
  await page.waitForLoadState('load');
  if (hydrated) await page.waitForFunction(() => (window as any).__ersatzrs_hydrated === true);
}

for (const javaScriptEnabled of [false, true]) {
  for (const language of ['en-us', 'nl']) {
    test(`finding 01 previews and retains rolling selection (${language}, ${javaScriptEnabled ? 'hydrated' : 'no JS'})`, async ({ browser, baseURL }, info) => {
      test.setTimeout(180000);
      test.skip(!dbPath, 'An isolated E2E database is required');
      const seed = seedMetadataEditor(info);
      sqlite(`DELETE FROM "AddonMediaListSelection" WHERE "AddonMediaListId"=${seed.listId};`);
      const recent = new Date(Date.now() - 2 * 86400000).toISOString();
      const old = new Date(Date.now() - 730 * 86400000).toISOString();
      sqlite(`UPDATE "AddonMediaListItem" SET "ProviderMetadataJson"=json_set("ProviderMetadataJson",'$.release_date','${sqlEscape(old)}'),"DurationSeconds"=60,"SelectionOverride"=NULL,"IncludedByUser"=0 WHERE "Id"=${seed.firstItemId};
        UPDATE "AddonMediaListItem" SET "ProviderMetadataJson"=json_set("ProviderMetadataJson",'$.release_date','${sqlEscape(recent)}'),"DurationSeconds"=120,"SelectionOverride"=NULL,"IncludedByUser"=0 WHERE "Id"=${seed.secondItemId};`);
      const nextPoll = new Date(Date.now() + 86400000).toISOString();
      sqlite(`UPDATE "AddonMediaList" SET "LastRefreshSucceededAt"='${sqlEscape(recent)}',"NextRefreshAt"='${sqlEscape(nextPoll)}' WHERE "Id"=${seed.listId};`);
      const initial = sqlite(`SELECT "Id","Included","Published" FROM "AddonMediaListItem" WHERE "AddonMediaListId"=${seed.listId} ORDER BY "Id";`);
      const published = sqlite(`SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id"=${seed.listId};`);
      const context = await browser.newContext({ baseURL, javaScriptEnabled });
      await context.request.post('/admin-api/settings/ui/save', { form: { is_dark_mode: 'true', language }, maxRedirects: 0 });
      const page = await context.newPage();
      const settledSettings = async () => {
        await expect.poll(() => sqlite(`SELECT COUNT(*) FROM "AddonMediaListAutoPublication" a
          JOIN "AddonMediaList" l ON l."Id"=a."AddonMediaListId"
          WHERE l."Id"=${seed.listId} AND a."DraftRevision"=l."DraftRevision"
          AND a."PublishedRevision"=l."PublishedRevision" AND a."State" IN ('done','held')
          AND NOT EXISTS (SELECT 1 FROM "AddonMediaListPublicationJob" j
            JOIN "ManagedLibraryOperation" o ON o."Id"=j."OperationId"
            WHERE j."AddonMediaListId"=l."Id" AND o."State" IN (0,1,2));`), {
          timeout: 45000, message: 'The saved revision finishes automatic assessment before the next edit',
        }).toBe('1');
        await page.goto(`/media/addon-lists/${seed.listId}/settings`);
        if (javaScriptEnabled) await page.waitForFunction(() => (window as any).__ersatzrs_hydrated === true);
      };
      await page.goto(`/media/addon-lists/${seed.listId}/settings`);
      if (javaScriptEnabled) await page.waitForFunction(() => (window as any).__ersatzrs_hydrated === true);
      await expect(page.getByTestId('addon-selection')).toBeVisible();
      await expect(page.locator('button[name="intent"][value="selection-auto"]')).toHaveText(language === 'nl' ? 'Selectiefilters volgen' : 'Follow selection filters');
      await expect(page.getByTestId('addon-selection')).toContainText(language === 'nl' ? 'Effectieve ouderdomsgrens' : 'Effective age cutoff');
      await expect(page.locator('#source')).toContainText(recent.replace(/\.000Z$/, 'Z').replace(/Z$/, '+00:00'));
      await expect(page.locator('#source')).toContainText(nextPoll.replace(/\.000Z$/, 'Z').replace(/Z$/, '+00:00'));
      await page.locator('[name="selection_enabled"][type="checkbox"]').check();
      await page.locator('[name="selection_age_days"]').fill('365');
      await page.locator('[name="selection_newest_count"]').fill('1');
      await page.locator('[name="selection_minimum_seconds"]').fill('60');
      await page.locator('[name="selection_maximum_seconds"]').fill('120');
      await page.locator('[name="selection_include_terms"]').fill(`${seed.firstTitle.toUpperCase()}\n${seed.secondTitle.toUpperCase()}`);
      await page.locator('[name="selection_exclude_terms"]').fill('A nonexistent topic');
      await submit(page, 'preview-selection', javaScriptEnabled);
      const preview = page.getByTestId('addon-selection-preview');
      await expect(preview).toContainText(language === 'nl' ? 'Buiten het ouderdomsvenster' : 'Outside the age window');
      await expect(preview).toContainText(seed.secondTitle);
      expect(sqlite(`SELECT "Id","Included","Published" FROM "AddonMediaListItem" WHERE "AddonMediaListId"=${seed.listId} ORDER BY "Id";`)).toBe(initial);
      expect(sqlite(`SELECT COUNT(*) FROM "AddonMediaListSelection" WHERE "AddonMediaListId"=${seed.listId};`)).toBe('0');
      expect(sqlite(`SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id"=${seed.listId};`)).toBe(published);
      await submit(page, 'save', javaScriptEnabled);
      expect(sqlite(`SELECT "Included" FROM "AddonMediaListItem" WHERE "Id"=${seed.firstItemId};`)).toBe('0');
      expect(sqlite(`SELECT "Included" FROM "AddonMediaListItem" WHERE "Id"=${seed.secondItemId};`)).toBe('1');
      const saved = sqlite(`SELECT "PolicyJson","CutoffAt" FROM "AddonMediaListSelection" WHERE "AddonMediaListId"=${seed.listId};`);
      expect(saved).toContain('"age_days":365');
      expect(saved).toContain('"newest_count":1');
      expect(saved.split('|')[1]).toMatch(/^20\d\d-/);
      await settledSettings();
      await page.locator('[name="selection_item_id"]').selectOption(String(seed.firstItemId));
      await submit(page, 'selection-pin', javaScriptEnabled);
      await expect(page.getByTestId('addon-selection-preview')).toContainText(language === 'nl' ? 'Vastgezet' : 'Pinned');
      expect(sqlite(`SELECT "Included","SelectionOverride" FROM "AddonMediaListItem" WHERE "Id"=${seed.firstItemId};`)).toBe('1|1');
      await settledSettings();
      await page.locator('[name="selection_item_id"]').selectOption(String(seed.secondItemId));
      await submit(page, 'selection-exclude', javaScriptEnabled);
      expect(sqlite(`SELECT "Included","SelectionOverride" FROM "AddonMediaListItem" WHERE "Id"=${seed.secondItemId};`)).toBe('0|0');
      await settledSettings();
      await page.locator('[name="selection_item_id"]').selectOption(String(seed.secondItemId));
      await submit(page, 'selection-auto', javaScriptEnabled);
      expect(sqlite(`SELECT "Included","SelectionOverride" FROM "AddonMediaListItem" WHERE "Id"=${seed.secondItemId};`)).toBe('1|');
      await settledSettings();
      await page.locator('[name="selection_paused"][type="checkbox"]').check();
      await submit(page, 'save', javaScriptEnabled);
      expect(sqlite(`SELECT "RefreshPaused" FROM "AddonMediaListSelection" WHERE "AddonMediaListId"=${seed.listId};`)).toBe('1');
      await settledSettings();
      await expect(page.locator('[name="selection_paused"][type="checkbox"]')).toBeChecked();
      await page.locator('[name="selection_paused"][type="checkbox"]').uncheck();
      await submit(page, 'save', javaScriptEnabled);
      expect(sqlite(`SELECT "RefreshPaused" FROM "AddonMediaListSelection" WHERE "AddonMediaListId"=${seed.listId};`)).toBe('0');
      await settledSettings();
      const beforeFailure = sqlite(`SELECT "PolicyJson","CutoffAt","RefreshPaused" FROM "AddonMediaListSelection" WHERE "AddonMediaListId"=${seed.listId};`);
      await page.locator('[name="selection_newest_count"]').fill('0');
      await submit(page, 'save', javaScriptEnabled);
      await expect(page.locator('#workspace-errors')).toContainText(language === 'nl' ? 'Controleer de selectiegrenzen' : 'Check the selection limits');
      expect(sqlite(`SELECT "PolicyJson","CutoffAt","RefreshPaused" FROM "AddonMediaListSelection" WHERE "AddonMediaListId"=${seed.listId};`)).toBe(beforeFailure);
      const selection = page.getByTestId('addon-selection');
      if (javaScriptEnabled) {
        await page.setViewportSize({ width: 390, height: 844 });
        const navigation = page.locator('#nav-toggle');
        if (await navigation.getAttribute('aria-expanded') === 'true') await navigation.click();
        await expect(navigation).toHaveAttribute('aria-expanded', 'false');
        await expect.poll(() => page.locator('[data-nav-drawer]')
          .evaluate(element => element.getBoundingClientRect().right)).toBeLessThanOrEqual(0);
        await selection.scrollIntoViewIfNeeded();
        const bounds = await selection.boundingBox();
        expect(bounds!.x).toBeGreaterThanOrEqual(0);
        expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(391);
        await selection.locator('h2').scrollIntoViewIfNeeded();
        await page.screenshot({ path: info.outputPath('selection-mobile.png') });
        await selection.locator('fieldset').scrollIntoViewIfNeeded();
        await page.screenshot({ path: info.outputPath('selection-mobile-reasons.png') });
      } else {
        await selection.screenshot({ path: info.outputPath('selection-native.png') });
      }
      // The editor seed has only a synthetic local-library link. A new import
      // needs the real add-on media-source registration used by the worker fixture.
      const source = { addonId: `org.ersatzrs.e2e.selection-review.${info.project.name}.${language}.${javaScriptEnabled ? 'hydrated' : 'native'}`, richMetadata: true };
      installPreparedFixture(dbPath!, false, false, source);
      await page.setViewportSize({ width: 1280, height: 900 });
      await page.goto(`/media/addon-lists/add?addon_id=${encodeURIComponent(source.addonId)}`);
      if (javaScriptEnabled) await page.waitForFunction(() => (window as any).__ersatzrs_hydrated === true);
      await page.locator('[name="source_url"]').fill(preparedSourceUrl(source));
      await page.locator('[name="presentation_kind"]').selectOption('remote_streams');
      await page.locator('[name="initial_selection_review"]').check();
      await page.locator('[name="load_item_details"]').uncheck();
      await Promise.all([
        page.waitForURL(/\/media\/addon-lists\/\d+/),
        page.locator('button[type="submit"]').last().click(),
      ]);
      const importedId = Number(new URL(page.url()).pathname.split('/')[3]);
      expect(importedId).toBeGreaterThan(0);
      await expect.poll(() => sqlite(`SELECT "State" FROM "AddonMediaListImportJob" WHERE "AddonMediaListId"=${importedId} ORDER BY "Generation" DESC LIMIT 1;`), { timeout: 30000 }).toBe('3');
      expect(sqlite(`SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id"=${importedId};`)).toBe('0');
      expect(sqlite(`SELECT COUNT(*) FROM "AddonMediaListItemOperationJob" WHERE "AddonMediaListId"=${importedId};`)).toBe('0');
      await page.goto(`/media/addon-lists/${importedId}/settings`);
      if (javaScriptEnabled) await page.waitForFunction(() => (window as any).__ersatzrs_hydrated === true);
      await expect(page.getByTestId('addon-selection-review-required')).toBeVisible();
      await page.locator('[name="selection_enabled"][type="checkbox"]').check();
      await page.locator('[name="selection_maximum_seconds"]').fill('600');
      await submit(page, 'preview-selection', javaScriptEnabled);
      expect(sqlite(`SELECT "ReviewRequired" FROM "AddonMediaListSelection" WHERE "AddonMediaListId"=${importedId};`)).toBe('1');
      expect(sqlite(`SELECT COUNT(*) FROM "AddonMediaListPublicationJob" WHERE "AddonMediaListId"=${importedId};`)).toBe('0');
      await submit(page, 'save', javaScriptEnabled);
      expect(sqlite(`SELECT "ReviewRequired" FROM "AddonMediaListSelection" WHERE "AddonMediaListId"=${importedId};`)).toBe('0');
      await expect.poll(() => sqlite(`SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id"=${importedId};`), { timeout: 45000 }).toBe('1');
      await context.close();
    });
  }
}


test('finding 01 bounds real media preparation and preserves archives after a failed candidate', async ({ page }, info) => {
  test.setTimeout(420000);
  test.skip(!dbPath, 'An isolated E2E database is required');
  test.skip(info.project.name !== 'chromium', 'One real worker proof per isolated database');
  await page.request.post('/admin-api/settings/ui/save', { form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0 });
  const variant = { addonId: 'org.ersatzrs.e2e.selection-preparation', processedDefault: true,
    itemCount: 3, richMetadata: true, failPreparationFor: 'prepared-video-3' };
  const fixture = installPreparedFixture(dbPath!, false, false, variant);
  await hydrated(page, `/media/addon-lists/add?addon_id=${encodeURIComponent(variant.addonId)}`);
  await page.locator('[name="source_url"]').fill(preparedSourceUrl(variant));
  await page.getByLabel('Delivery', { exact: true }).selectOption('download_ahead');
  await page.getByLabel('Collection Type', { exact: true }).selectOption('remote_streams');
  await page.locator('[name="folder_name"]').fill('selection-preparation');
  await page.locator('[name="initial_selection_review"]').check();
  await page.locator('[name="load_item_details"]').uncheck();
  await Promise.all([page.waitForURL(/\/media\/addon-lists\/\d+/), page.getByRole('button', { name: 'Import overview', exact: true }).click()]);
  const id = Number(new URL(page.url()).pathname.split('/')[3]);
  await waitForPreparedImport(dbPath!, id, 3);
  expect(sqlite(`SELECT COUNT(*) FROM "AddonMediaListItemOperationJob" WHERE "AddonMediaListId"=${id};`)).toBe('0');
  const completeRevision = async (revision: number) => {
    await expect.poll(() => sqlite(`SELECT COUNT(*) FROM "AddonMediaListRevision" WHERE "AddonMediaListId"=${id} AND "Revision"=${revision} AND "CompletedAt" IS NOT NULL;`), { timeout: 90000 }).toBe('1');
    expect(sqlite(`SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id"=${id};`)).toBe(String(revision));
  };
  const choose = async (terms: string) => {
    await hydrated(page, `/media/addon-lists/${id}/settings`);
    await page.locator('[name="selection_enabled"][type="checkbox"]').check();
    await page.locator('[name="selection_include_terms"]').fill(terms);
    await submit(page, 'preview-selection', true);
    await submit(page, 'save', true);
  };
  await choose('Automatic episode 1');
  await completeRevision(1);
  expect(sqlite(`SELECT COUNT(*) FROM "AddonMediaListItemOperationJob" WHERE "AddonMediaListId"=${id} AND "Kind"=1;`)).toBe('1');
  await choose('Automatic episode 1\nAutomatic episode 2');
  await completeRevision(2);
  const archived = JSON.parse(sqlite(`SELECT json_object('id',a."Id",'relativePath',a."RelativePath",'sha256',a."Sha256")
    FROM "AddonMediaListRevisionItem" ri JOIN "AddonMediaListRevision" r ON r."Id"=ri."AddonMediaListRevisionId"
    JOIN "AddonMediaListArtifact" a ON a."Id"=ri."PreparedArtifactId"
    WHERE r."AddonMediaListId"=${id} AND r."Revision"=2 AND ri."ProviderId"='prepared-video-2' LIMIT 1;`));
  archived.path = join(process.env.E2E_APP_STATE_DIR!, 'data', 'addon-processed-media', archived.relativePath);
  expect(existsSync(archived.path)).toBe(true);
  const bytes = readFileSync(archived.path);
  expect(createHash('sha256').update(bytes).digest('hex')).toBe(archived.sha256);
  const ffmpeg = process.env.ErsatzRS_FFMPEG__PATH ?? 'ffmpeg';
  execFileSync(ffmpeg, ['-hide_banner','-loglevel','error','-i',archived.path,'-f','null','-'], { timeout: 30000, stdio: ['ignore','ignore','pipe'] });
  await choose('Automatic episode 1');
  await completeRevision(3);
  expect(sqlite(`SELECT "Included","Published" FROM "AddonMediaListItem" WHERE "AddonMediaListId"=${id} AND "ProviderId"='prepared-video-2';`)).toBe('0|0');
  expect(sqlite(`SELECT "State" FROM "AddonMediaListArtifact" WHERE "Id"=${archived.id};`)).toBe('2');
  expect(readFileSync(archived.path)).toEqual(bytes);
  await choose('Automatic episode 1\nAutomatic episode 3');
  await expect.poll(() => Number(sqlite(`SELECT COUNT(*) FROM "AddonMediaListItemOperationJob" j JOIN "AddonMediaListItem" i ON i."Id"=j."AddonMediaListItemId" WHERE j."AddonMediaListId"=${id} AND i."ProviderId"='prepared-video-3' AND j."State"=3;`)), { timeout: 90000 }).toBeGreaterThan(0);
  expect(sqlite(`SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id"=${id};`)).toBe('3');
  expect(sqlite(`SELECT COUNT(*) FROM "AddonMediaListRevision" WHERE "AddonMediaListId"=${id} AND "Revision"=3 AND "CompletedAt" IS NOT NULL;`)).toBe('1');
  expect(readFileSync(archived.path)).toEqual(bytes);
  expect(sqlite(`SELECT "State" FROM "AddonMediaListArtifact" WHERE "Id"=${archived.id};`)).toBe('2');
  const calls = readFileSync(fixture.invocationLog,'utf8').trim().split(/\r?\n/).map(line => JSON.parse(line).command);
  expect(calls).not.toContain('play');
});
