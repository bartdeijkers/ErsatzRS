import { expect, test, type Locator, type Page } from '@playwright/test';
import { dirname, join, resolve } from 'node:path';
import {
  bounded, cultures, field, marker, publication, raster, rasterHash, submit, translation, visit,
} from './helpers/teletext-native-editor';
import { waitForHydration } from './hydration';

// The existing owned native fixture verifies each complete model/encoded unit,
// then separate publication and cold recovery. These are actual SSR form actions.
const readyPath = process.env.ERSATZRS_TELETEXT_POLISH_ADMIN_READY;
type Resource = { kind: string; page: { magazine: number; page: number }; logical_subpage: number };
type Address = { page: number; subcode: number };
type Ready = {
  probe_version: number; case: string; base_url: string; channel_id: number;
  design_id: number; revision: number; digest: string; resources: Resource[];
  displays: Address[]; marker_file: string; timeout_seconds: number;
};
function preview(ready: Ready, revision: number): string {
  return `/channels/${ready.channel_id}/teletext?design_id=${ready.design_id}&revision=${revision}&page=124&subcode=0020`;
}
async function option(select: Locator, expected: unknown): Promise<string> {
  const values = await select.locator('option').evaluateAll((options, expected) => {
    const normalized = (value: unknown): unknown => {
      if (Array.isArray(value)) return value.map(normalized);
      if (value !== null && typeof value === 'object') return Object.fromEntries(
        Object.entries(value).sort(([left], [right]) => left.localeCompare(right))
          .map(([key, item]) => [key, normalized(item)]));
      return value;
    };
    const key = JSON.stringify(normalized(expected));
    return options.filter(value => {
      const content = (value as HTMLOptionElement).value;
      return content && JSON.stringify(normalized(JSON.parse(content))) === key;
    }).map(value => (value as HTMLOptionElement).value);
  }, expected);
  expect(values).toHaveLength(1); return values[0];
}
async function settings(page: Page, language: string, dark: boolean): Promise<void> {
  expect((await page.request.post('/admin-api/settings/ui/save', {
    form: { language, is_dark_mode: String(dark) }, maxRedirects: 0,
  })).status()).toBe(303);
}
async function labels(page: Page, module: string): Promise<void> {
  for (const [id, prefix] of [
    ['teletext-resource-bank-add-form', 'TeletextResourceBank'],
    ['teletext-object-subpages-announce-form', 'TeletextSubpageAnnouncement'],
  ]) {
    const form = page.getByTestId(id);
    await expect(form.locator('legend')).toHaveText(translation(module, `${prefix}Title`));
    await expect(form.locator('p').first()).toHaveText(translation(module, `${prefix}Help`));
    await expect(form.locator('button[type="submit"]')).toHaveText(translation(module, `${prefix}Action`));
  }
}
async function cultureAndLayout(page: Page, ready: Ready, revision: number, hydrated: boolean, output: (name: string) => string): Promise<void> {
  for (const [language, module] of cultures) {
    await settings(page, language, false);
    await visit(page, preview(ready, revision)); if (hydrated) await waitForHydration(page);
    await expect(page.locator('html')).toHaveAttribute('lang', language);
    await labels(page, module);
  }
  for (const width of [320, 1920]) {
    await page.setViewportSize({ width, height: 900 });
    for (const id of ['teletext-resource-bank-add-form', 'teletext-object-subpages-announce-form']) {
      const form = page.getByTestId(id);
      await form.scrollIntoViewIfNeeded();
      const box = await form.boundingBox(); expect(box).not.toBeNull();
      expect(box!.x).toBeGreaterThanOrEqual(0); expect(box!.x + box!.width).toBeLessThanOrEqual(width);
      expect(await form.evaluate(element => element.scrollWidth <= element.clientWidth)).toBe(true);
      await expect(form.locator('button[type="submit"]')).toBeVisible();
      await page.screenshot({ path: output(`${id}-${width}.png`) });
    }
  }
  await page.setViewportSize({ width: 1280, height: 900 });
}
async function fill(page: Page, ready: Ready, resource: Resource, operation: number): Promise<Locator> {
  const id = operation === 0 ? 'teletext-resource-bank-add-form'
    : operation === 1 ? 'teletext-object-add-form' : 'teletext-object-subpages-announce-form';
  const form = page.getByTestId(id);
  await form.locator('[name="name"]').fill(`Explicit ${resource.kind} bank step ${operation}`);
  if (operation === 0) {
    await form.locator('[name="display"]').selectOption(await option(form.locator('[name="display"]'), ready.displays[0]));
    await form.locator('[name="resource_kind"]').selectOption(resource.kind);
    await form.locator('[name="resource_page"]').fill(`${resource.page.magazine}${resource.page.page.toString(16).padStart(2, '0')}`);
    await form.locator('[name="logical_subpage"]').fill('14');
    await form.locator('[name="counter"]').fill('7');
    await form.locator('[name="update"]').selectOption('changed');
    await form.locator('[name="last_subpage"]').selectOption('true');
  } else {
    await form.locator('[name="resource"]').selectOption(await option(form.locator('[name="resource"]'), resource));
    if (operation === 1) {
      await form.locator('[name="object_type"]').selectOption('active');
      await form.locator('[name="new_object_id"]').fill('32');
    } else {
      const values: string[] = [];
      for (const display of ready.displays) values.push(await option(form.locator('[name="display"]'), display));
      await form.locator('[name="display"]').selectOption(values);
    }
  }
  return form;
}

