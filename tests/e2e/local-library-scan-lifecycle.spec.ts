import { expect, Page, test } from '@playwright/test';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  addLocalLibrary, hydrated, MediaKind, quote, scanCounts, scanLibrary, sqlite, writeShowTree,
} from './helpers/scanned-library';

// End-to-end lifecycle of a local library that the real scanner populated:
// the shape that failed with `FOREIGN KEY constraint failed` before 4aa313a
// (nested folders plus media files inside child folders) is produced here by
// the application's own scanner, not by hand-inserted rows.

function cleanup(name: string) {
  const id = sqlite(`SELECT "Id" FROM "Library" WHERE "Name"=${quote(name)};`);
  if (!id) return;
  sqlite(`
    DELETE FROM "MediaItem" WHERE "LibraryPathId" IN (SELECT "Id" FROM "LibraryPath" WHERE "LibraryId"=${id});
    UPDATE "LibraryFolder" SET "ParentId"=NULL
      WHERE "LibraryPathId" IN (SELECT "Id" FROM "LibraryPath" WHERE "LibraryId"=${id});
    DELETE FROM "MediaSource" WHERE "Id"=(SELECT "MediaSourceId" FROM "Library" WHERE "Id"=${id});
  `);
}

function watchErrors(page: Page): string[] {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  return errors;
}

test.describe('scanned local library lifecycle', () => {
  let directory: string;
  const names: string[] = [];

  test.beforeAll(() => {
    directory = mkdtempSync(join(tmpdir(), 'ersatzrs-scan-e2e-'));
  });
  test.afterAll(() => {
    for (const name of names) cleanup(name);
    rmSync(directory, { recursive: true, force: true });
  });

  test('deleting a scanned library removes its folder tree and media', async ({ page }, testInfo) => {
    test.setTimeout(150_000);
    const errors = watchErrors(page);
    const name = `00 E2E real scan delete ${testInfo.project.name}`;
    names.push(name);
    const root = join(directory, `delete-${testInfo.project.name}`);
    const episodes = writeShowTree(root, 'Delete');

    const id = await addLocalLibrary(page, name, MediaKind.Shows, [root]);
    await scanLibrary(page, id, name, episodes);
    const scanned = scanCounts(id);
    // The scan must have produced the shape that used to break the cascade.
    expect(scanned.nestedFolders).toBeGreaterThan(0);
    expect(scanned.files).toBe(episodes);

    await page.goto('/media/sources/local');
    await hydrated(page);
    const row = page.getByRole('row').filter({ hasText: name });
    await row.getByRole('button', { name: 'Delete', exact: true }).click();
    const dialog = page.locator('#local-source-delete-dialog');
    await expect(dialog).toBeVisible();
    await dialog.getByRole('button', { name: 'Delete', exact: true }).click();

    await expect(dialog).toBeHidden();
    await expect(row).toHaveCount(0);
    await expect(page.locator('[data-toast-severity="error"]')).toHaveCount(0);
    expect(sqlite(`SELECT COUNT(*) FROM "Library" WHERE "Id"=${id};`)).toBe('0');
    expect(scanCounts(id)).toEqual({ folders: 0, nestedFolders: 0, items: 0, files: 0 });
    expect(sqlite(`SELECT COUNT(*) FROM "LibraryPath" WHERE "LibraryId"=${id};`)).toBe('0');
    expect(errors).toEqual([]);
  });

  test('removing a scanned path keeps the other path and its media', async ({ page }, testInfo) => {
    test.setTimeout(150_000);
    const errors = watchErrors(page);
    const name = `00 E2E real scan path ${testInfo.project.name}`;
    names.push(name);
    const removed = join(directory, `removed-${testInfo.project.name}`);
    const kept = join(directory, `kept-${testInfo.project.name}`);
    const episodes = writeShowTree(removed, 'Removed') + writeShowTree(kept, 'Kept');

    const id = await addLocalLibrary(page, name, MediaKind.Shows, [removed, kept]);
    await scanLibrary(page, id, name, episodes);
    const keptPath = sqlite(`SELECT "Id" FROM "LibraryPath" WHERE "LibraryId"=${id} AND "Path"=${quote(kept)};`);
    const keptItems = sqlite(`SELECT COUNT(*) FROM "MediaItem" WHERE "LibraryPathId"=${keptPath};`);
    expect(Number(keptItems)).toBeGreaterThan(0);

    await page.goto(`/media/sources/local/${id}/edit`);
    await hydrated(page);
    await page.locator('#local-library-paths').fill(kept);
    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await expect(page).toHaveURL(/\/media\/sources\/local$/);

    expect(sqlite(`SELECT "Path" FROM "LibraryPath" WHERE "LibraryId"=${id};`)).toBe(kept);
    expect(sqlite(`SELECT COUNT(*) FROM "MediaItem" WHERE "LibraryPathId"=${keptPath};`)).toBe(keptItems);
    const remaining = scanCounts(id);
    expect(remaining.items).toBe(Number(keptItems));
    expect(remaining.files).toBe(episodes / 2);
    expect(remaining.nestedFolders).toBeGreaterThan(0);
    expect(sqlite(`SELECT COUNT(*) FROM "LibraryFolder" WHERE "Path" LIKE ${quote(`${removed}%`)};`)).toBe('0');
    expect(errors).toEqual([]);
  });
});
