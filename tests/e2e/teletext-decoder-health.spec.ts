import { expect, Locator, test } from '@playwright/test';
import { DatabaseSync } from 'node:sqlite';

const databasePath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;
// Default to the unqualified Live-clock contract. Qualified runtime runs must
// declare their independently established expectation rather than infer it
// from the Health result being tested.
const originalPtsSetting = process.env.ERSATZRS_TELETEXT_E2E_ORIGINAL_PTS ?? 'false';
if (!['true', 'false'].includes(originalPtsSetting)) {
  throw new Error('ERSATZRS_TELETEXT_E2E_ORIGINAL_PTS must be true or false.');
}
const originalPts = originalPtsSetting === 'true';
const decoderReadyEnglish = 'The selected FFmpeg runtime passed teletext page selection, subtitle text and colour bitmap decoding.';
const clockUnavailableEnglish = 'The runtime can decode teletext but cannot preserve original Live caption timestamps. Live teletext captions are unavailable; ordinary playback remains available.';

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

// F5: invalid shared source-spool policy must remain visible in the existing localized
// Health flow. Actual AV fallback and quota ownership have native runtime proof.
for (const javaScriptEnabled of [true, false]) {
  for (const isDarkMode of [true, false]) {
    test(`local and provider source storage Health (JavaScript ${javaScriptEnabled}, dark ${isDarkMode})`, async ({ browser, baseURL }, testInfo) => {
      test.skip(!databasePath, 'Requires the disposable E2E database.');
      test.setTimeout(60_000);
      const database = new DatabaseSync(databasePath!);
      database.exec('PRAGMA busy_timeout=10000;');
      const keys = [
        'teletext.work.max_concurrent',
        'teletext.source_storage.total_bytes',
        'teletext.source_storage.maximum_source_bytes',
        'teletext.source_storage.minimum_free_bytes',
      ];
      const original = keys.map(key => database.prepare('SELECT "Value" FROM "ConfigElement" WHERE "Key"=?').get(key) as { Value: string } | undefined);
      const write = (key: string, value: string | undefined) => {
        if (value === undefined) database.prepare('DELETE FROM "ConfigElement" WHERE "Key"=?').run(key);
        else database.prepare('INSERT OR REPLACE INTO "ConfigElement" ("Key","Value") VALUES (?,?)').run(key, value);
      };
      const context = await browser.newContext({ baseURL, javaScriptEnabled });
      try {
        write(keys[0], undefined);
        write(keys[1], '21474836480');
        write(keys[3], '2147483648');
        const page = await context.newPage();
        const errors: string[] = [];
        page.on('pageerror', error => errors.push(error.message));
        page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
        for (const [language, title, invalid, healthy, refresh] of [
          ['en-us', 'Teletext work configuration',
            'Source storage configuration is invalid or unavailable. Optional local and provider teletext captions are unavailable; ordinary audio/video playback remains available.',
            'Native teletext work is serialized by default. This is not a memory limit or proof that authored pages are ready.', 'Refresh'],
          ['nl', 'Instellingen voor teleteksttaken',
            'De opslaginstellingen voor bronnen zijn ongeldig of niet beschikbaar. Optionele teletekstondertitels van lokale bronnen en providers zijn niet beschikbaar; gewone audio- en videoweergave blijft beschikbaar.',
            'Teleteksttaken worden standaard één voor één uitgevoerd. Dit is geen geheugenlimiet en bevestigt niet dat zelfgemaakte pagina’s klaar zijn voor uitzending.', 'Vernieuwen'],
        ]) {
          write(keys[2], '0');
          expect((await page.request.post('/admin-api/settings/ui/save', {
            form: { is_dark_mode: String(isDarkMode), language }, maxRedirects: 0,
          })).status()).toBe(303);
          expect((await page.request.post('/admin-api/system/health/refresh', { maxRedirects: 0 })).status()).toBe(303);
          await page.goto('/system/health');
          const row = page.getByRole('row').filter({ has: page.getByText(title, { exact: true }) });
          await expect(row).toContainText(invalid);
          if (javaScriptEnabled) {
            await page.waitForFunction(
              () => (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true,
              undefined, { timeout: 15_000 },
            );
          }
          const stored = () => {
            const value = database.prepare('SELECT "Value" FROM "ConfigElement" WHERE "Key"=?').get('health.results') as { Value: string };
            return (JSON.parse(value.Value) as { title: string; status: string }[])
              .find(result => result.title === 'Teletext work configuration');
          };
          expect(stored()?.status).toBe('Warning');
          await row.scrollIntoViewIfNeeded();
          await row.screenshot({ path: `test-results/teletext-storage-health-${testInfo.project.name}-${javaScriptEnabled}-${isDarkMode}-${language}.png` });
          write(keys[2], '8589934592');
          const refreshed = page.waitForResponse(response => response.request().method() === 'POST' && response.url().includes(
            javaScriptEnabled ? '/admin-api/refresh_health_snapshot' : '/admin-api/system/health/refresh',
          ));
          await page.getByRole('button', { name: refresh, exact: true }).click();
          expect((await refreshed).status()).toBe(javaScriptEnabled ? 200 : 303);
          await expect(row).toContainText(healthy);
          await expect(row).not.toContainText(invalid);
          await expect(page.getByRole('button', { name: refresh, exact: true })).toBeEnabled();
          expect(stored()?.status).toBe('Pass');
          expect(database.prepare('SELECT "Value" FROM "ConfigElement" WHERE "Key"=?').get(keys[2])).toEqual({ Value: '8589934592' });
          expect(errors).toEqual([]);
        }
      } finally {
        keys.forEach((key, index) => write(key, original[index]?.Value));
        await context.request.post('/admin-api/settings/ui/save', {
          form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0,
        });
        await context.request.post('/admin-api/system/health/refresh', { maxRedirects: 0 });
        database.close();
        await context.close();
      }
    });
  }
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
          ['en-us', 'Teletext decoding', originalPts ? decoderReadyEnglish : clockUnavailableEnglish, 'Refresh'],
          ['nl', 'Teletekstdecodering', originalPts
            ? 'De geselecteerde FFmpeg-runtime heeft de controles voor teletekstpaginaselectie, ondertiteltekst en kleurenbitmapdecodering doorstaan.'
            : 'De runtime kan teletekst decoderen, maar behoudt de oorspronkelijke tijdstempels van liveondertitels niet. Live-teletekstondertitels zijn niet beschikbaar; gewone weergave blijft beschikbaar.', 'Vernieuwen'],
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
          expect(result?.status).toBe(originalPts ? 'Pass' : 'Info');
          expect(result?.message).toBe(originalPts ? decoderReadyEnglish : clockUnavailableEnglish);
          await row.scrollIntoViewIfNeeded();
          await page.screenshot({ path: testInfo.outputPath(`teletext-health-${language}.png`), fullPage: true });
          await row.screenshot({ path: testInfo.outputPath(`teletext-health-row-${language}.png`) });
        }
        await page.goto('/system/troubleshooting');
        const general = JSON.parse((await page.locator('#general-panel pre').textContent())!);
        const decoder = general.FfmpegSettings.TeletextDecoder;
        expect(decoder.Capability.status).toBe('ready');
        expect(decoder.Capability.missing_options).toEqual([]);
        expect(decoder.Capability.original_pts).toBe(originalPts);
        expect(decoder.RuntimeGeneration).toBeGreaterThan(0);
        expect(decoder.FFmpegSha256).toMatch(/^[a-f0-9]{64}$/);
        expect(decoder.FFprobeSha256).toMatch(/^[a-f0-9]{64}$/);
        if (process.env.ERSATZRS_TELETEXT_E2E_FFMPEG_SHA256) {
          expect(decoder.FFmpegSha256).toBe(process.env.ERSATZRS_TELETEXT_E2E_FFMPEG_SHA256);
        }
        if (process.env.ERSATZRS_TELETEXT_E2E_FFPROBE_SHA256) {
          expect(decoder.FFprobeSha256).toBe(process.env.ERSATZRS_TELETEXT_E2E_FFPROBE_SHA256);
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
