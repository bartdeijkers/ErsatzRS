// Synthetic preview only. Native forms are exercised with JavaScript disabled.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { chromium, firefox, expect } = require('@playwright/test');
const base = process.argv[2] || 'http://127.0.0.1:8412';
const engine = process.argv[3] || 'chromium';
assert(['127.0.0.1', 'localhost'].includes(new URL(base).hostname));
assert(['chromium', 'firefox'].includes(engine));
(async () => {
  const browser = await ({ chromium, firefox }[engine]).launch();
  const report = { browser: engine, javaScriptEnabled: false, workflows: [], layouts: [] };
  const output = name => path.join(__dirname, 'screenshots', `replacement-${engine}-${name}.png`);
  try {
    let context = await browser.newContext({ javaScriptEnabled: false });
    let page = await context.newPage();
    await page.goto(base + '/item-1.html');
    for (const id of ['item-metadata', 'item-chapters']) {
      await page.locator(`#${id} > summary`).focus();
      await page.keyboard.press('Enter');
      await expect(page.locator(`#${id}`)).toHaveAttribute('open', '');
    }
    const fields = { plot: 'Synthetic summary with <angle brackets> & punctuation.\nSecond paragraph.', genres: 'Film, Documentary', tags: 'Restoration\nMiniatures', chapter_input: '0:00 Introduction\n0:30 Workshop' };
    for (const [name, value] of Object.entries(fields)) await page.locator(`[name="${name}"]`).fill(value);
    await page.locator('[value="nav:item-2.html"]').click();
    await page.locator('[value="nav:item-1.html"]').click();
    for (const [name, value] of Object.entries(fields)) await expect(page.locator(`[name="${name}"]`)).toHaveValue(value);
    await expect(page.locator('#item-metadata')).toHaveAttribute('open', '');
    await page.locator('[name="chapter_input"]').fill('0:30 Workshop\n0:00 Introduction');
    await page.locator('[value="apply"]').click();
    await expect(page.locator('#error-summary')).toContainText('times must increase');
    await expect(page.locator('#error-summary')).toBeFocused();
    await expect(page.locator('#item-chapters')).toHaveAttribute('open', '');
    await expect(page.locator('#chapter_input')).toHaveAttribute('aria-invalid', 'true');
    await expect(page.locator('[name="plot"]')).toHaveValue(fields.plot);
    await page.locator('[value="discard"]').click();
    await expect(page.locator('[name="plot"]')).toHaveValue(''); // Invalid chapter did not partly apply metadata.
    for (const id of ['item-metadata', 'item-chapters']) await page.locator(`#${id} > summary`).click();
    for (const [name, value] of Object.entries(fields)) await page.locator(`[name="${name}"]`).fill(value);
    await page.locator('[value="apply"]').click();
    await expect(page.getByRole('status')).toContainText('Changes saved');
    await page.reload();
    for (const id of ['item-metadata', 'item-chapters']) await page.locator(`#${id} > summary`).click();
    for (const [name, value] of Object.entries(fields)) await expect(page.locator(`[name="${name}"]`)).toHaveValue(value);
    await page.locator('[name="plot"]').fill('Discard this pending change');
    await page.locator('[value="discard"]').click();
    await expect(page.locator('[name="plot"]')).toHaveValue(fields.plot);
    for (const theme of ['dark', 'light']) {
      for (const width of [320, 390, 720, 1440]) {
        await page.setViewportSize({width, height:900});
        await page.emulateMedia({colorScheme:theme});
        for (const id of ['item-metadata', 'item-chapters']) await page.locator(`#${id}`).evaluate(el => { el.open = true; });
        const overflow = await page.evaluate(() => document.documentElement.scrollWidth - innerWidth);
        assert.equal(overflow, 0);
        for (const name of Object.keys(fields)) assert((await page.locator(`[name="${name}"]`).boundingBox()).height >= 44);
        report.layouts.push({page:'item', theme, width, overflow});
        if (theme === 'dark' && [390,1440].includes(width)) await page.screenshot({path:output(`item-${width}`),fullPage:true});
      }
    }
    report.workflows.push('All four metadata editors retain drafts across Previous/Next, reject invalid chapters before applying metadata, apply, reload, and discard correctly.');
    await context.close();

    context = await browser.newContext({javaScriptEnabled:false}); page = await context.newPage();
    await page.goto(base + '/recovery-list.html');
    await expect(page.locator('.publication-recovery')).toBeVisible();
    await expect(page.locator('.publication-details')).not.toHaveAttribute('open','');
    await expect(page.locator('[value="publish"]')).not.toBeVisible();
    await page.locator('[name="items[2].episode"]').fill('80');
    await page.locator('[value="nav:recovery.html"]').click();
    const stale = await page.locator('#workspace-form').evaluate(f => Object.fromEntries(new FormData(f)));
    await expect(page.getByRole('button',{name:'Retry pending publication',exact:true})).toBeVisible();
    await page.locator('[value="demo-retry-publication"]').click();
    await expect(page.locator('.publication-recovery')).toContainText('retry in progress');
    await expect(page.locator('[name="items[2].episode"]')).toHaveValue('80');
    await context.request.post(base+'/_preview',{form:{...stale,intent:'demo-retry-publication'}});
    await page.reload();
    await expect(page.locator('#error-summary')).toContainText('nothing was repeated');
    await expect(page.locator('.publication-details')).not.toHaveAttribute('open','');
    for (const width of [320,390,1440]) {
      await page.setViewportSize({width,height:900});
      assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth-innerWidth),0);
      await page.screenshot({path:output(`recovery-${width}`),fullPage:false});
    }
    await page.locator('[value="nav:recovery.html"]').click();
    await expect(page.locator('[value="demo-retry-publication"]')).toHaveCount(0);
    await page.locator('[value="demo-complete-publication"]').click();
    await expect(page.locator('[data-publication-revision]')).toContainText('Snapshot 4');
    await expect(page.locator('.publication-state')).toContainText('Published list is up to date');
    await expect(page.locator('.publication-recovery')).toHaveCount(0);
    await expect(page.locator('[name="items[2].episode"]')).toHaveValue('80');
    report.workflows.push('Recovery remains visible outside diagnostics; retry eligibility, stale generation rejection, running and completion states preserve numbering drafts.');
    await context.close();

    context = await browser.newContext({javaScriptEnabled:false}); page = await context.newPage();
    await page.goto(base + '/large-list.html');
    await expect(page.locator('.episode-row')).toHaveCount(10);
    await page.locator('[name="items[2].season"]').fill('7');
    await page.locator('[name="items[2].episode"]').fill('2');
    await page.locator('[name="selected_ids"][value="2"]').check();
    for (const size of ['25','50','100','All','10']) {
      await page.locator('[name="page_size"]').selectOption(size);
      await page.locator('[value="page-size"]').click();
      await expect(page.locator('.episode-row')).toHaveCount(size==='All'?165:Number(size));
      await expect(page.locator('[name="items[2].episode"]')).toHaveValue('2');
      await expect(page.locator('[name="selected_ids"][value="2"]')).toBeChecked();
      await expect(page.locator('.page-summary')).toContainText('165 matching');
    }
    await page.locator('[value="next-page"]').click();
    await page.locator('[name="selected_ids"][value="12"]').check();
    await page.locator('[value="previous-page"]').click();
    await expect(page.locator('.selection-label')).toContainText('1 on other pages');
    await expect(page.locator('[name="items[2].episode"]')).toHaveValue('2');
    await page.locator('[name="item_query"]').fill('conversation 165');
    await page.locator('button[value="filter"]').click();
    await expect(page.locator('.episode-row')).toHaveCount(1);
    await page.locator('[value="nav:item-165.html"]').click();
    await expect(page.locator('[name="title"]')).toHaveValue(/Archive conversation 165/);
    await expect(page.locator('.item-navigation button').nth(1)).toBeDisabled();
    await page.locator('[value="nav:index.html#episode-165"]').click();
    await page.locator('[name="item_query"]').fill('');
    await page.locator('button[value="filter"]').click();
    await page.locator('[value="apply-numbers"]').click();
    await expect(page.locator('#episode-2 .row-numbering')).toContainText('Saved: S7 · E2');
    await page.locator('[name="page_size"]').selectOption('All');
    await page.locator('[value="page-size"]').click();
    await page.locator('[value="clear-selection"]').click();
    for (const theme of ['dark','light']) {
      for (const width of [320,390,720,1024,1440]) {
        await page.setViewportSize({width,height:900}); await page.emulateMedia({colorScheme:theme});
        const geometry=await page.evaluate(()=>({overflow:document.documentElement.scrollWidth-innerWidth,rows:document.querySelectorAll('.episode-row').length}));
        assert.equal(geometry.overflow,0); assert.equal(geometry.rows,165);
        await expect(page.locator('.airdate time')).toHaveCount(0);
        await page.locator('[name="items[165].episode"]').focus();
        const bar=await page.locator('.editor-savebar').boundingBox(),field=await page.locator('[name="items[165].episode"]').boundingBox();
        assert(field.y>=bar.y+bar.height);
        report.layouts.push({page:'large-list',theme,width,...geometry});
        if(theme==='dark'&&[390,1440].includes(width)) {await page.evaluate(()=>window.scrollTo(0,0));await page.screenshot({path:output(`large-${width}`),fullPage:false});}
      }
    }
    report.workflows.push('165 long-title items with unknown dates and runtimes: 10/25/50/100/All, filtering, first/last navigation, off-page selection, draft retention, Apply and reflow.');
    await context.close();
    fs.writeFileSync(path.join(__dirname,`replacement-${engine}-verification.json`),JSON.stringify(report,null,2)+'\n');
    console.log(JSON.stringify(report));
  } finally { await browser.close(); }
})().catch(error=>{console.error(error);process.exitCode=1;});
