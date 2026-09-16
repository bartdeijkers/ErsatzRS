import { expect, Locator, Page, test } from '@playwright/test';
import { seedOptions, sqlite } from './fixtures/addon-options';
const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

type Seed = ReturnType<typeof seedOptions>;

function publishedSnapshot(seed: Seed): string {
  return sqlite(`SELECT "list"."PublishedRevision" || char(31) || "revision"."OptionValuesJson" || char(31) ||
    (SELECT group_concat("ProviderId" || ':' || "OptionOverridesJson", ';') FROM
      (SELECT * FROM "AddonMediaListRevisionItemOption" WHERE "AddonMediaListRevisionId" = ${seed.revisionId} ORDER BY "ProviderId"))
    FROM "AddonMediaList" AS "list" JOIN "AddonMediaListRevision" AS "revision"
      ON "revision"."Id" = ${seed.revisionId} WHERE "list"."Id" = ${seed.listId}`);
}

function defaults(seed: Seed): Record<string, unknown> {
  return JSON.parse(sqlite(`SELECT "DraftOptionValuesJson" FROM "AddonMediaList" WHERE "Id" = ${seed.listId}`));
}

function overrides(itemId: number): Record<string, unknown> {
  return JSON.parse(sqlite(`SELECT "DraftOptionOverridesJson" FROM "AddonMediaListItem" WHERE "Id" = ${itemId}`));
}

function option(page: Page, key: string): Locator {
  return page.getByTestId('addon-list-options').locator(`[data-option-key="${key}"]`);
}

