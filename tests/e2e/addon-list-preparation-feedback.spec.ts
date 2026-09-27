import { expect, test, type Page, type TestInfo } from '@playwright/test';
import { existsSync, readFileSync } from 'fs';
import {
  preparedSourceUrl, hydrated, installPreparedFixture, preparedInvocationLog, preparedSqlite,
  waitForPreparedImport, preparedWorkspaceReady,
  type PreparedFixtureVariant,
} from './addon-prepared-fixture';

// Add-on list preparation feedback (audit A01–A05). Run through
// scripts/e2e/playwright-e2e.sh with serial workers. Every fixture discovery
// returns the same provider list, and lists are unique per add-on and provider
// list, so each scenario, mode and browser project installs its own package
// identity. The fixture provider succeeds only when `processed` is true; the
// identity's declared default decides that without any saved option.
const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

function sqlite(sql: string): string {
  if (!dbPath) throw new Error('The isolated E2E database is required');
  return preparedSqlite(dbPath, sql);
}

/** One isolated package identity; installed once per database. */
function scenario(info: TestInfo, name: string, processedDefault: boolean): PreparedFixtureVariant {
  const variant = { addonId: `org.ersatzrs.e2e-feedback.${info.project.name}-${name}`, processedDefault };
  if (sqlite(`SELECT COUNT(*) FROM "AddonInstallation" WHERE "AddonId"='${variant.addonId}';`) === '0') {
    installPreparedFixture(dbPath!, false, false, variant);
  }
  return variant;
}

function prepareInvocations(variant: PreparedFixtureVariant): number {
  const log = preparedInvocationLog(dbPath!, variant);
  if (!existsSync(log)) return 0;
  return readFileSync(log, 'utf8').split('\n')
    .filter(line => line && JSON.parse(line).command === 'prepare').length;
}

function watchErrors(page: Page): string[] {
  const errors: string[] = [];
  page.on('pageerror', error => { if (error.name !== 'AbortError') errors.push(error.message); });
  page.on('console', message => {
    if (message.type() === 'error' && /hydration|failed_to_cast_marker_node|rust_panic/i.test(message.text())) errors.push(message.text());
  });
  return errors;
}

async function fillAdd(page: Page, variant: PreparedFixtureVariant, mode: string): Promise<string> {
  const name = variant.addonId!.split('.').pop()!;
  const url = `${preparedSourceUrl(variant)}/${name}`;
  await hydrated(page, '/media/addon-lists/add');
  await page.getByLabel('Add-on list URL').fill(url);
  await page.getByLabel('Add-on', { exact: true }).selectOption(variant.addonId!);
  await page.getByLabel('Delivery', { exact: true }).selectOption(mode);
  await page.getByLabel('Collection Type', { exact: true }).selectOption('remote_streams');
  await page.getByLabel('Storage folder').fill(`preparation-feedback-${name}`);
  await page.getByRole('combobox', { name: 'Automatic Refresh' }).selectOption('never');
  await page.getByRole('checkbox', { name: 'Load item details automatically' }).uncheck();
  return url;
}

function listIdFrom(page: Page): number {
  const listId = Number(page.url().match(/addon-lists\/(\d+)/)?.[1]);
  expect(listId).toBeGreaterThan(0);
  return listId;
}

async function importComplete(listId: number): Promise<void> {
  await waitForPreparedImport(dbPath!, listId);
}

async function importList(page: Page, variant: PreparedFixtureVariant, mode: string): Promise<number> {
  await fillAdd(page, variant, mode);
  await page.getByRole('button', { name: 'Import overview' }).click();
  await expect(page).toHaveURL(/\/media\/addon-lists\/\d+\?import=1$/);
  const listId = listIdFrom(page);
  await importComplete(listId);
  // Database completion precedes the landing page's automatic reload. Wait for
  // that transition before a caller navigates, so Firefox cannot abort its goto.
  await preparedWorkspaceReady(page);
  return listId;
}

