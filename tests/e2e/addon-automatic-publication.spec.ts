import { expect, test } from '@playwright/test';
import { hydrated, installPreparedFixture, preparedSourceUrl, preparedSqlite, waitForPreparedImport } from './addon-prepared-fixture';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;
const sqlite = (sql: string): string => preparedSqlite(dbPath!, sql);

test('automatically projects thirteen episodes and applies native saved edits', async ({ page, browser, baseURL }, info) => {
  test.skip(!dbPath, 'Requires the wrapper-owned isolated database');
  test.setTimeout(240000);
  const variant = { addonId: `org.ersatzrs.e2e-automatic.${info.project.name}`, itemCount: 13, richMetadata: true };
  installPreparedFixture(dbPath!, false, false, variant);
  const errors: string[] = [];
  page.on('pageerror', error => { if (error.name !== 'AbortError') errors.push(error.message); });
  await page.request.post('/api/settings/ui/save', { form: { language: 'en-us', is_dark_mode: 'true' }, maxRedirects: 0 });
  await hydrated(page, '/media/addon-lists/add');
  await page.getByLabel('Add-on list URL').fill(preparedSourceUrl(variant));
  await page.getByLabel('Add-on', { exact: true }).selectOption(variant.addonId);
  await page.getByLabel('Delivery', { exact: true }).selectOption('stream');
  await page.getByLabel('Collection Type', { exact: true }).selectOption('television_show');
  await page.getByLabel('Storage folder').fill(`automatic-${info.project.name}`);
  await page.getByRole('combobox', { name: 'Automatic Refresh' }).selectOption('never');
  await page.getByRole('checkbox', { name: 'Load item details automatically' }).uncheck();
  await page.getByRole('button', { name: 'Import overview' }).click();
  await expect(page).toHaveURL(/\/media\/addon-lists\/\d+\?import=1$/);
  const listId = Number(page.url().match(/addon-lists\/(\d+)/)![1]);
  await waitForPreparedImport(dbPath!, listId, 13);
  const metadataCount = `SELECT COUNT(*) FROM "EpisodeMetadata" m
    JOIN "AddonMediaListItemEpisode" e ON e."EpisodeId"=m."EpisodeId"
    JOIN "AddonMediaListItem" i ON i."Id"=e."AddonMediaListItemId"
    WHERE i."AddonMediaListId"=${listId} AND m."Plot" LIKE 'Full description %'
      AND m."ReleaseDate" LIKE '2026-05-05%' AND m."ContentRating" IN ('age:0','age:18');`;
  await expect.poll(() => sqlite(metadataCount), { timeout: 120000 }).toBe('13');
  await expect.poll(() => sqlite(`SELECT "ReviewState"=2 AND "PublishedRevision">0 AND "PublicationJournalJson" IS NULL
    FROM "AddonMediaList" WHERE "Id"=${listId};`), { timeout: 120000 }).toBe('1');
  await hydrated(page, `/media/addon-lists/${listId}`);
  await expect(page.getByRole('button', { name: 'Publish saved changes', exact: true })).toBeHidden();
  const parent = sqlite(`SELECT json_extract("MetadataJson",'$.content_ratings[0]') FROM "AddonMediaListRevision"
    WHERE "AddonMediaListId"=${listId} ORDER BY "Revision" DESC LIMIT 1;`);
  // Header snapshots stay source-owned; derived metadata belongs to the show/NFO.
  expect(parent).toBe('');
  await expect.poll(() => sqlite(`SELECT "ContentRating" FROM "ShowMetadata" WHERE "ShowId"=(SELECT "ShowId" FROM "AddonMediaListTelevisionShow" WHERE "AddonMediaListId"=${listId});`)).toBe('age:18');

  for (const [table, column, expected] of [
    ['Genre', 'Name', 'Documentary'], ['Tag', 'Name', 'Archive'],
    ['Studio', 'Name', 'Fixture Channel'], ['MetadataLanguage', 'Code', 'eng'],
  ]) {
    expect(sqlite(`SELECT f."${column}" FROM "${table}" f JOIN "ShowMetadata" m ON m."Id"=f."ShowMetadataId"
      WHERE m."ShowId"=(SELECT "ShowId" FROM "AddonMediaListTelevisionShow" WHERE "AddonMediaListId"=${listId});`)).toBe(expected);
  }

  const context = await browser.newContext({ baseURL, javaScriptEnabled: false });
  try {
    const plain = await context.newPage();
    await plain.goto(`/media/addon-lists/${listId}/settings`);
    const title = plain.getByLabel('Title', { exact: true });
    await title.fill('Saved automatically without JavaScript');
    expect(sqlite(`SELECT "Title" FROM "ShowMetadata" WHERE "ShowId"=(SELECT "ShowId" FROM "AddonMediaListTelevisionShow" WHERE "AddonMediaListId"=${listId});`)).not.toBe('Saved automatically without JavaScript');
    await Promise.all([
      plain.waitForEvent('framenavigated', frame => frame === plain.mainFrame()),
      plain.locator('button[name="intent"][value="save"]').click(),
    ]);
    await expect(plain.locator('#workspace-errors')).toHaveCount(0);
    await expect.poll(() => sqlite(`SELECT "Title" FROM "ShowMetadata" WHERE "ShowId"=(SELECT "ShowId" FROM "AddonMediaListTelevisionShow" WHERE "AddonMediaListId"=${listId});`), { timeout: 120000 }).toBe('Saved automatically without JavaScript');
    expect(sqlite(metadataCount)).toBe('13');
  } finally {
    await context.close();
  }
  await expect.poll(() => sqlite(`SELECT "ReviewState"=2 AND "PublicationJournalJson" IS NULL
    FROM "AddonMediaList" WHERE "Id"=${listId};`), {timeout:60000}).toBe('1');
  const itemId = sqlite(`SELECT MIN("Id") FROM "AddonMediaListItem" WHERE "AddonMediaListId"=${listId};`);
  const mediaId = sqlite(`SELECT "MediaItemId" FROM "AddonMediaListItem" WHERE "Id"=${itemId};`);
  const revision = Number(sqlite(`SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id"=${listId};`));
  // Simulate provider source loss, with no local artifact in this streaming list.
  sqlite(`BEGIN IMMEDIATE;
    UPDATE "AddonMediaListItem" SET "Availability"=1 WHERE "Id"=${itemId};
    UPDATE "AddonMediaList" SET "DraftRevision"="DraftRevision"+1,"ReviewState"=1 WHERE "Id"=${listId};
    COMMIT;`);
  await expect.poll(() => sqlite(`SELECT "PublishedRevision"||','||"ReviewState" FROM "AddonMediaList"
    WHERE "Id"=${listId} AND "PublicationJournalJson" IS NULL;`), {timeout:120000}).toBe(`${revision+1},2`);
  expect(sqlite(`SELECT "MediaItemId" FROM "AddonMediaListItem" WHERE "Id"=${itemId};`)).toBe(mediaId);
  expect(sqlite(`SELECT "State" FROM "MediaItem" WHERE "Id"=${mediaId};`)).toBe('2');
  expect(sqlite(metadataCount)).toBe('13');
  expect(errors).toEqual([]);
});