test.describe('add-on list draft options', () => {
  test.skip(!dbPath, 'E2E_DB_PATH or ERSATZRS_DB_PATH is required');
  for (const javaScriptEnabled of [true,false]) {
    test(`access checks preserve drafts and cancel the owned job (${javaScriptEnabled ? 'hydrated' : 'no JS'})`,async({browser,baseURL},info)=>{
      const seed=seedOptions(true),published=publishedSnapshot(seed);
      const host=`options-${seed.listId}.example.invalid`;
      const cooldown=new Date(Date.now()+10*60*1000).toISOString();
      sqlite(`UPDATE "AddonMediaListItem" SET "SourceUrl"='https://${host}/item/' || "Id" WHERE "AddonMediaListId"=${seed.listId};
INSERT INTO "AddonProviderHostCooldown" ("ProviderHost","NextEligibleAt","SampledDelaySeconds","LastAttemptAt","UpdatedAt") VALUES ('${host}','${cooldown}',5,'2026-09-09 00:00:00','2026-09-09 00:00:00');`);
      const context=await browser.newContext({baseURL,javaScriptEnabled}),page=await context.newPage();
      const settle=async()=>{await page.waitForLoadState('load');if(javaScriptEnabled)await page.waitForFunction(()=>(window as any).__ersatzrs_hydrated===true);};
      const submit=async(intent:string)=>{await Promise.all([page.waitForEvent('framenavigated',f=>f===page.mainFrame()),page.locator(`button[name="intent"][value="${intent}"]:not([formaction])`).first().click()]);await settle();await expect(page.locator('#workspace-errors')).toHaveCount(0);};
      try {
        await page.request.post('/api/settings/ui/save',{form:{is_dark_mode:'true',language:'en-us'},maxRedirects:0});
        await page.goto(`/media/addon-lists/${seed.listId}/playback`);await settle();
        await expect(page.locator('button[value="access-test"]')).toBeDisabled();
        await option(page,'browser').locator('select').selectOption('"firefox"');await submit('save');
        const revision=sqlite(`SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id"=${seed.listId}`);
        await option(page,'browser').locator('select').selectOption('"chrome"');
        await submit('access-test');
        const job=sqlite(`SELECT "Id" FROM "AddonMediaListItemOperationJob" WHERE "AddonMediaListId"=${seed.listId} AND "Kind"=0 ORDER BY "Id" DESC LIMIT 1`);
        expect(Number(job)).toBeGreaterThan(0);
        await expect(page.locator('button[value="access-test"]')).toBeDisabled();
        await expect(option(page,'browser').locator('select')).toHaveValue('"chrome"');
        expect(defaults(seed)).toEqual({browser:'firefox'});
        // Wait until the worker has parked behind the provider cooldown and
        // released its process lease before testing cancellation and retry.
        await expect.poll(()=>sqlite(`SELECT "State" || '|' || COALESCE("LeaseOwner",'') || '|' || CASE WHEN "NextEligibleAt" IS NULL THEN 'missing' ELSE 'scheduled' END FROM "AddonMediaListItemOperationJob" WHERE "Id"=${job}`),{timeout:20000}).toBe('0||scheduled');
        await submit('access-cancel');
        expect(sqlite(`SELECT "State" FROM "AddonMediaListItemOperationJob" WHERE "Id"=${job}`)).toBe('4');
        await expect(page.locator('button[value="access-test"]')).toBeEnabled();
        await submit('access-test');
        const retried=sqlite(`SELECT "Id" FROM "AddonMediaListItemOperationJob" WHERE "AddonMediaListId"=${seed.listId} AND "Kind"=0 ORDER BY "Id" DESC LIMIT 1`);
        expect(Number(retried)).toBeGreaterThan(Number(job));
        await expect.poll(()=>sqlite(`SELECT "State" || '|' || COALESCE("LeaseOwner",'') || '|' || CASE WHEN "NextEligibleAt" IS NULL THEN 'missing' ELSE 'scheduled' END FROM "AddonMediaListItemOperationJob" WHERE "Id"=${retried}`),{timeout:20000}).toBe('0||scheduled');await submit('access-cancel');
        expect(sqlite(`SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id"=${seed.listId}`)).toBe(revision);
        expect(publishedSnapshot(seed)).toBe(published);
      } finally {await context.close();}
    });
    test(`inheritance, published isolation and bulk undo (${javaScriptEnabled ? 'hydrated' : 'no JS'})`,async({browser,baseURL},info)=>{
      test.setTimeout(120_000);
      const seed=seedOptions();const published=publishedSnapshot(seed);
      expect(published).toContain('"sponsorblock_remove":false');
      const context=await browser.newContext({baseURL,javaScriptEnabled});const page=await context.newPage();
      const runtimeErrors:string[]=[];
      page.on('pageerror',error=>runtimeErrors.push(error.stack??error.message));
      const settle=async()=>{await page.waitForLoadState('load');if(javaScriptEnabled)await page.waitForFunction(()=>(window as any).__ersatzrs_hydrated===true);};
      const visit=async(url:string)=>{await page.goto(url);await settle();await expect(page.locator('[data-not-found]')).toHaveCount(0);};
      const submit=async(intent:string)=>{await Promise.all([page.waitForEvent('framenavigated',f=>f===page.mainFrame()),page.locator(`button[name="intent"][value="${intent}"]`).first().click()]);await settle();await expect(page.locator('#workspace-errors')).toHaveCount(0);};
      const save=()=>submit('save');
      const listUrl=`/media/addon-lists/${seed.listId}`;const playback=`${listUrl}/playback`;const firstUrl=`${listUrl}/items/${seed.firstItemId}`;
      try {
        await page.request.post('/api/settings/ui/save',{form:{is_dark_mode:'true',language:'en-us'},maxRedirects:0});
        await visit(playback);
        for (const theme of ['dark', 'light']) {
          await page.request.post('/api/settings/ui/save',{form:{is_dark_mode:String(theme==='dark'),language:'en-us'},maxRedirects:0});
          await visit(playback);
          for (const width of [390,3840]) {
            await page.setViewportSize({width,height:1080});
            if(width<768 && javaScriptEnabled) await expect.poll(()=>page.locator('[data-nav-drawer]').evaluate(e=>e.getBoundingClientRect().right)).toBeLessThanOrEqual(1);
            // Measure one layout frame: desktop drawer expansion can move the
            // whole stack between separate asynchronous boundingBox requests.
            const { access, browserBox, segmentBox } = await page.locator('.access-stack').evaluate(stack => ({
              access: stack.getBoundingClientRect().toJSON(),
              browserBox: stack.querySelector('[data-option-key="browser"]')!.getBoundingClientRect().toJSON(),
              segmentBox: stack.querySelector('[data-option-key="sponsorblock_remove"]')!.getBoundingClientRect().toJSON(),
            }));
            expect(access!.width).toBeLessThanOrEqual(768);
            expect(segmentBox!.y).toBeGreaterThanOrEqual(browserBox!.y+browserBox!.height);
            expect(Math.abs(browserBox!.x-segmentBox!.x)).toBeLessThanOrEqual(1);
            expect(await page.evaluate(()=>document.documentElement.scrollWidth<=document.documentElement.clientWidth+1)).toBe(true);
            await page.screenshot({path:info.outputPath(`access-${theme}-${width}.png`),fullPage:true,animations:'disabled'});
          }
        }
        await page.setViewportSize({width:1440,height:1080});
        await expect(option(page,'browser').locator('select')).toHaveValue('');
        await expect(option(page,'sponsorblock_remove').locator('select')).toHaveValue('');
        expect(defaults(seed)).toEqual({});
        expect(sqlite(`SELECT COALESCE("DraftOptionSchemaId",0) FROM "AddonMediaList" WHERE "Id"=${seed.listId}`)).toBe('0');
        await option(page,'browser').locator('select').selectOption('"firefox"');await save();
        expect(defaults(seed)).toEqual({browser:'firefox'});expect(publishedSnapshot(seed)).toBe(published);
        await submit('undo-options');expect(defaults(seed)).toEqual({});
        expect(sqlite(`SELECT COALESCE("DraftOptionSchemaId",0) FROM "AddonMediaList" WHERE "Id"=${seed.listId}`)).toBe('0');
        // Undo refreshes its option baseline, so later metadata progress is disjoint.
        sqlite(`UPDATE "AddonMediaList" SET "DraftRevision"="DraftRevision"+1 WHERE "Id"=${seed.listId}`);
        await option(page,'browser').locator('select').selectOption('"firefox"');await save();
        await visit(firstUrl);
        await expect(option(page,'browser')).toContainText('From list');
        await expect(option(page,'browser').locator('select')).toHaveValue('');
        await expect(option(page,'browser')).toContainText('firefox');
        await option(page,'browser').locator('select').selectOption('"chrome"');await save();
        expect(overrides(seed.firstItemId)).toEqual({browser:'chrome'});
        await expect(option(page,'browser')).toContainText('Item override');
        await option(page,'browser').locator('select').selectOption('');await save();
        expect(overrides(seed.firstItemId)).toEqual({});
        await expect(option(page,'browser')).toContainText('firefox');
        await option(page,'sponsorblock_remove').locator('select').selectOption('false');await save();
        expect(overrides(seed.firstItemId)).toEqual({sponsorblock_remove:false});
        await option(page,'sponsorblock_remove').locator('select').selectOption('');await save();
        expect(overrides(seed.firstItemId)).toEqual({});
        // Explicit equality to the package default remains an override.
        await option(page,'sponsorblock_remove').locator('select').selectOption('true');await save();
        expect(overrides(seed.firstItemId)).toEqual({sponsorblock_remove:true});
        await option(page,'sponsorblock_remove').locator('select').selectOption('');
        await option(page,'browser').locator('select').selectOption('"chrome"');await save();
        await visit(listUrl);await page.locator('.bulk-tools > summary').click();
        await page.locator('[name="bulk_scope"]').selectOption('all');await submit('bulk-options');
        await expect(option(page,'browser').locator('select')).toHaveValue('__unchanged');
        await option(page,'browser').locator('select').selectOption('"chromium"');await submit('preview-options');
        const preview=page.getByTestId('option-preview');await expect(preview).toBeVisible();await expect(preview.locator('tbody tr')).toHaveCount(2);
        await expect(preview).toContainText('Second option item');
        expect(overrides(seed.firstItemId)).toEqual({browser:'chrome'});expect(overrides(seed.secondItemId)).toEqual({});
        await save();expect(overrides(seed.firstItemId)).toEqual({browser:'chromium'});expect(overrides(seed.secondItemId)).toEqual({browser:'chromium'});
        expect(publishedSnapshot(seed)).toBe(published);
        await submit('undo-options');expect(overrides(seed.firstItemId)).toEqual({browser:'chrome'});expect(overrides(seed.secondItemId)).toEqual({});
        expect(defaults(seed)).toEqual({browser:'firefox'});
        await visit(playback);await option(page,'browser').locator('select').selectOption('');await save();expect(defaults(seed)).toEqual({});
        await visit(firstUrl);await expect(option(page,'browser').locator('select')).toHaveValue('"chrome"');
        await visit(`${listUrl}/items/${seed.secondItemId}`);await expect(option(page,'browser').locator('select')).toHaveValue('');await expect(option(page,'browser')).toContainText('None');
        expect(publishedSnapshot(seed)).toBe(published);expect(runtimeErrors).toEqual([]);
      } finally {await context.close();}
    });
  }
});
