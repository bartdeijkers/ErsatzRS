import { expect, Locator, Page, test } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { DatabaseSync } from 'node:sqlite';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Run Chromium and Firefox sequentially with --headed --workers 1. Only the
// wrapper's disposable E2E_DB_PATH is accepted, never the normal server DB.
const databasePath = process.env.E2E_DB_PATH;
const specUrl = pathToFileURL(__filename);
const policyKey = 'teletext.output.policy';
const policyTitle = 'Teletext output configuration';
const validPolicy = readFileSync(fileURLToPath(new URL(
  '../../crates/ersatzrs-web/tests/fixtures/teletext-output-policy-v1.json', specUrl,
)), 'utf8');
const languages = [
  ['en-us', 'en_us'], ['nl', 'nl'], ['pl', 'pl'], ['es-es', 'es_es'],
  ['es-419', 'es_419'], ['fr-fr', 'fr_fr'], ['de-de', 'de_de'],
  ['it-it', 'it_it'], ['pt-br', 'pt_br'],
] as const;

function translations(module: string): (key: string) => string {
  const source = readFileSync(fileURLToPath(new URL(
    `../../crates/ersatzrs-web/src/admin/languages/${module}.rs`, specUrl,
  )), 'utf8');
  return key => {
    const literal = source.match(new RegExp(String.raw`TextKey::${key}\s*=>\s*("(?:[^"\\]|\\.)*")`));
    if (!literal) throw new Error(`Missing literal translation for ${module}/${key}`);
    return JSON.parse(literal[1]) as string;
  };
}

function setting(database: DatabaseSync, key: string): string | undefined {
  const row = database.prepare('SELECT "Value" FROM "ConfigElement" WHERE "Key"=?')
    .get(key) as { Value: string } | undefined;
  return row?.Value;
}

function setSetting(database: DatabaseSync, key: string, value: string | undefined): void {
  if (value === undefined) database.prepare('DELETE FROM "ConfigElement" WHERE "Key"=?').run(key);
  else database.prepare('REPLACE INTO "ConfigElement" ("Key","Value") VALUES (?,?)').run(key, value);
}

