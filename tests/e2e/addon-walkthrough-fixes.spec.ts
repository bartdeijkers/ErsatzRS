import { expect, test, type Page, type TestInfo } from '@playwright/test';
import { existsSync, readFileSync } from 'fs';
import {
  preparedSourceUrl, hydrated, installPreparedFixture, preparedInvocationLog, preparedSqlite,
  waitForPreparedImport, preparedWorkspaceReady,
  type PreparedFixtureVariant,
} from './addon-prepared-fixture';

// Regressions for the 2026-09-12 joint add-on list walkthrough. Run through
// scripts/e2e/playwright-e2e.sh with serial workers on its own wrapper
// database. Every fixture discovery returns the same provider list, and lists
// are unique per add-on and provider list, so each scenario, mode and browser
// project installs its own package identity. No network is used.
const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

function sqlite(sql: string): string {
  if (!dbPath) throw new Error('The isolated E2E database is required');
  return preparedSqlite(dbPath, sql);
}

/** One isolated package identity; installed once per database. */
function scenario(
  info: TestInfo, name: string, processedDefault: boolean, extra: Omit<PreparedFixtureVariant, 'addonId' | 'processedDefault'> = {},
): PreparedFixtureVariant {
  const variant = { ...extra, addonId: `org.ersatzrs.e2e-walkthrough.${info.project.name}-${name}`, processedDefault };
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

/**
 * Fill Add like a first-time operator: only the URL, provider and delivery.
 * `presentation` is set only where a scenario must not depend on the
 * television projection; otherwise Add's default stays untouched.
 */
async function fillAdd(page: Page, variant: PreparedFixtureVariant, mode: string, presentation?: string): Promise<void> {
  const name = variant.addonId!.split('.').pop()!;
  await hydrated(page, '/media/addon-lists/add');
  await page.getByLabel('Add-on list URL').fill(`${preparedSourceUrl(variant)}/${name}`);
  await page.getByLabel('Add-on', { exact: true }).selectOption(variant.addonId!);
  // The reported failure used Add's default presentation.
  if (!presentation) await expect(page.getByLabel('Collection Type', { exact: true })).toHaveValue('television_show');
  if (presentation) await page.getByLabel('Collection Type', { exact: true }).selectOption(presentation);
  await page.getByLabel('Delivery', { exact: true }).selectOption(mode);
  await page.getByLabel('Storage folder').fill(`walkthrough-${name}`);
  await page.getByRole('combobox', { name: 'Automatic Refresh' }).selectOption('never');
  await page.getByRole('checkbox', { name: 'Load item details automatically' }).uncheck();
}

async function importList(page: Page, variant: PreparedFixtureVariant, mode: string, presentation?: string, source?: string): Promise<number> {
  await fillAdd(page, variant, mode, presentation);
  if (source) await page.getByLabel('Add-on list URL').fill(source);
  await page.getByRole('button', { name: 'Import overview' }).click();
  await expect(page).toHaveURL(/\/media\/addon-lists\/\d+\?import=1$/);
  const listId = Number(page.url().match(/addon-lists\/(\d+)/)?.[1]);
  expect(listId).toBeGreaterThan(0);
  await waitForPreparedImport(dbPath!, listId, variant.itemCount ?? 1);
  // Let the landing page finish its automatic reload before callers navigate
  // or mutate the fixture revision; database completion alone is too early.
  await preparedWorkspaceReady(page);
  return listId;
}

/** PublishedRevision, ReviewState, cleared journal and a completed retained revision. */
function publicationState(listId: number): string {
  return sqlite(`SELECT list."PublishedRevision"||','||list."ReviewState"||','||(list."PublicationJournalJson" IS NULL)||','||COUNT(r."Id")
FROM "AddonMediaList" list
LEFT JOIN "AddonMediaListRevision" r ON r."AddonMediaListId"=list."Id" AND r."Revision"=list."PublishedRevision" AND r."CompletedAt" IS NOT NULL
WHERE list."Id"=${listId};`);
}

/** Show and episode rows owned by this list's television projection. */
function televisionProjection(listId: number): string {
  return sqlite(`SELECT
 (SELECT COUNT(*) FROM "AddonMediaListTelevisionShow" WHERE "AddonMediaListId"=${listId})||','||
 (SELECT COUNT(*) FROM "AddonMediaListItemEpisode" ie JOIN "AddonMediaListItem" i ON i."Id"=ie."AddonMediaListItemId" WHERE i."AddonMediaListId"=${listId})||','||
 (SELECT COUNT(*) FROM "AddonMediaListItem" WHERE "AddonMediaListId"=${listId} AND "Published"=1 AND "MediaItemId" IS NOT NULL);`);
}

function includedItems(listId: number): string {
  return sqlite(`SELECT COUNT(*) FROM "AddonMediaListItem" WHERE "AddonMediaListId"=${listId} AND "Included"=1;`);
}

function publishedItems(listId: number): string {
  return sqlite(`SELECT COUNT(*) FROM "AddonMediaListItem" WHERE "AddonMediaListId"=${listId} AND "Published"=1;`);
}

async function publishAndWait(page: Page, listId: number, revision: number): Promise<void> {
  await expect.poll(() => publicationState(listId), { timeout: 120000 }).toBe(`${revision},2,1,1`);
  await hydrated(page, `/media/addon-lists/${listId}`);
}

/** Bulk commands live in the Edit multiple items disclosure. */
async function bulkCommand(page: Page, name: string): Promise<void> {
  const command = page.getByRole('button', { name, exact: true });
  if (!(await command.isVisible())) await page.locator('.bulk-tools > summary').click();
  await command.click();
  await hydrated(page);
}

test.beforeEach(async ({ page }) => {
  test.skip(!dbPath, 'Requires the wrapper-owned isolated E2E database');
  await page.request.post('/api/settings/ui/save', { form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0 });
});

test('a new list first publishes with the default TV Shows presentation', async ({ page }, info) => {
  test.setTimeout(600000);
  const errors = watchErrors(page);
  // Stream publishes straight away; Download ahead mirrors the walkthrough,
  // where real preparation succeeded and the first commit then failed.
  for (const [mode, preparesMedia] of [['stream', false], ['download_ahead_new_videos', true]] as const) {
    const variant = scenario(info, `first-tv-${mode.replace(/_/g, '-')}`, preparesMedia);
    const listId = await importList(page, variant, mode);

    await hydrated(page, `/media/addon-lists/${listId}`);
    if (preparesMedia) {
      await hydrated(page);
      await expect.poll(() => sqlite(`SELECT COUNT(*) FROM "AddonMediaListItemOperationJob" WHERE "AddonMediaListId"=${listId} AND "Kind"=1 AND "State"=2;`), { timeout: 180000 }).toBe('1');
      expect(prepareInvocations(variant)).toBeGreaterThan(0);
    }

    await expect.poll(() => publicationState(listId), { timeout: 120000 }).toBe('1,2,1,1');
    expect(sqlite(`SELECT COUNT(*) FROM "AddonMediaListPublicationJob" WHERE "AddonMediaListId"=${listId} AND "OriginalFailureCode" IS NOT NULL;`)).toBe('0');
    expect(televisionProjection(listId)).toBe('1,1,1');
    // The operator sees the published state on a fresh render.
    await hydrated(page, `/media/addon-lists/${listId}`);
    await expect(page.getByText('Published list is up to date', { exact: true })).toBeVisible();
  }
  expect(errors).toEqual([]);
});

test('the import landing page shows the imported list without a manual reload', async ({ page }, info) => {
  test.setTimeout(240000);
  const errors = watchErrors(page);
  // Discovery finishes several seconds after the landing page has rendered.
  const variant = scenario(info, 'landing-delayed', false, { discoverDelayMs: 8000, itemCount: 2 });
  await fillAdd(page, variant, 'stream', 'remote_streams');
  await page.getByRole('button', { name: 'Import overview' }).click();
  await expect(page).toHaveURL(/\/media\/addon-lists\/\d+\?import=1$/);
  const listId = Number(page.url().match(/addon-lists\/(\d+)/)?.[1]);
  await hydrated(page);
  const importState = () => sqlite(`SELECT "State"||'|'||"DiscoveredCount" FROM "AddonMediaListImportJob" WHERE "AddonMediaListId"=${listId} ORDER BY "Generation" DESC LIMIT 1;`);
  expect(importState(), 'the landing page must render before discovery completes').not.toBe('3|2');
  // While discovery is in flight the landing page shows progress, not an empty editable list.
  await expect(page.getByText('No items.', { exact: true })).toHaveCount(0);
  await expect(page.locator('button[name="intent"][value="save"]')).toHaveCount(0);

  // No reload or navigation by the test: the page itself must catch up.
  await expect(page.getByRole('heading', { level: 1, name: 'Prepared playback fixture' })).toBeVisible({ timeout: 60000 });
  expect(importState()).toBe('3|2');
  await expect(page.getByRole('button', { name: 'Prepared calibration 2', exact: true })).toBeVisible();
  // The workspace keeps a hidden empty-state paragraph for a stable DOM shape.
  await expect(page.getByText('No items.', { exact: true })).toBeHidden();
  await expect(page.locator('select[name="category"] option').first()).toHaveText('All (2)');
  expect(errors).toEqual([]);
});

test('bulk exclude and include publish automatically only after saving', async ({ page }, info) => {
  test.setTimeout(480000);
  const errors = watchErrors(page);
  const variant = scenario(info, 'bulk-inclusion', false, { itemCount: 3 });
  // A Remote Streams presentation keeps this scenario independent of the
  // television projection covered above.
  const listId = await importList(page, variant, 'stream', 'remote_streams');
  await hydrated(page, `/media/addon-lists/${listId}`);
  await publishAndWait(page, listId, 1);
  expect(publishedItems(listId)).toBe('3');

  const selectFor = (title: string) => page.getByRole('checkbox', { name: `Selected for editing: ${title}`, exact: true });
  for (const title of ['Prepared calibration 2', 'Prepared calibration 3']) await selectFor(title).check();
  const draftRevision = Number(sqlite(`SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id"=${listId};`));

  await bulkCommand(page, 'Exclude selected items');
  // The command changes retained input only; nothing is saved or published yet.
  expect(includedItems(listId)).toBe('3');
  await page.locator('button[name="intent"][value="save"]').first().click();
  await hydrated(page);
  await expect(page.locator('#workspace-errors')).toHaveCount(0);
  expect(includedItems(listId)).toBe('1');
  expect(Number(sqlite(`SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id"=${listId};`))).toBeGreaterThan(draftRevision);
  // Selection is editing state and survives the save.
  await expect(selectFor('Prepared calibration 2')).toBeChecked();
  await expect(selectFor('Prepared calibration 3')).toBeChecked();
  await expect(selectFor('Prepared calibration')).not.toBeChecked();

  await publishAndWait(page, listId, 2);
  expect(publishedItems(listId)).toBe('1');

  for (const title of ['Prepared calibration 2', 'Prepared calibration 3']) {
    if (!(await selectFor(title).isChecked())) await selectFor(title).check();
  }
  await bulkCommand(page, 'Include selected items');
  await page.locator('button[name="intent"][value="save"]').first().click();
  await hydrated(page);
  await expect(page.locator('#workspace-errors')).toHaveCount(0);
  expect(includedItems(listId)).toBe('3');
  await publishAndWait(page, listId, 3);
  expect(publishedItems(listId)).toBe('3');
  expect(errors).toEqual([]);
});

test('a restored publication failure stays visible as a failure on fresh renders', async ({ page }, info) => {
  test.setTimeout(240000);
  const errors = watchErrors(page);
  const variant = scenario(info, 'restored-failure', false);
  const listId = await importList(page, variant, 'stream', 'remote_streams');
  expect(Number(sqlite(`SELECT COUNT(*) FROM "Library";`))).toBeGreaterThan(0);
  await publishAndWait(page, listId, 1);
  // State-only fixture matching the walkthrough rows: a completed publication
  // operation whose commit failed and restored the previous (empty) revision.
  // publication_status does not join Library, so any existing library satisfies the key.
  sqlite(`INSERT INTO "ManagedLibraryOperation" ("LibraryId","OwnerToken","Kind","State","AttemptCount","CreatedAt","UpdatedAt","CompletedAt")
VALUES ((SELECT "Id" FROM "Library" ORDER BY "Id" DESC LIMIT 1),'e2e-restored-failure',3,3,1,'2026-09-12T11:22:26Z','2026-09-12T11:22:27Z','2026-09-12T11:22:27Z');
INSERT INTO "AddonMediaListPublicationJob" ("OperationId","AddonMediaListId","Generation","SavedRevision","TargetPublishedRevision","Phase","OriginalFailureCode","Outcome","CreatedAt","UpdatedAt","CompletedAt")
VALUES (last_insert_rowid(),${listId},2,(SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id"=${listId}),1,7,'publication_commit_failed',1,'2026-09-12T11:22:26Z','2026-09-12T11:22:27Z','2026-09-12T11:22:27Z');
UPDATE "AddonMediaList" SET "ReviewState"=1 WHERE "Id"=${listId};`);

  const expectFailure = async () => {
    const recovery = page.getByTestId('addon-publication-recovery');
    await expect(recovery).toBeVisible();
    await expect(recovery).toHaveClass(/\berror\b/);
    await expect(recovery.getByText('Publication failed', { exact: true })).toBeVisible();
    await expect(recovery.getByText('Completed', { exact: true })).toHaveCount(0);
    await expect(recovery.locator('a, button').filter({ hasText: /Activity/ })).not.toHaveCount(0);
  };

  await hydrated(page, `/media/addon-lists/${listId}`);
  await expectFailure();
  const overview = page.locator('.publication-overview');
  await expect(overview).toContainText('Publication failed');
  await expect(page.getByText('Saved changes ready to publish', { exact: true })).toHaveCount(0);
  await expect(page.getByText('Published list is up to date', { exact: true })).toHaveCount(0);
  // A settled list intentionally stops polling. Give this list owned, queued
  // optional detail work so the live-failure assertion actually exercises a poll.
  // Future eligibility is a fixture barrier: no provider process should run.
  try {
    expect(sqlite(`UPDATE "AddonMediaListDetailJob" SET "State"=1,
"NextEligibleAt"=strftime('%Y-%m-%d %H:%M:%f','now','+1 hour')
WHERE "AddonMediaListId"=${listId} AND "State"=0; SELECT changes();`)).toBe('1');
    await hydrated(page, `/media/addon-lists/${listId}`);
    await expectFailure();
    const recovery = page.getByTestId('addon-publication-recovery');
    await expect(recovery.getByText('Attempts: 1', { exact: true })).toBeVisible();
    sqlite(`UPDATE "ManagedLibraryOperation" SET "AttemptCount"=2
WHERE "Id"=(SELECT "OperationId" FROM "AddonMediaListPublicationJob"
WHERE "AddonMediaListId"=${listId} AND "Generation"=2);`);
    await expect(recovery.getByText('Attempts: 2', { exact: true })).toBeVisible({ timeout: 7000 });
    await expect(overview).toContainText('Publication failed');
    await expect(page.getByText('Saved changes ready to publish', { exact: true })).toHaveCount(0);
  } finally {
    sqlite(`UPDATE "AddonMediaListDetailJob" SET "State"=0,"NextEligibleAt"=NULL
WHERE "AddonMediaListId"=${listId} AND "State"=1;`);
  }

  const preservation = 'Processing stopped to preserve edited or invalid parent NFO metadata. Reconcile the file before retrying.';
  await hydrated(page, `/media/addon-lists/${listId}/activity`);
  await expectFailure();
  await expect(page.getByTestId('addon-publication-recovery').getByText(preservation)).toBeHidden();

  // The closed preservation marker adds contextual guidance on both fresh renders.
  sqlite(`UPDATE "AddonMediaListPublicationJob" SET "OriginalFailureCode"='parent_nfo_preservation_required' WHERE "AddonMediaListId"=${listId};`);
  for (const path of [`/media/addon-lists/${listId}`, `/media/addon-lists/${listId}/activity`]) {
    await hydrated(page, path);
    await expectFailure();
    const guidance = page.getByTestId('addon-publication-recovery').getByText(preservation);
    await expect(guidance).toBeVisible();
    await expect(guidance).toContainText('Reconcile the file');
  }
  expect(errors).toEqual([]);
});

test('add-on settings show Check results live and ask for a new check after a saved change', async ({ page }, info) => {
  test.setTimeout(180000);
  const errors = watchErrors(page);
  const variant = scenario(info, 'manager', false, { extraSetting: true });

  // Manager filters carry programmatic names.
  await hydrated(page, '/settings/addons');
  await expect(page.getByRole('searchbox', { name: 'Search', exact: true })).toBeVisible();
  await expect(page.getByRole('combobox', { name: 'Status', exact: true })).toBeVisible();
  await expect(page.locator('main main')).toHaveCount(0);

  await hydrated(page, `/settings/addons/${variant.addonId}`);
  await expect(page.locator('main main')).toHaveCount(0);
  const storageRow = page.getByTestId('addon-setting-row').filter({ hasText: 'Media storage location' });
  const storage = storageRow.getByRole('textbox', { name: /Media storage location/ });
  await expect(storage).toBeVisible();

  let loads = 0;
  page.on('load', () => { loads += 1; });
  const status = page.locator('#status [role="status"]');
  await expect(status).toHaveText('—');
  await page.getByRole('button', { name: 'Check', exact: true }).click();
  // The persisted result appears in place, with a readable UTC time.
  await expect(status).not.toHaveText('—', { timeout: 30000 });
  await expect(status).not.toHaveText('Checking add-on…');
  const checked = (await status.textContent())?.trim() ?? '';
  await expect(page.locator('#status .muted, #status p').filter({ hasText: /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} UTC$/ })).toHaveCount(1);
  expect(loads, 'Check must not reload the page').toBe(0);
  expect(sqlite(`SELECT "LastCheckAt" IS NOT NULL FROM "AddonInstallation" WHERE "AddonId"='${variant.addonId}';`)).toBe('1');
  await hydrated(page, `/settings/addons/${variant.addonId}`);
  await expect(page.locator('#status [role="status"]')).toHaveText(checked);

  // Saving configuration disables the add-on and asks for a new check, in place,
  // without discarding an unsaved edit in a sibling setting.
  loads = 0;
  const noteRow = page.getByTestId('addon-setting-row').filter({ hasText: 'Display note' });
  const note = noteRow.getByRole('textbox', { name: /Display note/ });
  await note.fill('Unsaved sibling edit');
  const edited = `${await storage.inputValue()}-edited`;
  await storage.fill(edited);
  await storageRow.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(storageRow.getByRole('status')).toHaveText('Saved. The add-on is disabled until you check and enable it again.', { timeout: 30000 });
  await expect(page.locator('#status [role="status"]')).toHaveText('Configuration changed. Check the add-on again before enabling it.');
  await expect(storage).toHaveValue(edited);
  await expect(storage).toBeEnabled();
  await expect(note).toHaveValue('Unsaved sibling edit');
  await expect(noteRow.getByRole('status')).toHaveText('');
  expect(sqlite(`SELECT COUNT(*) FROM "AddonSetting" WHERE "AddonId"='${variant.addonId}' AND "Key"='DISPLAY_NOTE';`)).toBe('0');
  expect(loads, 'Save must not reload the page').toBe(0);
  await expect.poll(() => sqlite(`SELECT "Value" FROM "AddonSetting" WHERE "AddonId"='${variant.addonId}' AND "Key"='MEDIA_STORAGE_PATH';`)).toBe(edited);
  expect(errors).toEqual([]);
});

