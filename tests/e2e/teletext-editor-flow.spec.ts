import { expect, Locator, Page, test } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { waitForHydration } from './hydration';
import { TeletextChannelFixture } from './helpers/teletext-channel-fixture';

const databasePath = process.env.E2E_DB_PATH;
const cultures = [['en-us', 'en_us'], ['nl', 'nl'], ['de-de', 'de_de'],
  ['fr-fr', 'fr_fr'], ['es-es', 'es_es'], ['es-419', 'es_419'], ['it-it', 'it_it'],
  ['pt-br', 'pt_br'], ['pl', 'pl']] as const;
type File = { name: string; mimeType: string; buffer: Buffer };

function text(module: string): (key: string) => string {
  const source = readFileSync(fileURLToPath(new URL(
    `../../crates/ersatzrs-web/src/admin/languages/${module}.rs`, pathToFileURL(__filename))), 'utf8');
  return key => {
    const value = source.match(new RegExp(String.raw`TextKey::${key}\s*=>\s*(?:\{\s*)?("(?:[^"\\]|\\.)*")`));
    if (!value) throw new Error(`Missing localized editor key ${module}/${key}.`);
    return JSON.parse(value[1]);
  };
}

async function ready(page: Page, javaScriptEnabled: boolean): Promise<void> {
  await expect(page.locator('[data-not-found]')).toHaveCount(0);
  if (javaScriptEnabled) await waitForHydration(page);
}

async function submit(page: Page, form: Locator, javaScriptEnabled: boolean): Promise<void> {
  const [response] = await Promise.all([
    page.waitForResponse(value => value.request().method() === 'POST'
      && new URL(value.url()).pathname.startsWith('/admin-api/teletext/')),
    page.waitForEvent('framenavigated', { predicate: frame => frame === page.mainFrame() }),
    form.locator('button[type="submit"]').click(),
  ] as const);
  expect(response.status()).toBe(303);
  await ready(page, javaScriptEnabled);
}

async function profile(page: Page, fixture: TeletextChannelFixture, theme: string, enabled: boolean): Promise<number> {
  const form = page.locator(`[data-addon-id="${fixture.addonId}"] [data-testid="teletext-profile-form"][data-profile-id=""]`);
  await expect(form).toHaveCount(1);
  const details = form.locator('..');
  if (await details.getAttribute('open') === null) await details.locator(':scope > summary').click();
  await expect(form).toBeVisible();
  await form.locator('[name="name"]').fill(`Synthetic editor ${theme}`);
  await form.locator('[data-setting-key="THEME"] select[name$=".mode"]').selectOption('literal');
  await form.locator('[data-setting-key="THEME"] [name$=".value"]').selectOption(theme);
  await submit(page, form, enabled);
  return fixture.profiles().find(value => value.Name === `Synthetic editor ${theme}`)!.Id;
}

async function original(page: Page, fixture: TeletextChannelFixture, theme: string, profileId: number, enabled: boolean): Promise<string> {
  const binding = page.locator(`[data-addon-id="${fixture.addonId}"] [data-testid="teletext-binding-form"]`);
  const revision = fixture.profiles().find(value => value.Id === profileId)!.Revision;
  await binding.locator('[name="profile_choice"]').selectOption(`${profileId}:${revision}`);
  await submit(page, binding, enabled);
  const form = page.getByTestId('teletext-authoring-form');
  await form.locator('[name="name"]').fill(`Synthetic editor original ${theme}`);
  await form.locator('[name="entry_addon_id"]').selectOption(fixture.addonId);
  await form.locator('[name="entry_page"]').fill('200');
  await form.locator('[name="entry_subcode"]').fill('0000');
  await form.locator('[name="v1_region"]').selectOption('western_latin');
  await form.locator('[name="resource_transmissions"]').fill('[]');
  await submit(page, form, enabled);
  await expect(page.getByTestId('teletext-preview-model')).toHaveAttribute('data-page', '200');
  return page.url();
}

