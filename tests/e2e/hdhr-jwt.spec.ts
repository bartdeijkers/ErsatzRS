import { expect, Page, test } from '@playwright/test';
import { execFileSync } from 'child_process';

const jwtEnabled = process.env.E2E_HDHR_JWT_ENABLED === '1';
const dbPath = process.env.E2E_DB_PATH;
const languages = [
  ['en-us', 'HDHomeRun endpoints are disabled because IPTV JWT protection is enabled. These settings have no effect.'],
  ['nl', 'HDHomeRun-eindpunten zijn uitgeschakeld omdat JWT-beveiliging voor IPTV is ingeschakeld. Deze instellingen hebben geen effect.'],
] as const;

function storedTuners(): string {
  return execFileSync('sqlite3', [dbPath!, 'SELECT "Value" FROM "ConfigElement" WHERE "Key" = \'hdhr.tuner_count\''], { encoding: 'utf8' }).trim();
}

async function hydrated(page: Page): Promise<void> {
  await expect.poll(() => page.evaluate(() =>
    (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true,
  ), { timeout: 20_000 }).toBe(true);
}

async function language(page: Page, value: string, javascript: boolean): Promise<void> {
  await page.goto('/settings/ui');
  if (javascript) await hydrated(page);
  const form = page.locator('form[action="/admin-api/settings/ui/save"]');
  await form.locator('select[name="language"]').selectOption(value);
  await form.locator('button[type="submit"]').click();
  await expect(page.locator('html')).toHaveAttribute('lang', value);
  if (javascript) await hydrated(page);
}

test.describe('HDHomeRun JWT policy', () => {
  test.describe.configure({ mode: 'serial' });
  test.skip(!dbPath, 'requires the isolated E2E database');

  for (const javascript of [true, false]) {
    test(`${javascript ? 'hydrated' : 'no-JS'} settings and discovery follow JWT policy`, async ({ browser, baseURL }, testInfo) => {
      const context = await browser.newContext({ baseURL, javaScriptEnabled: javascript });
      const page = await context.newPage();
      const errors: string[] = [];
      page.on('pageerror', error => errors.push(error.message));
      const before = storedTuners();
      try {
        for (const [code, warning] of languages) {
          await language(page, code, javascript);
          await page.goto('/settings/hdhr');
          await expect(page.getByRole('heading', { name: 'HDHomeRun', exact: true })).toBeVisible();
          if (javascript) await hydrated(page);
          const form = page.locator('form[action="/admin-api/settings/hdhr/save"]');
          const tuner = form.locator('input[name="tuner_count"]');
          const save = form.locator('button[type="submit"]');
          if (jwtEnabled) {
            await expect(tuner).toBeDisabled();
            await expect(save).toBeDisabled();
            await expect(form.getByRole('status')).toHaveText(warning);
            await expect(form.getByRole('status')).toBeVisible();
          } else {
            await expect(tuner).toBeEnabled();
            await expect(save).toBeEnabled();
            await expect(form.getByRole('status')).toBeHidden();
          }
        }
        if (javascript && testInfo.project.name === 'chromium') {
          await page.screenshot({ path: testInfo.outputPath('hdhr-settings-desktop.png') });
          await page.setViewportSize({ width: 390, height: 844 });
          await page.reload();
          await hydrated(page);
          const navigationToggle = page.getByRole('button', { name: 'Navigatie omschakelen' });
          if (await navigationToggle.getAttribute('aria-expanded') === 'true') {
            await navigationToggle.click();
            await expect(navigationToggle).toHaveAttribute('aria-expanded', 'false');
          }
          expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
          await page.screenshot({ path: testInfo.outputPath('hdhr-settings-mobile.png'), animations: 'disabled' });
        }
        for (const path of ['/discover.json', '/device.xml', '/lineup_status.json', '/lineup.json']) {
          expect((await context.request.get(path)).status(), path).toBe(jwtEnabled ? 404 : 200);
          expect((await context.request.head(path)).status(), `HEAD ${path}`).toBe(jwtEnabled ? 404 : 200);
        }
        if (jwtEnabled) {
          const response = await context.request.post('/admin-api/settings/hdhr/save', {
            form: { tuner_count: '7', uuid: 'ignored' }, maxRedirects: 0,
          });
          expect(response.status()).toBe(400);
          expect(await response.text()).toContain(languages[1][1]);
          expect(storedTuners()).toBe(before);
        } else {
          const next = before === '7' ? '8' : '7';
          const form = page.locator('form[action="/admin-api/settings/hdhr/save"]');
          await form.locator('input[name="tuner_count"]').fill(next);
          await form.locator('button[type="submit"]').click();
          await expect(page).toHaveURL(/\/settings\/hdhr$/);
          expect(storedTuners()).toBe(next);
          const discovery = await (await context.request.get('/discover.json')).json();
          expect(discovery.TunerCount).toBe(Number(next));
        }
        expect(errors).toEqual([]);
      } finally {
        if (!jwtEnabled) {
          await context.request.post('/admin-api/settings/hdhr/save', { form: { tuner_count: before || '2' } });
        }
        await language(page, 'en-us', javascript);
        await context.close();
      }
    });
  }
});