function preparationJobs(listId: number, state: number): string {
  return sqlite(`SELECT COUNT(*) FROM "AddonMediaListItemOperationJob" WHERE "AddonMediaListId"=${listId} AND "Kind"=1 AND "State"=${state};`);
}

test.beforeEach(async ({ page }) => {
  test.skip(!dbPath, 'Requires the wrapper-owned isolated E2E database');
  await page.request.post('/admin-api/settings/ui/save', { form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0 });
});

test('untouched add-on defaults prepare and publish in both download modes', async ({ page }, info) => {
  test.setTimeout(600000);
  const errors = watchErrors(page);
  for (const mode of ['download_ahead', 'download_ahead_new_videos']) {
    const variant = scenario(info, `defaults-${mode.replace(/_/g, '-')}`, true);
    const listId = await importList(page, variant, mode);
    // A01: creating the list retains the installed option schema without a Save.
    expect(sqlite(`SELECT "DraftOptionSchemaId" IS NOT NULL FROM "AddonMediaList" WHERE "Id"=${listId};`)).toBe('1');

    await hydrated(page, `/media/addon-lists/${listId}`);
    // Saved imports own preparation and publication without an extra action.
    expect(sqlite(`SELECT "RefreshPublicationPolicy" FROM "AddonMediaList" WHERE "Id"=${listId};`)).toBe('2');
    await expect(page.getByRole('button', { name: 'Prepare publication', exact: true })).toBeHidden();
    await expect(page.getByRole('button', { name: 'Publish saved changes', exact: true })).toBeHidden();
    await expect.poll(() => preparationJobs(listId, 2), { timeout: 180000 }).toBe('1');
    expect(prepareInvocations(variant)).toBeGreaterThan(0);
    expect(preparationJobs(listId, 3)).toBe('0');
    await expect(page.getByTestId('addon-preparation-failed')).toBeHidden();

    await expect.poll(() => sqlite(`SELECT list."PublishedRevision"||','||list."ReviewState"||','||(list."PublicationJournalJson" IS NULL)||','||COUNT(r."Id")
FROM "AddonMediaList" list
LEFT JOIN "AddonMediaListRevision" r ON r."AddonMediaListId"=list."Id" AND r."Revision"=list."PublishedRevision" AND r."CompletedAt" IS NOT NULL
WHERE list."Id"=${listId};`), { timeout: 90000 }).toBe('1,2,1,1');

    // Replay a provider result reporting source loss after a local publication.
    // It must publish automatically while keeping the validated file playable.
    const artifact = sqlite(`SELECT "PreparedArtifactId" FROM "AddonMediaListRevisionItem" i
      JOIN "AddonMediaListRevision" r ON r."Id"=i."AddonMediaListRevisionId"
      WHERE r."AddonMediaListId"=${listId} AND r."Revision"=1;`);
    expect(Number(artifact)).toBeGreaterThan(0);
    sqlite(`UPDATE "AddonMediaListItem" SET "Availability"=1,"AvailabilityReasonCode"=NULL
      WHERE "AddonMediaListId"=${listId};
      UPDATE "AddonMediaList" SET "DraftRevision"="DraftRevision"+1,"ReviewState"=1 WHERE "Id"=${listId};`);
    await expect.poll(() => sqlite(`SELECT "PublishedRevision"||','||"ReviewState" FROM "AddonMediaList" WHERE "Id"=${listId};`), { timeout: 90000 }).toBe('2,2');
    expect(sqlite(`SELECT "PreparedArtifactId" FROM "AddonMediaListRevisionItem" i
      JOIN "AddonMediaListRevision" r ON r."Id"=i."AddonMediaListRevisionId"
      WHERE r."AddonMediaListId"=${listId} AND r."Revision"=2;`)).toBe(artifact);
    expect(sqlite(`SELECT m."State" FROM "MediaItem" m JOIN "AddonMediaListItem" i ON i."MediaItemId"=m."Id"
      WHERE i."AddonMediaListId"=${listId};`)).toBe('0');
    await hydrated(page, `/media/addon-lists/${listId}`);
    await expect(page.getByText('Local copy playable', { exact: true })).toBeVisible();
  }
  expect(errors).toEqual([]);
});