function files(archive: string): File[] {
  // Standard ZIP reader only, bounded synthetic fixture; no production parser substitute.
  const script = String.raw`import base64,json,sys,zipfile
with zipfile.ZipFile(sys.argv[1]) as archive:
    entries=archive.infolist()
    assert 0<len(entries)<=64 and sum(e.file_size for e in entries)<=1048576
    assert len({e.filename for e in entries})==len(entries)
    print(json.dumps([{'name':e.filename,'base64':base64.b64encode(archive.read(e)).decode()} for e in entries]))
`;
  const raw = execFileSync('python3', ['-c', script, archive],
    { encoding: 'utf8', timeout: 10_000, maxBuffer: 2 * 1024 * 1024 });
  return (JSON.parse(raw) as { name: string; base64: string }[]).map(value => {
    expect(value.name).toMatch(/^[A-Za-z0-9_.-]+$/);
    return { name: value.name, mimeType: 'application/octet-stream', buffer: Buffer.from(value.base64, 'base64') };
  });
}

function edit(input: File[], row: number): File[] {
  return input.map(file => {
    if (file.name !== 'page-200-0000.tti') return file;
    let body = file.buffer.toString('latin1');
    const pattern = new RegExp(`^OL,${row},([^\\r\\n]*)(\\r?)(?=\\n|$)`, 'm');
    const changed = ' '.repeat(8) + 'Q' + ' '.repeat(31);
    if (pattern.test(body)) {
      body = body.replace(pattern, (_match, cells: string, ending: string) => {
        expect(cells).toBe(' '.repeat(40));
        return `OL,${row},${changed}${ending}`;
      });
    } else {
      const offset = body.indexOf('OL,');
      expect(offset).toBeGreaterThan(-1);
      body = body.slice(0, offset) + `OL,${row},${changed}\n` + body.slice(offset);
    }
    return { ...file, buffer: Buffer.from(body, 'latin1') };
  });
}

async function upload(page: Page, input: File[], enabled: boolean, name: string): Promise<void> {
  const form = page.getByTestId('teletext-editor-import-form');
  await expect(form).toHaveAttribute('enctype', 'multipart/form-data');
  await form.locator('[name="name"]').fill(name);
  await form.locator('[name="sidecar"]').setInputFiles(input.filter(file => file.name === 'project.ersatzrs.json'));
  await form.locator('[name="files"]').setInputFiles(input.filter(file => file.name !== 'project.ersatzrs.json'));
  await submit(page, form, enabled);
}

async function pixels(page: Page): Promise<{ red: number; blue: number; whiteEdit: number }> {
  const image = page.getByTestId('teletext-preview-model').locator('img');
  await expect.poll(() => image.evaluate(element => (element as HTMLImageElement).complete
    && (element as HTMLImageElement).naturalWidth > 0)).toBe(true);
  return image.evaluate(element => {
    const encoded = (element as HTMLImageElement).src.split(',')[1];
    const svg = new DOMParser().parseFromString(atob(encoded), 'image/svg+xml');
    if (svg.documentElement.getAttribute('viewBox') !== '0 0 480 250') {
      throw new Error('Synthetic editor preview changed its validated raster geometry.');
    }
    const canvas = document.createElement('canvas'); canvas.width = 480; canvas.height = 250;
    const context = canvas.getContext('2d')!;
    context.imageSmoothingEnabled = false;
    context.drawImage(element as HTMLImageElement, 0, 0, 480, 250);
    const data = context.getImageData(0, 0, 480, 250).data;
    let red = 0; let blue = 0; let whiteEdit = 0;
    for (let offset = 0; offset < data.length; offset += 4) {
      if (data[offset] === 255 && data[offset + 1] === 0 && data[offset + 2] === 0) red++;
      if (data[offset] === 0 && data[offset + 1] === 0 && data[offset + 2] === 255) blue++;
    }
    for (let y = 20; y < 30; y++) for (let x = 96; x < 108; x++) {
      const offset = (y * 480 + x) * 4;
      if (data[offset] === 255 && data[offset + 1] === 255 && data[offset + 2] === 255) whiteEdit++;
    }
    return { red, blue, whiteEdit };
  });
}

