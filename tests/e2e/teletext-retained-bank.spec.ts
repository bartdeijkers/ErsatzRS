import { expect, test, type Page } from '@playwright/test';
import { dirname, join, resolve } from 'node:path';
import {
  bounded, field, marker, publication, raster, rasterHash, submit, translation, visit,
} from './helpers/teletext-native-editor';
import { waitForHydration } from './hydration';

// The same native forms and owned fixture/marker boundary as definition Add/Copy.
// Full native models/units, publication and cold reopen are checked by the owner.
// This full-bank/side-panel fixture retains the existing strict672x250 oracle;
// the separate empty first-definition fixture uses480x250.
const readyPath = process.env.ERSATZRS_TELETEXT_POLISH_ADMIN_READY;
type Resource = { kind: string; page: { magazine: number; page: number }; logical_subpage: number };
type ObjectIdentity = { scope: string; resource: Resource; object_id: number };
type Ready = {
  probe_version: number; case: string; base_url: string; channel_id: number;
  design_id: number; revision: number; digest: string; empty_resource: Resource;
  source_object: ObjectIdentity; new_ids: number[]; marker_file: string; timeout_seconds: number;
};
function preview(ready: Ready, revision: number): string {
  return `/channels/${ready.channel_id}/teletext?design_id=${ready.design_id}&revision=${revision}&page=124&subcode=0020`;
}
async function resourceValue(page: Page, expected: Resource): Promise<string> {
  const values = await page.locator('#teletext-object-add-resource option').evaluateAll((options, expected) =>
    options.filter(option => {
      if (!(option as HTMLOptionElement).value) return false;
      const actual = JSON.parse((option as HTMLOptionElement).value);
      return actual.kind === expected.kind && actual.page.magazine === expected.page.magazine
        && actual.page.page === expected.page.page && actual.logical_subpage === expected.logical_subpage;
    }).map(option => (option as HTMLOptionElement).value), expected);
  expect(values).toHaveLength(1); return values[0];
}
async function sourceValue(page: Page, expected: ObjectIdentity): Promise<string> {
  const values = await page.locator('#teletext-object-copy-source option').evaluateAll((options, expected) =>
    options.filter(option => {
      const actual = JSON.parse((option as HTMLOptionElement).value);
      return actual.scope === expected.scope && actual.object_id === expected.object_id
        && actual.resource.kind === expected.resource.kind
        && actual.resource.page.magazine === expected.resource.page.magazine
        && actual.resource.page.page === expected.resource.page.page
        && actual.resource.logical_subpage === expected.resource.logical_subpage;
    }).map(option => (option as HTMLOptionElement).value), expected);
  expect(values).toHaveLength(1); return values[0];
}

