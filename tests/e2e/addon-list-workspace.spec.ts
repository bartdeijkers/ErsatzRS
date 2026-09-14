import { test, expect, Page } from '@playwright/test';
import { seedMetadataEditor, sqlite, sqlEscape } from './fixtures/addon-metadata';

const hydratedPages = new WeakSet<Page>();
async function settled(page: Page) {
  await page.waitForLoadState('load');
  if (hydratedPages.has(page)) await page.waitForFunction(() => (window as any).__ersatzrs_hydrated === true);
}

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

async function submit(page: Page, intent: string) {
  await settled(page);
  await Promise.all([
    page.waitForEvent('framenavigated', f => f === page.mainFrame()),
    page.locator(`button[name="intent"][value="${intent}"]`).first().click(),
  ]);
  await settled(page);
}

for (const javaScriptEnabled of [false, true]) {
  test.describe(`locked add-on list workspace (${javaScriptEnabled ? 'hydrated' : 'no JS'})`, () => {
    test.skip(!dbPath, 'An isolated E2E database is required');
    test('saves playback after metadata progress and retains a competing edit', async ({ browser, baseURL }, info) => {
      const seed = seedMetadataEditor(info);
      const context = await browser.newContext({ baseURL, javaScriptEnabled });
      await context.request.post('/api/settings/ui/save', { form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0 });
      const page = await context.newPage();
      const other = await context.newPage();
      if (javaScriptEnabled) { hydratedPages.add(page); hydratedPages.add(other); }
      await page.goto(`/media/addon-lists/${seed.listId}/playback`);
      await other.goto(`/media/addon-lists/${seed.listId}/playback`);
      await settled(page); await settled(other);
      await expect(page.locator('[data-workspace-save-status]')).toHaveText('Saved.');
      await page.locator('[name="disk_warning_gb"]').fill('42');
      if (javaScriptEnabled) await expect(page.locator('[data-workspace-save-status]')).toHaveText('Unsaved changes');
      await other.locator('[name="disk_warning_gb"]').fill('43');
      const published = sqlite(`SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id"=${seed.listId};`);
      sqlite(`UPDATE "AddonMediaList" SET "DraftRevision"="DraftRevision"+1 WHERE "Id"=${seed.listId};`);
      await submit(page, 'save');
      await expect(page.locator('#workspace-errors')).toHaveCount(0);
      await expect(page.locator('[data-workspace-save-status]')).toHaveText('Saved.');
      expect(sqlite(`SELECT "DiskUsageWarningGb" FROM "AddonMediaList" WHERE "Id"=${seed.listId};`)).toBe('42');
      expect(sqlite(`SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id"=${seed.listId};`)).toBe(published);
      await submit(other, 'save');
      await expect(other.locator('#workspace-errors')).toBeVisible();
      await expect(other.locator('[name="disk_warning_gb"]')).toHaveValue('43');
      expect(sqlite(`SELECT "DiskUsageWarningGb" FROM "AddonMediaList" WHERE "Id"=${seed.listId};`)).toBe('42');
      await context.close();
    });

    test('separates unknown entries and opens publication blockers directly', async ({ browser, baseURL }, info) => {
      const seed = seedMetadataEditor(info);
      sqlite(`UPDATE "AddonMediaListItem" SET "DurationSeconds"=NULL,"ProviderLiveness"=0,"EffectiveLiveness"=0 WHERE "Id"=${seed.secondItemId};`);
      const context = await browser.newContext({ baseURL, javaScriptEnabled, viewport: { width: 320, height: 800 } });
      await context.request.post('/api/settings/ui/save', { form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0 });
      const page = await context.newPage();
      if (javaScriptEnabled) hydratedPages.add(page);
      await page.goto(`/media/addon-lists/${seed.listId}`);
      await settled(page);
      const categories = page.locator('[name="category"]');
      await expect(categories.locator('option[value="unknown"]')).toHaveText('Unknown (1)');
      await expect(categories.locator('option[value="unavailable"]')).toHaveText('Unavailable (0)');
      await categories.selectOption('unknown');
      await submit(page, 'filter');
      await expect(page.locator('.episode-row')).toHaveCount(1);
      await expect(page.locator('.episode-row .runtime')).toContainText('Unknown');
      const blocker = page.locator('.notice.error button').filter({ hasText: seed.secondTitle }).first();
      await expect(blocker).toBeVisible();
      await blocker.click();
      await settled(page);
      await expect(page).toHaveURL(new RegExp(`/items/${seed.secondItemId}$`));
      expect(sqlite(`SELECT "Included" FROM "AddonMediaListItem" WHERE "Id"=${seed.secondItemId};`)).toBe('1');
      await context.close();
    });

    test('retains task input and commits only the selected task', async ({ browser, baseURL }, info) => {
      const seed = seedMetadataEditor(info);
      const context = await browser.newContext({ baseURL, javaScriptEnabled });
      await context.request.post('/api/settings/ui/save', { form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0 });
      const page = await context.newPage();
      if(javaScriptEnabled) hydratedPages.add(page);
      const errors: string[] = [];
      page.on('pageerror', error => errors.push(error.message));
      await page.goto(`/media/addon-lists/${seed.listId}/settings`);
      await settled(page);
      await expect(page.locator('.addon-workspace')).toBeVisible();
      await page.locator('[name="metadata_title"]').fill('Retained settings title');
      await page.locator('button[formaction$="/playback"]').click();
      await settled(page);
      await page.locator('[name="disk_warning_gb"]').fill('42');
      await submit(page, 'save');
      await expect(page.locator('#workspace-errors')).toHaveCount(0);
      expect(sqlite(`SELECT "DiskUsageWarningGb" FROM "AddonMediaList" WHERE "Id"=${seed.listId};`)).toBe('42');
      expect(sqlite(`SELECT json_extract("MetadataOverrideJson",'$.title') FROM "AddonMediaList" WHERE "Id"=${seed.listId};`)).toBe('Operator draft');
      await page.locator('button[formaction$="/settings"]').click();
      await settled(page);
      await expect(page.locator('[name="metadata_title"]')).toHaveValue('Retained settings title');
      await submit(page, 'save');
      expect(sqlite(`SELECT json_extract("MetadataOverrideJson",'$.title') FROM "AddonMediaList" WHERE "Id"=${seed.listId};`)).toBe('Retained settings title');
      expect(errors).toEqual([]);
      await context.close();
    });

    test('keeps adjacent buttons fixed and preserves item drafts through artwork', async ({ browser, baseURL }, info) => {
      const seed=seedMetadataEditor(info);
      const context=await browser.newContext({baseURL,javaScriptEnabled});
      const page=await context.newPage(); if(javaScriptEnabled) hydratedPages.add(page);
      await page.goto(`/media/addon-lists/${seed.listId}/items/${seed.firstItemId}`);
      await settled(page);
      const adjacent=page.locator('.item-adjacent');
      await expect(adjacent).toHaveCount(2);
      await expect(adjacent.first()).toBeDisabled();
      const nextBefore=await adjacent.last().boundingBox();
      await page.locator('[name="metadata_title"]').fill('Unsaved item title');
      await adjacent.last().click();
      await settled(page);
      const nextAfter=await adjacent.last().boundingBox();
      expect(nextAfter?.x).toBe(nextBefore?.x);
      await expect(adjacent.last()).toBeDisabled();
      await adjacent.first().click();
      await settled(page);
      await expect(page.locator('[name="metadata_title"]')).toHaveValue('Unsaved item title');
      await page.locator('button[formaction*="/artwork?"]').click();
      await settled(page);
      await expect(page.locator('[name="metadata_poster"]')).toBeVisible();
      await page.locator(`button[formaction$="/items/${seed.firstItemId}"]`).click();
      await settled(page);
      await expect(page.locator('[name="metadata_title"]')).toHaveValue('Unsaved item title');
      await submit(page,'save');
      await expect(page.locator('#workspace-errors')).toHaveCount(0);
      expect(sqlite(`SELECT json_extract("MetadataOverrideJson",'$.title') FROM "AddonMediaListItem" WHERE "Id"=${seed.firstItemId};`)).toBe('Unsaved item title');
      await context.close();
    });

    test('shows every page size and saves an exact off-page selection with undo', async ({browser,baseURL},info)=>{
      const seed=seedMetadataEditor(info);
      for(let i=2;i<165;i++) {
        const id=seed.secondItemId+i;
        const title=`Archive item ${String(i+1).padStart(3,'0')}`;
        sqlite(`INSERT INTO "AddonMediaListItem" ("Id","AddonMediaListId","ProviderId","DisplayTitle","Kind","Rank","Title","Included","ProviderMetadataJson") VALUES (${id},${seed.listId},'workspace-${i}','${sqlEscape(title)}',4,${i},'${sqlEscape(title)}',1,'{"title":"${sqlEscape(title)}"}');`);
      }
      const context=await browser.newContext({baseURL,javaScriptEnabled});const page=await context.newPage(); if(javaScriptEnabled) hydratedPages.add(page);
      await page.goto(`/media/addon-lists/${seed.listId}`);
      await settled(page);
      await expect(page.locator('.episode-row')).toHaveCount(10);
      expect(await page.locator('[name="page_size"] option').evaluateAll(options=>options.map(o=>(o as HTMLOptionElement).value))).toEqual(['10','25','50','100','0']);
      await page.locator(`input[type="checkbox"][name="selected_${seed.firstItemId}"]`).check();
      await submit(page,'page:1');
      const offPage=await page.locator('.episode-row').first().getAttribute('id');
      const target=Number(offPage!.slice(5));
      await page.locator(`input[type="checkbox"][name="selected_${target}"]`).check();
      await page.locator('.bulk-tools > summary').click();
      await page.locator('[name="bulk_season"]').fill('3');
      await page.locator('[name="bulk_episode"]').fill('1');
      await submit(page,'number');
      await expect(page.locator(`[name="season_${target}"]`)).toHaveValue('3');
      await submit(page,'undo');
      await expect(page.locator(`[name="season_${target}"]`)).toHaveValue('');
      await submit(page,'number');
      await submit(page,'save');
      await expect(page.locator('#workspace-errors')).toHaveCount(0);
      expect(sqlite(`SELECT COUNT(*) FROM "AddonMediaListItem" WHERE "AddonMediaListId"=${seed.listId} AND json_extract("MetadataOverrideJson",'$.season')=3;`)).toBe('2');
      await page.locator('[name="page_size"]').selectOption('0');await submit(page,'page-size');
      await expect(page.locator('.episode-row')).toHaveCount(165);
      await context.close();
    });

    test('paginates a thousand-item list without truncating All or exceeding the form bound',async({browser,baseURL},info)=>{
      test.setTimeout(120000);
      const seed=seedMetadataEditor(info);
      sqlite(`WITH RECURSIVE entries(n) AS (SELECT 2 UNION ALL SELECT n+1 FROM entries WHERE n<1000)
INSERT INTO "AddonMediaListItem" ("Id","AddonMediaListId","ProviderId","DisplayTitle","Kind","Rank","Title","Included")
SELECT ${seed.secondItemId}+n,${seed.listId},'large-'||n,'Large archive '||n,4,n,'Large archive '||n,1 FROM entries;
UPDATE "AddonMediaList" SET "ItemCount"=1001 WHERE "Id"=${seed.listId};`);
      const context=await browser.newContext({baseURL,javaScriptEnabled}),page=await context.newPage();if(javaScriptEnabled)hydratedPages.add(page);
      try {
        await page.goto(`/media/addon-lists/${seed.listId}`);await settled(page);
        for(const size of [10,25,50,100,0]) {
          await page.locator('[name="page_size"]').selectOption(String(size));await submit(page,'page-size');
          await expect(page.locator('.episode-row')).toHaveCount(size||1001);
        }
        const payloadBytes=await page.locator('#addon-list-workspace').evaluate(form=>new TextEncoder().encode(new URLSearchParams(new FormData(form as HTMLFormElement) as any).toString()).length);
        expect(payloadBytes).toBeLessThan(8*1024*1024);
        await info.attach('native-large-list-payload',{body:JSON.stringify({items:1001,payloadBytes,limitBytes:8*1024*1024}),contentType:'application/json'});
        const last=page.locator('.episode-row').last();
        await last.locator('[name^="season_"]').fill('2');await last.locator('[name^="episode_"]').fill('1');
        await submit(page,'save');await expect(page.locator('#workspace-errors')).toHaveCount(0);
        expect(sqlite(`SELECT COUNT(*) FROM "AddonMediaListItem" WHERE "AddonMediaListId"=${seed.listId} AND json_extract("MetadataOverrideJson",'$.season')=2`)).toBe('1');
      } finally {await context.close();}
    });

    test('saves artwork while preserving unsaved list metadata',async({browser,baseURL},info)=>{
      const seed=seedMetadataEditor(info);const context=await browser.newContext({baseURL,javaScriptEnabled});const page=await context.newPage();if(javaScriptEnabled)hydratedPages.add(page);
      const base=`/media/addon-lists/${seed.listId}`;
      await page.goto(`${base}/settings`);await settled(page);
      await page.locator('[name="metadata_title"]').fill('Title retained through artwork');
      await page.locator(`button[formaction="${base}/artwork"]`).click();await settled(page);
      await page.locator('[name="metadata_poster"]').fill('https://example.invalid/operator-poster.png');await submit(page,'save');
      await expect(page.locator('#workspace-errors')).toHaveCount(0);
      expect(sqlite(`SELECT json_extract("MetadataOverrideJson",'$.title') FROM "AddonMediaList" WHERE "Id"=${seed.listId}`)).toBe('Operator draft');
      await page.locator(`button[formaction="${base}/settings"]`).last().click();await settled(page);
      await expect(page.locator('[name="metadata_title"]')).toHaveValue('Title retained through artwork');await submit(page,'save');
      await expect(page.locator('#workspace-errors')).toHaveCount(0);
      const saved=JSON.parse(sqlite(`SELECT "MetadataOverrideJson" FROM "AddonMediaList" WHERE "Id"=${seed.listId}`));
      expect(saved.title).toBe('Title retained through artwork');expect(saved.artwork.some((image:{url:string})=>image.url==='https://example.invalid/operator-poster.png')).toBe(true);
      await context.close();
    });

    test('saves chapters atomically and restores provider chapters without editing derived items',async({browser,baseURL},info)=>{
      const seed=seedMetadataEditor(info),provider='0:00 Introduction\n5:00 Provider chapter';
      sqlite(`UPDATE "AddonMediaListItem" SET "ProviderChapterInput"='${sqlEscape(provider)}' WHERE "Id"=${seed.firstItemId};
CREATE TRIGGER "workspace_chapters_${seed.listId}" AFTER INSERT ON "AddonMediaListLifecycleJob" WHEN NEW."AddonMediaListId"=${seed.listId} BEGIN UPDATE "ManagedLibraryOperation" SET "NextEligibleAt"='2099-01-01T00:00:00Z' WHERE "Id"=NEW."OperationId"; END;`);
      const context=await browser.newContext({baseURL,javaScriptEnabled}),page=await context.newPage();if(javaScriptEnabled)hydratedPages.add(page);
      try {
        await page.goto(`/media/addon-lists/${seed.listId}/items/${seed.firstItemId}`);await settled(page);
        await page.locator('details').filter({has:page.locator('[name="chapter_input"]')}).locator('summary').click();
        await expect(page.locator('[name="chapter_input"]')).toHaveValue(provider);
        await page.locator('[name="metadata_title"]').fill('Title and chapters together');
        await page.locator('[name="chapter_input"]').fill('invalid chapter');await submit(page,'save');
        await expect(page.locator('#workspace-errors')).toBeFocused();
        await expect(page.locator('[name="chapter_input"]')).toHaveAttribute('aria-invalid','true');
        expect(sqlite(`SELECT "MetadataOverrideJson" FROM "AddonMediaListItem" WHERE "Id"=${seed.firstItemId}`)).toBe('{}');
        await page.locator('[name="chapter_input"]').fill('0:00 Introduction\n10:00 Operator chapter');await submit(page,'save');
        await expect(page.locator('#workspace-errors')).toHaveCount(0);
        expect(sqlite(`SELECT json_extract("MetadataOverrideJson",'$.title') || '|' || "ChapterInput" FROM "AddonMediaListItem" WHERE "Id"=${seed.firstItemId}`).replace(/\r\n/g,'\n')).toBe('Title and chapters together|0:00 Introduction\n10:00 Operator chapter');
        expect(Number(sqlite(`SELECT COUNT(*) FROM "AddonMediaListLifecycleJob" WHERE "AddonMediaListId"=${seed.listId}`))).toBeGreaterThan(0);
        await page.locator('details').filter({has:page.locator('[name="chapter_input"]')}).locator('summary').click();
        await submit(page,'restore-chapters');
        expect(sqlite(`SELECT "ChapterInput" FROM "AddonMediaListItem" WHERE "Id"=${seed.firstItemId}`)).toContain('Operator chapter');
        await submit(page,'save');await expect(page.locator('#workspace-errors')).toHaveCount(0);
        await expect(page.locator('[name="chapter_input"]')).toHaveValue(provider);
        expect(sqlite(`SELECT COALESCE("ChapterInput",'') FROM "AddonMediaListItem" WHERE "Id"=${seed.firstItemId}`)).toBe('');
        sqlite(`UPDATE "AddonMediaListItem" SET "ParentProviderId"='item:parent' WHERE "Id"=${seed.secondItemId}`);
        await page.locator('.item-adjacent').last().click();await settled(page);
        await expect(page.locator('[name="chapter_input"]')).toHaveCount(0);
        await expect(page.locator('button[value="restore-chapters"]')).toHaveCount(0);
      } finally {sqlite(`DROP TRIGGER "workspace_chapters_${seed.listId}"`);await context.close();}
    });

    test('retains selected upload bytes across navigation and saves artwork coherently',async({browser,baseURL},info)=>{
      const seed=seedMetadataEditor(info);const context=await browser.newContext({baseURL,javaScriptEnabled});const page=await context.newPage();if(javaScriptEnabled)hydratedPages.add(page);
      const base=`/media/addon-lists/${seed.listId}`;
      const before=sqlite(`SELECT "DraftRevision" || ':' || "MetadataOverrideJson" FROM "AddonMediaList" WHERE "Id"=${seed.listId}`);
      await page.goto(`${base}/artwork`);await settled(page);
      const png=Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII=','base64');
      await page.locator('[name="upload_poster"]').setInputFiles({name:'poster.png',mimeType:'image/png',buffer:png});
      await page.locator('[name="upload_fanart"]').setInputFiles({name:'fanart.png',mimeType:'image/png',buffer:png});
      await page.locator('button[formaction$="/playback"]').click();await settled(page);
      expect(sqlite(`SELECT "DraftRevision" || ':' || "MetadataOverrideJson" FROM "AddonMediaList" WHERE "Id"=${seed.listId}`)).toBe(before);
      await page.locator('button[formaction$="/settings"]').click();await settled(page);
      await page.locator(`button[formaction="${base}/artwork"]`).click();await settled(page);
      const poster=await page.locator('[name="metadata_poster"]').inputValue();
      const fanart=await page.locator('[name="metadata_fanart"]').inputValue();
      expect(poster).toMatch(/^ersatzrs-upload:\/\/\d+$/);expect(fanart).not.toBe(poster);
      const preview=await page.request.get(`/api/addon-media-lists/artwork/${poster.split('//')[1]}`);
      expect(preview.ok()).toBe(true);expect(await preview.body()).toEqual(png);
      await submit(page,'save');await expect(page.locator('#workspace-errors')).toHaveCount(0);
      const saved=JSON.parse(sqlite(`SELECT "MetadataOverrideJson" FROM "AddonMediaList" WHERE "Id"=${seed.listId}`));
      expect(saved.artwork.map((a:{url:string})=>a.url)).toEqual(expect.arrayContaining([poster,fanart]));
      expect(Number(sqlite(`SELECT "DraftRevision" FROM "AddonMediaList" WHERE "Id"=${seed.listId}`))).toBe(Number(before.split(':')[0])+1);
      await context.close();
    });

    test('keeps clean rows current and rejects conflicting retained numbering',async({browser,baseURL},info)=>{
      const seed=seedMetadataEditor(info);const context=await browser.newContext({baseURL,javaScriptEnabled});const page=await context.newPage();if(javaScriptEnabled)hydratedPages.add(page);
      const base=`/media/addon-lists/${seed.listId}`;
      await page.goto(base);await settled(page);
      await page.locator(`button[formaction$="/items/${seed.firstItemId}"]`).click();await settled(page);
      await page.locator('[name="numbering_mode"][value="numbered"]').check();
      await page.locator('[name="season"]').fill('5');await page.locator('[name="episode"]').fill('5');await submit(page,'save');
      await page.locator(`.item-navigation button[formaction="${base}#item-${seed.firstItemId}"]`).click();await settled(page);
      await expect(page.locator(`[name="season_${seed.firstItemId}"]`)).toHaveValue('5');
      await page.locator(`[name="season_${seed.firstItemId}"]`).fill('3');
      await page.locator(`button[formaction$="/items/${seed.firstItemId}"]`).click();await settled(page);
      await page.locator('[name="season"]').fill('7');await submit(page,'save');
      await page.locator(`.item-navigation button[formaction="${base}#item-${seed.firstItemId}"]`).click();await settled(page);
      await submit(page,'save');await expect(page.locator('#workspace-errors')).toBeVisible();
      await expect(page.locator(`[name="season_${seed.firstItemId}"]`)).toHaveValue('3');
      expect(sqlite(`SELECT json_extract("MetadataOverrideJson",'$.season') FROM "AddonMediaListItem" WHERE "Id"=${seed.firstItemId}`)).toBe('7');
      await context.close();
    });

    test('retains invalid input and refuses a stale task without partial writes',async({browser,baseURL},info)=>{
      const seed=seedMetadataEditor(info);const context=await browser.newContext({baseURL,javaScriptEnabled});const page=await context.newPage(); if(javaScriptEnabled) hydratedPages.add(page);
      await page.goto(`/media/addon-lists/${seed.listId}/items/${seed.firstItemId}`);
      await settled(page);
      await page.locator('[name="metadata_title"]').fill('Must remain unsaved');
      await page.locator('[name="numbering_mode"][value="numbered"]').check();
      await page.locator('[name="season"]').fill('2');
      await submit(page,'save');await expect(page.locator('#workspace-errors')).toBeVisible();
      await expect(page.locator('[name="metadata_title"]')).toHaveValue('Must remain unsaved');
      expect(sqlite(`SELECT "MetadataOverrideJson" FROM "AddonMediaListItem" WHERE "Id"=${seed.firstItemId};`)).toBe('{}');
      await page.locator('[name="episode"]').fill('1');
      sqlite(`UPDATE "AddonMediaList" SET "DraftRevision"="DraftRevision"+1 WHERE "Id"=${seed.listId};`);
      await submit(page,'save');await expect(page.locator('#workspace-errors')).toBeVisible();
      expect(sqlite(`SELECT "MetadataOverrideJson" FROM "AddonMediaListItem" WHERE "Id"=${seed.firstItemId};`)).toBe('{}');
      await expect(page.locator('#workspace-errors')).toBeFocused();
      await submit(page,'review-current');
      await expect(page.locator('[name="metadata_title"]')).toHaveValue('Must remain unsaved');
      await submit(page,'save');
      await expect(page.locator('#workspace-errors')).toHaveCount(0);
      expect(JSON.parse(sqlite(`SELECT "MetadataOverrideJson" FROM "AddonMediaListItem" WHERE "Id"=${seed.firstItemId};`)).title).toBe('Must remain unsaved');
      await context.close();
    });
  });
}