test('bulk exclusion can be discarded or saved without JavaScript', async ({ page, browser, baseURL }, info) => {
  test.setTimeout(240000);
  if (!baseURL) throw new Error('Playwright baseURL is required');
  const variant = scenario(info, 'bulk-no-js', false, { itemCount: 2 });
  const listId = await importList(page, variant, 'stream', 'remote_streams');
  // Editing starts from the completed initial publication. Active publication
  // deliberately holds a write lease and rejects concurrent saved changes.
  await publishAndWait(page, listId, 1);
  const context = await browser.newContext({ baseURL, javaScriptEnabled: false });
  try {
    const plain = await context.newPage();
    expect((await plain.goto(`/media/addon-lists/${listId}`))?.status()).toBe(200);
    const submit = async (locator: ReturnType<typeof plain.locator>) => {
      await Promise.all([plain.waitForEvent('framenavigated', frame => frame === plain.mainFrame()), locator.click()]);
    };
    const selected = () => plain.getByRole('checkbox', { name: 'Selected for editing: Prepared calibration 2', exact: true });
    const excludedBadge = () => plain.locator('.episode-row').filter({ hasText: 'Prepared calibration 2' }).locator('.item-state-badge.is-excluded');
    const openBulk = async () => {
      if (!(await plain.getByRole('button', { name: 'Exclude selected items', exact: true }).isVisible())) {
        await plain.locator('.bulk-tools > summary').click();
      }
    };
    await selected().check();
    await openBulk();
    await submit(plain.getByRole('button', { name: 'Exclude selected items', exact: true }));
    // Retained input only: the row shows the pending exclusion, nothing is saved.
    await expect(excludedBadge()).toHaveCount(1);
    expect(includedItems(listId)).toBe('2');
    await expect(selected()).toBeChecked();

    await submit(plain.locator('button[name="intent"][value="discard"]').first());
    await expect(excludedBadge()).toHaveCount(0);
    expect(includedItems(listId)).toBe('2');
    await expect(selected()).toBeChecked();

    await openBulk();
    await submit(plain.getByRole('button', { name: 'Exclude selected items', exact: true }));
    await submit(plain.locator('button[name="intent"][value="save"]').first());
    await expect(plain.locator('#workspace-errors')).toHaveCount(0);
    expect(includedItems(listId)).toBe('1');
    await expect(excludedBadge()).toHaveCount(1);
    await expect(selected()).toBeChecked();
  } finally {
    await context.close();
  }
});