async function hydrated(page: Page): Promise<void> {
  await page.waitForFunction(
    () => (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true,
    undefined, { timeout: 15_000 },
  );
}

async function saveUi(
  page: Page, database: DatabaseSync, language: string, dark: boolean, javaScriptEnabled: boolean,
): Promise<void> {
  await page.goto('/settings/ui');
  if (javaScriptEnabled) await hydrated(page);
  const form = page.locator('form[action="/admin-api/settings/ui/save"]');
  await form.locator('select[name="language"]').selectOption(language);
  await form.locator('select[name="is_dark_mode"]').selectOption(String(dark));
  const saved = page.waitForResponse(response => response.request().method() === 'POST'
    && new URL(response.url()).pathname === '/admin-api/settings/ui/save');
  const navigated = page.waitForEvent('framenavigated', {
    predicate: frame => frame === page.mainFrame(),
  });
  await form.locator('button[type="submit"]').click();
  expect((await saved).status()).toBe(303);
  await navigated;
  await page.waitForLoadState('domcontentloaded');
  if (javaScriptEnabled) await hydrated(page);
  await expect.poll(() => setting(database, 'pages.language')).toBe(language);
  await expect.poll(() => setting(database, 'pages.is_dark_mode')).toBe(dark ? 'True' : 'False');
}

async function assertRow(row: Locator, message: string, status: string): Promise<void> {
  await expect(row).toHaveCount(1);
  await expect(row).toBeVisible();
  await expect(row.locator('td').nth(1)).toHaveText(message);
  await expect(row.locator('span[aria-label]').first()).toHaveAttribute('aria-label', status);
  await expect(row).not.toContainText('synthetic-private-policy');
}

// /system/health -> HealthPage -> existing HealthIsland and SSR/no-JS form.
// This proves fresh settings reporting, not stream delivery or engine acceptance.
test.describe.configure({ mode: 'serial' });
for (const javaScriptEnabled of [true, false]) {
  for (const dark of [false, true]) {
    test(`teletext output policy health (JavaScript ${javaScriptEnabled}, dark ${dark})`, async ({ browser, baseURL }, testInfo) => {
      test.skip(!databasePath, 'Requires the disposable E2E_DB_PATH.');
      test.setTimeout(120_000);
      const database = new DatabaseSync(databasePath!);
      database.exec('PRAGMA busy_timeout=10000;');
      const original = new Map([policyKey, 'pages.language', 'pages.is_dark_mode']
        .map(key => [key, setting(database, key)] as const));
      const context = await browser.newContext({ baseURL, javaScriptEnabled });
      try {
        const page = await context.newPage();
        const errors: string[] = [];
        page.on('pageerror', error => errors.push(error.message));
        page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
        expect(JSON.parse(validPolicy).version).toBe(1);
        expect(translations('en_us')('TeletextOutputConfiguredHelp'))
          .toContain('does not verify teletext delivery, full Level 2.5 support or memory limits');
        for (const [language, module] of languages) {
          const text = translations(module);
          const capture = async (row: Locator, state: string) => {
            await row.scrollIntoViewIfNeeded();
            await row.screenshot({ path: testInfo.outputPath(`output-policy-${language}-${state}.png`) });
          };
          setSetting(database, policyKey, undefined);
          await saveUi(page, database, language, dark, javaScriptEnabled);
          await page.goto('/system/health');
          await expect(page.locator('[data-not-found]')).toHaveCount(0);
          await expect(page.locator('[data-theme]').first()).toHaveAttribute('data-theme', dark ? 'dark' : 'light');
          if (javaScriptEnabled) await hydrated(page);
          const row = page.getByRole('row').filter({ has: page.getByText(text('TeletextOutputTitle'), { exact: true }) });
          await assertRow(row, text('TeletextOutputAbsentHelp'), text('Info'));
          await capture(row, 'absent');
          const cached = setting(database, 'health.results');
          expect(cached).toBeTruthy();
          expect(JSON.parse(cached!).some((result: { title: string }) => result.title === policyTitle)).toBe(false);

          // Navigation reuses the completed baseline while the host overlays a
          // fresh configuration read; no refresh or persisted-result edit here.
          setSetting(database, policyKey, '{"synthetic-private-policy":"malformed"}');
          await page.reload();
          if (javaScriptEnabled) await hydrated(page);
          await assertRow(row, text('TeletextOutputInvalidHelp'), text('Warning'));
          expect(setting(database, 'health.results')).toBe(cached);
          await capture(row, 'invalid');

          setSetting(database, policyKey, validPolicy);
          if (language === 'en-us') {
            const refreshed = page.waitForResponse(response => response.request().method() === 'POST'
              && response.url().includes(javaScriptEnabled
                ? '/admin-api/refresh_health_snapshot' : '/admin-api/system/health/refresh'));
            await page.getByRole('button', { name: text('Refresh'), exact: true }).click();
            expect((await refreshed).status()).toBe(javaScriptEnabled ? 200 : 303);
          } else {
            await page.reload();
            if (javaScriptEnabled) await hydrated(page);
            expect(setting(database, 'health.results')).toBe(cached);
          }
          await assertRow(row, text('TeletextOutputConfiguredHelp'), text('Pass'));
          await expect(page.getByRole('button', { name: text('Refresh'), exact: true })).toBeEnabled();
          const persisted = JSON.parse(setting(database, 'health.results')!) as { title: string }[];
          expect(persisted.some(result => result.title === policyTitle)).toBe(false);
          expect(setting(database, policyKey)).toBe(validPolicy);
          await capture(row, 'configured');
          expect(errors).toEqual([]);
        }
      } finally {
        try {
          await context.request.post('/admin-api/settings/ui/save', {
            form: {
              language: original.get('pages.language') ?? 'en-us',
              is_dark_mode: String(/^(1|true)$/i.test(original.get('pages.is_dark_mode') ?? 'True')),
            }, maxRedirects: 0,
          });
        } finally {
          for (const [key, value] of original) setSetting(database, key, value);
          database.close();
          await context.close();
        }
      }
    });
  }
}
