import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { expect, test } from '@playwright/test';

// Explicit opt-in to the disposable DRCS delivery probe's actual Router.
// Preferences return to its seeded English/light baseline before completion.
const enabled = process.env.ERSATZRS_TELETEXT_DRCS_DIAGNOSTICS === '1';
const cssSha256 = process.env.ERSATZRS_TELETEXT_DIAGNOSTIC_CSS_SHA256;
const specUrl = pathToFileURL(__filename);
const cultures = [
  ['en-us', 'en_us'], ['nl', 'nl'], ['pl', 'pl'], ['es-es', 'es_es'],
  ['es-419', 'es_419'], ['fr-fr', 'fr_fr'], ['de-de', 'de_de'],
  ['it-it', 'it_it'], ['pt-br', 'pt_br'],
] as const;

function translation(module: string, key: string): string {
  const source = readFileSync(fileURLToPath(new URL(
    `../../crates/ersatzrs-web/src/admin/languages/${module}.rs`, specUrl,
  )), 'utf8');
  const found = source.match(new RegExp(String.raw`TextKey::${key}\s*=>\s*(?:\{\s*)?("(?:[^"\\]|\\.)*")`));
  if (!found) throw new Error(`Missing ${module}/${key} translation.`);
  return JSON.parse(found[1]) as string;
}

test('typed resource diagnostics preserve raw identity in every culture and theme without JavaScript', async ({ browser, baseURL }, testInfo) => {
  test.skip(!enabled, 'Requires the explicitly owned live DRCS delivery fixture.');
  test.setTimeout(75_000);
  expect(baseURL).toMatch(/^http:\/\/127\.0\.0\.1:\d+$/);
  expect(cssSha256).toMatch(/^[a-f0-9]{64}$/);
  const context = await browser.newContext({
    baseURL, javaScriptEnabled: false, viewport: { width: 1280, height: 900 },
  });
  const page = await context.newPage();
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  try {
    const stylesheet = await page.request.get('/public/bundle.css');
    expect(stylesheet.status()).toBe(200);
    expect(createHash('sha256').update(await stylesheet.body()).digest('hex')).toBe(cssSha256);
    const resource = { kind: 'drcs', page: { magazine: 1, page: 192 }, logical_subpage: 1 };
    const query = new URLSearchParams({
      error: 'editorial-protected', error_resource: JSON.stringify(resource),
      error_row: '28', error_column: '5', error_designation: '3',
    });
    for (const [language, module] of cultures) {
      for (const dark of [false, true]) {
        const saved = await page.request.post('/admin-api/settings/ui/save', {
          form: { language, is_dark_mode: String(dark) }, maxRedirects: 0,
        });
        expect(saved.status()).toBe(303);
        await page.goto(`/channels/1/teletext?${query}`);
        await expect(page.locator('html')).toHaveAttribute('lang', language);
        await expect(page.locator('[data-theme]').first()).toHaveAttribute('data-theme', dark ? 'dark' : 'light');
        await expect(page.locator('[data-not-found]')).toHaveCount(0);
        const location = page.getByTestId('teletext-error-location');
        await expect(location).toBeVisible();
        await expect(location.locator('dt')).toHaveText([
          translation(module, 'TeletextResourceLifecycle'), translation(module, 'TeletextPreviewPosition'),
        ]);
        await expect(location.locator('dd')).toHaveText(['drcs 1C0 / 1', '28, 5']);
        await expect(location).not.toContainText('448');
        await location.scrollIntoViewIfNeeded();
        await page.screenshot({ path: testInfo.outputPath(`${language}-${dark ? 'dark' : 'light'}.png`) });
      }
    }
    expect(errors).toEqual([]);
  } finally {
    const restored = await page.request.post('/admin-api/settings/ui/save', {
      form: { language: 'en-us', is_dark_mode: 'false' }, maxRedirects: 0,
    });
    expect(restored.status()).toBe(303);
    await context.close();
  }
});
