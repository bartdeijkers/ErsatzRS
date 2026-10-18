import { expect, test, type Page } from '@playwright/test';
import { dirname, join, resolve } from 'node:path';
import {
  bounded, cultures, field, marker, publication, raster, rasterHash, submit, translation, visit,
} from './helpers/teletext-native-editor';
import { waitForHydration } from './hydration';

// Existing saved preview -> explicit Add form -> native Add/V18 concept.
// The owned native fixture starts with genuinely empty POP/GPOP resources and
// checks complete output, exports, reopen and unchanged publication.
const readyPath = process.env.ERSATZRS_TELETEXT_POLISH_ADMIN_READY;
type ResourceIdentity = {
  kind: string; page: { magazine: number; page: number }; logical_subpage: number;
};
type Ready = {
  probe_version: number; case: string; base_url: string; channel_id: number;
  design_id: number; revision: number; digest: string; resources: ResourceIdentity[];
  new_ids: number[]; object_types: string[]; resource_indices: number[];
  page: number; subcode: string; marker_file: string; timeout_seconds: number;
};
function preview(ready: Ready, revision: number): string {
  return `/channels/${ready.channel_id}/teletext?design_id=${ready.design_id}&revision=${revision}&page=${ready.page}&subcode=${ready.subcode}`;
}
async function loaded(page: Page, url: string, js: boolean): Promise<void> {
  await visit(page, url);
  if (js) await waitForHydration(page);
}
async function resourceValue(page: Page, expected: ResourceIdentity): Promise<string> {
  const values = await page.locator('#teletext-object-add-resource option').evaluateAll((options, expected) => {
    return options.filter(option => {
      const value = (option as HTMLOptionElement).value;
      if (!value) return false;
      const actual = JSON.parse(value);
      return actual.kind === expected.kind && actual.page.magazine === expected.page.magazine
        && actual.page.page === expected.page.page && actual.logical_subpage === expected.logical_subpage;
    }).map(option => (option as HTMLOptionElement).value);
  }, expected);
  expect(values).toHaveLength(1);
  return values[0];
}