for (const javaScriptEnabled of [false, true]) {
  test(`processing option Save and Undo survive metadata progress (${javaScriptEnabled ? 'hydrated' : 'no JS'})`, async ({ page, browser, baseURL }, info) => {
    test.setTimeout(180000);
    const variant = scenario(info, `metadata-option-${javaScriptEnabled}`, false);
    const listId = await importList(page, variant, 'stream', 'remote_streams');
    await hydrated(page, `/media/addon-lists/${listId}`);
    await publishAndWait(page, listId, 1);
    const publishedBefore = sqlite(`SELECT "OptionValuesJson" FROM "AddonMediaListRevision" WHERE "AddonMediaListId"=${listId} ORDER BY "Revision" DESC LIMIT 1;`);
    const context = await browser.newContext({ baseURL, javaScriptEnabled });
    await context.request.post('/api/settings/ui/save', { form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0 });
    const editor = await context.newPage();
    await editor.goto(`/media/addon-lists/${listId}/playback`);
    if (javaScriptEnabled) await hydrated(editor);
    const initialOption = await editor.locator('[name="option_processed"]').inputValue();
    const draftOptionsBefore = sqlite(`SELECT COALESCE("DraftOptionSchemaId",0)||'|'||"DraftOptionValuesJson" FROM "AddonMediaList" WHERE "Id"=${listId};`);
    await expect(editor.locator('.fresh-cutoff-notice')).toBeHidden();
    await editor.locator('[name="delivery_mode"][value="download_ahead_new_videos"]').check();
    await expect(editor.locator('.fresh-cutoff-notice')).toBeVisible();
    await editor.locator('[name="delivery_mode"][value="stream"]').check();
    await expect(editor.locator('.fresh-cutoff-notice')).toBeHidden();
    await editor.locator('[name="option_processed"]').selectOption('true');
    sqlite(`UPDATE "AddonMediaList" SET "DraftRevision"="DraftRevision"+1 WHERE "Id"=${listId};`);
    const metadataRevision = Number(sqlite(`SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id"=${listId};`));
    await editor.locator('button[name="intent"][value="save"]').click();
    await editor.waitForLoadState('load');
    await expect(editor.locator('#workspace-errors')).toHaveCount(0);
    await expect(editor.locator('[name="option_processed"]')).toHaveValue('true');
    expect(sqlite(`SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id"=${listId};`)).toBe(String(metadataRevision + 1));
    expect(sqlite(`SELECT "OptionValuesJson" FROM "AddonMediaListRevision" WHERE "AddonMediaListId"=${listId} ORDER BY "Revision" DESC LIMIT 1;`)).toBe(publishedBefore);
    await editor.locator('button[name="intent"][value="undo-options"]').click();
    await editor.waitForLoadState('load');
    await expect(editor.locator('#workspace-errors')).toHaveCount(0);
    await expect(editor.locator('[name="option_processed"]')).toHaveValue(initialOption);
    expect(sqlite(`SELECT COALESCE("DraftOptionSchemaId",0)||'|'||"DraftOptionValuesJson" FROM "AddonMediaList" WHERE "Id"=${listId};`)).toBe(draftOptionsBefore);
    expect(sqlite(`SELECT "OptionValuesJson" FROM "AddonMediaListRevision" WHERE "AddonMediaListId"=${listId} ORDER BY "Revision" DESC LIMIT 1;`)).toBe(publishedBefore);
    // Undo must update its own option baseline without making the next Save stale.
    sqlite(`UPDATE "AddonMediaList" SET "DraftRevision"="DraftRevision"+1 WHERE "Id"=${listId};`);
    await editor.locator('[name="option_processed"]').selectOption('true');
    await editor.locator('button[name="intent"][value="save"]').click();
    await editor.waitForLoadState('load');
    await expect(editor.locator('#workspace-errors')).toHaveCount(0);
    await expect(editor.locator('[name="option_processed"]')).toHaveValue('true');
    expect(sqlite(`SELECT "OptionValuesJson" FROM "AddonMediaListRevision" WHERE "AddonMediaListId"=${listId} ORDER BY "Revision" DESC LIMIT 1;`)).toBe(publishedBefore);
    await context.close();
  });
}

