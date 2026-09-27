import { test, expect } from '@playwright/test';
import { seedMetadataEditor } from './fixtures/addon-metadata';

test('locked workspace reflows in both themes from mobile to 4K', async ({page},info)=>{
  test.skip(!process.env.E2E_DB_PATH,'An isolated E2E database is required');
  test.setTimeout(240_000);
  const seed=seedMetadataEditor(info);
  const base=`/media/addon-lists/${seed.listId}`;
  const routes=[['items',base],['settings',`${base}/settings`],['playback',`${base}/playback`],['activity',`${base}/activity`],['item',`${base}/items/${seed.firstItemId}`],['artwork',`${base}/artwork`]];
  for(const theme of ['dark','light']) {
    await page.emulateMedia({colorScheme:theme==='dark'?'light':'dark'});
    await page.request.post('/admin-api/settings/ui/save',{form:{is_dark_mode:String(theme==='dark'),language:'en-us'},maxRedirects:0});
    for(const [task,route] of routes) {
      await page.goto(route);
      await page.waitForFunction(()=>(window as any).__ersatzrs_hydrated===true);
      await expect(page.locator(`[data-theme="${theme}"]`)).toBeVisible();
      if(task==='activity') await expect(page.locator('[name="compare_left"], [name="compare_right"], button[value="compare"]')).toHaveCount(0);
      for(const width of [320,390,720,1024,1440,1920,3840]) {
        await page.setViewportSize({width,height:1080});
        if(width<768) await expect.poll(()=>page.locator('[data-nav-drawer]').evaluate(e=>e.getBoundingClientRect().right)).toBeLessThanOrEqual(1);
        const geometry=await page.locator('.addon-workspace').evaluate(element=>({width:element.clientWidth,scroll:element.scrollWidth,pageWidth:document.documentElement.clientWidth,pageScroll:document.documentElement.scrollWidth}));
        expect.soft(geometry.scroll,`${task}/${theme}/${width}: content reflow`).toBeLessThanOrEqual(geometry.width+1);
        expect.soft(geometry.pageScroll,`${task}/${theme}/${width}: page reflow`).toBeLessThanOrEqual(geometry.pageWidth+1);
        if(width===1440) {
          const contrast=await page.locator('.addon-workspace').evaluate(root=>{
            const canvas=document.createElement('canvas');canvas.width=canvas.height=1;
            const ctx=canvas.getContext('2d')!;
            const rgba=(color:string)=>{ctx.clearRect(0,0,1,1);ctx.fillStyle=color;ctx.fillRect(0,0,1,1);const p=ctx.getImageData(0,0,1,1).data;return [p[0],p[1],p[2],p[3]/255];};
            const mix=(fg:number[],bg:number[])=>fg.slice(0,3).map((c,i)=>c*fg[3]+bg[i]*(1-fg[3]));
            const lum=(rgb:number[])=>rgb.map(c=>c/255).map(c=>c<=0.04045?c/12.92:((c+0.055)/1.055)**2.4).reduce((sum,c,i)=>sum+c*[.2126,.7152,.0722][i],0);
            return Array.from(root.querySelectorAll('.numbering-save-bar > span, .inline-pair label > span, .tab-link, .apply-bar .primary, .publication-state strong')).filter(e=>(e as HTMLElement).offsetHeight>0).map(e=>{
              const ancestors:Element[]=[];for(let a:Element|null=e;a;a=a.parentElement)ancestors.unshift(a);
              let bg=[255,255,255];for(const a of ancestors)bg=mix(rgba(getComputedStyle(a).backgroundColor),bg);
              const fg=mix(rgba(getComputedStyle(e).color),bg),a=lum(fg),b=lum(bg);
              return {text:e.textContent?.trim(),ratio:(Math.max(a,b)+.05)/(Math.min(a,b)+.05)};
            });
          });
          for(const result of contrast)expect.soft(result.ratio,`${task}/${theme} contrast: ${result.text}`).toBeGreaterThanOrEqual(4.5);
          for(const button of await page.locator('.tab-link, .apply-bar button, .item-adjacent').all()) {
            const box=await button.boundingBox();expect.soft(box?.height,`${task} principal target`).toBeGreaterThanOrEqual(44);
          }
        }
        if(task==='playback') {
          const access=await page.locator('.access-stack').boundingBox();
          expect.soft(access?.width).toBeLessThanOrEqual(768);
        }
        if([390,1440,3840].includes(width)) await page.screenshot({path:info.outputPath(`${task}-${theme}-${width}.png`),fullPage:true,animations:'disabled'});
      }
    }
  }
  await page.setViewportSize({width:1280,height:900});
  await page.goto(base);
  await page.waitForFunction(()=>(window as any).__ersatzrs_hydrated===true);
  await page.evaluate(()=>{document.body.style.zoom='2';});
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=document.documentElement.clientWidth+1)).toBe(true);
});

test('long Dutch and German selected filters stay readable at 320 pixels', async ({ page }, info) => {
  test.skip(!process.env.E2E_DB_PATH, 'An isolated E2E database is required');
  const seed = seedMetadataEditor(info);
  await page.setViewportSize({ width: 320, height: 800 });
  for (const language of ['nl', 'de-de']) {
    for (const dark of [false, true]) {
      await page.request.post('/admin-api/settings/ui/save', { form: { is_dark_mode: String(dark), language }, maxRedirects: 0 });
      await page.goto(`/media/addon-lists/${seed.listId}`);
      await page.waitForFunction(() => (window as any).__ersatzrs_hydrated === true);
      const longestValue = await page.locator('[name="category"] option').evaluateAll(options =>
        (options.reduce((a, b) => (b.textContent?.length ?? 0) > (a.textContent?.length ?? 0) ? b : a) as HTMLOptionElement).value);
      // Picking a category applies it; the reloaded page keeps the choice.
      await Promise.all([
        page.waitForEvent('framenavigated', f => f === page.mainFrame()),
        page.locator('[name="category"]').selectOption(longestValue),
      ]);
      await page.waitForFunction(() => (window as any).__ersatzrs_hydrated === true);
      const selected = await page.locator('[name="category"] option:checked').textContent();
      await expect(page.locator('.selected-filter-label')).toHaveText(selected!);
      await expect(page.locator('.selected-filter-label')).toBeVisible();
      const box = await page.locator('.selected-filter-label').boundingBox();
      expect(box!.x + box!.width).toBeLessThanOrEqual(320);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= 321)).toBe(true);
      await page.screenshot({ path: info.outputPath(`filter-${language}-${dark ? 'dark' : 'light'}.png`), fullPage: true });
    }
  }
});
