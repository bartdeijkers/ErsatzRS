import { expect, test, type Page } from '@playwright/test';
import { dirname, join, resolve } from 'node:path';
import { bounded, cultures, field, marker, publication, submit, translation, visit } from './helpers/teletext-native-editor';
import { waitForHydration } from './hydration';

const readyPath = process.env.ERSATZRS_TELETEXT_POLISH_ADMIN_READY;
type Ready = {
  probe_version: number; case: string; base_url: string; channel_id: number;
  design_id: number; revision: number; digest: string; marker_file: string; timeout_seconds: number;
};
function preview(ready: Ready, revision: number): string {
  return `/channels/${ready.channel_id}/teletext?design_id=${ready.design_id}&revision=${revision}&page=124&subcode=0020`;
}
async function loaded(page: Page, url: string, js: boolean): Promise<void> {
  await visit(page, url);
  if (js) await waitForHydration(page);
}

test('explicit secondary agreement survives localized native forms and exact undo', async ({ browser }, testInfo) => {
  test.skip(!readyPath, 'Requires the owned secondary-default native router fixture.');
  test.setTimeout(540_000);
  const directory = dirname(resolve(readyPath!));
  const ready = JSON.parse(bounded(readyPath!).toString('utf8')) as Ready;
  expect(ready.probe_version).toBe(1);
  expect(ready.case).toBe('teletext-secondary-default');
  expect(ready.base_url).toMatch(/^http:\/\/127\.0\.0\.1:\d+$/);
  expect(ready.revision).toBe(1);
  expect(ready.timeout_seconds).toBe(600);
  expect(resolve(ready.marker_file)).toBe(join(directory, 'browser-complete.json'));
  expect(['chromium', 'firefox']).toContain(testInfo.project.name);
  const offset = testInfo.project.name === 'chromium' ? 0 : 6;
  if (offset) expect(JSON.parse(bounded(join(directory, 'chromium-secondary.json')).toString('utf8')).revision).toBe(7);
  let revision = 1 + offset;
  const observations: unknown[] = [];
  for (const js of [true, false]) {
    const context = await browser.newContext({ baseURL: ready.base_url, javaScriptEnabled: js,
      viewport: { width: 1280, height: 900 } });
    const page = await context.newPage();
    page.setDefaultTimeout(15_000); page.setDefaultNavigationTimeout(30_000);
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
    try {
      expect((await page.request.post('/admin-api/settings/ui/save', {
        form: { language: 'en-us', is_dark_mode: 'false' }, maxRedirects: 0,
      })).status()).toBe(303);
      await loaded(page, preview(ready, revision), js);
      const before = await publication(page);
      for (const choice of ['no_second', 'designated', 'original']) {
        const form = page.getByTestId('teletext-secondary-default-form');
        await form.locator('[name="name"]').fill(`Synthetic context ${revision}`);
        await form.locator('[name="choice"]').selectOption(choice);
        await form.locator('[name="designation"]').fill(choice === 'designated' ? '37' : '');
        await submit(page, form);
        if (js) await waitForHydration(page);
        revision += 1;
        expect(new URL(page.url()).searchParams.get('revision')).toBe(String(revision));
        await expect(form.locator('[name="choice"]')).toHaveValue(choice);
        if (choice === 'designated') await expect(form.locator('[name="designation"]')).toHaveValue('37');
        const state = await publication(page);
        expect(state.generation_id).toBe(before.generation_id);
        expect(state.request_id).toBe(before.request_id);
        if (choice === 'original') expect(state.digest).toBe(ready.digest);
        if (choice !== 'no_second') continue;
        for (const [language, module] of cultures) for (const dark of [false, true]) {
          expect((await page.request.post('/admin-api/settings/ui/save', {
            form: { language, is_dark_mode: String(dark) }, maxRedirects: 0,
          })).status()).toBe(303);
          await loaded(page, `${preview(ready, revision)}&error=secondary-default-invalid`, js);
          await expect(page.locator('html')).toHaveAttribute('lang', language);
          await expect(page.locator('[data-theme]').first()).toHaveAttribute('data-theme', dark ? 'dark' : 'light');
          await expect(form.locator('legend')).toContainText(translation(module, 'TeletextSecondaryDefaultTitle'));
          await expect(form.locator('[name="choice"]')).toHaveAccessibleName(translation(module, 'TeletextSecondaryDefaultChoice'));
          await expect(form.locator('[name="designation"]')).toHaveAccessibleName(translation(module, 'TeletextSecondaryDefaultCode'));
          await expect(form.locator('button')).toHaveText(translation(module, 'TeletextSecondaryDefaultSave'));
          await expect(page.getByText(translation(module, 'TeletextSecondaryDefaultHelp'), { exact: true })).toBeVisible();
          await expect(page.getByText(translation(module, 'TeletextSecondaryDefaultInvalid'), { exact: true })).toBeVisible();
          expect(await field(form, 'revision')).toBe(String(revision));
          await expect(form.locator('[name="choice"]')).toHaveValue('no_second');
          await form.scrollIntoViewIfNeeded();
          const screenshot = `${js ? 'js' : 'no-js'}-${language}-${dark ? 'dark' : 'light'}-secondary.png`;
          await page.screenshot({ path: testInfo.outputPath(screenshot) });
          observations.push({ js, language, dark, screenshot });
        }
        expect((await page.request.post('/admin-api/settings/ui/save', {
          form: { language: 'en-us', is_dark_mode: 'false' }, maxRedirects: 0,
        })).status()).toBe(303);
        await loaded(page, preview(ready, revision), js);
      }
      await loaded(page, preview(ready, revision - 1), js);
      const historical = page.getByTestId('teletext-secondary-default-form');
      await expect(historical.locator('fieldset')).toHaveAttribute('disabled', '');
      await expect(historical.locator('button')).toBeDisabled();
      await expect(historical.locator('[name="choice"]')).toHaveValue('designated');
      await loaded(page, preview(ready, revision), js);
      const form = page.getByTestId('teletext-secondary-default-form');
      for (const width of [320, 1920]) {
        await page.setViewportSize({ width, height: 900 });
        await form.scrollIntoViewIfNeeded();
        expect(await form.evaluate(element => element.scrollWidth <= element.clientWidth + 1)).toBe(true);
        await page.screenshot({ path: testInfo.outputPath(`${js ? 'js' : 'no-js'}-${width}-secondary.png`) });
      }
      expect(errors).toEqual([]);
    } finally { await context.close(); }
  }
  expect(revision).toBe(offset ? 13 : 7);
  const complete = { probe_version: 1, case: ready.case, design_id: ready.design_id, revision, digest: ready.digest };
  marker(directory, `${testInfo.project.name}-secondary.json`, { ...complete, observations });
  if (offset) marker(directory, 'browser-complete.json', complete);
});
