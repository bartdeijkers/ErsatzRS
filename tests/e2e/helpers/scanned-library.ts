import { expect, Page } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';

// Real-scan fixture: generates a tiny movie tree with the pinned FFmpeg, adds
// it as a local library through the admin UI and runs the application's own
// scanner through the Libraries page. Specs that need scan-shaped data (nested
// `LibraryFolder` rows, `MediaVersion`/`MediaFile` rows written by ffprobe)
// should use this instead of hand-inserting rows, so the data stays in step
// with whatever the scanner writes.

export function sqlite(statement: string): string {
  const db = process.env.E2E_DB_PATH;
  if (!db || !process.env.E2E_APP_STATE_DIR) throw new Error('Use the isolated Playwright wrapper');
  return execFileSync('sqlite3', [db], {
    input: `.timeout 10000\nPRAGMA foreign_keys=ON;\n${statement}`, encoding: 'utf8',
  }).trim();
}

export const quote = (value: string) => `'${value.replaceAll("'", "''")}'`;

function ffmpegPath(): string {
  return process.env.ErsatzRS_FFMPEG__PATH
    ?? process.env.ERSATZRS_FFMPEG__PATH
    ?? process.env.FFMPEG_PATH
    ?? 'ffmpeg';
}

/** Writes one short, valid H.264/AAC movie at `file`. */
function writeClip(file: string) {
  execFileSync(ffmpegPath(), [
    '-hide_banner', '-loglevel', 'error', '-y',
    '-f', 'lavfi', '-i', 'testsrc2=size=160x90:rate=25',
    '-f', 'lavfi', '-i', 'sine=frequency=440:sample_rate=48000',
    '-t', '2', '-c:v', 'libx264', '-pix_fmt', 'yuv420p', '-preset', 'ultrafast',
    '-c:a', 'aac', '-b:a', '64k', file,
  ], { stdio: 'inherit', timeout: 30_000 });
}

/**
 * Creates `root` as a local TV library: two shows, one with two seasons, so
 * the television scanner writes a folder tree whose children reference their
 * parents (`LibraryFolder.ParentId`) and episode files that live in season
 * folders (`MediaFile.LibraryFolderId`). The television scanner is the one
 * that writes `LibraryFolder` rows; the movie scanner does not.
 *
 * Returns the number of episode files written.
 */
export function writeShowTree(root: string, prefix: string): number {
  const episodes = [
    join(root, `${prefix} Alpha (2001)`, 'Season 01', `${prefix} Alpha (2001) - S01E01.mkv`),
    join(root, `${prefix} Alpha (2001)`, 'Season 01', `${prefix} Alpha (2001) - S01E02.mkv`),
    join(root, `${prefix} Alpha (2001)`, 'Season 02', `${prefix} Alpha (2001) - S02E01.mkv`),
    join(root, `${prefix} Beta (2002)`, 'Season 01', `${prefix} Beta (2002) - S01E01.mkv`),
  ];
  for (const episode of episodes) {
    mkdirSync(dirname(episode), { recursive: true });
    writeClip(episode);
  }
  return episodes.length;
}

export async function hydrated(page: Page) {
  await page.waitForFunction(
    () => (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true,
    undefined,
    { timeout: 15_000 },
  );
}

/** Local library media kinds, as stored in `Library.MediaKind`. */
export const MediaKind = { Movies: '1', Shows: '2' } as const;

/** Adds a local library through the admin form and returns its id. */
export async function addLocalLibrary(
  page: Page, name: string, kind: string, paths: string[],
): Promise<number> {
  await page.goto('/media/sources/local/add');
  await hydrated(page);
  await page.locator('#local-library-name').fill(name);
  await page.locator('#local-library-kind').selectOption(kind);
  await page.locator('#local-library-paths').fill(paths.join('\n'));
  await page.getByRole('button', { name: 'Add', exact: true }).click();
  await expect(page).toHaveURL(/\/media\/sources\/local$/);
  expect(sqlite(`SELECT "MediaKind" FROM "Library" WHERE "Name"=${quote(name)};`)).toBe(kind);
  const id = sqlite(`SELECT "Id" FROM "Library" WHERE "Name"=${quote(name)};`);
  expect(id).toMatch(/^\d+$/);
  return Number(id);
}

export type ScanCounts = { folders: number; nestedFolders: number; items: number; files: number };

export function scanCounts(libraryId: number): ScanCounts {
  const [folders, nestedFolders, items, files] = sqlite(`
    SELECT
      (SELECT COUNT(*) FROM "LibraryFolder" f JOIN "LibraryPath" p ON p."Id" = f."LibraryPathId"
        WHERE p."LibraryId" = ${libraryId}),
      (SELECT COUNT(*) FROM "LibraryFolder" f JOIN "LibraryPath" p ON p."Id" = f."LibraryPathId"
        WHERE p."LibraryId" = ${libraryId} AND f."ParentId" IS NOT NULL),
      (SELECT COUNT(*) FROM "MediaItem" i JOIN "LibraryPath" p ON p."Id" = i."LibraryPathId"
        WHERE p."LibraryId" = ${libraryId}),
      (SELECT COUNT(*) FROM "MediaFile" m JOIN "LibraryFolder" f ON f."Id" = m."LibraryFolderId"
        JOIN "LibraryPath" p ON p."Id" = f."LibraryPathId" WHERE p."LibraryId" = ${libraryId});
  `).split('|').map(Number);
  return { folders, nestedFolders, items, files };
}

/**
 * Starts a real scan from the Libraries page and waits until the scanner has
 * recorded the library's scan time and written every expected media file.
 */
export async function scanLibrary(page: Page, libraryId: number, name: string, expectedFiles: number) {
  await page.goto('/media/libraries');
  await hydrated(page);
  const row = page.getByRole('row').filter({ hasText: name });
  await row.getByRole('button', { name: 'Scan Library', exact: true }).click();
  await expect.poll(() => {
    const scanned = sqlite(`SELECT "LastScan" IS NOT NULL FROM "Library" WHERE "Id"=${libraryId};`);
    return scanned === '1' ? scanCounts(libraryId).files : -1;
  }, { timeout: 90_000, message: `scan of library ${libraryId} did not finish` }).toBe(expectedFiles);
}
