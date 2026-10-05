import { Browser, expect, Page, TestInfo, test } from '@playwright/test';
import { execFileSync } from 'child_process';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;
const baseURL = process.env.ERSATZRS_BASE_URL ?? 'http://127.0.0.1:8409';
const sqliteBin = process.env.E2E_SQLITE_BIN ?? 'sqlite3';

function sqlite(sql: string): string {
  if (!dbPath) {
    throw new Error('E2E_DB_PATH or ERSATZRS_DB_PATH is required for graphics-element seeding');
  }
  return execFileSync(sqliteBin, [dbPath], {
    input: `.timeout 10000\n${sql}`,
    encoding: 'utf8',
  }).trim();
}

function projectBase(testInfo: TestInfo): number {
  const offset = Array.from(testInfo.project.name).reduce(
    (total, character) => total + character.charCodeAt(0),
    0,
  );
  return 790_000 + offset * 10;
}

async function forceEnglish(page: Page): Promise<void> {
  await page.request.post('/admin-api/settings/ui/save', {
    form: { is_dark_mode: 'true', language: 'en-us' },
    maxRedirects: 0,
  });
}

async function waitForHydration(page: Page): Promise<void> {
  await expect.poll(
    () => page.evaluate(
      () => (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true,
    ),
    { timeout: 15_000, message: 'graphics-element editor did not finish hydration' },
  ).toBe(true);
}

function watchHydrationErrors(page: Page): string[] {
  const errors: string[] = [];
  page.on('pageerror', (error) => {
    const text = error.stack ?? error.message;
    const benignAbort = text.includes('AbortError')
      && text.toLowerCase().includes('operation was aborted');
    if (!benignAbort) {
      errors.push(text);
    }
  });
  page.on('console', (message) => {
    if (message.type() === 'error' && /failed_to_cast_marker_node|hydration|rust_panic/i.test(message.text())) {
      errors.push(message.text());
    }
  });
  return errors;
}

function seedGraphicsElements(testInfo: TestInfo): { textId: number; imageId: number; name: string } {
  const base = projectBase(testInfo);
  const textId = base + 1;
  const imageId = base + 2;
  const name = `DTX ${testInfo.project.name} existing`;
  const config = JSON.stringify({
    kind: 'text',
    text: 'Existing {{ title }}',
    renderer: 'drawvg',
    drawtext: {
      font_file: null,
      refresh_interval_milliseconds: 1000,
      reload_interval_frames: 1,
    },
    text_fit: 'Wrap',
    text_align: 'Center',
    width_percent: 72.5,
    base_style: 'headline',
    include_fonts_from: null,
    styles: [],
  }).replace(/'/g, "''");

  sqlite(`
DELETE FROM "GraphicsElement" WHERE "Id" IN (${textId}, ${imageId});
INSERT INTO "GraphicsElement" (
  "Id", "Name", "Kind", "Path", "ZIndex", "Config"
) VALUES
  (${textId}, '${name}', 1, 'text/e2e.yml', 9, '${config}'),
  (${imageId}, 'DTX ${testInfo.project.name} image', 0, 'image/e2e.yml', 3, '{"kind":"image","image":"logo.png"}');
`);
  return { textId, imageId, name };
}

async function createNoScriptContext(browser: Browser) {
  return browser.newContext({ baseURL, javaScriptEnabled: false });
}

test.describe('localized graphics-elements selector and editor', () => {
  test.skip(!dbPath, 'E2E_DB_PATH or ERSATZRS_DB_PATH is required to verify persistence');

  test('lists every kind and persists hydrated add/edit without corrupting other fields', async ({
    page,
  }, testInfo) => {
    test.setTimeout(60_000);
    const runtimeErrors = watchHydrationErrors(page);
    await forceEnglish(page);
    const seed = seedGraphicsElements(testInfo);

    await page.goto('/graphics-elements');
    await expect(page.locator('[data-not-found]')).toHaveCount(0);
    await expect(page.locator(`[data-graphics-element-row="${seed.textId}"]`)).toContainText(seed.name);
    await expect(page.locator(`[data-graphics-element-row="${seed.imageId}"]`)).toContainText('Image');
    await expect(page.locator(`[data-graphics-element-row="${seed.imageId}"] [data-graphics-element-edit]`))
      .toBeVisible();
    await expect(page.getByText('Existing elements and new elements use Drawvg by default.'))
      .toBeVisible();
    await expect(page.getByText('Changes affect newly started streams and do not restart active streams.'))
      .toBeVisible();

    await page.locator('[data-add-graphics-element]').click();
    await waitForHydration(page);
    const editor = page.locator('[data-graphics-element-editor]');
    await expect(editor).toBeVisible();
    await expect(page.locator('[data-graphics-element-renderer-select]')).toHaveValue('drawvg');
    await expect(page.getByText('Drawvg uses built-in bitmap glyphs')).toBeVisible();
    await expect(page.locator('[data-drawtext-refresh-period]')).toBeEnabled();

    const addedName = `DTX ${testInfo.project.name} added`;
    sqlite(`DELETE FROM "GraphicsElement" WHERE "Name" = '${addedName}';`);
    await page.locator('[data-graphics-element-name]').fill(addedName);
    await page.locator('[data-graphics-element-text]').fill('Static {{ title }}');
    await page.locator('[name="preview_variables"]').fill('{"title":"Actual preview title"}');
    await page.locator('[data-save-graphics-element]').click();
    await expect(page).toHaveURL(/\/graphics-elements\/\d+\?saved=1$/);
    await expect.poll(() => sqlite(`
SELECT json_extract("Config", '$.renderer')
FROM "GraphicsElement" WHERE "Name" = '${addedName}';
`)).toBe('drawvg');

    await page.goto(`/graphics-elements/${seed.textId}`);
    await waitForHydration(page);
    await page.locator('[data-graphics-element-renderer-select]').selectOption('drawtext');
    await expect(page.locator('[data-drawtext-font-file]')).toBeVisible();
    await expect(page.locator('[data-drawtext-refresh-period]')).toBeEnabled();
    await expect(page.locator('[data-drawtext-font-file]')).toHaveValue('');
    await expect(page.getByText('With neither selection, packaged Roboto is used.'))
      .toBeVisible();

    await page.locator('[data-graphics-element-name]').fill(`${seed.name} edited`);
    await page.locator('[data-graphics-element-text]').fill('Now: {{ title }}');
    await page.locator('[data-drawtext-refresh-period]').fill('750');
    await page.locator('[data-drawtext-reload-frames]').fill('3');
    await page.locator('[name="preview_variables"]').fill('{"title":"Actual preview title"}');
    await page.locator('[data-save-graphics-element]').click();
    await expect(page).toHaveURL(new RegExp(`/graphics-elements/${seed.textId}\\?saved=1$`));

    await expect.poll(() => sqlite(`
SELECT json_extract("Config", '$.renderer') || '|' ||
       json_extract("Config", '$.drawtext.refresh_interval_milliseconds') || '|' ||
       json_extract("Config", '$.drawtext.reload_interval_frames') || '|' ||
       json_extract("Config", '$.text_fit') || '|' ||
       json_extract("Config", '$.text_align') || '|' ||
       json_extract("Config", '$.base_style') || '|' ||
       "Path" || '|' || "ZIndex"
FROM "GraphicsElement" WHERE "Id" = ${seed.textId};
`)).toBe('drawtext|750|3|Wrap|Center|headline|text/e2e.yml|9');
    expect(sqlite(`SELECT "Name" FROM "GraphicsElement" WHERE "Id" = ${seed.imageId};`))
      .toBe(`DTX ${testInfo.project.name} image`);

    const box = await editor.boundingBox();
    expect(box?.width ?? Number.POSITIVE_INFINITY).toBeLessThanOrEqual(1088);
    expect(runtimeErrors).toEqual([]);
  });

  test('submits and persists through the no-JavaScript form fallback', async ({ browser }, testInfo) => {
    test.setTimeout(45_000);
    const context = await createNoScriptContext(browser);
    const page = await context.newPage();
    try {
      const name = `DTX ${testInfo.project.name} no-js`;
      sqlite(`DELETE FROM "GraphicsElement" WHERE "Name" = '${name}';`);
      await page.goto('/graphics-elements/add');
      let editor = page.locator(
        '[data-graphics-element-noscript] [data-graphics-element-editor]',
      );
      await expect(editor).toBeVisible();
      await editor.locator('[data-graphics-element-name]').fill(name);
      await editor.locator('[data-graphics-element-text]').fill('No script {{ title }}');
      await editor.locator('[data-graphics-element-renderer-select]').selectOption('drawtext');
      await editor.locator('[name="preview_variables"]').fill('{"title":"Actual preview title"}');
      await editor.locator('[data-save-graphics-element]').click();
      await expect(page).toHaveURL(/\/graphics-elements\/\d+\?saved=1$/);

      const id = sqlite(`SELECT "Id" FROM "GraphicsElement" WHERE "Name" = '${name}';`);
      expect(id).not.toBe('');
      expect(sqlite(`
SELECT json_extract("Config", '$.renderer') || '|' ||
       json_extract("Config", '$.drawtext.refresh_interval_milliseconds') || '|' ||
       json_extract("Config", '$.drawtext.reload_interval_frames')
FROM "GraphicsElement" WHERE "Id" = ${id};
`)).toBe('drawtext|1000|1');

      await page.goto(`/graphics-elements/${id}`);
      editor = page.locator(
        '[data-graphics-element-noscript] [data-graphics-element-editor]',
      );
      await expect(editor.locator('[data-drawtext-font-file]')).toBeVisible();
      await editor.locator('[data-drawtext-refresh-period]').fill('1250');
      await editor.locator('[data-drawtext-reload-frames]').fill('4');
      await editor.locator('[name="preview_variables"]').fill('{"title":"Actual preview title"}');
      await editor.locator('[data-save-graphics-element]').click();
      await expect(page).toHaveURL(new RegExp(`/graphics-elements/${id}\\?saved=1$`));
      expect(sqlite(`
SELECT json_extract("Config", '$.drawtext.refresh_interval_milliseconds') || '|' ||
       json_extract("Config", '$.drawtext.reload_interval_frames')
FROM "GraphicsElement" WHERE "Id" = ${id};
`)).toBe('1250|4');
    } finally {
      await context.close();
    }
  });
});