test('create first native objects in empty resources through localized hydrated and no-JS draft forms', async ({ browser }, testInfo) => {
  test.skip(!readyPath, 'Requires the owned native definition-add fixture.');
  test.setTimeout(540_000);
  testInfo.annotations.push({ type: 'browser-version', description: browser.version() });
  const directory = dirname(resolve(readyPath!));
  const ready = JSON.parse(bounded(readyPath!).toString('utf8')) as Ready;
  expect(ready.probe_version).toBe(1);
  expect(ready.case).toBe('teletext-definition-add');
  expect(ready.base_url).toMatch(/^http:\/\/127\.0\.0\.1:\d+$/);
  expect(ready.new_ids).toEqual([25, 26, 27, 28]);
  expect(ready.resource_indices).toEqual([0, 0, 1, 1]);
  expect(ready.object_types).toEqual(['passive', 'active', 'adaptive', 'passive']);
  expect(ready.resources.map(value => value.kind)).toEqual(['pop', 'gpop']);
  expect(ready.page).toBe(124); expect(ready.subcode).toBe('0020');
  expect(resolve(ready.marker_file)).toBe(join(directory, 'browser-complete.json'));
  expect(ready.timeout_seconds).toBe(600);
  const offset = testInfo.project.name === 'chromium' ? 0 : 2;
  expect(['chromium', 'firefox']).toContain(testInfo.project.name);
  if (offset === 2) {
    const first = JSON.parse(bounded(join(directory, 'chromium-add.json')).toString('utf8'));
    expect(first.revision).toBe(3);
    expect(first.new_ids).toEqual([25, 26]);
  }
  const observations: unknown[] = [];
  let revision = ready.revision + offset;
  let digest = '';
  for (const js of [true, false]) {
    const context = await browser.newContext({ baseURL: ready.base_url, javaScriptEnabled: js,
      viewport: { width: 1280, height: 900 } });
    const page = await context.newPage();
    page.setDefaultTimeout(15_000); page.setDefaultNavigationTimeout(30_000);
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
    try {
      for (const [language, module] of cultures) for (const dark of [false, true]) {
        expect((await page.request.post('/admin-api/settings/ui/save', {
          form: { language, is_dark_mode: String(dark) }, maxRedirects: 0,
        })).status()).toBe(303);
        await loaded(page, `${preview(ready, revision)}&error=object-add-invalid`, js);
        await expect(page.locator('html')).toHaveAttribute('lang', language);
        await expect(page.locator('[data-theme]').first()).toHaveAttribute('data-theme', dark ? 'dark' : 'light');
        const form = page.getByTestId('teletext-object-add-form');
        await expect(form.locator('legend')).toHaveText(translation(module, 'TeletextObjectAddTitle'));
        await expect(form.locator('[name="name"]')).toHaveAccessibleName(translation(module, 'TeletextDesignName'));
        await expect(form.locator('[name="resource"]')).toHaveAccessibleName(translation(module, 'TeletextObjectAddResource'));
        await expect(form.locator('[name="object_type"]')).toHaveAccessibleName(translation(module, 'TeletextObjectAddType'));
        await expect(form.locator('[name="object_type"] option')).toHaveText([translation(module, 'TeletextObjectAddType'), translation(module, 'TeletextObjectTypeActive'), translation(module, 'TeletextObjectTypeAdaptive'), translation(module, 'TeletextObjectTypePassive')]);
        await expect(form.locator('[name="new_object_id"]')).toHaveAccessibleName(translation(module, 'TeletextObjectCopyNumber'));
        await expect(form.locator('button')).toHaveText(translation(module, 'TeletextObjectAddAction'));
        await expect(form.locator('#teletext-object-add-help')).toHaveText(translation(module, 'TeletextObjectAddHelp'));
        await expect(page.getByText(translation(module, 'TeletextObjectAddInvalid'), { exact: true })).toBeVisible();
        expect(await field(form, 'revision')).toBe(String(revision));
        expect(await field(form, 'expected_design_head')).toBe(String(revision));
        for (const resource of ready.resources) await resourceValue(page, resource);
        await form.scrollIntoViewIfNeeded();
        const screenshot = `${js ? 'js' : 'no-js'}-${language}-${dark ? 'dark' : 'light'}-add.png`;
        await page.screenshot({ path: testInfo.outputPath(screenshot) });
        observations.push({ js, language, dark, screenshot });
      }
      expect((await page.request.post('/admin-api/settings/ui/save', {
        form: { language: 'en-us', is_dark_mode: 'false' }, maxRedirects: 0,
      })).status()).toBe(303);
      await loaded(page, preview(ready, revision), js);
      const before = await publication(page), pixels = rasterHash(await raster(page, ready.page, 480));
      const form = page.getByTestId('teletext-object-add-form');
      const id = ready.new_ids[offset + Number(!js)];
      await form.locator('[name="name"]').fill(`Synthetic definition add ${id}`);
      await form.locator('[name="resource"]').selectOption(await resourceValue(page, ready.resources[ready.resource_indices[offset + Number(!js)]]));
      await form.locator('[name="object_type"]').selectOption(ready.object_types[offset + Number(!js)]);
      await form.locator('[name="new_object_id"]').fill(String(id));
      await submit(page, form);
      if (js) await waitForHydration(page);
      revision += 1;
      const current = page.getByTestId('teletext-object-add-form');
      expect(await field(current, 'revision')).toBe(String(revision));
      expect(new URL(page.url()).searchParams.get('saved')).toBe('draft');
      digest = await field(current, 'digest');
      expect(digest).toMatch(/^[a-f0-9]{64}$/);
      expect(rasterHash(await raster(page, ready.page, 480))).toBe(pixels);
      const after = await publication(page);
      expect([after.generation_id, after.request_id]).toEqual([before.generation_id, before.request_id]);
      for (const resource of ready.resources) await resourceValue(page, resource);
      await loaded(page, preview(ready, revision - 1), js);
      const historical = page.getByTestId('teletext-object-add-form');
      await expect(historical.locator('fieldset')).toHaveAttribute('disabled', '');
      await expect(historical.locator('[name="name"]')).toBeDisabled();
      await expect(historical.locator('button[type="submit"]')).toBeDisabled();
      await expect(page.getByText(translation('en_us', 'TeletextObjectAddCurrentHelp'), { exact: true })).toBeVisible();
      await loaded(page, preview(ready, revision), js);
      for (const width of [320, 1920]) {
        await page.setViewportSize({ width, height: 900 });
        if (width < 768) await expect.poll(async () => {
          const drawer = await page.locator('[data-nav-drawer]').boundingBox();
          return drawer ? drawer.x + drawer.width : 0;
        }).toBeLessThanOrEqual(1);
        await current.scrollIntoViewIfNeeded();
        expect(await current.evaluate(element => {
          const bounds = element.getBoundingClientRect();
          return element.scrollWidth <= element.clientWidth + 1
            && bounds.left >= 0 && bounds.right <= window.innerWidth;
        })).toBe(true);
        await page.screenshot({ path: testInfo.outputPath(`${js ? 'js' : 'no-js'}-${width}-add.png`), animations: 'disabled' });
      }
      await page.setViewportSize({ width: 640, height: 900 });
      await page.evaluate(() => { document.documentElement.style.zoom = '2'; });
      await expect.poll(async () => {
        const drawer = await page.locator('[data-nav-drawer]').boundingBox();
        return drawer ? drawer.x + drawer.width : 0;
      }).toBeLessThanOrEqual(1);
      await current.scrollIntoViewIfNeeded();
      expect(await current.evaluate(element => {
        const bounds = element.getBoundingClientRect();
        return element.scrollWidth <= element.clientWidth + 1
          && bounds.left >= 0 && bounds.right <= window.innerWidth;
      })).toBe(true);
      await page.screenshot({ path: testInfo.outputPath(`${js ? 'js' : 'no-js'}-200-percent-add.png`), animations: 'disabled' });
      await page.evaluate(() => { document.documentElement.style.zoom = ''; });
      expect(errors).toEqual([]);
    } finally {
      await context.close();
    }
  }
  expect(revision).toBe(offset === 0 ? 3 : 5);
  marker(directory, `${testInfo.project.name}-add.json`, {
    probe_version: 1, case: ready.case, design_id: ready.design_id, revision, digest,
    new_ids: ready.new_ids.slice(offset, offset + 2), observations,
  });
  if (offset === 2) marker(directory, 'browser-complete.json', {
    probe_version: 1, case: ready.case, design_id: ready.design_id, revision, digest, new_ids: ready.new_ids,
  });
});
