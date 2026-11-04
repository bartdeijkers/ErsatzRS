import { expect, Page, test } from '@playwright/test';
import { execFileSync } from 'node:child_process';

// Deleting a local library is only interesting once a scan has populated it:
// a nested `LibraryFolder` tree plus a `MediaFile` inside a child folder is the
// shape that tripped the `ON DELETE RESTRICT` keys during the cascade. Seed that
// shape directly so the browser flow exercises the same rows a real scan writes.
const source = 941_000;
const library = 941_001;
const path = 941_002;
const folders = [941_003, 941_004, 941_005];
const item = 941_006;
const version = 941_007;
const file = 941_008;
const trigger = 'e2e_local_library_delete_failure';

function sql(statement: string): string {
  const db = process.env.E2E_DB_PATH;
  if (!db || !process.env.E2E_APP_STATE_DIR) throw new Error('Use the isolated Playwright wrapper');
  return execFileSync('sqlite3', [db], {
    input: `.timeout 10000\nPRAGMA foreign_keys=ON;\n${statement}`, encoding: 'utf8',
  }).trim();
}
const quote = (value: string) => `'${value.replaceAll("'", "''")}'`;

// Switch the language through the settings route so the running host picks
// it up, exactly as an operator would.
async function useLanguage(page: Page, language: 'en-us' | 'nl') {
  const response = await page.request.post('/admin-api/settings/ui/save', {
    form: { is_dark_mode: 'true', language }, maxRedirects: 0,
  });
  expect(response.status()).toBe(303);
}

function seedScannedLibrary(name: string) {
  const root = `/e2e/scanned/${library}`;
  sql(`
    INSERT INTO "MediaSource" ("Id") VALUES (${source});
    INSERT INTO "LocalMediaSource" ("Id") VALUES (${source});
    INSERT INTO "Library" ("Id", "MediaSourceId", "MediaKind", "Name")
      VALUES (${library}, ${source}, 1, ${quote(name)});
    INSERT INTO "LocalLibrary" ("Id") VALUES (${library});
    INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (${path}, ${library}, ${quote(root)});
    INSERT INTO "LibraryFolder" ("Id", "LibraryPathId", "ParentId", "Path") VALUES
      (${folders[0]}, ${path}, NULL, ${quote(root)}),
      (${folders[1]}, ${path}, ${folders[0]}, ${quote(`${root}/season`)}),
      (${folders[2]}, ${path}, ${folders[1]}, ${quote(`${root}/season/extras`)});
    INSERT INTO "MediaItem" ("Id", "LibraryPathId") VALUES (${item}, ${path});
    INSERT INTO "Movie" ("Id") VALUES (${item});
    INSERT INTO "MediaVersion" ("Id", "MovieId", "Name", "Duration", "VideoScanKind",
        "DateAdded", "DateUpdated", "Width", "Height")
      VALUES (${version}, ${item}, 'Main', '00:20:00', 0, '2026-01-01 00:00:00',
        '2026-01-01 00:00:00', 1920, 1080);
    INSERT INTO "MediaFile" ("Id", "LibraryFolderId", "MediaVersionId", "Path", "PathHash")
      VALUES (${file}, ${folders[1]}, ${version}, ${quote(`${root}/season/episode.mkv`)}, 'e2e-delete-hash');
  `);
}

function remaining(): Record<string, string> {
  return {
    MediaSource: sql(`SELECT COUNT(*) FROM "MediaSource" WHERE "Id"=${source};`),
    Library: sql(`SELECT COUNT(*) FROM "Library" WHERE "Id"=${library};`),
    LibraryPath: sql(`SELECT COUNT(*) FROM "LibraryPath" WHERE "Id"=${path};`),
    LibraryFolder: sql(`SELECT COUNT(*) FROM "LibraryFolder" WHERE "LibraryPathId"=${path};`),
    MediaItem: sql(`SELECT COUNT(*) FROM "MediaItem" WHERE "Id"=${item};`),
    MediaFile: sql(`SELECT COUNT(*) FROM "MediaFile" WHERE "Id"=${file};`),
  };
}

function cleanup() {
  sql(`
    DROP TRIGGER IF EXISTS ${trigger};
    DELETE FROM "MediaItem" WHERE "Id"=${item};
    UPDATE "LibraryFolder" SET "ParentId"=NULL WHERE "LibraryPathId"=${path};
    DELETE FROM "MediaSource" WHERE "Id"=${source};
  `);
}

async function openDeleteDialog(page: Page, language: string, name: string, remove: RegExp) {
  await page.goto('/media/sources/local');
  await expect(page.locator('html')).toHaveAttribute('lang', language);
  await expect.poll(() => page.evaluate(
    () => (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated,
  )).toBe(true);
  const row = page.getByRole('row').filter({ hasText: name });
  await expect(row).toBeVisible();
  await row.getByRole('button', { name: remove }).click();
  const dialog = page.locator('#local-source-delete-dialog');
  await expect(dialog).toBeVisible();
  await dialog.getByRole('button', { name: remove }).click();
  return { row, dialog };
}

test.describe('local library deletion', () => {
  test.describe.configure({ mode: 'serial' });
  test.afterEach(async ({ page }) => {
    cleanup();
    // Restore the wrapper baseline even when a localized assertion fails.
    await useLanguage(page, 'en-us');
  });

  for (const [language, remove, failed] of [
    ['en-us', /^Delete$/, 'Delete failed:'],
    ['nl', /^Verwijderen$/, 'Verwijderen mislukt:'],
  ] as const) {
    test(`${language} deletes a scanned library with its folder tree and media`, async ({ page }) => {
      const errors: string[] = [];
      page.on('pageerror', error => errors.push(error.message));
      await useLanguage(page, language);
      const name = `00 E2E scanned ${language}`;
      seedScannedLibrary(name);

      const { row, dialog } = await openDeleteDialog(page, language, name, remove);

      await expect(dialog).toBeHidden();
      await expect(row).toHaveCount(0);
      await expect(page.locator('[data-toast-severity="error"]')).toHaveCount(0);
      expect(remaining()).toEqual({
        MediaSource: '0', Library: '0', LibraryPath: '0',
        LibraryFolder: '0', MediaItem: '0', MediaFile: '0',
      });
      expect(errors).toEqual([]);
    });

    test(`${language} reports a failed delete instead of leaving the dialog open`, async ({ page }) => {
      await useLanguage(page, language);
      const name = `00 E2E undeletable ${language}`;
      seedScannedLibrary(name);
      // Force the repository delete to fail inside its transaction.
      sql(`CREATE TRIGGER ${trigger} BEFORE DELETE ON "MediaSource" WHEN OLD."Id"=${source}
        BEGIN SELECT RAISE(ABORT, 'e2e forced local library delete failure'); END;`);

      const { row, dialog } = await openDeleteDialog(page, language, name, remove);

      const toast = page.locator('[data-toast-severity="error"]').filter({ hasText: failed });
      await expect(toast).toBeVisible();
      await expect(toast).toHaveAttribute('role', 'alert');
      await expect(dialog).toBeHidden();
      await expect(row).toBeVisible();
      // The failed transaction must leave the scanned library intact.
      expect(remaining()).toEqual({
        MediaSource: '1', Library: '1', LibraryPath: '1',
        LibraryFolder: '3', MediaItem: '1', MediaFile: '1',
      });
    });
  }
});
