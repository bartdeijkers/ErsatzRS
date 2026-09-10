// Run against preview_server.py, never the production host:
// node plans/admin-ux-organize/verify-ux-fixes.cjs http://127.0.0.1:8412 chromium
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
  const report = { browser: engine, javaScriptEnabled: false, layouts: [], workflows: [] };
  try {
    const context = await browser.newContext({ javaScriptEnabled: false });
    const page = await context.newPage();
    for (const theme of ['dark', 'light']) {
      for (const width of [320, 390, 720, 1024, 1440]) {
        await page.setViewportSize({ width, height: width === 720 ? 500 : 900 });
        await page.emulateMedia({ colorScheme: theme });
        await page.goto(base + '/index.html');
        await expect(page.locator('#prototype-note')).toContainText('Synthetic data');
        const geometry = await page.evaluate(() => {
          const rect = selector => document.querySelector(selector).getBoundingClientRect();
          return { width: innerWidth, overflow: document.documentElement.scrollWidth - innerWidth,
            firstRowY: rect('.episode-row').y, applyY: rect('[value="apply-numbers"]').y,
            rowHeights: [...document.querySelectorAll('.episode-row')].map(e => Math.round(e.getBoundingClientRect().height)) };
        });
        assert.equal(geometry.overflow, 0);
        assert(geometry.applyY < geometry.firstRowY);
        await expect(page.locator('button[value="apply-numbers"]')).toHaveCount(1);
        await expect(page.locator('button[value="discard"]')).toHaveCount(1);
        await expect(page.locator('a[href="#group-actions"],a[href="#numbering-help"],.text-action')).toHaveCount(0);
        await expect(page.locator('button[value="undo-numbers"],button[value="undo-bulk"]')).toHaveCount(0);
        await expect(page.locator('button[value="publish"]')).not.toBeVisible();
        if (width < 768) {
          await expect(page.locator('.mobile-navigation')).not.toHaveAttribute('open', '');
          await expect(page.locator('#mobile-navigation-drawer')).not.toBeVisible();
          await page.locator('#mobile-nav-toggle').focus();
          await page.keyboard.press('Enter');
          await expect(page.locator('#mobile-navigation-drawer')).toBeVisible();
          await page.keyboard.press('Enter');
          await expect(page.locator('#mobile-navigation-drawer')).not.toBeVisible();
        } else {
          await expect(page.locator('#admin-navigation-drawer')).toBeVisible();
        }
        if (width <= 390) {
          assert(geometry.rowHeights[1] < 160, 'Ordinary mobile rows should be more compact than the reviewed 183px');
          assert(geometry.firstRowY < 800, 'The action redesign must not push all items beyond a phone viewport');
          const field = await page.locator('[name="items[2].episode"]').boundingBox();
          assert(field.height >= 44);
        }
        const ids = await page.locator('[id]').evaluateAll(nodes => nodes.map(n => n.id));
        assert.equal(new Set(ids).size, ids.length);
        if ([390, 1440].includes(width)) await page.screenshot({ path: path.join(__dirname, 'screenshots', `ux-fixes-${engine}-${width}-${theme}.png`), fullPage: true });
        await page.locator('[name="items[9].episode"]').focus();
        const sticky = await page.locator('.editor-savebar').boundingBox();
        const focused = await page.locator('[name="items[9].episode"]').boundingBox();
        assert(sticky.y >= 56 && sticky.y + sticky.height < (width === 720 ? 500 : 900));
        assert(focused.y >= sticky.y + sticky.height, 'The save bar must not cover the focused field');
        report.layouts.push({ theme, ...geometry });
      }
    }
    // Keyboard popovers remain available after the mobile grid changes.
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto(base + '/index.html');
    const trigger = page.locator('.row-command-trigger').first();
    await trigger.focus(); await page.keyboard.press('Enter');
    await expect(page.locator(':popover-open')).toBeVisible();
    await page.keyboard.press('Tab');
    await expect(page.getByRole('button', { name: 'Number from here', exact: true }).first()).toBeFocused();
    await page.keyboard.press('Escape'); await expect(trigger).toBeFocused();
    report.workflows.push('Keyboard row menu and focus return');

    // A rejected edit keeps its draft and directly identifies both correction targets.
    await page.locator('[name="items[2].episode"]').fill('4');
    await page.locator('[value="apply-numbers"]').first().click();
    await expect(page.locator('#error-summary')).toBeFocused();
    await expect(page.locator('#error-summary')).toContainText('The art of an opening scene');
    await expect(page.locator('#error-summary')).toContainText('Rediscovering the double feature');
    await expect(page.locator('#mobile-navigation-drawer')).not.toBeVisible();
    await expect(page.locator('[aria-invalid=true]')).toHaveCount(4);
    await expect(page.locator('[name="items[2].episode"]')).toHaveValue('4');
    await expect(page.locator('#episode-2 .row-numbering small')).toHaveText('Saved: S1 · E1');
    await expect(page.locator('.publication-details')).not.toHaveAttribute('open', '');
    await page.getByRole('link', { name: 'Item 2:', exact: false }).click();
    await expect(page.locator('#numbering-2-episode')).toBeFocused();
    const fieldY = await page.locator('#numbering-2-episode').boundingBox();
    assert(fieldY.y >= 56 && fieldY.y + fieldY.height <= 844);
    await page.locator('#numbering-2-episode').fill('6');
    await page.locator('[value="apply-numbers"]').first().click();
    await expect(page.locator('[role=status]')).toContainText('Numbering saved');
    await expect(page.locator('[aria-invalid=true]')).toHaveCount(0);
    await expect(page.locator('#episode-2 .row-numbering small')).toHaveText('Saved: S1 · E6');
    await expect(page.locator('button[value="publish"]')).toBeVisible();
    await page.locator('button[value="undo-numbers"]').click();
    await expect(page.locator('#episode-2 .row-numbering small')).toHaveText('Saved: S1 · E1');
    await expect(page.locator('button[value="undo-numbers"]')).toHaveCount(0);
    report.workflows.push('Conflict: named summary, field association, focus, draft retention and successful correction');
    report.workflows.push('Single persistent Apply/Discard bar, no duplicate shortcuts, unobscured focus and contextual Undo');

    // Both ends retain their navigation slots, including after a real Next submit.
    for (const width of [320, 1440]) {
      await page.setViewportSize({ width, height: 900 });
      await page.goto(base + '/item-1.html');
      const previous = page.locator('.item-navigation').getByRole('button', { name: '← Previous', exact: true });
      const next = page.locator('.item-navigation').getByRole('button', { name: 'Next →', exact: true });
      await expect(previous).toBeVisible(); await expect(previous).toBeDisabled();
      const first = await next.boundingBox();
      await next.click(); await expect(page).toHaveURL(/item-2.html$/);
      const second = await next.boundingBox();
      assert.equal(first.x, second.x); assert.equal(first.width, second.width);
      await expect(previous).toBeEnabled();
      await page.goto(base + '/item-9.html');
      await expect(next).toBeVisible(); await expect(next).toBeDisabled();
    }
    report.workflows.push('Previous/Next keep fixed horizontal slots at list boundaries');

    // Optional disclosure groups stay keyboard-operable and submit their original forms.
    for (const width of [320, 1440]) {
      await page.setViewportSize({ width, height: 900 });
      for (const theme of ['dark', 'light']) {
        await page.emulateMedia({ colorScheme: theme });
        for (const [file, selector] of [
          ['index.html', '#group-actions'],
          ['index.html', '.publication-details'],
          ['settings.html', 'details.disclosure:has(> summary :text("Credits"))'],
        ]) {
          await page.goto(base + '/' + file);
          const details = page.locator(selector);
          await expect(details).toBeVisible();
          await expect(details).not.toHaveAttribute('open', '');
          await details.locator(':scope > summary').focus(); await page.keyboard.press('Enter');
          await expect(details).toHaveAttribute('open', '');
          assert.equal(await page.evaluate(() => document.documentElement.scrollWidth - innerWidth), 0);
          await details.locator(':scope > summary').focus(); await page.keyboard.press('Space');
          await expect(details).not.toHaveAttribute('open', '');
        }
      }
    }
    await page.goto(base + '/options.html');
    await page.locator('[name="warning_gb"]').fill('-1');
    await page.locator('button[value="apply"]').click();
    await expect(page.locator('[name="warning_gb"]')).toBeVisible();
    await expect(page.locator('#error-summary')).toBeFocused();
    await page.locator('[name="warning_gb"]').fill('30');
    await page.locator('button[value="apply"]').click();
    await expect(page.locator('[role=status]')).toContainText('Changes saved');
    await expect(page.locator('[name="warning_gb"]')).toHaveValue('30');
    await page.goto(base + '/item-1.html');
    await page.locator('#media_kind').selectOption('Movie');
    await page.locator('.movie-only > summary').click();
    await expect(page.locator('#collection_role')).toBeVisible();
    report.workflows.push('Disclosures: keyboard expand/collapse, responsive reflow, conditional fields and invalid-input recovery');
    await page.goto(base + '/settings.html');
    await page.locator('[name="metadata_title"]').fill('Pending list title');
    await page.getByRole('button', { name: 'Test saved source', exact: true }).click();
    await expect(page).toHaveURL(/source-check.html$/);
    await page.goto(base + '/settings.html');
    await expect(page.locator('[name="metadata_title"]')).toHaveValue('Pending list title');
    await page.locator('button[value="discard"]').click();
    report.workflows.push('Source tools preserve pending settings when opening a saved-source check');

    // Ordinary bulk controls and settings are visible without nested sections.
    for (const theme of ['dark', 'light']) {
      for (const width of [320, 390, 720, 1440]) {
        await page.setViewportSize({width,height:900});
        await page.emulateMedia({colorScheme:theme});
        for (const file of ['index.html','settings.html','options.html']) {
          await page.goto(base+'/'+file);
          if(file==='index.html') {
            await page.locator('#group-actions > summary').click();
            for(const name of ['selected_season','selected_first_episode','bulk_kind','bulk_behavior','bulk_cookies','bulk_sponsor']) await expect(page.locator(`[name="${name}"]`)).toBeVisible();
            await expect(page.locator('#group-actions details')).toHaveCount(1);
          } else if(file==='settings.html') {
            for(const name of ['metadata_genres','metadata_tags']) await expect(page.locator(`[name="${name}"]`)).toBeVisible();
            await expect(page.getByRole('button',{name:'Test saved source',exact:true})).toBeVisible();
          } else {
            await expect(page.locator('main details')).toHaveCount(0);
            for(const name of ['cookie_policy','sponsor_policy','warning_gb']) await expect(page.locator(`[name="${name}"]`)).toBeVisible();
          }
          assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth-innerWidth),0);
          if(theme==='dark' && [390,1440].includes(width)) await page.screenshot({path:path.join(__dirname,'screenshots',`simplified-${engine}-${file.replace('.html','')}-${width}.png`),fullPage:file!=='index.html'});
        }
      }
    }
    // The exposed fields still participate in their original preview and Apply paths.
    await page.goto(base+'/index.html');
    await page.locator('[name="selected_ids"][value="2"]').check();
    await page.locator('#group-actions > summary').click();
    await page.locator('[name="bulk_kind"]').selectOption('Movie');
    await page.locator('[name="bulk_cookies"]').selectOption('Firefox');
    await page.locator('[value="preview-bulk"]').click();
    await expect(page.locator('[data-bulk-result]')).toContainText('Movie');
    await expect(page.locator('[data-bulk-result]')).toContainText('Firefox');
    await page.goto(base+'/options.html');
    await page.locator('[name="cookie_policy"]').selectOption('Firefox');
    await page.locator('[value="apply"]').click();
    await expect(page.locator('[data-saved="cookies"]')).toContainText('Firefox');
    await page.locator('[name="cookie_policy"]').selectOption('default');
    await page.locator('[value="apply"]').click();
    await expect(page.locator('[data-saved="cookies"]')).toContainText('No cookies');
    await expect(page.locator('[data-saved="cookies"]')).toContainText('Using add-on default');
    await page.locator('[name="cookie_policy"]').selectOption('None');
    await page.locator('[name="sponsor_policy"]').selectOption('on');
    await page.locator('[value="apply"]').click();
    await expect(page.locator('[data-saved="cookies"]')).toContainText('Set for this list');
    await expect(page.locator('[data-saved="sponsor"]')).toContainText('On');
    await page.locator('[name="cookie_policy"]').selectOption('default');
    await page.locator('[name="warning_gb"]').fill('-1');
    await page.locator('[value="apply"]').click();
    await expect(page.locator('#error-summary')).toContainText('Disk warning');
    await expect(page.locator('[data-saved="cookies"]')).toContainText('Set for this list');
    await page.locator('[value="discard"]').click();
    await expect(page.locator('[name="cookie_policy"]')).toHaveValue('None');
    await page.goto(base+'/index.html');
    await expect(page.locator('.publication-state')).toContainText('Saved changes ready to publish');
    await page.locator('[name="items[2].episode"]').fill('81');
    await page.locator('[value="publish"]').click();
    await expect(page.locator('[name="items[2].episode"]')).toHaveValue('81');
    await expect(page.locator('.publication-state')).toContainText('Published list is up to date');
    await expect(page.locator('[value="publish"]')).not.toBeVisible();
    report.workflows.push('Default versus explicit equal-valued override remains distinct; invalid settings do not partly save; publishing saved changes retains newer unsaved numbering');
    report.workflows.push('Simplified bulk groups and visible settings: reflow, field visibility, metadata/options preview and package-default reset');

    // A conflict outside the filter/page stays reachable without losing other drafts or selection.
    const isolated = await browser.newContext({ javaScriptEnabled: false, viewport: { width: 390, height: 844 } });
    const long = await isolated.newPage();
    await long.goto(base + '/long-list.html');
    await long.locator('[name="selected_ids"][value="2"]').check();
    await long.locator('[name="items[2].season"]').fill('3');
    await long.locator('[name="items[2].episode"]').fill('4');
    await long.locator('[name="item_query"]').fill('opening');
    await long.locator('button[value="filter"]').click();
    await long.locator('[value="apply-numbers"]').first().click();
    await expect(long.locator('#error-summary')).toBeFocused();
    await long.locator('[value="focus-numbering:13"]').click();
    await expect(long.locator('#numbering-13-episode')).toBeFocused();
    await long.locator('#numbering-13-episode').fill('40');
    await long.locator('[value="focus-numbering:2"]').click();
    await expect(long.locator('#numbering-2-episode')).toBeFocused();
    await expect(long.locator('[name="selected_ids"][value="2"]')).toBeChecked();
    await expect(long.locator('#numbering-2-episode')).toHaveValue('4');
    await long.locator('[value="apply-numbers"]').first().click();
    await expect(long.locator('[role=status]')).toContainText('Numbering saved');
    report.workflows.push('Off-page and filtered conflict recovery retains edits and checked selection');
    await long.locator('[name="items[2].episode"]').fill('41');
    const pager = long.getByRole('navigation', { name: 'Item pagination' });
    const sizes = pager.getByLabel('Items per page');
    assert.deepEqual(await sizes.locator('option').allTextContents(), ['10', '25', '50', '100', 'All items']);
    await expect(long.locator('select[name=page_size]')).toHaveCount(1);
    for (const size of ['25', '50', '100', 'All', '10']) {
      await sizes.selectOption(size);
      await pager.getByRole('button', { name: 'Apply page size' }).click();
      await expect(long.locator('.episode-row')).toHaveCount(size === 'All' ? 36 : Math.min(36, Number(size)));
      await expect(sizes).toHaveValue(size);
      await expect(long.locator('[name="items[2].episode"]')).toHaveValue('41');
      await expect(long.locator('[name="selected_ids"][value="2"]')).toBeChecked();
      assert.equal(await long.evaluate(() => document.documentElement.scrollWidth - innerWidth), 0);
    }
    await pager.getByRole('button', { name: 'Next page' }).click();
    await expect(long.locator('.episode-row').first()).toHaveAttribute('id', 'episode-11');
    await pager.getByRole('button', { name: 'Previous page' }).click();
    await expect(long.locator('[name="items[2].episode"]')).toHaveValue('41');
    await expect(long.locator('[name="selected_ids"][value="2"]')).toBeChecked();
    report.workflows.push('Pagination sizes 10/25/50/100/All, next/previous, draft and selection retention');
    await isolated.close(); await context.close();
    fs.writeFileSync(path.join(__dirname, `ux-fixes-${engine}-verification.json`), JSON.stringify(report, null, 2) + '\n');
    console.log(JSON.stringify(report));
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
