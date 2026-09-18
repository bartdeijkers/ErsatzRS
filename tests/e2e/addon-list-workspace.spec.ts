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

/** Picks a filter category or page size. With JavaScript the choice applies
 *  itself; without it the native button submits the draft. */
async function choose(page: Page, name: 'category' | 'page_size', value: string) {
  await settled(page);
  const select = page.locator(`[name="${name}"]`);
  if (hydratedPages.has(page) && await select.inputValue() !== value) {
    await Promise.all([
      page.waitForEvent('framenavigated', f => f === page.mainFrame()),
      select.selectOption(value),
    ]);
    await settled(page);
  } else {
    await select.selectOption(value);
    await submit(page, name === 'category' ? 'filter' : 'page-size');
  }
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
      await choose(page, 'category', 'unknown');
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
      await choose(page,'page_size','0');
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
          await choose(page,'page_size',String(size));
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

    test('renames the list from the Items heading',async({browser,baseURL},info)=>{
      const seed=seedMetadataEditor(info);const context=await browser.newContext({baseURL,javaScriptEnabled});const page=await context.newPage();if(javaScriptEnabled)hydratedPages.add(page);
      const base=`/media/addon-lists/${seed.listId}`;
      await page.goto(base);await settled(page);
      const title=page.locator('h1 #workspace-metadata_title');
      await expect(title).toBeVisible();
      const saveButton=page.locator('button[data-save-indicator]');
      await expect(saveButton).toHaveAttribute('data-dirty','false');
      await expect(saveButton).toHaveClass(/secondary/);
      await title.fill('Renamed from Items');
      if(javaScriptEnabled){
        await expect(page.locator('[data-workspace-save-status]')).toHaveText('Unsaved changes');
        await expect(saveButton).toHaveClass(/primary/);
      }
      // A native round trip keeps the unsaved title.
      await submit(page,'filter');
      await expect(title).toHaveValue('Renamed from Items');
      // A retained draft makes the Save button primary without JavaScript too.
      await expect(saveButton).toHaveAttribute('data-dirty','true');
      await submit(page,'save');
      await expect(page.locator('#workspace-errors')).toHaveCount(0);
      await expect(title).toHaveValue('Renamed from Items');
      await expect(page.locator('[data-workspace-save-status]')).toHaveText('Saved.');
      await expect(saveButton).toHaveAttribute('data-dirty','false');
      await expect(page.locator('.notice.success')).toHaveCount(0);
      await page.goto(`${base}/settings`);await settled(page);
      await expect(page.locator('[name="metadata_title"]')).toHaveValue('Renamed from Items');
      await context.close();
    });

    test('marks edited rows and opens items in a new tab without losing the draft',async({browser,baseURL},info)=>{
      const seed=seedMetadataEditor(info);const context=await browser.newContext({baseURL,javaScriptEnabled});
      await context.request.post('/api/settings/ui/save',{form:{is_dark_mode:'true',language:'nl'},maxRedirects:0});
      try{
      const page=await context.newPage();if(javaScriptEnabled)hydratedPages.add(page);
      const base=`/media/addon-lists/${seed.listId}`;
      await page.goto(base);await settled(page);
      const row=page.locator(`#item-${seed.firstItemId}`);
      // Fixed S/E symbols in every language; the accessible names stay localized.
      await expect(row.locator('.inline-pair label > span[aria-hidden="true"]')).toHaveText(['S','E']);
      await expect(row.locator(`[name="episode_${seed.firstItemId}"]`)).toHaveAttribute('aria-label',/^Aflevering: /);
      await expect(row).not.toHaveClass(/row-dirty/);
      const season=row.locator(`[name="season_${seed.firstItemId}"]`);
      await season.fill('9');
      if(javaScriptEnabled){
        await expect(row).toHaveClass(/row-dirty/);await expect(row.locator('.row-dirty-note')).toBeVisible();
      }
      // A retained draft stays marked after a native round trip.
      await submit(page,'filter');
      await expect(season).toHaveValue('9');
      await expect(row).toHaveClass(/row-dirty/);await expect(row.locator('.row-dirty-note')).toBeVisible();
      const link=row.locator('a.row-open-tab');
      await expect(link).toHaveAttribute('href',`${base}/items/${seed.firstItemId}`);
      await expect(link).toHaveAttribute('target','_blank');
      }finally{
        // The UI language is global; later tests expect English.
        await context.request.post('/api/settings/ui/save',{form:{is_dark_mode:'true',language:'en-us'},maxRedirects:0});
        await context.close();
      }
    });

    test('reviews bulk numbering beside its panels and applies or cancels it',async({browser,baseURL},info)=>{
      const seed=seedMetadataEditor(info);const context=await browser.newContext({baseURL,javaScriptEnabled});
      await context.request.post('/api/settings/ui/save',{form:{is_dark_mode:'true',language:'en-us'},maxRedirects:0});
      try{
      const page=await context.newPage();if(javaScriptEnabled)hydratedPages.add(page);
      await page.goto(`/media/addon-lists/${seed.listId}`);await settled(page);
      await page.locator('.bulk-tools > summary').click();
      // Three numbering panels, each holding only its own inputs.
      const panels=page.locator('.bulk-numbering-panels > .bulk-panel');
      await expect(panels.locator('h3')).toHaveText(['Items to change','Number episodes','Detect seasons']);
      for(const name of ['bulk_scope','start_item','end_item','minimum_length'])await expect(panels.nth(0).locator(`[name="${name}"]`)).toHaveCount(1);
      for(const name of ['bulk_season','bulk_episode','bulk_count'])await expect(panels.nth(1).locator(`[name="${name}"]`)).toHaveCount(1);
      for(const name of ['detect_season','gap_days'])await expect(panels.nth(2).locator(`[name="${name}"]`)).toHaveCount(1);
      const first=page.locator(`[name="season_${seed.firstItemId}"]`),second=page.locator(`[name="season_${seed.secondItemId}"]`);
      const saved=()=>sqlite(`SELECT group_concat(COALESCE(json_extract("MetadataOverrideJson",'$.season'),'-') || ',' || COALESCE(json_extract("MetadataOverrideJson",'$.episode'),'-'),';') FROM (SELECT * FROM "AddonMediaListItem" WHERE "Id" IN (${seed.firstItemId},${seed.secondItemId}) ORDER BY "Id")`);
      const before=saved();
      // An unsaved row edit made before the proposal is part of what Apply saves.
      await second.fill('5');await page.locator(`[name="episode_${seed.secondItemId}"]`).fill('5');
      await page.locator('[name="bulk_scope"]').selectOption('all');
      await submit(page,'number');
      const review=page.getByTestId('bulk-preview');
      await expect(review).toBeFocused();
      await expect(review.locator('tbody tr')).toHaveCount(2);
      await expect(review.locator('button[value="save"]')).toHaveText('Apply numbering changes');
      await expect(page.locator('.numbering-save-bar button[value="undo"]')).toHaveCount(0);
      // Cancel restores the draft from before the proposal and saves nothing.
      await submit(page,'undo');
      await expect(review).toHaveCount(0);
      await expect(first).toHaveValue('');await expect(second).toHaveValue('5');
      expect(saved()).toBe(before);
      // Season detection starts from its own first season, not the manual one.
      await page.locator('[name="detect_season"]').fill('3');
      await submit(page,'auto-number');
      await expect(review.locator('tbody')).toContainText('Season: 3');
      await submit(page,'undo');
      await expect(second).toHaveValue('5');
      await submit(page,'number');
      await Promise.all([page.waitForEvent('framenavigated',f=>f===page.mainFrame()),review.locator('button[value="save"]').click()]);
      await settled(page);
      await expect(page.locator('#workspace-errors')).toHaveCount(0);
      await expect(review).toHaveCount(0);
      expect(saved()).toBe('1,1;5,5');
      }finally{await context.close();}
    });

    test('shows and clears the active filter and jumps to the first and last page',async({browser,baseURL},info)=>{
      const seed=seedMetadataEditor(info);
      for(let i=2;i<25;i++){
        const id=seed.secondItemId+i;
        sqlite(`INSERT INTO "AddonMediaListItem" ("Id","AddonMediaListId","ProviderId","DisplayTitle","Kind","Rank","Title","Included","ProviderMetadataJson") VALUES (${id},${seed.listId},'pager-${i}','Pager item ${i}',4,${i},'Pager item ${i}',1,'{"title":"Pager item ${i}"}');`);
      }
      const context=await browser.newContext({baseURL,javaScriptEnabled});
      await context.request.post('/api/settings/ui/save',{form:{is_dark_mode:'true',language:'en-us'},maxRedirects:0});
      try{
      const page=await context.newPage();if(javaScriptEnabled)hydratedPages.add(page);
      await page.goto(`/media/addon-lists/${seed.listId}`);await settled(page);
      const active=page.getByTestId('active-filter');
      await expect(active).toHaveCount(0);
      await expect(page.locator('.page-position')).toHaveText('Page 1 of 3');
      await submit(page,'page:2');
      await expect(page.locator('.page-position')).toHaveText('Page 3 of 3');
      await expect(page.locator('button[name="intent"]').filter({hasText:'Last page'})).toBeDisabled();
      await submit(page,'page:0');
      await expect(page.locator('.page-position')).toHaveText('Page 1 of 3');
      // An applied category and search show as chips with one Clear action.
      await page.locator('#workspace-query').fill('Pager item 1');
      await choose(page,'category','unnumbered');
      await expect(active).toContainText('Unnumbered');
      await expect(active).toContainText('\u201cPager item 1\u201d');
      await expect(page.locator('.episode-row')).toHaveCount(10);
      if(javaScriptEnabled)await expect(page.locator('[name="category"]')).toBeFocused();
      await submit(page,'clear-filter');
      await expect(active).toHaveCount(0);
      await expect(page.locator('[name="category"]')).toHaveValue('');
      await expect(page.locator('#workspace-query')).toHaveValue('');
      await expect(page.locator('.page-position')).toHaveText('Page 1 of 3');
      }finally{await context.close();}
    });

    test('offers Save item at the top of the item page and hides numbers when unnumbered',async({browser,baseURL},info)=>{
      const seed=seedMetadataEditor(info);
      const context=await browser.newContext({baseURL,javaScriptEnabled,viewport:{width:1440,height:700}});
      await context.request.post('/api/settings/ui/save',{form:{is_dark_mode:'true',language:'en-us'},maxRedirects:0});
      try{
      const page=await context.newPage();if(javaScriptEnabled)hydratedPages.add(page);
      await page.goto(`/media/addon-lists/${seed.listId}/items/${seed.firstItemId}`);await settled(page);
      // Save item is in the first viewport, beside the item navigation.
      const topSave=page.locator('.item-navigation [data-save-indicator]');
      await expect(topSave).toHaveText('Save item');
      const box=await topSave.boundingBox();
      expect(box!.y+box!.height).toBeLessThanOrEqual(700);
      // Without a chosen replacement there is no extra Save beside the search.
      await expect(page.locator('#item-replacement [data-save-indicator]')).toBeHidden();
      // Unnumbered hides the number inputs instead of leaving them editable.
      await page.locator('[name="numbering_mode"][value="none"]').check();
      await expect(page.locator('.item-numbering [name="season"]')).toBeHidden();
      await page.locator('[name="metadata_title"]').fill('Saved from the top');
      await Promise.all([page.waitForEvent('framenavigated',f=>f===page.mainFrame()),topSave.click()]);
      await settled(page);
      await expect(page.locator('#workspace-errors')).toHaveCount(0);
      expect(sqlite(`SELECT json_extract("MetadataOverrideJson",'$.title') FROM "AddonMediaListItem" WHERE "Id"=${seed.firstItemId};`)).toBe('Saved from the top');
      }finally{await context.close();}
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