test('create and announce POP and GPOP banks through localized current draft forms', async ({ browser }, testInfo) => {
  test.skip(!readyPath, 'Requires the owned explicit-bank native fixture.');
  test.setTimeout(240_000);
  const directory = dirname(resolve(readyPath!));
  const ready = JSON.parse(bounded(readyPath!).toString('utf8')) as Ready;
  expect(ready.probe_version).toBe(1); expect(ready.case).toBe('teletext-explicit-bank-dependencies');
  expect(ready.base_url).toMatch(/^http:\/\/127\.0\.0\.1:\d+$/);
  expect(ready.revision).toBe(3); expect(ready.resources).toHaveLength(2); expect(ready.displays).toHaveLength(3);
  expect(resolve(ready.marker_file)).toBe(join(directory, 'browser-complete.json'));
  expect(ready.timeout_seconds).toBe(600);
  expect(['chromium', 'firefox']).toContain(testInfo.project.name);
  const js = testInfo.project.name === 'chromium', language = js ? 'nl' : 'en-us', module = js ? 'nl' : 'en_us';
  if (!js) expect(JSON.parse(bounded(join(directory, 'chromium-bank-dependencies.json')).toString('utf8')).revision).toBe(6);
  const resource = ready.resources[js ? 0 : 1];
  expect(resource.kind).toBe(js ? 'pop' : 'gpop'); expect(resource.logical_subpage).toBe(14);
  expect(resource.page).toEqual({ magazine: 2, page: js ? 0xc0 : 0xd0 });
  testInfo.annotations.push({ type: 'browser-version', description: browser.version() });
  const context = await browser.newContext({ baseURL: ready.base_url, javaScriptEnabled: js, viewport: { width: 1280, height: 900 } });
  const page = await context.newPage(); page.setDefaultTimeout(15_000); page.setDefaultNavigationTimeout(30_000);
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
  let revision = js ? 3 : 6, digest = '';
  try {
    if (js) await cultureAndLayout(page, ready, revision, js, name => testInfo.outputPath(name));
    await settings(page, language, !js);
    for (let operation = 0; operation < 3; operation += 1) {
      await visit(page, preview(ready, revision)); if (js) await waitForHydration(page);
      await expect(page.locator('html')).toHaveAttribute('lang', language);
      await expect(page.locator('[data-theme]').first()).toHaveAttribute('data-theme', js ? 'light' : 'dark');
      await labels(page, module);
      const before = await publication(page), pixels = rasterHash(await raster(page, 124, 672));
      const form = await fill(page, ready, resource, operation);
      expect(await field(form, 'revision')).toBe(String(revision));
      expect(await field(form, 'expected_design_head')).toBe(String(revision));
      await form.scrollIntoViewIfNeeded(); await page.screenshot({ path: testInfo.outputPath(`${operation}-before.png`) });
      await submit(page, form); if (js) await waitForHydration(page); revision += 1;
      const current = page.getByTestId('teletext-resource-bank-add-form');
      expect(await field(current, 'revision')).toBe(String(revision));
      expect(new URL(page.url()).searchParams.get('saved')).toBe('draft');
      digest = await field(current, 'digest'); expect(digest).toMatch(/^[a-f0-9]{64}$/);
      expect(rasterHash(await raster(page, 124, 672))).toBe(pixels);
      const after = await publication(page);
      expect([after.generation_id, after.request_id]).toEqual([before.generation_id, before.request_id]);
      await page.getByTestId('teletext-object-subpages-announce-form').scrollIntoViewIfNeeded();
      await page.screenshot({ path: testInfo.outputPath(`${operation}-after.png`) });
    }
    await visit(page, preview(ready, revision - 1)); if (js) await waitForHydration(page);
    for (const id of ['teletext-resource-bank-add-form', 'teletext-object-subpages-announce-form']) {
      await expect(page.getByTestId(id).locator('fieldset')).toHaveAttribute('disabled', '');
    }
    expect(errors).toEqual([]);
  } finally { await settings(page, 'en-us', false); await context.close(); }
  expect(revision).toBe(js ? 6 : 9);
  marker(directory, `${testInfo.project.name}-bank-dependencies.json`, {
    probe_version: 1, case: ready.case, design_id: ready.design_id, revision, digest,
    resource, language, dark: !js, javascript: js, browser: browser.version(), errors,
  });
  if (!js) marker(directory, 'browser-complete.json', {
    probe_version: 1, case: ready.case, design_id: ready.design_id, revision, digest,
  });
});
