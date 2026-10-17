import { expect, test, type Page } from '@playwright/test';
import { dirname, join, resolve } from 'node:path';
import {
  bounded, cultures, field, marker, publication, raster, rasterHash, submit, translation, visit,
} from './helpers/teletext-native-editor';
import { waitForHydration } from './hydration';

// Existing ChannelTeletextPage -> resolved TeletextDraftPreview -> ordinary
// POST/303 Copy form. No new island or shared primitive. The native fixture
// checks real stored concepts, complete output and unchanged publication.
const readyPath = process.env.ERSATZRS_TELETEXT_POLISH_ADMIN_READY;
type ObjectIdentity = {
  scope: string; object_id: number;
  resource: { kind: string; page: { magazine: number; page: number }; logical_subpage: number };
};
type Ready = {
  probe_version: number; case: string; base_url: string; channel_id: number;
  design_id: number; revision: number; digest: string; source_object: ObjectIdentity;
  new_ids: number[]; marker_file: string; timeout_seconds: number;
};
function preview(ready: Ready, revision: number): string {
  return `/channels/${ready.channel_id}/teletext?design_id=${ready.design_id}&revision=${revision}&page=124&subcode=0020`;
}
async function loaded(page: Page, url: string, js: boolean): Promise<void> {
  await visit(page, url);
  if (js) await waitForHydration(page);
}
async function sourceValue(page: Page, expected: ObjectIdentity): Promise<string> {
  const values = await page.locator('#teletext-object-copy-source option').evaluateAll((options, expected) => {
    return options.filter(option => {
      const actual = JSON.parse((option as HTMLOptionElement).value);
      return actual.scope === expected.scope && actual.object_id === expected.object_id
        && actual.resource.kind === expected.resource.kind
        && actual.resource.page.magazine === expected.resource.page.magazine
        && actual.resource.page.page === expected.resource.page.page
        && actual.resource.logical_subpage === expected.resource.logical_subpage;
    }).map(option => (option as HTMLOptionElement).value);
  }, expected);
  expect(values).toHaveLength(1);
  return values[0];
}

test('copy a native object through localized hydrated and no-JS draft forms', async ({ browser }, testInfo) => {
  test.skip(!readyPath, 'Requires the owned native definition-copy fixture.');
  test.setTimeout(540_000);
  const directory = dirname(resolve(readyPath!));
  const ready = JSON.parse(bounded(readyPath!).toString('utf8')) as Ready;
  expect(ready.probe_version).toBe(1);
  expect(ready.case).toBe('teletext-definition-copy');
  expect(ready.base_url).toMatch(/^http:\/\/127\.0\.0\.1:\d+$/);
  expect(ready.new_ids).toEqual([25, 26, 27, 28]);
  expect(resolve(ready.marker_file)).toBe(join(directory, 'browser-complete.json'));
  expect(ready.timeout_seconds).toBe(600);
  const offset = testInfo.project.name === 'chromium' ? 0 : 2;
  expect(['chromium', 'firefox']).toContain(testInfo.project.name);
  if (offset === 2) {
    const first = JSON.parse(bounded(join(directory, 'chromium-copy.json')).toString('utf8'));
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
        await loaded(page, `${preview(ready, revision)}&error=object-copy-invalid`, js);
        await expect(page.locator('html')).toHaveAttribute('lang', language);
        await expect(page.locator('[data-theme]').first()).toHaveAttribute('data-theme', dark ? 'dark' : 'light');
        const form = page.getByTestId('teletext-object-copy-form');
        await expect(form.locator('legend')).toHaveText(translation(module, 'TeletextObjectCopyTitle'));
        await expect(form.locator('[name="name"]')).toHaveAccessibleName(translation(module, 'TeletextDesignName'));
        await expect(form.locator('[name="source_object"]')).toHaveAccessibleName(translation(module, 'TeletextObjectCopySource'));
        await expect(form.locator('[name="new_object_id"]')).toHaveAccessibleName(translation(module, 'TeletextObjectCopyNumber'));
        await expect(form.locator('button')).toHaveText(translation(module, 'TeletextObjectCopyAction'));
        await expect(form.locator('#teletext-object-copy-help')).toHaveText(translation(module, 'TeletextObjectCopyHelp'));
        await expect(page.getByText(translation(module, 'TeletextObjectCopyInvalid'), { exact: true })).toBeVisible();
        expect(await field(form, 'revision')).toBe(String(revision));
        expect(await field(form, 'expected_design_head')).toBe(String(revision));
        await sourceValue(page, ready.source_object);
        await form.scrollIntoViewIfNeeded();
        const screenshot = `${js ? 'js' : 'no-js'}-${language}-${dark ? 'dark' : 'light'}-copy.png`;
        await page.screenshot({ path: testInfo.outputPath(screenshot) });
        observations.push({ js, language, dark, screenshot });
      }
      expect((await page.request.post('/admin-api/settings/ui/save', {
        form: { language: 'en-us', is_dark_mode: 'false' }, maxRedirects: 0,
      })).status()).toBe(303);
      await loaded(page, preview(ready, revision), js);
      const before = await publication(page), pixels = rasterHash(await raster(page, 124));
      const form = page.getByTestId('teletext-object-copy-form');
      const id = ready.new_ids[offset + Number(!js)];
      await form.locator('[name="name"]').fill(`Synthetic definition copy ${id}`);
      await form.locator('[name="source_object"]').selectOption(await sourceValue(page, ready.source_object));
      await form.locator('[name="new_object_id"]').fill(String(id));
      await submit(page, form);
      if (js) await waitForHydration(page);
      revision += 1;
      const current = page.getByTestId('teletext-object-copy-form');
      expect(await field(current, 'revision')).toBe(String(revision));
      expect(new URL(page.url()).searchParams.get('saved')).toBe('draft');
      digest = await field(current, 'digest');
      expect(digest).toMatch(/^[a-f0-9]{64}$/);
      expect(rasterHash(await raster(page, 124))).toBe(pixels);
      const after = await publication(page);
      expect([after.generation_id, after.request_id]).toEqual([before.generation_id, before.request_id]);
      await sourceValue(page, { ...ready.source_object, object_id: id });
      await loaded(page, preview(ready, revision - 1), js);
      const historical = page.getByTestId('teletext-object-copy-form');
      await expect(historical.locator('fieldset')).toHaveAttribute('disabled', '');
      await expect(historical.locator('[name="name"]')).toBeDisabled();
      await expect(historical.locator('button[type="submit"]')).toBeDisabled();
      await expect(page.getByText(translation('en_us', 'TeletextObjectCopyCurrentHelp'), { exact: true })).toBeVisible();
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
        await page.screenshot({ path: testInfo.outputPath(`${js ? 'js' : 'no-js'}-${width}-copy.png`), animations: 'disabled' });
      }
      expect(errors).toEqual([]);
    } finally {
      await context.close();
    }
  }
  expect(revision).toBe(offset === 0 ? 3 : 5);
  marker(directory, `${testInfo.project.name}-copy.json`, {
    probe_version: 1, case: ready.case, design_id: ready.design_id, revision, digest,
    new_ids: ready.new_ids.slice(offset, offset + 2), observations,
  });
  if (offset === 2) marker(directory, 'browser-complete.json', {
    probe_version: 1, case: ready.case, design_id: ready.design_id, revision, digest, new_ids: ready.new_ids,
  });
});