test.describe.configure({ mode: 'serial' });
for (const javaScriptEnabled of [true, false]) for (const dark of [false, true]) {
  test(`teletext editor file flow (JavaScript ${javaScriptEnabled}, dark ${dark})`, async ({ browser, baseURL }, testInfo) => {
    test.skip(!databasePath, 'Requires the runner-owned disposable database.');
    test.setTimeout(600_000);
    testInfo.annotations.push({ type: 'browser-version', description: browser.version() });
    const fixture = new TeletextChannelFixture(databasePath!, { authoredPages: true, editorPages: true });
    fixture.setAuthoringPolicy();
    const context = await browser.newContext({ baseURL, javaScriptEnabled, viewport: { width: 1280, height: 900 } });
    context.setDefaultTimeout(15_000);
    context.setDefaultNavigationTimeout(30_000);
    const pages = [await context.newPage(), await context.newPage()];
    const errors: string[] = [];
    for (const page of pages) {
      page.on('pageerror', error => errors.push(error.message));
      page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
    }
    try {
      await pages[0].request.post('/admin-api/settings/ui/save', {
        form: { language: 'en-us', is_dark_mode: String(dark) }, maxRedirects: 0,
      });
      await pages[0].goto(`/channels/${fixture.channels[0]}/teletext`); await ready(pages[0], javaScriptEnabled);
      const day = await profile(pages[0], fixture, 'day', javaScriptEnabled);
      const night = await profile(pages[0], fixture, 'night', javaScriptEnabled);
      const originals: string[] = [];
      for (const [index, theme, selected] of [[0, 'day', day], [1, 'night', night]] as const) {
        await pages[index].goto(`/channels/${fixture.channels[index]}/teletext`);
        await ready(pages[index], javaScriptEnabled);
        originals.push(await original(pages[index], fixture, theme, selected, javaScriptEnabled));
      }
      const ids = fixture.designs().map(value => value.Id);
      for (const [language, module] of cultures) {
        const translated = text(module);
        await pages[0].request.post('/admin-api/settings/ui/save', {
          form: { language, is_dark_mode: String(dark) }, maxRedirects: 0,
        });
        await pages[0].goto(originals[0]); await ready(pages[0], javaScriptEnabled);
        const editor = pages[0].getByTestId('teletext-editor-controls');
        await expect(editor).toHaveAttribute('data-state', 'available');
        for (const key of ['Title', 'Help', 'OwnershipHelp', 'TemplateHelp', 'ImportHelp', 'NoPublicationHelp']) {
          await expect(editor.getByText(translated(`TeletextEditor${key}`), { exact: true })).toBeVisible();
        }
        await expect(editor.locator('[name="sidecar"]')).toHaveAccessibleName(translated('TeletextEditorSidecar'));
        await expect(editor.locator('[name="files"]')).toHaveAccessibleName(translated('TeletextEditorFiles'));
        const downloaded = pages[0].waitForEvent('download');
        await editor.getByTestId('teletext-editor-export-all-form').getByRole('button', {
          name: translated('TeletextEditorExportSet'), exact: true,
        }).click();
        const download = await downloaded;
        expect(download.suggestedFilename()).toMatch(/^teletext-[0-9]+-1\.zip$/);
        const archive = testInfo.outputPath(`${module}-original.zip`);
        await download.saveAs(archive);
        const exported = files(archive);
        expect(exported.filter(file => file.name.endsWith('.tti'))).toHaveLength(2);
        const sidecar = JSON.parse(exported.find(file => file.name === 'project.ersatzrs.json')!.buffer.toString('utf8'));
        expect(sidecar.schema_version).toBe(1);
        expect(sidecar.project.channel_id).toBe(fixture.channels[0]);
        const selectedDownload = pages[0].waitForEvent('download');
        await editor.getByTestId('teletext-editor-export-page-form').getByRole('button', {
          name: translated('TeletextEditorExportPage'), exact: true,
        }).click();
        const selectedPath = testInfo.outputPath(`${module}-selected-page.zip`);
        await (await selectedDownload).saveAs(selectedPath);
        expect(files(selectedPath).filter(file => file.name.endsWith('.tti')).map(file => file.name))
          .toEqual(['page-200-0000.tti']);
        const before = fixture.designs(fixture.channels[0])[0].Revision;
        const pointerBefore = fixture.safetySnapshot();
        if (module === 'en_us') {
          const oversized = exported.map(file => file.name === 'page-200-0000.tti'
            ? { ...file, buffer: Buffer.alloc(8 * 1024 * 1024 + 1, 120) } : file);
          await upload(pages[0], oversized, javaScriptEnabled, 'Synthetic oversized upload');
          expect(new URL(pages[0].url()).pathname).toBe(`/channels/${fixture.channels[0]}/teletext`);
          await expect(pages[0].getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'editorial-limit');
          await expect(pages[0].getByTestId('teletext-feedback')).toContainText(translated('TeletextEditorLimit'));
          expect(fixture.safetySnapshot()).toEqual(pointerBefore);
          await pages[0].goto(originals[0]); await ready(pages[0], javaScriptEnabled);
        }
        await upload(pages[0], edit(exported, 4), javaScriptEnabled, `Synthetic protected ${module}`);
        await expect(pages[0].getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'editorial-protected');
        await expect(pages[0].getByTestId('teletext-feedback')).toContainText(translated('TeletextEditorProtected'));
        expect(fixture.designs(fixture.channels[0])[0].Revision).toBe(before);
        expect(fixture.safetySnapshot()).toEqual(pointerBefore);
        await pages[0].goto(originals[0]); await ready(pages[0], javaScriptEnabled);
        await upload(pages[0], edit(exported, 2), javaScriptEnabled, `Synthetic edited ${module}`);
        await expect(pages[0].getByTestId('teletext-feedback')).toHaveAttribute('data-code', 'draft');
        expect(fixture.designs(fixture.channels[0])[0].Revision).toBe(before + 1);
        expect(fixture.safetySnapshot()).toEqual(pointerBefore);
        const model = pages[0].getByTestId('teletext-preview-model');
        await expect(model).toHaveAttribute('data-page', '200');
        const edited = await pixels(pages[0]);
        expect(edited.red).toBeGreaterThan(0); expect(edited.blue).toBe(0); expect(edited.whiteEdit).toBeGreaterThan(0);
        await pages[1].goto(originals[1]); await ready(pages[1], javaScriptEnabled);
        const untouched = await pixels(pages[1]);
        expect(untouched.blue).toBeGreaterThan(0); expect(untouched.red).toBe(0); expect(untouched.whiteEdit).toBe(0);
        await model.locator('img').scrollIntoViewIfNeeded();
        await pages[0].screenshot({ path: testInfo.outputPath(`${module}-edited-preview.png`), fullPage: false });
        const importForm = pages[0].getByTestId('teletext-editor-import-form');
        await importForm.scrollIntoViewIfNeeded();
        await pages[0].screenshot({ path: testInfo.outputPath(`${module}-editor-controls.png`), fullPage: false });
        if (module === 'en_us') {
          await pages[0].setViewportSize({ width: 320, height: 900 });
          const name = importForm.locator('[name="name"]');
          await name.focus(); await pages[0].keyboard.press('Tab');
          const sidecar = importForm.locator('[name="sidecar"]');
          await expect(sidecar).toBeFocused();
          await sidecar.scrollIntoViewIfNeeded();
          await expect(sidecar).toBeInViewport();
          await expect.poll(() => pages[0].evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
          await pages[0].screenshot({ path: testInfo.outputPath('editor-320-keyboard.png'), fullPage: false });
          await pages[0].setViewportSize({ width: 640, height: 900 });
          await pages[0].evaluate(() => { document.documentElement.style.zoom = '2'; });
          await sidecar.scrollIntoViewIfNeeded();
          await expect(sidecar).toBeInViewport();
          await expect.poll(() => pages[0].evaluate(() => document.documentElement.getBoundingClientRect().right <= innerWidth + 1)).toBe(true);
          await pages[0].screenshot({ path: testInfo.outputPath('editor-200-percent.png'), fullPage: false });
          await pages[0].evaluate(() => { document.documentElement.style.zoom = ''; });
          await pages[0].setViewportSize({ width: 1280, height: 900 });
        }
        await submit(pages[0], pages[0].getByTestId('teletext-publication-request-form'), javaScriptEnabled);
        await expect(pages[0].getByTestId('teletext-publication')).toHaveAttribute('data-state', 'pending');
        await submit(pages[0], pages[0].getByTestId('teletext-publication-publish-form'), javaScriptEnabled);
        await expect(pages[0].getByTestId('teletext-publication')).toHaveAttribute('data-state', 'selected');
        await model.locator('nav a[href*="page=201"]').click(); await ready(pages[0], javaScriptEnabled);
        await expect(model).toHaveAttribute('data-page', '201');
        expect((await pixels(pages[0])).whiteEdit).toBe(0);
        expect(fixture.unchangedPackage()).toBe(true);
      }
      expect(fixture.calls()).toEqual(['day', 'night']);
      expect(fixture.designs().map(value => value.Id)).toEqual(ids);
      expect(fixture.binding(fixture.channels[1])?.ProfileId).toBe(night);
      // Publication intentionally changed only through the explicit actions above.
      expect(errors).toEqual([]);
    } finally { await context.close(); fixture.close(); }
  });
}
