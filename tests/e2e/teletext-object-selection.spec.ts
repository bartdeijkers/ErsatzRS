import { expect, test, type Locator, type Page } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import {
  bounded, cultures, field, marker, publication, raster, rasterHash, submit, translation, visit,
} from './helpers/teletext-native-editor';
import { waitForHydration } from './hydration';

// The existing owned native fixture independently checks all models and units,
// then performs separate publication and cold recovery. No receiver/editor claim.
const readyPath = process.env.ERSATZRS_TELETEXT_POLISH_ADMIN_READY;
const formId = 'teletext-object-pages-choose-form';
type RawPage = { magazine: number; page: number };
type ObjectIdentity = { scope: string; resource: { kind: string; page: RawPage; logical_subpage: number }; object_id: number };
type Ready = {
  probe_version: number; case: string; base_url: string; channel_id: number;
  design_id: number; revision: number; digest: string; original_page: RawPage;
  selected_page: RawPage; caller: ObjectIdentity; original_target: ObjectIdentity;
  selected_target: ObjectIdentity; marker_file: string; timeout_seconds: number;
};
function normalized(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(normalized);
  if (value !== null && typeof value === 'object') return Object.fromEntries(
    Object.entries(value).sort(([left], [right]) => left.localeCompare(right))
      .map(([key, item]) => [key, normalized(item)]));
  return value;
}
function key(value: unknown): string { return JSON.stringify(normalized(value)); }
function objectLabel(identity: ObjectIdentity): string {
  const resource = identity.resource;
  expect(['pop', 'gpop']).toContain(resource.kind);
  return `${resource.kind.toUpperCase()} ${resource.page.magazine.toString(16).toUpperCase()}${resource.page.page.toString(16).padStart(2, '0').toUpperCase()} S1=${resource.logical_subpage} #${identity.object_id}`;
}
function preview(ready: Ready, revision: number): string {
  return `/channels/${ready.channel_id}/teletext?design_id=${ready.design_id}&revision=${revision}&page=124&subcode=0020`;
}
async function settings(page: Page, language: string, dark: boolean): Promise<void> {
  expect((await page.request.post('/admin-api/settings/ui/save', {
    form: { language, is_dark_mode: String(dark) }, maxRedirects: 0,
  })).status()).toBe(303);
}
async function labels(page: Page, module: string): Promise<void> {
  const form = page.getByTestId(formId);
  await expect(form.locator('legend').first()).toHaveText(translation(module, 'TeletextObjectSelectionTitle'));
  await expect(form.locator('#teletext-object-pages-help')).toHaveText(translation(module, 'TeletextObjectSelectionHelp'));
  await expect(form.locator('#teletext-object-pages-mask-help')).toHaveText(translation(module, 'TeletextObjectSelectionMaskHelp'));
  await expect(form.locator('button[type="submit"]')).toHaveText(translation(module, 'TeletextObjectSelectionAction'));
  for (const label of ['Caller', 'Command', 'CurrentTarget', 'Offset']) {
    await expect(form.locator('dt').filter({ hasText: translation(module, `TeletextObjectSelection${label}`) }).first()).toBeVisible();
  }
  await expect(form.locator('dd').filter({ hasText: translation(module, 'TeletextObjectTypePassive') }).first()).toBeVisible();
  await expect(form.locator('dd').filter({ hasText: /^0, 0$/ }).first()).toBeVisible();
}
async function cultureAndLayout(page: Page, ready: Ready, output: (name: string) => string): Promise<void> {
  for (const [language, module] of cultures) {
    await settings(page, language, false);
    await visit(page, preview(ready, 4)); await waitForHydration(page);
    await expect(page.locator('html')).toHaveAttribute('lang', language);
    await labels(page, module);
  }
  for (const width of [320, 1920]) {
    await page.setViewportSize({ width, height: 900 });
    const form = page.getByTestId(formId);
    await form.scrollIntoViewIfNeeded();
    const box = await form.boundingBox(); expect(box).not.toBeNull();
    expect(box!.x).toBeGreaterThanOrEqual(0); expect(box!.x + box!.width).toBeLessThanOrEqual(width);
    expect(await form.evaluate(element => element.scrollWidth <= element.clientWidth)).toBe(true);
    for (const input of await form.locator('input:not([type="hidden"])').all()) {
      expect(await input.evaluate(element => element.getBoundingClientRect().width <= element.closest('form')!.clientWidth)).toBe(true);
    }
    if (width === 320) {
      for (const value of await form.locator('dd').all()) {
        const layout = await value.evaluate(element => {
          const value = element.getBoundingClientRect(), label = element.previousElementSibling!.getBoundingClientRect();
          return { width: value.width, available: element.parentElement!.clientWidth, top: value.top, labelBottom: label.bottom };
        });
        // A fitting form can still have an unreadable one-character value
        // column. Every narrow-screen value needs the full line below its label.
        expect(layout.width).toBeGreaterThanOrEqual(layout.available - 1);
        expect(layout.top).toBeGreaterThanOrEqual(layout.labelBottom);
      }
    }
    await form.locator('button[type="submit"]').scrollIntoViewIfNeeded();
    await expect(form.locator('button[type="submit"]')).toBeVisible();
    await page.screenshot({ path: output(`${formId}-${width}.png`) });
  }
  await page.setViewportSize({ width: 1280, height: 900 });
}
async function fill(form: Locator, ready: Ready, selected: boolean): Promise<void> {
  await form.locator('[name="name"]').fill(selected ? 'Explicit GPOP selection and Passive call' : 'Exact original GPOP and Passive call');
  const beforeCalls = await form.locator('input[name^="call_"][name$="_target"]').evaluateAll(elements => elements.map(
    element => ({ name: (element as HTMLInputElement).name, value: (element as HTMLInputElement).value })));
  const beforePop: { prefix: string; target: string; mask: string }[] = [];
  let changedPages = 0;
  for (const input of await form.locator('input[name^="page_"][name$="_role"]').all()) {
    const prefix = (await input.getAttribute('name'))!.slice(0, -5);
    const target = form.locator(`[name="${prefix}_target"]`), mask = form.locator(`[name="${prefix}_mask"]`);
    if (await input.inputValue() === 'gpop') {
      const raw = selected ? ready.selected_page : ready.original_page;
      const label = `${raw.magazine.toString(16).toUpperCase()}${raw.page.toString(16).padStart(2, '0').toUpperCase()}`;
      await expect(target).toHaveAttribute('list', 'teletext-object-pages-gpop');
      expect(await form.locator('#teletext-object-pages-gpop option').evaluateAll(elements => elements.map(element => (element as HTMLOptionElement).value))).toContain(label);
      await target.fill(label);
      await mask.fill('0001'); changedPages += 1;
    } else {
      expect(await input.inputValue()).toBe('pop');
      beforePop.push({ prefix, target: await target.inputValue(), mask: await mask.inputValue() });
    }
  }
  expect(changedPages).toBe(3); expect(beforePop).toHaveLength(3);
  // Page selection must not silently retarget any call. The separate explicit
  // selection below is the user's only resource-body change.
  for (const before of beforeCalls) expect(await form.locator(`[name="${before.name}"]`).inputValue()).toBe(before.value);
  let changedCalls = 0;
  for (const input of await form.locator('input[name^="call_"][name$="_caller"]').all()) {
    const prefix = (await input.getAttribute('name'))!.slice(0, -7);
    if (key(JSON.parse(await input.inputValue())) !== key(ready.caller)
      || await field(form, `${prefix}_command`) !== '4') continue;
    const target = form.locator(`[name="${prefix}_target"]`);
    expect(await target.inputValue()).toBe(objectLabel(selected ? ready.original_target : ready.selected_target));
    await expect(target).toHaveAttribute('list', 'teletext-object-targets-passive');
    const label = objectLabel(selected ? ready.selected_target : ready.original_target);
    expect(await form.locator('#teletext-object-targets-passive option').evaluateAll(elements => elements.map(element => (element as HTMLOptionElement).value))).toContain(label);
    await target.fill(label);
    changedCalls += 1;
  }
  expect(changedCalls).toBe(1);
  let changed = 0;
  for (const before of beforeCalls) {
    const after = await form.locator(`[name="${before.name}"]`).inputValue();
    if (after !== before.value) changed += 1;
  }
  expect(changed).toBe(1);
  for (const before of beforePop) {
    expect(await field(form, `${before.prefix}_target`)).toBe(before.target);
    expect(await field(form, `${before.prefix}_mask`)).toBe(before.mask);
    expect(before.mask).toBe('8001');
  }
}

