import { expect, test, TestInfo } from '@playwright/test';
import { execFileSync } from 'child_process';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

function seed(info: TestInfo, hydrated: boolean): { movie: number; show: number } {
  if (!dbPath) throw new Error('E2E_DB_PATH is required for the synthetic tag fixture');
  const offset = Array.from(info.project.name).reduce((sum, char) => sum + char.charCodeAt(0), 0);
  const base = 4_800_000 + offset * 100 + (hydrated ? 50 : 0);
  const movie = base + 10;
  const show = base + 20;
  execFileSync('sqlite3', [dbPath], {
    encoding: 'utf8',
    input: `.timeout 10000
PRAGMA foreign_keys=ON;
BEGIN;
DELETE FROM "MediaSource" WHERE "Id" = ${base};
INSERT INTO "MediaSource" ("Id") VALUES (${base});
INSERT INTO "LocalMediaSource" ("Id") VALUES (${base});
INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
VALUES (${base}, 'Synthetic tag ownership', 1, ${base});
INSERT INTO "LocalLibrary" ("Id") VALUES (${base});
INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (${base}, ${base}, '/synthetic/tag-ownership');
INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (${movie}, ${base}, 0), (${show}, ${base}, 0);
INSERT INTO "Movie" ("Id") VALUES (${movie});
INSERT INTO "Show" ("Id") VALUES (${show});
INSERT INTO "MovieMetadata" ("Id", "MovieId", "MetadataKind", "Title", "SortTitle", "DateAdded", "DateUpdated")
VALUES (${movie}, ${movie}, 1, 'Country fixture', 'Country fixture', '2026-09-27T00:00:00Z', '2026-09-27T00:00:00Z');
INSERT INTO "ShowMetadata" ("Id", "ShowId", "MetadataKind", "Title", "SortTitle", "DateAdded", "DateUpdated")
VALUES (${show}, ${show}, 2, 'Owned tag fixture', 'Owned tag fixture', '2026-09-27T00:00:00Z', '2026-09-27T00:00:00Z');
INSERT INTO "Tag" ("Name", "MovieMetadataId", "ExternalTypeId") VALUES
('Netherlands', ${movie}, 'nfo/country'), ('Owned movie label', ${movie}, 'plex/label');
INSERT INTO "Tag" ("Name", "ShowMetadataId", "ExternalTypeId") VALUES
('Owned label', ${show}, 'plex/label'), ('Owned collection', ${show}, 'jellyfin/collection'),
('Owned network', ${show}, '319'), ('Hidden country', ${show}, 'nfo/country');
COMMIT;
`,
  });
  return { movie, show };
}

for (const hydrated of [true, false]) {
  test.describe(`tag ownership (${hydrated ? 'hydrated' : 'no JS'})`, () => {
    test.use({ javaScriptEnabled: hydrated });
    test.skip(!dbPath, 'A disposable E2E database is required');

    test('separates countries and networks while retaining typed labels and collections', async ({ page }, info) => {
      const ids = seed(info, hydrated);
      const errors: string[] = [];
      page.on('pageerror', (error) => errors.push(error.message));
      const goto = async (path: string) => {
        const response = await page.goto(path);
        expect(response?.status()).toBe(200);
        if (hydrated) {
          await expect.poll(() => page.evaluate(
            () => (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true,
          ), { timeout: 20_000 }).toBe(true);
        }
      };
      await goto(`/search?query=${encodeURIComponent('country:Netherlands')}`);
      await expect(page.locator(`a[href="/media/movies/${ids.movie}"]`).first()).toBeVisible();
      await goto(`/search?query=${encodeURIComponent('tag:Netherlands')}`);
      await expect(page.locator(`a[href="/media/movies/${ids.movie}"]`)).toHaveCount(0);
      await goto(`/search?query=${encodeURIComponent('tag:"Owned movie label"')}`);
      await expect(page.locator(`a[href="/media/movies/${ids.movie}"]`).first()).toBeVisible();
      await goto(`/media/tv/shows/${ids.show}`);
      await expect(page.getByRole('link', { name: 'Owned label', exact: true })).toBeVisible();
      await expect(page.getByRole('link', { name: 'Owned collection', exact: true })).toBeVisible();
      const network = page.getByRole('link', { name: 'Owned network', exact: true });
      await expect(network).toBeVisible();
      expect(decodeURIComponent(await network.getAttribute('href') ?? '')).toContain('network:');
      await expect(page.getByText('Hidden country', { exact: true })).toHaveCount(0);
      expect(errors).toEqual([]);
    });
  });
}
