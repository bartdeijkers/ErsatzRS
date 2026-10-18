import { expect, test, type Locator, type Page } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { resolve, sep } from 'node:path';
import { cultures, translation } from './helpers/teletext-native-editor';
import { waitForHydration } from './hydration';

const dbPath = process.env.E2E_DB_PATH;

function sqlite(sql: string): string {
  if (!dbPath || !resolve(dbPath).startsWith(`${resolve('tmp')}${sep}`)) {
    throw new Error('This proof requires an explicit throwaway database under repository tmp/.');
  }
  return execFileSync('sqlite3', [dbPath], {
    input: `.timeout 10000\n${sql}`, encoding: 'utf8',
  }).trim();
}

async function load(page: Page, id: number, js: boolean): Promise<void> {
  expect((await page.goto(`/channels/${id}`))?.status()).toBe(200);
  await expect(page.locator('[data-not-found]')).toHaveCount(0);
  if (js) await waitForHydration(page);
}

async function waitForSavedList(page: Page, id: number, js: boolean): Promise<void> {
  await expect(page).toHaveURL(/\/channels$/);
  if (js) {
    await waitForHydration(page);
    await expect(page.locator(`a[href="/channels/${id}"]`)).toBeVisible();
  }
}

async function assertViewportFit(locator: Locator) {
  await expect(locator).toBeVisible();
  const geometry = await locator.evaluate(element => {
    const bounds = element.getBoundingClientRect();
    return {
      left: bounds.left, right: bounds.right, top: bounds.top, bottom: bounds.bottom,
      width: bounds.width,
      clientWidth: element.clientWidth, scrollWidth: element.scrollWidth,
      viewportWidth: document.documentElement.clientWidth,
      viewportHeight: document.documentElement.clientHeight,
    };
  });
  expect(geometry.width).toBeGreaterThan(0);
  expect(geometry.left).toBeGreaterThanOrEqual(-1);
  expect(geometry.right).toBeLessThanOrEqual(geometry.viewportWidth + 1);
  expect(geometry.top).toBeGreaterThanOrEqual(-1);
  expect(geometry.bottom).toBeLessThanOrEqual(geometry.viewportHeight + 1);
  expect(geometry.scrollWidth).toBeLessThanOrEqual(geometry.clientWidth + 1);
  return geometry;
}

test('localized source-caption choice persists through native forms and rejects incompatible output atomically', async ({ browser }, testInfo) => {
  test.skip(!dbPath, 'Requires an explicit throwaway E2E_DB_PATH with a native test channel.');
  test.setTimeout(360_000);
  testInfo.annotations.push({ type: 'browser-version', description: browser.version() });
  const channel = JSON.parse(sqlite(`SELECT json_object('id', "Id", 'uuid', "UniqueId", 'engine', "StreamingEngine", 'mode', "StreamingMode") FROM "Channel" ORDER BY "Id" LIMIT 1;`));
  expect(channel.engine).toBe(0);
  expect([1, 5]).toContain(channel.mode);
  const key = `channel.${channel.uuid}.source_caption_delivery`;
  const selected = () => sqlite(`SELECT "Value" FROM "ConfigElement" WHERE "Key"='${key}';`);
  const others = () => sqlite(`SELECT "Key", "Value" FROM "ConfigElement" WHERE "Key" LIKE 'channel.%.source_caption_delivery' AND "Key"<>'${key}' ORDER BY "Key";`);
  const otherChoices = others();
  const observations: unknown[] = [];
  for (const js of [true, false]) {
    const context = await browser.newContext({
      baseURL: process.env.ERSATZRS_BASE_URL ?? 'http://127.0.0.1:8409',
      javaScriptEnabled: js, viewport: { width: 1280, height: 900 },
    });
    const page = await context.newPage();
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
    try {
      for (const [language, module] of cultures) for (const dark of [false, true]) {
        expect((await page.request.post('/admin-api/settings/ui/save', {
          form: { language, is_dark_mode: String(dark) }, maxRedirects: 0,
        })).status()).toBe(303);
        await load(page, channel.id, js);
        await expect(page.locator('html')).toHaveAttribute('lang', language);
        await expect(page.locator('[data-theme]').first()).toHaveAttribute('data-theme', dark ? 'dark' : 'light');
        const field = page.locator('select[name="source_teletext_delivery"]');
        await expect(field).toHaveAccessibleName(translation(module, 'SourceTeletextDelivery'));
        await expect(field.locator('option')).toHaveText([
          translation(module, 'SourceTeletextDeliveryBurn'), translation(module, 'SourceTeletextDeliveryDvb'),
        ]);
        await expect(field).toBeEnabled();
        await field.selectOption('0');
        await field.focus();
        await page.keyboard.press('ArrowDown');
        await expect(field).toHaveValue('1');
        await page.getByRole('button', { name: translation(module, 'SaveChannel'), exact: true }).click();
        await waitForSavedList(page, channel.id, js);
        expect(selected()).toBe('1');
        await load(page, channel.id, js);
        await expect(page.locator('select[name="source_teletext_delivery"]')).toHaveValue('1');
        await page.locator('select[name="streaming_engine"]').selectOption('1');
        await page.getByRole('button', { name: translation(module, 'SaveChannel'), exact: true }).click();
        await expect(page).toHaveURL(new RegExp(`/channels/${channel.id}\\?error=`));
        await expect(page.getByText(translation(module, 'SourceTeletextDeliveryUnsupported'), { exact: true })).toBeVisible();
        expect(selected()).toBe('1');
        expect(sqlite(`SELECT "StreamingEngine" FROM "Channel" WHERE "Id"=${channel.id};`)).toBe('0');
        expect(others()).toBe(otherChoices);
        const screenshot = `${js ? 'js' : 'no-js'}-${language}-${dark ? 'dark' : 'light'}.png`;
        await page.screenshot({ path: testInfo.outputPath(screenshot), fullPage: true });
        const fieldScreenshot = screenshot.replace('.png', '-delivery.png');
        await field.scrollIntoViewIfNeeded();
        await expect(field).toBeVisible();
        await page.screenshot({ path: testInfo.outputPath(fieldScreenshot) });
        const widths = [];
        for (const width of [320, 1920]) {
          await page.setViewportSize({ width, height: 900 });
          if (width === 320) {
            await expect.poll(() => page.locator('[data-nav-drawer]').evaluate(
              element => element.getBoundingClientRect().right,
            )).toBeLessThanOrEqual(1);
          }
          await field.scrollIntoViewIfNeeded();
          const fieldGeometry = await assertViewportFit(field);
          const labelGeometry = await assertViewportFit(page.locator('label[for="source_teletext_delivery"]'));
          const widthScreenshot = screenshot.replace('.png', `-delivery-${width}.png`);
          await page.screenshot({ path: testInfo.outputPath(widthScreenshot) });
          widths.push({ width, fieldGeometry, labelGeometry, screenshot: widthScreenshot });
        }
        await page.setViewportSize({ width: 1280, height: 900 });
        observations.push({ js, language, dark, screenshot, fieldScreenshot, widths });
        await page.locator('select[name="source_teletext_delivery"]').selectOption('0');
        await page.getByRole('button', { name: translation(module, 'SaveChannel'), exact: true }).click();
        await waitForSavedList(page, channel.id, js);
        expect(selected()).toBe('0');
      }
      expect(errors).toEqual([]);
    } finally {
      await context.close();
    }
  }
  await testInfo.attach('source-caption-delivery-observations', {
    body: JSON.stringify({ browser: browser.version(), channel_id: channel.id, observations }), contentType: 'application/json',
  });
});