test('coordinate selected object pages and existing calls through localized current draft forms', async ({ browser }, testInfo) => {
  test.skip(!readyPath, 'Requires the owned explicit-object-selection native fixture.');
  test.setTimeout(240_000);
  const directory = dirname(resolve(readyPath!));
  const ready = JSON.parse(bounded(readyPath!).toString('utf8')) as Ready;
  expect(ready.probe_version).toBe(1); expect(ready.case).toBe('teletext-explicit-object-selection');
  expect(ready.base_url).toMatch(/^http:\/\/127\.0\.0\.1:\d+$/);
  expect(ready.channel_id).toBe(1); expect(ready.revision).toBe(4);
  expect(ready.digest).toMatch(/^[a-f0-9]{64}$/);
  expect(ready.original_page).toEqual({ magazine: 2, page: 0xd0 });
  expect(ready.selected_page).toEqual({ magazine: 2, page: 0xe1 });
  expect(ready.caller.object_id).toBe(20); expect(ready.original_target.object_id).toBe(21); expect(ready.selected_target.object_id).toBe(32);
  expect(resolve(ready.marker_file)).toBe(join(directory, 'browser-complete.json'));
  expect(ready.timeout_seconds).toBe(600);
  expect(['chromium', 'firefox']).toContain(testInfo.project.name);
  const js = testInfo.project.name === 'chromium', language = js ? 'nl' : 'en-us', module = js ? 'nl' : 'en_us';
  expect(existsSync(ready.marker_file)).toBe(false);
  expect(existsSync(join(directory, `${testInfo.project.name}-object-selection.json`))).toBe(false);
  let revision = js ? 4 : 5, digest = ready.digest;
  if (!js) {
    const previous = JSON.parse(bounded(join(directory, 'chromium-object-selection.json')).toString('utf8'));
    expect(previous.probe_version).toBe(1); expect(previous.case).toBe(ready.case); expect(previous.design_id).toBe(ready.design_id);
    expect(previous.revision).toBe(5); expect(previous.errors).toEqual([]); expect(previous.digest).toMatch(/^[a-f0-9]{64}$/);
    digest = previous.digest;
  }
  testInfo.annotations.push({ type: 'browser-version', description: browser.version() });
  const context = await browser.newContext({ baseURL: ready.base_url, javaScriptEnabled: js, viewport: { width: 1280, height: 900 } });
  const page = await context.newPage(); page.setDefaultTimeout(15_000); page.setDefaultNavigationTimeout(30_000);
  const errors: string[] = [], hashes: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
  try {
    if (js) await cultureAndLayout(page, ready, name => testInfo.outputPath(name));
    await settings(page, language, !js);
    for (const [operation, selected] of (js ? [true] : [false, true]).entries()) {
      await visit(page, preview(ready, revision)); if (js) await waitForHydration(page);
      await expect(page.locator('html')).toHaveAttribute('lang', language);
      await expect(page.locator('[data-theme]').first()).toHaveAttribute('data-theme', js ? 'light' : 'dark');
      await labels(page, module);
      const before = await publication(page), pixels = rasterHash(await raster(page, 124, 672));
      if (hashes.length === 0) hashes.push(pixels);
      const form = page.getByTestId(formId);
      expect(await field(form, 'revision')).toBe(String(revision));
      expect(await field(form, 'expected_design_head')).toBe(String(revision));
      expect(await field(form, 'digest')).toBe(digest);
      await fill(form, ready, selected);
      await form.locator('button[type="submit"]').scrollIntoViewIfNeeded();
      await page.screenshot({ path: testInfo.outputPath(`${operation}-before.png`) });
      await submit(page, form); if (js) await waitForHydration(page); revision += 1;
      const current = page.getByTestId(formId);
      expect(await field(current, 'revision')).toBe(String(revision));
      expect(await field(current, 'expected_design_head')).toBe(String(revision));
      expect(new URL(page.url()).searchParams.get('saved')).toBe('draft');
      const nextDigest = await field(current, 'digest'); expect(nextDigest).toMatch(/^[a-f0-9]{64}$/); expect(nextDigest).not.toBe(digest); digest = nextDigest;
      const afterPixels = rasterHash(await raster(page, 124, 672)); expect(afterPixels).not.toBe(pixels); hashes.push(afterPixels);
      const after = await publication(page);
      expect([after.generation_id, after.request_id]).toEqual([before.generation_id, before.request_id]);
      await current.locator('button[type="submit"]').scrollIntoViewIfNeeded();
      await page.screenshot({ path: testInfo.outputPath(`${operation}-after.png`) });
    }
    if (!js) expect(hashes[2]).toBe(hashes[0]);
    await visit(page, preview(ready, revision - 1)); if (js) await waitForHydration(page);
    await expect(page.getByTestId(formId).locator('fieldset').first()).toHaveAttribute('disabled', '');
    expect(errors).toEqual([]);
  } finally { await settings(page, 'en-us', false); await context.close(); }
  expect(revision).toBe(js ? 5 : 7);
  marker(directory, `${testInfo.project.name}-object-selection.json`, {
    probe_version: 1, case: ready.case, design_id: ready.design_id, revision, digest,
    language, dark: !js, javascript: js, browser: browser.version(), hashes, errors,
  });
  if (!js) marker(directory, 'browser-complete.json', {
    probe_version: 1, case: ready.case, design_id: ready.design_id, revision, digest,
  });
});
