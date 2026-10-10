import { expect, Locator, test } from '@playwright/test';
import { DatabaseSync } from 'node:sqlite';

const databasePath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

async function linkContrast(link: Locator): Promise<number> {
  // Use the same computed-colour method as the metadata-editor contrast gate.
  return link.evaluate(element => {
    const parse = (value: string): number[] => {
      const values = value.match(/[\d.]+/g)?.map(Number) ?? [];
      if (values.length < 3) throw new Error('Missing computed link colour');
      return [values[0], values[1], values[2], values[3] ?? 1];
    };
    const probe = document.createElement('span');
    probe.style.color = 'var(--color-bg)';
    element.appendChild(probe);
    const background = parse(getComputedStyle(probe).color);
    probe.remove();
    const foreground = parse(getComputedStyle(element).color);
    const luminance = (channels: number[]): number => channels.slice(0, 3)
      .map(value => value / 255)
      .map(value => value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4)
      .reduce((sum, value, index) => sum + value * [0.2126, 0.7152, 0.0722][index], 0);
    const fg = luminance(foreground.slice(0, 3).map((value, index) =>
      value * foreground[3] + background[index] * (1 - foreground[3])));
    const bg = luminance(background);
    return (Math.max(fg, bg) + 0.05) / (Math.min(fg, bg) + 0.05);
  });
}

// P9A-02 proves the selected decoder, not authored-page or TV-guide delivery.
// The wrapper supplies a real FFmpeg 9 runtime and an isolated SQLite database.
for (const javaScriptEnabled of [true, false]) {
  for (const isDarkMode of [true, false]) {
    test(`teletext decoder health and identity (JavaScript ${javaScriptEnabled}, dark ${isDarkMode})`, async ({ browser, baseURL }, testInfo) => {
      test.skip(!databasePath, 'Requires the disposable E2E database.');
      test.setTimeout(60_000);
      const context = await browser.newContext({ baseURL, javaScriptEnabled });
      const database = new DatabaseSync(databasePath!);
      database.exec('PRAGMA busy_timeout=10000;');
      try {
        const page = await context.newPage();
        const errors: string[] = [];
        page.on('pageerror', error => errors.push(error.message));
        page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
        for (const [language, title, message, refreshLabel] of [
          ['en-us', 'Teletext decoding', 'The selected FFmpeg runtime passed teletext page selection, subtitle text and colour bitmap decoding.', 'Refresh'],
          ['nl', 'Teletekstdecodering', 'De geselecteerde FFmpeg-runtime heeft de controles voor teletekstpaginaselectie, ondertiteltekst en kleurenbitmapdecodering doorstaan.', 'Vernieuwen'],
        ]) {
          expect((await page.request.post('/admin-api/settings/ui/save', {
            form: { is_dark_mode: String(isDarkMode), language }, maxRedirects: 0,
          })).status()).toBe(303);
          expect((await page.request.post('/admin-api/system/health/refresh', { maxRedirects: 0 })).status()).toBe(303);
          await page.goto('/system/health');
          const row = page.getByRole('row').filter({ has: page.getByText(title, { exact: true }) });
          await expect(row).toContainText(message);
          const settingsLink = row.locator('a[href="/settings/ffmpeg"]');
          await expect(settingsLink).toBeVisible();
          expect(await linkContrast(settingsLink)).toBeGreaterThanOrEqual(4.5);
          if (javaScriptEnabled) {
            await page.waitForFunction(
              () => (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true,
              undefined, { timeout: 15_000 },
            );
          }
          const refreshed = page.waitForResponse(response => response.request().method() === 'POST' && response.url().includes(
            javaScriptEnabled ? '/admin-api/refresh_health_snapshot' : '/admin-api/system/health/refresh',
          ));
          await page.getByRole('button', { name: refreshLabel, exact: true }).click();
          expect((await refreshed).status()).toBe(javaScriptEnabled ? 200 : 303);
          await expect(row).toContainText(message);
          await expect(page.getByRole('button', { name: refreshLabel, exact: true })).toBeEnabled();
          const stored = database.prepare('SELECT "Value" FROM "ConfigElement" WHERE "Key"=?').get('health.results') as { Value: string };
          const result = (JSON.parse(stored.Value) as { title: string; status: string; message: string }[])
            .find(result => result.title === 'Teletext decoding');
          expect(result?.status).toBe('Pass');
          expect(result?.message).toContain('colour bitmap decoding');
          await row.scrollIntoViewIfNeeded();
          await page.screenshot({ path: `test-results/teletext-health-${testInfo.project.name}-${javaScriptEnabled}-${isDarkMode}-${language}.png`, fullPage: true });
          await row.screenshot({ path: `test-results/teletext-health-row-${testInfo.project.name}-${javaScriptEnabled}-${isDarkMode}-${language}.png` });
        }
        await page.goto('/system/troubleshooting');
        const general = JSON.parse((await page.locator('#general-panel pre').textContent())!);
        const decoder = general.FfmpegSettings.TeletextDecoder;
        expect(decoder.Capability.status).toBe('ready');
        expect(decoder.Capability.missing_options).toEqual([]);
        expect(decoder.RuntimeGeneration).toBeGreaterThan(0);
        expect(decoder.FFmpegSha256).toMatch(/^[a-f0-9]{64}$/);
        expect(decoder.FFprobeSha256).toMatch(/^[a-f0-9]{64}$/);
        if (process.env.ERSATZRS_TELETEXT_E2E_FFMPEG_SHA256) {
          expect(decoder.FFmpegSha256).toBe(process.env.ERSATZRS_TELETEXT_E2E_FFMPEG_SHA256);
        }
        expect(errors).toEqual([]);
      } finally {
        await context.request.post('/admin-api/settings/ui/save', {
          form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0,
        });
        database.close();
        await context.close();
      }
    });
  }
}