test('a failed preparation attempt is explained and retryable with and without JavaScript', async ({ page, browser, baseURL }, info) => {
  test.setTimeout(480000);
  if (!baseURL) throw new Error('Playwright baseURL is required');
  const errors = watchErrors(page);
  const variant = scenario(info, 'rejected-default', false);
  const listId = await importList(page, variant, 'download_ahead');
  await hydrated(page, `/media/addon-lists/${listId}`);
  await expect.poll(() => Number(preparationJobs(listId, 3)), { timeout: 180000 }).toBeGreaterThan(0);
  // The host validates schema and options before invoking the provider, so a
  // prepare invocation proves the untouched request itself was valid.
  expect(prepareInvocations(variant)).toBeGreaterThan(0);
  expect(sqlite(`SELECT COUNT(*) FROM "AddonMediaListItemOperationJob" WHERE "AddonMediaListId"=${listId} AND "OutcomeJson" LIKE '%InvalidRequest%';`)).toBe('0');

  // A03: without a reload, the live summary turns into an actionable failure.
  const failed = page.getByTestId('addon-preparation-failed');
  await expect(failed).toBeVisible({ timeout: 20000 });
  await expect(failed).toContainText('Preparation tasks: 1');
  await expect(page.getByText('Media preparation failed', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Retry preparation', exact: true })).toBeEnabled();
  await hydrated(page, `/media/addon-lists/${listId}/activity`);
  await expect(page.getByTestId('addon-preparation-failures').locator('.notice.error')).toHaveCount(1);
  expect(errors).toEqual([]);

  // Without JavaScript the server renders the same guidance, and the native
  // Retry submit starts a new provider attempt.
  const context = await browser.newContext({ baseURL, javaScriptEnabled: false });
  try {
    const plain = await context.newPage();
    expect((await plain.goto(`/media/addon-lists/${listId}`))?.status()).toBe(200);
    const plainFailed = plain.getByTestId('addon-preparation-failed');
    await expect(plainFailed).toBeVisible();
    await expect(plainFailed).toContainText('Preparation tasks: 1');
    const attempts = prepareInvocations(variant);
    await Promise.all([
      plain.waitForEvent('framenavigated', frame => frame === plain.mainFrame()),
      plain.getByRole('button', { name: 'Retry preparation', exact: true }).click(),
    ]);
    await expect.poll(() => prepareInvocations(variant), { timeout: 180000 }).toBeGreaterThan(attempts);
    expect(prepareInvocations(variant)).toBeGreaterThan(attempts);
    await expect.poll(() => sqlite(`SELECT "State" FROM "AddonMediaListItemOperationJob"
      WHERE "AddonMediaListId"=${listId} AND "Kind"=1 ORDER BY "Id" DESC LIMIT 1;`), { timeout: 180000 }).toBe('3');

    expect((await plain.goto(`/media/addon-lists/${listId}/activity`))?.status()).toBe(200);
    await expect(plain.getByTestId('addon-preparation-failures')).toContainText('Preparation tasks');
    // The arrival notice is server-rendered for the no-JS add redirect.
    expect((await plain.goto(`/media/addon-lists/${listId}?error=save`))?.status()).toBe(200);
    await expect(plain.getByTestId('addon-list-arrival-error')).toContainText('The list was added, but some of its settings were not saved.');
  } finally {
    await context.close();
  }
});

test('a legacy unbound draft adopts defaults on retry and keeps unsaved input at the returned revision', async ({ page }, info) => {
  test.setTimeout(240000);
  const errors = watchErrors(page);
  const listId = await importList(page, scenario(info, 'legacy-unbound', false), 'download_ahead');
  await expect.poll(() => Number(preparationJobs(listId, 3)), { timeout: 180000 }).toBeGreaterThan(0);
  // Reproduce a failed draft saved before creation retained option defaults.
  sqlite(`UPDATE "AddonMediaList" SET "DraftOptionSchemaId"=NULL,"DraftOptionValuesJson"='{}' WHERE "Id"=${listId};
UPDATE "AddonMediaListItem" SET "DraftOptionOverridesJson"='{}' WHERE "AddonMediaListId"=${listId};`);
  const revision = Number(sqlite(`SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id"=${listId};`));

  await hydrated(page, `/media/addon-lists/${listId}`);
  // An unsaved task field carried by the native draft, not a submitted filter.
  await page.locator('.bulk-tools > summary').click();
  await page.locator('input[name="bulk_season"]').fill('7');
  await page.getByRole('button', { name: 'Retry preparation', exact: true }).click();
  await hydrated(page);
  await expect(page.locator('#workspace-errors')).toHaveCount(0);
  await expect(page.locator('input[name="bulk_season"]')).toHaveValue('7');

  // Binding saved exactly one revision, and the native draft follows that receipt.
  const saved = Number(sqlite(`SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id"=${listId};`));
  expect(saved).toBe(revision + 1);
  expect(sqlite(`SELECT "DraftOptionSchemaId" IS NOT NULL FROM "AddonMediaList" WHERE "Id"=${listId};`)).toBe('1');
  const draft = JSON.parse(await page.locator('form#addon-list-workspace input[name="draft"]').inputValue());
  expect(draft.revision).toBe(saved);

  // The next save uses that fence instead of reporting a stale draft.
  await page.getByRole('button', { name: 'List settings', exact: true }).click();
  await hydrated(page);
  await Promise.all([
    page.waitForEvent('framenavigated', frame => frame === page.mainFrame()),
    page.getByRole('button', { name: 'Save settings', exact: true }).click(),
  ]);
  await hydrated(page);
  await expect(page.locator('#workspace-errors')).toHaveCount(0);
  expect(errors).toEqual([]);
});

test('an add whose settings fail opens the created list with a persistent notice', async ({ page }, info) => {
  test.setTimeout(180000);
  const errors = watchErrors(page);
  const variant = scenario(info, 'partial-add', false);
  // Fail only the settings step that follows successful creation.
  const settingsCall = /\/admin-api\/update_addon_media_list\d*(?:\?|$)/;
  await page.route(settingsCall, route => route.fulfill({ status: 500, contentType: 'text/plain', body: 'injected settings failure' }));
  const url = await fillAdd(page, variant, 'download_ahead');
  await page.getByRole('button', { name: 'Import overview' }).click();
  await expect(page).toHaveURL(/\/media\/addon-lists\/\d+\?error=save$/);
  const listId = listIdFrom(page);
  await hydrated(page);

  const notice = page.getByTestId('addon-list-arrival-error');
  await expect(notice).toContainText('The list was added, but some of its settings were not saved.');
  await expect(notice.getByRole('button', { name: 'List settings', exact: true })).toBeVisible();
  await page.reload();
  await hydrated(page);
  await expect(page.getByTestId('addon-list-arrival-error')).toBeVisible();

  // Retrying the same add-on and source reuses the created list rather than adding another.
  await page.unroute(settingsCall);
  await importComplete(listId);
  await preparedWorkspaceReady(page);
  await fillAdd(page, variant, 'download_ahead');
  await page.getByRole('button', { name: 'Import overview' }).click();
  await expect(page).toHaveURL(new RegExp(`/media/addon-lists/${listId}\\?import=1$`));
  expect(sqlite(`SELECT COUNT(*) FROM "AddonMediaList" WHERE "AddonId"='${variant.addonId}' AND "SourceUrl"='${url}';`)).toBe('1');
  expect(errors).toEqual([]);
});
