import { expect, test } from '@playwright/test';
import { cultures, translation } from './helpers/teletext-native-editor';

// Only the isolated wrapper may change the global interface settings.
test.skip(!process.env.E2E_DB_PATH, 'Run through scripts/e2e/playwright-e2e.sh.');
test.describe.configure({ mode: 'serial' });

for (const javaScriptEnabled of [true, false]) {
  for (const isDarkMode of [true, false]) {
    test(`help disclosure: JS=${javaScriptEnabled}, dark=${isDarkMode}`, async ({ browser, baseURL }) => {
      test.setTimeout(120_000);
      const context = await browser.newContext({ baseURL, javaScriptEnabled });
      const page = await context.newPage();
      const errors: string[] = [];
      page.on('pageerror', error => errors.push(error.message));
      page.on('console', message => {
        if (message.type() === 'error' && /hydration|marker node|wasm.*mismatch/i.test(message.text())) {
          errors.push(message.text());
        }
      });
      try {
        for (const [language, module] of cultures) {
          expect((await page.request.post('/admin-api/settings/ui/save', {
            form: { is_dark_mode: String(isDarkMode), language }, maxRedirects: 0,
          })).status()).toBe(303);
          await page.goto('/teletext-elements/add');
          if (javaScriptEnabled) {
            await expect.poll(() => page.evaluate(() =>
              (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated),
            { timeout: 15_000 }).toBe(true);
          }
          const disclosure = page.locator('[data-help-disclosure]');
          const summary = disclosure.locator('summary');
          const content = disclosure.locator('p').first();
          const show = translation(module, 'ShowExplanation');
          const hide = translation(module, 'HideExplanation');
          await expect(disclosure).toHaveCount(1);
          await expect(summary).toContainText(translation(module, 'TeletextElementFieldMarkerTipTitle'));
          for (const width of [1280, 320]) {
            await page.setViewportSize({ width, height: 900 });
            await expect(summary.getByText(show, { exact: true })).toBeVisible();
            await expect(content).toBeHidden();
            const bounds = await summary.boundingBox();
            expect(bounds!.height).toBeGreaterThanOrEqual(44);
            expect(bounds!.x).toBeGreaterThanOrEqual(0);
            expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(width + 1);
            // A real pointer action reveals the explanation without submitting.
            await summary.click();
            await expect(content).toBeVisible();
            const summaryBox = await summary.boundingBox();
            const contentBox = await content.boundingBox();
            expect(contentBox!.y - (summaryBox!.y + summaryBox!.height)).toBeGreaterThanOrEqual(12);
            await expect(summary.getByText(hide, { exact: true })).toBeVisible();
            await expect(summary.getByText(show, { exact: true })).toBeHidden();
            // Native keyboard toggling and visible focus also work without JS.
            await summary.focus();
            await page.keyboard.press('Tab');
            await page.keyboard.press('Shift+Tab');
            await page.keyboard.press('Enter');
            await expect(content).toBeHidden();
            await expect(summary).toBeFocused();
            expect(await summary.evaluate(element => getComputedStyle(element).outlineStyle)).toBe('solid');
            await page.keyboard.press('Space');
            await expect(content).toBeVisible();
            await page.keyboard.press('Space');
            await expect(content).toBeHidden();
          }
          await expect(page.locator('[data-teletext-element-upload]')).toBeVisible();
        }
        expect(errors).toEqual([]);
      } finally {
        await page.request.post('/admin-api/settings/ui/save', {
          form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0,
        });
        await context.close();
      }
    });
  }
}