test('166-entry overview reports overlapping preparation tasks separately from items', async ({ page }, info) => {
  test.setTimeout(120000);
  const variant = { addonId: `org.ersatzrs.e2e-walkthrough.${info.project.name}-overlapping`, processedDefault: true, itemCount: 166 };
  installPreparedFixture(dbPath!, false, true, variant);
  sqlite(`INSERT INTO "ConfigElement" ("Key","Value") VALUES('ffmpeg.preferred_subtitle_language_code','eng'),('ffmpeg.fallback_subtitle_language_code','eng')
    ON CONFLICT("Key") DO UPDATE SET "Value"=excluded."Value";`);
  const listId = await importList(page, variant, 'stream', 'remote_streams', `${preparedSourceUrl(variant)}?delivery=stream`);
  const unresolved = Number(sqlite(`SELECT MIN("Id") FROM "AddonMediaListItem" WHERE "AddonMediaListId"=${listId};`));
  // Hold automatic admission at a persisted terminal state for this counter-only
  // fixture, and supersede any work admitted before the overview was replaced.
  sqlite(`BEGIN IMMEDIATE;
    UPDATE "AddonMediaListItem" SET "DurationSeconds"=NULL,"ProviderLiveness"=0,"EffectiveLiveness"=0,"Availability"=2,"ProviderMetadataJson"='{"title":"Unresolved fixture"}' WHERE "Id"=${unresolved};
    UPDATE "AddonMediaList" SET "DraftRevision"="DraftRevision"+1 WHERE "Id"=${listId};
    UPDATE "AddonMediaListAutoPublication" SET "State"='held',"Reason"='"preparation_failed"',"DraftRevision"=(SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id"=${listId}) WHERE "AddonMediaListId"=${listId};
    UPDATE "AddonMediaListItemOperationJob" SET "State"=5 WHERE "AddonMediaListId"=${listId} AND "State" IN (0,1);
    COMMIT;`);
  await hydrated(page, `/media/addon-lists/${listId}`);
  // Items continues its counts line; Activity & recovery owns the full counters.
  const preparation = page.getByTestId('addon-preparation-tasks');
  // The worker can finish a task between the import and the hold above, and
  // a finished result stays complete. Tasks, not items, must add up: 165 items
  // with an interval and a subtitle task each are 330 tasks.
  await expect(preparation).toContainText(/Completed \d+\/165/);
  const counts = (await preparation.textContent()) ?? '';
  const completed = Number(/Completed (\d+)\/165/.exec(counts)?.[1]);
  const notQueued = Number(/Not queued (\d+)/.exec(counts)?.[1]);
  expect(completed + notQueued).toBe(330);
  await expect(page.locator('[name="category"] option[value="unknown"]')).toHaveText('Unknown (1)');
  // Stable durable states exercise Activity without racing or accelerating the worker.
  // The last item is the one the worker reaches last, so its tasks are open.
  const pendingItem = Number(sqlite(`SELECT MAX("Id") FROM "AddonMediaListItem" WHERE "AddonMediaListId"=${listId} AND "Id"<>${unresolved};`));
  sqlite(`INSERT INTO "AddonMediaListItemOperationJob"
    ("AddonMediaListId","AddonMediaListItemId","Kind","SavedDraftRevision","InputJson","ContextFingerprint","State","NextEligibleAt","LeaseExpiresAt","CreatedAt","UpdatedAt")
    SELECT ${listId},${pendingItem},2,"DraftRevision",'{}','activity-running',1,NULL,datetime('now','+1 hour'),datetime('now','-1 minute'),datetime('now') FROM "AddonMediaList" WHERE "Id"=${listId};
    INSERT INTO "AddonMediaListItemOperationJob"
    ("AddonMediaListId","AddonMediaListItemId","Kind","SavedDraftRevision","InputJson","ContextFingerprint","State","NextEligibleAt","CreatedAt","UpdatedAt")
    SELECT ${listId},${pendingItem},3,"DraftRevision",'{}','activity-waiting',0,datetime('now','+1 hour'),datetime('now','-1 minute'),datetime('now') FROM "AddonMediaList" WHERE "Id"=${listId};`);
  await page.locator('button[formaction$="/activity"]').first().click();
  await hydrated(page);
  const activity = page.getByTestId('addon-preparation-tasks');
  await expect(activity.getByText('Affected items', { exact: true }).locator('..').locator('dd')).toHaveText('165');
  await expect(activity.getByText('Interval / SponsorBlock tasks', { exact: true }).locator('..').locator('dd')).toHaveText('165');
  await expect(activity.getByText('Subtitle tasks', { exact: true }).locator('..').locator('dd')).toHaveText('165');
  await expect(activity.getByText('Download tasks', { exact: true }).locator('..').locator('dd')).toHaveText('0');
  await expect(activity.getByText('Running', { exact: true }).locator('..').locator('dd')).toHaveText('1');
  await expect(activity.getByText('Waiting', { exact: true }).locator('..').locator('dd')).toHaveText('1');
  const activityCount = async (label: string): Promise<number> =>
    Number(await activity.getByText(label, { exact: true }).locator('..').locator('dd').textContent());
  expect((await activityCount('Completed')) + (await activityCount('Not queued'))).toBe(328);
  expect(sqlite(`SELECT "Included" FROM "AddonMediaListItem" WHERE "Id"=${unresolved};`)).toBe('1');
});
