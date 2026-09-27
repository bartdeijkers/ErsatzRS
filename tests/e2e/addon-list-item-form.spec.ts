import { test, expect, Page } from '@playwright/test';
import { seedOptions, seedItemReplacement, sqlite } from './fixtures/addon-options';

const hydratedPages = new WeakSet<Page>();
async function settled(page: Page) {
  await page.waitForLoadState('domcontentloaded');
  if (hydratedPages.has(page)) await page.waitForFunction(() => (window as any).__ersatzrs_hydrated === true);
}
async function submit(page: Page, intent: string) {
  await Promise.all([
    page.waitForEvent('framenavigated', frame => frame === page.mainFrame()),
    page.locator(`button[name="intent"][value="${intent}"]`).first().click(),
  ]);
  await settled(page);
}

for (const javaScriptEnabled of [false, true]) {
  test.describe(`item form comprehension (${javaScriptEnabled ? 'hydrated' : 'no JS'})`, () => {
    test.skip(!process.env.E2E_DB_PATH, 'An isolated E2E database is required');
    test('groups populated editing, options and saved-item tools without losing native drafts', async ({ browser, baseURL }, info) => {
      test.setTimeout(180_000);
      const seed = seedOptions(true);
      const replacement = seedItemReplacement(seed.firstItemId);
      sqlite(`UPDATE "AddonMediaListItem" SET "Availability"=2 WHERE "Id"=${seed.secondItemId}`);
      sqlite(`UPDATE "AddonMediaListItem" SET "Season"=1,"Episode"=6,"ProviderMetadataJson"='{"title":"Synthetic review episode","season":1,"episode":6,"release_date":"2025-06-02T12:30:00Z","plot":"A synthetic episode with metadata for form review.","languages":["en","nl"],"genres":["Documentary"],"people":[{"name":"Example Presenter","role":"Host"}]}' WHERE "Id"=${seed.firstItemId}`);
      const context = await browser.newContext({ baseURL, javaScriptEnabled });
      const page = await context.newPage();
      if (javaScriptEnabled) hydratedPages.add(page);
      const errors: string[] = [];
      page.on('pageerror', e => errors.push(e.message));
      const route = `/media/addon-lists/${seed.listId}/items/${seed.firstItemId}`;
      await page.request.post('/admin-api/settings/ui/save', { form: { language: 'en-us', is_dark_mode: 'true' } });
      await page.goto(route);
      if (javaScriptEnabled) await page.waitForFunction(() => (window as any).__ersatzrs_hydrated === true);
      await expect(page.locator('#item-review-heading')).toBeVisible();
      await expect(page.locator('.tab-link[aria-current="page"]')).toHaveCount(1);
      await expect(page.getByLabel('First aired', { exact: true })).toHaveCount(1);
      const firstAired = page.getByLabel('First aired', { exact: true });
      await expect(firstAired).toBeVisible();
      await expect(firstAired).toHaveAttribute('type', 'text');
      await expect(firstAired).toHaveValue('2025-06-02T12:30:00Z');
      await expect(page.getByRole('group', { name: 'Episode position', exact: true })).toBeVisible();
      await expect(page.getByRole('radio', { name: 'Season & episode', exact: true })).toBeChecked();
      await expect(page.locator('#item-replacement')).not.toHaveAttribute('open');
      await expect(page.locator('.item-edit button[value="access-test"]')).toHaveCount(0);
      await expect(page.locator('button[value="access-test"]')).toHaveCount(1);
      await expect(page.locator('button[value="access-test"]')).toBeDisabled();
      await expect(page.locator('.item-access-trigger')).toContainText('Save an option first');
      await expect(page.locator('.item-options')).toContainText('Package default; not yet activated');
      expect(await page.locator('#item-save').evaluate(e => !!(e.compareDocumentPosition(document.querySelector('.item-tools')!) & Node.DOCUMENT_POSITION_FOLLOWING))).toBe(true);
      await page.locator('#item-metadata > summary').focus();
      expect(await page.locator('#item-metadata > summary').evaluate(e => parseFloat(getComputedStyle(e).outlineWidth))).toBeGreaterThanOrEqual(2);
      await page.keyboard.press('Enter');
      await expect(page.getByLabel('Cast and credits', { exact: true })).toBeVisible();
      await expect(page.locator('#item-metadata details')).toHaveCount(0);
      expect(await page.locator('#item-metadata > summary').evaluate(e => getComputedStyle(e, '::before').content)).toBe('none');
      await page.getByLabel('Cast and credits', { exact: true }).fill('Example Presenter | Narrator');
      await page.getByRole('radio', { name: 'Unnumbered', exact: true }).check();
      await expect(page.getByLabel('Season', { exact: true })).toBeHidden();
      await page.getByLabel('Browser', { exact: true }).selectOption('"none"');
      await submit(page, 'preview-options');
      await expect(page.getByRole('radio', { name: 'Unnumbered', exact: true })).toBeChecked();
      await expect(page.locator('[data-testid="option-preview"]').first()).toBeVisible();
      await page.locator('.item-adjacent').last().click();await settled(page);
      await expect(page.locator('.item-status')).toContainText(' · Unknown · ');
      await page.locator('.item-adjacent').first().click();await settled(page);
      await expect(page.getByRole('radio', { name: 'Unnumbered', exact: true })).toBeChecked();
      await submit(page, 'save');
      await expect(page.locator('#workspace-errors')).toHaveCount(0);
      expect(sqlite(`SELECT json_type("MetadataOverrideJson",'$.season') || '|' || json_type("MetadataOverrideJson",'$.episode') FROM "AddonMediaListItem" WHERE "Id"=${seed.firstItemId}`)).toBe('null|null');
      expect(sqlite(`SELECT json_extract("MetadataOverrideJson",'$.people[0].role') FROM "AddonMediaListItem" WHERE "Id"=${seed.firstItemId}`)).toBe('Narrator');
      await expect(firstAired).toHaveValue('2025-06-02T12:30:00Z');
      expect(sqlite(`SELECT coalesce(json_extract("MetadataOverrideJson",'$.release_date'),json_extract("ProviderMetadataJson",'$.release_date')) FROM "AddonMediaListItem" WHERE "Id"=${seed.firstItemId}`)).toBe('2025-06-02T12:30:00Z');
      await expect(page.locator('[data-option-key="browser"]')).toContainText('Item override');
      await expect(page.locator('[data-option-key="browser"]')).toContainText('Saved: None');
      await page.getByRole('radio', { name: 'Season & episode', exact: true }).check();
      await expect(page.getByLabel('Season', { exact: true })).toBeVisible();
      await submit(page, 'save');
      await expect(page.locator('#workspace-errors')).toBeVisible();
      await expect(page.getByLabel('Season', { exact: true })).toHaveAttribute('aria-invalid', 'true');
      await page.getByLabel('Season', { exact: true }).fill('0');
      await page.getByLabel('Episode', { exact: true }).fill('3');
      await firstAired.fill('2025-06-02');
      await submit(page, 'save');
      await expect(page.locator('#workspace-errors')).toHaveCount(0);
      await expect(firstAired).toHaveAttribute('type', 'date');
      expect(sqlite(`SELECT json_extract("MetadataOverrideJson",'$.season') || '|' || json_extract("MetadataOverrideJson",'$.episode') FROM "AddonMediaListItem" WHERE "Id"=${seed.firstItemId}`)).toBe('0|3');
      await page.locator('#item-replacement > summary').click();
      await page.getByLabel('Search local media', { exact: true }).fill('No such synthetic media');
      await submit(page, 'replacement-search');
      await expect(page.locator('#item-replacement')).toHaveAttribute('open');
      await expect(page.getByLabel('Search local media', { exact: true })).toHaveValue('No such synthetic media');
      await expect(page.locator('#item-replacement')).toContainText('No matching records found');
      await page.getByLabel('Search local media', {exact:true}).fill(`Synthetic replacement ${replacement}`);
      await submit(page, 'replacement-search');
      await submit(page, `replacement:${replacement}:${replacement}`);
      await expect(page.locator('#item-replacement')).toContainText('Replacement change is unsaved.');
      expect(sqlite(`SELECT coalesce((SELECT "MediaItemId" FROM "AddonMediaListItemReplacement" WHERE "AddonMediaListItemId"=${seed.firstItemId}),'null')`)).toBe('null');
      await submit(page,'save');
      expect(sqlite(`SELECT "MediaItemId" || '|' || "MediaVersionId" FROM "AddonMediaListItemReplacement" WHERE "AddonMediaListItemId"=${seed.firstItemId}`)).toBe(`${replacement}|${replacement}`);
      await page.locator('#item-replacement > summary').click();
      await submit(page, 'replacement-clear');
      await expect(page.locator('#item-replacement')).toContainText('Replacement change is unsaved.');
      await submit(page, 'discard');
      await expect(page.locator('#item-replacement')).not.toHaveAttribute('open');
      expect(sqlite(`SELECT "MediaItemId" FROM "AddonMediaListItemReplacement" WHERE "AddonMediaListItemId"=${seed.firstItemId}`)).toBe(String(replacement));
      for (const theme of ['dark', 'light']) {
        await page.request.post('/admin-api/settings/ui/save', { form: { language: 'en-us', is_dark_mode: String(theme === 'dark') } });
        await page.goto(route);await settled(page);
        for (const width of [320,390,720,1024,1440,1920,3840]) {
          await page.setViewportSize({ width, height: 1080 });
          await page.locator('main').evaluate(e => e.scrollTop = 0);
          expect(await page.locator('.addon-workspace').evaluate(e => e.scrollWidth <= e.clientWidth + 1)).toBe(true);
          if (width === 1440) {
            const layout = await page.locator('.item-review').evaluate(root => ({
              reviewWidth:root.getBoundingClientRect().width,
              saveTop:root.querySelector('#item-save')!.getBoundingClientRect().top,
              toolsTop:root.querySelector('.item-tools')!.getBoundingClientRect().top,
            }));
            await info.attach(`item-${theme}-layout.json`, {body:Buffer.from(JSON.stringify(layout)),contentType:'application/json'});
            const contrasts = await page.locator('.item-review').evaluate(root => {
              const canvas = document.createElement('canvas'); canvas.width = canvas.height = 1;
              const ctx = canvas.getContext('2d')!;
              const rgba = (color: string) => { ctx.clearRect(0,0,1,1);ctx.fillStyle=color;ctx.fillRect(0,0,1,1);const p=ctx.getImageData(0,0,1,1).data;return [p[0],p[1],p[2],p[3]/255]; };
              const mix = (fg: number[], bg: number[]) => fg.slice(0,3).map((c,i)=>c*fg[3]+bg[i]*(1-fg[3]));
              const lum = (rgb: number[]) => rgb.map(c=>c/255).map(c=>c<=.04045?c/12.92:((c+.055)/1.055)**2.4).reduce((sum,c,i)=>sum+c*[.2126,.7152,.0722][i],0);
              const ratio = (a: number[], b: number[]) => (Math.max(lum(a),lum(b))+.05)/(Math.min(lum(a),lum(b))+.05);
              return Array.from(root.querySelectorAll('.item-status, .option-effective, .option-setting label, .option-setting p, .disclosure-label small, .apply-bar small, input:not([type="radio"]):not([type="checkbox"]), select, textarea')).filter(e=>(e as HTMLElement).offsetHeight>0 && !e.hasAttribute('disabled')).map(e => {
                const ancestors:Element[]=[];for(let a:Element|null=e;a;a=a.parentElement)ancestors.unshift(a);
                let bg=[255,255,255];for(const a of ancestors)bg=mix(rgba(getComputedStyle(a).backgroundColor),bg);
                const style=getComputedStyle(e);
                return {element:e.getAttribute('name') || e.className,text:ratio(mix(rgba(style.color),bg),bg),border:e.matches('input,select,textarea')?ratio(mix(rgba(style.borderTopColor),bg),bg):null};
              });
            });
            for (const c of contrasts) {
              expect(c.text, `${theme} text contrast: ${c.element}`).toBeGreaterThanOrEqual(4.5);
              if(c.border !== null) expect(c.border, `${theme} control contrast: ${c.element}`).toBeGreaterThanOrEqual(3);
            }
          }
          const accessButton = await page.locator('.item-access-trigger button').boundingBox();
          const backButton = await page.locator('.item-navigation > button').last().boundingBox();
          expect(accessButton!.y, 'visual action order agrees with keyboard order').toBeLessThanOrEqual(backButton!.y);
          const rows = page.locator('.item-options .option-setting select');
          const boxes = await rows.evaluateAll(es => es.map(e => ({ x:e.getBoundingClientRect().x,width:e.getBoundingClientRect().width })));
          expect(boxes[0].width).toBeLessThanOrEqual(768);
          expect(boxes[0].x).toBe(boxes[1].x);
          expect(boxes[0].width).toBe(boxes[1].width);
          if ([390,1440,3840].includes(width)) {
            await page.screenshot({ path: info.outputPath(`item-${theme}-${width}-top.png`), animations:'disabled' });
            await page.locator('main').evaluate(e => e.scrollTop = e.scrollHeight);
            await page.screenshot({ path: info.outputPath(`item-${theme}-${width}-tools.png`), animations:'disabled' });
          }
        }
        await page.setViewportSize({ width:1440,height:1080 });
        await page.locator('#item-metadata > summary').click();
        await page.locator('#item-metadata').scrollIntoViewIfNeeded();
        await page.screenshot({ path:info.outputPath(`item-${theme}-metadata.png`), animations:'disabled' });
      }
      await page.setViewportSize({width:1280,height:900});
      await page.evaluate(() => document.body.style.zoom='2');
      expect(await page.locator('.addon-workspace').evaluate(e => e.scrollWidth <= e.clientWidth + 1)).toBe(true);
      await page.evaluate(() => document.body.style.zoom='1');
      for (const language of ['de-de','fr-fr']) {
        await page.request.post('/admin-api/settings/ui/save', { form: {language,is_dark_mode:'true'} });
        await page.goto(route);await settled(page);
        await page.setViewportSize({width:320,height:900});
        await expect(page.locator('#item-review-heading')).toBeVisible();
        expect(await page.locator('.addon-workspace').evaluate(e => e.scrollWidth <= e.clientWidth + 1)).toBe(true);
        await page.screenshot({path:info.outputPath(`item-${language}-320.png`),animations:'disabled'});
      }
      await page.request.post('/admin-api/settings/ui/save', {form:{language:'en-us',is_dark_mode:'true'}});
      expect(errors).toEqual([]);
      await context.close();
    });
  });
}
