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
  main_original_target?: ObjectIdentity; main_selected_target?: ObjectIdentity;
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
type PreviewReady = Pick<Ready, 'channel_id' | 'design_id' | 'revision'>;
function preview(ready: PreviewReady, revision: number): string {
  return `/channels/${ready.channel_id}/teletext?design_id=${ready.design_id}&revision=${revision}&page=124&subcode=0020`;
}
async function settings(page: Page, language: string, dark: boolean): Promise<void> {
  expect((await page.request.post('/admin-api/settings/ui/save', {
    form: { language, is_dark_mode: String(dark) }, maxRedirects: 0,
  })).status()).toBe(303);
}
async function labels(page: Page, module: string, drcs = false): Promise<void> {
  const form = page.getByTestId(formId);
  await expect(form.locator('legend').first()).toHaveText(translation(module, 'TeletextObjectSelectionTitle'));
  await expect(form.locator('#teletext-object-pages-help')).toHaveText(translation(module, 'TeletextObjectSelectionHelp'));
  await expect(form.locator('#teletext-object-pages-mask-help')).toHaveText(translation(module, 'TeletextObjectSelectionMaskHelp'));
  await expect(form.locator('button[type="submit"]')).toHaveText(translation(module, 'TeletextObjectSelectionAction'));
  if (drcs) return;
  for (const label of ['Caller', 'Command', 'CurrentTarget', 'Offset']) {
    await expect(form.locator('dt').filter({ hasText: translation(module, `TeletextObjectSelection${label}`) }).first()).toBeVisible();
  }
  await expect(form.locator('dd').filter({ hasText: translation(module, 'TeletextObjectTypePassive') }).first()).toBeVisible();
  await expect(form.locator('dd').filter({ hasText: /^0, 0$/ }).first()).toBeVisible();
  if (await form.locator('input[name^="main_"][name$="_owner"]').count()) {
    await expect(form.locator('legend').filter({ hasText: `${translation(module, 'TeletextObjectSelectionMainCalls')} 1` })).toBeVisible();
  }
}
async function cultureAndLayout(page: Page, ready: PreviewReady, output: (name: string) => string, drcs = false): Promise<void> {
  for (const [language, module] of cultures) {
    await settings(page, language, false);
    await visit(page, preview(ready, ready.revision)); await waitForHydration(page);
    await expect(page.locator('html')).toHaveAttribute('lang', language);
    await labels(page, module, drcs);
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
  const mainMode = ready.case === 'teletext-main-local-object-selection';
  const beforeMain = await form.locator('input[name^="main_"][name$="_target"]').evaluateAll(elements => elements.map(
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
  for (const before of beforeMain) expect(await form.locator(`[name="${before.name}"]`).inputValue()).toBe(before.value);
  let changedCalls = 0, changedLocal = 0;
  for (const input of await form.locator('input[name^="call_"][name$="_caller"]').all()) {
    const prefix = (await input.getAttribute('name'))!.slice(0, -7);
    const caller = JSON.parse(await input.inputValue());
    const local = mainMode && caller.scope === 'local' && caller.object_id === 65000;
    if (key(caller) !== key(ready.caller) && !local) continue;
    expect(await field(form, `${prefix}_command`)).toBe(local ? '1' : '4');
    if (local) {
      expect([124, 125, 126]).toContain(caller.owner.page); expect(caller.owner.subcode).toBe(32);
      changedLocal += 1;
    }
    const target = form.locator(`[name="${prefix}_target"]`);
    expect(await target.inputValue()).toBe(objectLabel(selected ? ready.original_target : ready.selected_target));
    await expect(target).toHaveAttribute('list', 'teletext-object-targets-passive');
    const label = objectLabel(selected ? ready.selected_target : ready.original_target);
    expect(await form.locator('#teletext-object-targets-passive option').evaluateAll(elements => elements.map(element => (element as HTMLOptionElement).value))).toContain(label);
    await target.fill(label);
    changedCalls += 1;
  }
  expect(changedCalls).toBe(mainMode ? 4 : 1);
  expect(changedLocal).toBe(mainMode ? 3 : 0);
  let changed = 0;
  for (const before of beforeCalls) {
    const after = await form.locator(`[name="${before.name}"]`).inputValue();
    if (after !== before.value) changed += 1;
  }
  expect(changed).toBe(mainMode ? 4 : 1);
  await fillMain(form, ready, selected, mainMode);
  for (const before of beforePop) {
    expect(await field(form, `${before.prefix}_target`)).toBe(before.target);
    expect(await field(form, `${before.prefix}_mask`)).toBe(before.mask);
    expect(before.mask).toBe(mainMode ? '0001' : '8001');
  }
}

async function fillMain(form: Locator, ready: Ready, selected: boolean, mainMode: boolean): Promise<void> {
  const owners = await form.locator('input[name^="main_"][name$="_owner"]').all();
  expect(owners).toHaveLength(mainMode ? 3 : 0);
  for (const input of owners) {
    const owner = JSON.parse(await input.inputValue());
    expect([124, 125, 126]).toContain(owner.page); expect(owner.subcode).toBe(32);
    const prefix = (await input.getAttribute('name'))!.slice(0, -6);
    expect(await field(form, `${prefix}_command`)).toBe('0');
    const target = form.locator(`[name="${prefix}_target"]`);
    await expect(target).toHaveAttribute('list', 'teletext-object-targets-active');
    expect(await target.inputValue()).toBe(objectLabel(selected ? ready.main_original_target! : ready.main_selected_target!));
    const label = objectLabel(selected ? ready.main_selected_target! : ready.main_original_target!);
    expect(await form.locator('#teletext-object-targets-active option').evaluateAll(elements => elements.map(element => (element as HTMLOptionElement).value))).toContain(label);
    await target.fill(label);
  }
}

test('coordinate selected object pages and existing calls through localized current draft forms', async ({ browser }, testInfo) => {
  test.skip(!readyPath, 'Requires the owned explicit-object-selection native fixture.');
  test.setTimeout(240_000);
  const directory = dirname(resolve(readyPath!));
  const ready = JSON.parse(bounded(readyPath!).toString('utf8')) as Ready;
  test.skip(['teletext-drcs-raw-page-selection', 'teletext-first-retained-role-attachment'].includes(ready.case), 'Another owned retained-role case runs below.');
  expect(ready.probe_version).toBe(1); expect(['teletext-explicit-object-selection', 'teletext-main-local-object-selection']).toContain(ready.case);
  expect(ready.base_url).toMatch(/^http:\/\/127\.0\.0\.1:\d+$/);
  const mainMode = ready.case === 'teletext-main-local-object-selection';
  expect(ready.channel_id).toBe(1); expect(ready.revision).toBe(mainMode ? 1 : 4);
  expect(ready.digest).toMatch(/^[a-f0-9]{64}$/);
  expect(ready.original_page).toEqual({ magazine: 2, page: 0xd0 });
  expect(ready.selected_page).toEqual({ magazine: 2, page: 0xe1 });
  expect(ready.caller.object_id).toBe(mainMode ? 33 : 20); expect(ready.original_target.object_id).toBe(mainMode ? 34 : 21); expect(ready.selected_target.object_id).toBe(32);
  if (mainMode) { expect(ready.main_original_target?.object_id).toBe(20); expect(ready.main_selected_target?.object_id).toBe(33); }
  expect(resolve(ready.marker_file)).toBe(join(directory, 'browser-complete.json'));
  expect(ready.timeout_seconds).toBe(600);
  expect(['chromium', 'firefox']).toContain(testInfo.project.name);
  const js = testInfo.project.name === 'chromium', language = js ? 'nl' : 'en-us', module = js ? 'nl' : 'en_us';
  expect(existsSync(ready.marker_file)).toBe(false);
  expect(existsSync(join(directory, `${testInfo.project.name}-object-selection.json`))).toBe(false);
  let revision = ready.revision + (js ? 0 : 1), digest = ready.digest;
  if (!js) {
    const previous = JSON.parse(bounded(join(directory, 'chromium-object-selection.json')).toString('utf8'));
    expect(previous.probe_version).toBe(1); expect(previous.case).toBe(ready.case); expect(previous.design_id).toBe(ready.design_id);
    expect(previous.revision).toBe(ready.revision + 1); expect(previous.errors).toEqual([]); expect(previous.digest).toMatch(/^[a-f0-9]{64}$/);
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
  expect(revision).toBe(ready.revision + (js ? 1 : 3));
  marker(directory, `${testInfo.project.name}-object-selection.json`, {
    probe_version: 1, case: ready.case, design_id: ready.design_id, revision, digest,
    language, dark: !js, javascript: js, browser: browser.version(), hashes, errors,
  });
  if (!js) marker(directory, 'browser-complete.json', {
    probe_version: 1, case: ready.case, design_id: ready.design_id, revision, digest,
  });
});

type DrcsReady = PreviewReady & Pick<Ready, 'probe_version' | 'case' | 'base_url' | 'digest' | 'marker_file' | 'timeout_seconds'> & {
  selected_display: { page: number; subcode: number };
  preserved_display: { page: number; subcode: number };
  raw_roles: { role: 'drcs' | 'gdrcs'; original_page: RawPage; selected_page: RawPage; mask: string }[];
};
async function fillDrcs(form: Locator, ready: DrcsReady, selected: boolean): Promise<void> {
  await form.locator('[name="name"]').fill(selected ? 'Retained DRCS and GDRCS banks' : 'Exact original DRCS and GDRCS banks');
  const changed = new Set<string>(), preserved = new Set<string>();
  const untouched = await form.locator('input[name^="call_"], input[name^="main_"]').evaluateAll(elements => elements.map(
    element => ({ name: (element as HTMLInputElement).name, value: (element as HTMLInputElement).value })));
  for (const input of await form.locator('input[name^="page_"][name$="_role"]').all()) {
    const prefix = (await input.getAttribute('name'))!.slice(0, -5);
    const role = await input.inputValue(), owner = JSON.parse(await field(form, `${prefix}_display`));
    const target = form.locator(`[name="${prefix}_target"]`), mask = form.locator(`[name="${prefix}_mask"]`);
    const choice = ready.raw_roles.find(choice => choice.role === role);
    expect(choice).toBeDefined();
    const rawLabel = (raw: RawPage) => `${raw.magazine.toString(16).toUpperCase()}${raw.page.toString(16).padStart(2, '0').toUpperCase()}`;
    const original = rawLabel(choice!.original_page), next = rawLabel(choice!.selected_page);
    await expect(target).toHaveAttribute('list', `teletext-object-pages-${role}`);
    const options = await form.locator(`#teletext-object-pages-${role} option`).evaluateAll(elements => elements.map(element => (element as HTMLOptionElement).value));
    expect(options).toContain(original); expect(options).toContain(next);
    expect(await mask.inputValue()).toBe(choice!.mask);
    if (key(owner) === key(ready.selected_display)) {
      expect(await target.inputValue()).toBe(selected ? original : next);
      await target.fill(selected ? next : original); changed.add(role);
    } else {
      expect(owner).toEqual(ready.preserved_display);
      expect(await target.inputValue()).toBe(original); preserved.add(role);
    }
    expect(await mask.inputValue()).toBe(choice!.mask);
  }
  expect([...changed].sort()).toEqual(['drcs', 'gdrcs']);
  expect([...preserved].sort()).toEqual(['drcs', 'gdrcs']);
  for (const before of untouched) expect(await field(form, before.name)).toBe(before.value);
}

test('choose retained DRCS banks and restore exact source through localized current draft forms', async ({ browser }, testInfo) => {
  test.skip(!readyPath, 'Requires the owned retained DRCS selection native fixture.');
  test.setTimeout(240_000);
  const ready = JSON.parse(bounded(readyPath!).toString('utf8')) as DrcsReady;
  test.skip(ready.case !== 'teletext-drcs-raw-page-selection', 'Another owned selection case runs above.');
  const directory = dirname(resolve(readyPath!));
  expect(ready.probe_version).toBe(1); expect(ready.base_url).toMatch(/^http:\/\/127\.0\.0\.1:\d+$/);
  expect(ready.channel_id).toBe(1); expect(ready.revision).toBe(1); expect(ready.digest).toMatch(/^[a-f0-9]{64}$/);
  expect(ready.selected_display).toEqual({ page: 124, subcode: 32 });
  expect(ready.preserved_display).toEqual({ page: 125, subcode: 32 });
  expect(ready.raw_roles).toEqual([
    { role: 'drcs', original_page: { magazine: 1, page: 0xc0 }, selected_page: { magazine: 1, page: 0xc2 }, mask: '0003' },
    { role: 'gdrcs', original_page: { magazine: 1, page: 0xc1 }, selected_page: { magazine: 1, page: 0xc3 }, mask: '0003' },
  ]);
  expect(resolve(ready.marker_file)).toBe(join(directory, 'browser-complete.json')); expect(ready.timeout_seconds).toBe(600);
  expect(['chromium', 'firefox']).toContain(testInfo.project.name);
  const js = testInfo.project.name === 'chromium', language = js ? 'nl' : 'en-us', module = js ? 'nl' : 'en_us';
  expect(existsSync(ready.marker_file)).toBe(false);
  expect(existsSync(join(directory, `${testInfo.project.name}-object-selection.json`))).toBe(false);
  let revision = ready.revision + (js ? 0 : 1), digest = ready.digest;
  if (!js) {
    const previous = JSON.parse(bounded(join(directory, 'chromium-object-selection.json')).toString('utf8'));
    expect(previous.probe_version).toBe(1); expect(previous.case).toBe(ready.case); expect(previous.design_id).toBe(ready.design_id);
    expect(previous.revision).toBe(2); expect(previous.errors).toEqual([]); expect(previous.digest).toMatch(/^[a-f0-9]{64}$/);
    digest = previous.digest;
  }
  testInfo.annotations.push({ type: 'browser-version', description: browser.version() });
  const context = await browser.newContext({ baseURL: ready.base_url, javaScriptEnabled: js, viewport: { width: 1280, height: 900 } });
  const page = await context.newPage(); page.setDefaultTimeout(15_000); page.setDefaultNavigationTimeout(30_000);
  const errors: string[] = [], hashes: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
  try {
    if (js) await cultureAndLayout(page, ready, name => testInfo.outputPath(name), true);
    await settings(page, language, !js);
    for (const [operation, selected] of (js ? [true] : [false, true]).entries()) {
      await visit(page, preview(ready, revision)); if (js) await waitForHydration(page);
      await expect(page.locator('html')).toHaveAttribute('lang', language);
      await expect(page.locator('[data-theme]').first()).toHaveAttribute('data-theme', js ? 'light' : 'dark');
      await labels(page, module, true);
      const before = await publication(page), pixels = rasterHash(await raster(page, 124));
      if (hashes.length === 0) hashes.push(pixels);
      const form = page.getByTestId(formId);
      expect(await field(form, 'revision')).toBe(String(revision)); expect(await field(form, 'expected_design_head')).toBe(String(revision));
      expect(await field(form, 'digest')).toBe(digest);
      await fillDrcs(form, ready, selected);
      await form.locator('button[type="submit"]').scrollIntoViewIfNeeded(); await page.screenshot({ path: testInfo.outputPath(`${operation}-before.png`) });
      await submit(page, form); if (js) await waitForHydration(page); revision += 1;
      const current = page.getByTestId(formId);
      expect(await field(current, 'revision')).toBe(String(revision)); expect(await field(current, 'expected_design_head')).toBe(String(revision));
      expect(new URL(page.url()).searchParams.get('saved')).toBe('draft');
      const nextDigest = await field(current, 'digest'); expect(nextDigest).toMatch(/^[a-f0-9]{64}$/); expect(nextDigest).not.toBe(digest); digest = nextDigest;
      const afterPixels = rasterHash(await raster(page, 124)); expect(afterPixels).not.toBe(pixels); hashes.push(afterPixels);
      const after = await publication(page); expect([after.generation_id, after.request_id]).toEqual([before.generation_id, before.request_id]);
      await current.locator('button[type="submit"]').scrollIntoViewIfNeeded(); await page.screenshot({ path: testInfo.outputPath(`${operation}-after.png`) });
    }
    if (!js) expect(hashes[2]).toBe(hashes[0]);
    await visit(page, preview(ready, revision - 1)); if (js) await waitForHydration(page);
    await expect(page.getByTestId(formId).locator('fieldset').first()).toHaveAttribute('disabled', ''); expect(errors).toEqual([]);
  } finally { await settings(page, 'en-us', false); await context.close(); }
  expect(revision).toBe(js ? 2 : 4);
  marker(directory, `${testInfo.project.name}-object-selection.json`, {
    probe_version: 1, case: ready.case, design_id: ready.design_id, revision, digest,
    language, dark: !js, javascript: js, browser: browser.version(), hashes, errors,
  });
  if (!js) marker(directory, 'browser-complete.json', { probe_version: 1, case: ready.case, design_id: ready.design_id, revision, digest });
});


type AttachmentReady = Omit<DrcsReady, 'raw_roles'> & {
  raw_roles: { role: 'drcs'; selected_page: RawPage; mask: string }[];
};
function attachmentPreview(ready: AttachmentReady, revision: number): string {
  return `/channels/${ready.channel_id}/teletext?design_id=${ready.design_id}&revision=${revision}&page=${ready.selected_display.page}&subcode=0020`;
}
async function attachmentRoleForms(page: Page, ready: AttachmentReady, restore: boolean): Promise<Locator[]> {
  const forms = await page.getByTestId(restore ? 'teletext-object-page-restore-form' : 'teletext-object-page-attach-form').all();
  const found: Locator[] = [];
  for (const form of forms) {
    if (await field(form, 'role') === ready.raw_roles[0].role
      && key(JSON.parse(await field(form, 'display'))) === key(ready.selected_display)) found.push(form);
  }
  return found;
}
async function attachmentRoleForm(page: Page, ready: AttachmentReady, restore: boolean): Promise<Locator> {
  const forms = await attachmentRoleForms(page, ready, restore); expect(forms).toHaveLength(1); return forms[0];
}
async function attachmentLabels(form: Locator, module: string, restore: boolean): Promise<void> {
  const prefix = restore ? 'TeletextObjectAttachmentRestore' : 'TeletextObjectAttachment';
  await expect(form.locator('legend')).toContainText(translation(module, `${prefix}Title`));
  await expect(form.locator('p').first()).toHaveText(translation(module, `${prefix}Help`));
  await expect(form.locator('button[type="submit"]')).toHaveText(translation(module, `${prefix}Action`));
  await expect(form.locator('label').first()).toHaveText(translation(module, 'TeletextDesignName'));
  if (!restore) {
    await expect(form.locator('label').nth(1)).toHaveText(translation(module, 'TeletextObjectSelectionPageTarget'));
    await expect(form.locator('label').nth(2)).toHaveText(translation(module, 'TeletextObjectSelectionMask'));
    await expect(form.locator('p').nth(1)).toHaveText(translation(module, 'TeletextObjectSelectionMaskHelp'));
  }
}
async function attachmentCultures(page: Page, ready: AttachmentReady, revision: number, restore: boolean): Promise<void> {
  for (const [language, module] of cultures) {
    await settings(page, language, false);
    await visit(page, attachmentPreview(ready, revision)); await waitForHydration(page);
    await expect(page.locator('html')).toHaveAttribute('lang', language);
    await attachmentLabels(await attachmentRoleForm(page, ready, restore), module, restore);
  }
}

test('attach an unselected retained role and undo to its exact root through localized draft forms', async ({ browser }, testInfo) => {
  test.skip(!readyPath, 'Requires the owned first retained-role attachment native fixture.');
  test.setTimeout(240_000);
  const ready = JSON.parse(bounded(readyPath!).toString('utf8')) as AttachmentReady;
  test.skip(ready.case !== 'teletext-first-retained-role-attachment', 'Another owned selection case runs above.');
  const directory = dirname(resolve(readyPath!));
  expect(ready.probe_version).toBe(1); expect(ready.base_url).toMatch(/^http:\/\/127\.0\.0\.1:\d+$/);
  expect(ready.channel_id).toBe(1); expect(ready.revision).toBe(1); expect(ready.digest).toMatch(/^[a-f0-9]{64}$/);
  expect(ready.selected_display).toEqual({ page: 227, subcode: 32 });
  expect(ready.preserved_display).toEqual({ page: 125, subcode: 32 });
  expect(ready.raw_roles).toEqual([{ role: 'drcs', selected_page: { magazine: 1, page: 0xc0 }, mask: '0003' }]);
  expect(resolve(ready.marker_file)).toBe(join(directory, 'browser-complete.json')); expect(ready.timeout_seconds).toBe(600);
  expect(['chromium', 'firefox']).toContain(testInfo.project.name);
  const js = testInfo.project.name === 'chromium', language = js ? 'nl' : 'en-us', module = js ? 'nl' : 'en_us';
  expect(existsSync(ready.marker_file)).toBe(false);
  expect(existsSync(join(directory, `${testInfo.project.name}-object-selection.json`))).toBe(false);
  let revision = js ? 1 : 2, digest = ready.digest;
  if (!js) {
    const previous = JSON.parse(bounded(join(directory, 'chromium-object-selection.json')).toString('utf8'));
    expect(previous.probe_version).toBe(1); expect(previous.case).toBe(ready.case); expect(previous.design_id).toBe(ready.design_id);
    expect(previous.revision).toBe(2); expect(previous.errors).toEqual([]); expect(previous.digest).toMatch(/^[a-f0-9]{64}$/);
    digest = previous.digest;
  }
  testInfo.annotations.push({ type: 'browser-version', description: browser.version() });
  const context = await browser.newContext({ baseURL: ready.base_url, javaScriptEnabled: js, viewport: { width: 1280, height: 900 } });
  const page = await context.newPage(); page.setDefaultTimeout(15_000); page.setDefaultNavigationTimeout(30_000);
  const errors: string[] = [], hashes: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
  try {
    if (js) await attachmentCultures(page, ready, revision, false);
    await settings(page, language, !js);
    for (const [operation, restore] of (js ? [false] : [true, false]).entries()) {
      await visit(page, attachmentPreview(ready, revision)); if (js) await waitForHydration(page);
      await expect(page.locator('html')).toHaveAttribute('lang', language);
      await expect(page.locator('[data-theme]').first()).toHaveAttribute('data-theme', js ? 'light' : 'dark');
      const form = await attachmentRoleForm(page, ready, restore);
      expect(await attachmentRoleForms(page, ready, !restore)).toHaveLength(0);
      await attachmentLabels(form, module, restore);
      expect(await field(form, 'revision')).toBe(String(revision)); expect(await field(form, 'expected_design_head')).toBe(String(revision));
      expect(await field(form, 'digest')).toBe(digest);
      const before = await publication(page), pixels = rasterHash(await raster(page, ready.selected_display.page, 480));
      hashes.push(pixels);
      await form.locator('[name="name"]').fill(restore ? 'Exact originally unselected role' : 'Retained normal DRCS first attachment');
      if (!restore) {
        const target = form.locator('[name="target"]'), list = await target.getAttribute('list');
        expect(list).toBeTruthy(); expect(await form.locator(`#${list} option`).evaluateAll(elements => elements.map(element => (element as HTMLOptionElement).value))).toContain('1C0');
        await target.fill('1C0'); await form.locator('[name="mask"]').fill('0003');
      }
      for (const width of [320, 1920]) {
        await page.setViewportSize({ width, height: 900 }); await form.scrollIntoViewIfNeeded();
        const box = await form.boundingBox(); expect(box).not.toBeNull();
        expect(box!.x).toBeGreaterThanOrEqual(0); expect(box!.x + box!.width).toBeLessThanOrEqual(width);
        expect(await form.evaluate(element => element.scrollWidth <= element.clientWidth)).toBe(true);
        await page.screenshot({ path: testInfo.outputPath(`attachment-${operation}-${width}.png`) });
      }
      await page.setViewportSize({ width: 1280, height: 900 });
      await submit(page, form); if (js) await waitForHydration(page); revision += 1;
      expect(new URL(page.url()).searchParams.get('saved')).toBe('draft');
      await visit(page, attachmentPreview(ready, revision)); if (js) await waitForHydration(page);
      const current = await attachmentRoleForm(page, ready, !restore);
      expect(await attachmentRoleForms(page, ready, restore)).toHaveLength(0);
      expect(await field(current, 'revision')).toBe(String(revision)); expect(await field(current, 'expected_design_head')).toBe(String(revision));
      const nextDigest = await field(current, 'digest'); expect(nextDigest).toMatch(/^[a-f0-9]{64}$/); expect(nextDigest).not.toBe(digest); digest = nextDigest;
      // This links-only attachment does not alter authored visible content.
      expect(rasterHash(await raster(page, ready.selected_display.page, 480))).toBe(pixels);
      const after = await publication(page); expect([after.generation_id, after.request_id]).toEqual([before.generation_id, before.request_id]);
    }
    if (js) await attachmentCultures(page, ready, revision, true);
    await visit(page, attachmentPreview(ready, revision - 1)); if (js) await waitForHydration(page);
    const historical = await attachmentRoleForm(page, ready, false);
    await expect(historical.locator('fieldset')).toHaveAttribute('disabled', ''); expect(errors).toEqual([]);
  } finally { await settings(page, 'en-us', false); await context.close(); }
  expect(revision).toBe(js ? 2 : 4);
  marker(directory, `${testInfo.project.name}-object-selection.json`, {
    probe_version: 1, case: ready.case, design_id: ready.design_id, revision, digest,
    language, dark: !js, javascript: js, browser: browser.version(), hashes, errors,
  });
  if (!js) marker(directory, 'browser-complete.json', { probe_version: 1, case: ready.case, design_id: ready.design_id, revision, digest });
});
