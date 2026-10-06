import { expect, test } from '@playwright/test';
import { execFileSync } from 'node:child_process';

const first = 940_156;
function sql(statement: string): string {
  const db = process.env.E2E_DB_PATH;
  if (!db || !process.env.E2E_APP_STATE_DIR) throw new Error('Use the isolated Playwright wrapper');
  return execFileSync('sqlite3', [db], {
    input: `.timeout 10000\nPRAGMA foreign_keys=ON;\n${statement}`, encoding: 'utf8',
  }).trim();
}
const quote = (value: string) => `'${value.replaceAll("'", "''")}'`;
function restore(key: string, value: string | undefined) {
  if (value === undefined) sql(`DELETE FROM "ConfigElement" WHERE "Key"=${quote(key)};`);
  else sql(`INSERT INTO "ConfigElement" ("Key","Value") VALUES (${quote(key)},${quote(value)})
    ON CONFLICT("Key") DO UPDATE SET "Value"=excluded."Value";`);
}
function setting(key: string): string | undefined {
  const result = sql(`SELECT json_array("Value") FROM "ConfigElement" WHERE "Key"=${quote(key)};`);
  return result ? JSON.parse(result)[0] : undefined;
}
function clone(table: 'FFmpegProfile' | 'Channel', overrides: Record<string, string>) {
  const columns = sql(`PRAGMA table_info("${table}");`).split('\n').map(row => row.split('|')[1]);
  const select = columns.map(name => overrides[name] ?? `"${name}"`);
  sql(`INSERT INTO "${table}" (${columns.map(name => `"${name}"`).join(',')})
    SELECT ${select.join(',')} FROM "${table}" WHERE "Id" < ${first} ORDER BY "Id" LIMIT 1;`);
}

test.describe('profile deletion safety', () => {
  test.describe.configure({ mode: 'serial' });
  let originalDefault: string | undefined;
  let originalLanguage: string | undefined;
  test.beforeAll(() => {
    originalDefault = setting('ffmpeg.default_profile_id');
    originalLanguage = setting('pages.language');
    for (const [offset, name] of [[0, '00 Row156 in use'], [1, '00 Row156 default'], [2, '00 Row156 removable']] as const) {
      clone('FFmpegProfile', { Id: `${first + offset}`, Name: quote(name) });
    }
    clone('Channel', {
      Id: `${first}`, Name: quote('Row156 owned channel'), Number: quote(`${first}`),
      UniqueId: quote('00000000-0000-0000-0000-000000940156'),
      FFmpegProfileId: `${first}`, IsEnabled: '0',
    });
    restore('ffmpeg.default_profile_id', `${first + 1}`);
  });
  test.afterAll(() => {
    restore('ffmpeg.default_profile_id', originalDefault);
    restore('pages.language', originalLanguage);
    sql(`DELETE FROM "Channel" WHERE "Id"=${first};
      DELETE FROM "FFmpegProfile" WHERE "Id" BETWEEN ${first} AND ${first + 2};`);
  });
  for (const [language, remove, used, defaultMessage] of [
    ['en-US', /^Delete$/, 'This FFmpeg profile is used by a channel and cannot be deleted.', 'The default FFmpeg profile cannot be deleted.'],
    ['nl-NL', /^Verwijderen$/, 'Dit FFmpeg-profiel wordt door een kanaal gebruikt en kan niet worden verwijderd.', 'Het standaard-FFmpeg-profiel kan niet worden verwijderd.'],
  ] as const) {
    test(`${language} explains blocked deletion and preserves the selected profiles`, async ({ page }) => {
      restore('pages.language', language);
      for (const [name, message, id] of [['00 Row156 in use', used, first], ['00 Row156 default', defaultMessage, first + 1]] as const) {
        await page.goto('/ffmpeg');
        await expect(page.locator('html')).toHaveAttribute('lang', language === 'nl-NL' ? 'nl' : 'en-us');
        await expect.poll(() => page.evaluate(() => (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated)).toBe(true);
        const row = page.getByRole('row').filter({ hasText: name });
        await expect(row).toBeVisible();
        await row.getByRole('button', { name: remove }).click();
        const dialog = page.locator('#ffmpeg-profile-delete-dialog');
        await expect(dialog).toBeVisible();
        await dialog.getByRole('button', { name: remove }).click();
        await expect(page.getByText(message, { exact: true })).toBeVisible();
        await expect(dialog).toBeHidden();
        await expect(row).toBeVisible();
        expect(sql(`SELECT COUNT(*) FROM "FFmpegProfile" WHERE "Id"=${id};`)).toBe('1');
        expect(sql(`SELECT COUNT(*) FROM "Channel" WHERE "Id"=${first} AND "FFmpegProfileId"=${first};`)).toBe('1');
      }
    });
  }
  test('successful deletion still closes the dialog and refreshes the list', async ({ page }) => {
    restore('pages.language', 'en-US');
    await page.goto('/ffmpeg');
    await expect.poll(() => page.evaluate(() => (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated)).toBe(true);
    const row = page.getByRole('row').filter({ hasText: '00 Row156 removable' });
    await row.getByRole('button', { name: /^Delete$/ }).click();
    const dialog = page.locator('#ffmpeg-profile-delete-dialog');
    await dialog.getByRole('button', { name: /^Delete$/ }).click();
    await expect(dialog).toBeHidden();
    await expect(row).toHaveCount(0);
    expect(sql(`SELECT COUNT(*) FROM "FFmpegProfile" WHERE "Id"=${first + 2};`)).toBe('0');
  });
});