test('fill and copy retained new banks through localized current native draft forms', async ({ browser }, testInfo) => {
  test.skip(!readyPath, 'Requires the owned retained-bank native fixture.');
  test.setTimeout(180_000);
  const directory = dirname(resolve(readyPath!));
  const ready = JSON.parse(bounded(readyPath!).toString('utf8')) as Ready;
  expect(ready.probe_version).toBe(1); expect(ready.case).toBe('teletext-retained-bank-definitions');
  expect(ready.base_url).toMatch(/^http:\/\/127\.0\.0\.1:\d+$/);
  expect(ready.revision).toBe(3); expect(ready.new_ids).toEqual([32, 33, 34, 35]);
  expect(resolve(ready.marker_file)).toBe(join(directory, 'browser-complete.json'));
  expect(ready.timeout_seconds).toBe(600);
  expect(['chromium', 'firefox']).toContain(testInfo.project.name);
  const js = testInfo.project.name === 'chromium';
  const offset = js ? 0 : 2, language = js ? 'nl' : 'en-us', module = js ? 'nl' : 'en_us';
  if (!js) expect(JSON.parse(bounded(join(directory, 'chromium-retained-bank.json')).toString('utf8')).revision).toBe(5);
  testInfo.annotations.push({ type: 'browser-version', description: browser.version() });
  const context = await browser.newContext({ baseURL: ready.base_url, javaScriptEnabled: js,
    viewport: { width: 1280, height: 900 } });
  const page = await context.newPage();
  page.setDefaultTimeout(15_000); page.setDefaultNavigationTimeout(30_000);
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
  let revision = ready.revision + offset, digest = '';
  try {
    expect((await page.request.post('/admin-api/settings/ui/save', {
      form: { language, is_dark_mode: String(!js) }, maxRedirects: 0,
    })).status()).toBe(303);
    for (const [index, operation] of ['add', 'copy'].entries()) {
      await visit(page, preview(ready, revision)); if (js) await waitForHydration(page);
      await expect(page.locator('html')).toHaveAttribute('lang', language);
      await expect(page.locator('[data-theme]').first()).toHaveAttribute('data-theme', js ? 'light' : 'dark');
      const before = await publication(page), pixels = rasterHash(await raster(page, 124, 672));
      const form = page.getByTestId(`teletext-object-${operation}-form`);
      await expect(form.locator('legend')).toHaveText(translation(module, operation === 'add' ? 'TeletextObjectAddTitle' : 'TeletextObjectCopyTitle'));
      expect(await field(form, 'revision')).toBe(String(revision));
      expect(await field(form, 'expected_design_head')).toBe(String(revision));
      await form.locator('[name="name"]').fill(`Retained bank ${operation} ${ready.new_ids[offset + index]}`);
      if (operation === 'add') {
        await form.locator('[name="resource"]').selectOption(await resourceValue(page, ready.empty_resource));
        await form.locator('[name="object_type"]').selectOption('active');
      } else await form.locator('[name="source_object"]').selectOption(await sourceValue(page, ready.source_object));
      await form.locator('[name="new_object_id"]').fill(String(ready.new_ids[offset + index]));
      await form.scrollIntoViewIfNeeded();
      await page.screenshot({ path: testInfo.outputPath(`${operation}-before.png`) });
      await submit(page, form); if (js) await waitForHydration(page);
      revision += 1;
      const current = page.getByTestId('teletext-object-add-form');
      expect(await field(current, 'revision')).toBe(String(revision));
      expect(new URL(page.url()).searchParams.get('saved')).toBe('draft');
      digest = await field(current, 'digest'); expect(digest).toMatch(/^[a-f0-9]{64}$/);
      expect(rasterHash(await raster(page, 124, 672))).toBe(pixels);
      const after = await publication(page);
      expect([after.generation_id, after.request_id]).toEqual([before.generation_id, before.request_id]);
      if (operation === 'copy') await sourceValue(page, { ...ready.source_object, object_id: ready.new_ids[offset + index] });
      await page.getByTestId(`teletext-object-${operation}-form`).scrollIntoViewIfNeeded();
      await page.screenshot({ path: testInfo.outputPath(`${operation}-after.png`) });
    }
    await visit(page, preview(ready, revision - 1)); if (js) await waitForHydration(page);
    for (const operation of ['add', 'copy']) await expect(page.getByTestId(`teletext-object-${operation}-form`).locator('fieldset')).toHaveAttribute('disabled', '');
    expect(errors).toEqual([]);
  } finally {
    expect((await page.request.post('/admin-api/settings/ui/save', {
      form: { language: 'en-us', is_dark_mode: 'false' }, maxRedirects: 0,
    })).status()).toBe(303);
    await context.close();
  }
  expect(revision).toBe(js ? 5 : 7);
  marker(directory, `${testInfo.project.name}-retained-bank.json`, {
    probe_version: 1, case: ready.case, design_id: ready.design_id, revision, digest,
    new_ids: ready.new_ids.slice(offset, offset + 2), language, dark: !js, javascript: js,
    browser: browser.version(), errors,
  });
  if (!js) marker(directory, 'browser-complete.json', {
    probe_version: 1, case: ready.case, design_id: ready.design_id, revision, digest, new_ids: ready.new_ids,
  });
});
