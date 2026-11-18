import { expect, Locator, Page, test } from '@playwright/test';
import { DatabaseSync, SQLInputValue } from 'node:sqlite';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { waitForHydration } from './hydration';

const databasePath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;
const applicationDataPath = process.env.E2E_APP_DATA_DIR ?? process.env.ERSATZRS_APP_DATA_DIR;
// An independent expectation, never inferred from Health or worker describe.
// The application candidate pins the modern contract. An explicit false value
// retains the actual C5 compatibility/refusal control without inferring capability.
const modernRuntime = (process.env.ERSATZRS_E2E_NEXT_SESSION_CACHE ?? 'true') === 'true';

test.describe.configure({ mode: 'serial' });

async function refresh(page: Page, javaScriptEnabled: boolean, name: string): Promise<void> {
  const response = page.waitForResponse(value => value.request().method() === 'POST'
    && new URL(value.url()).pathname.startsWith(javaScriptEnabled
      ? '/admin-api/refresh_health_snapshot' : '/admin-api/system/health/refresh'));
  await page.getByRole('button', { name, exact: true }).click();
  expect((await response).status()).toBe(javaScriptEnabled ? 200 : 303);
  if (javaScriptEnabled) await waitForHydration(page);
}

function stored(database: DatabaseSync, title: string): { status: string; message: string } | undefined {
  const row = database.prepare('SELECT "Value" FROM "ConfigElement" WHERE "Key"=?')
    .get('health.results') as { Value: string };
  return JSON.parse(row.Value).find((value: { title: string }) => value.title === title);
}

async function visibleStatus(row: Locator, status: string): Promise<void> {
  await expect(row).toHaveCount(1);
  await expect(row).toBeVisible();
  await expect(row.locator('span[aria-label]').first()).toHaveAttribute('aria-label', status);
}

// HealthPage -> resolved SSR snapshot -> HealthIsland -> RefreshHealthSnapshot;
// no-JS uses the existing POST/303 form. No shared primitive or UI change.
// Backend rescan tests own actual creation-time repair; this proves its Health
// warning, persisted refresh and recovery after the already-tested write effect.
for (const javaScriptEnabled of [true, false]) {
  for (const dark of [false, true]) {
    test(`local date-added and Jellyfin index Health refresh and recovery (JavaScript ${javaScriptEnabled}, dark ${dark})`, async ({ browser, baseURL }, testInfo) => {
      test.skip(!databasePath, 'Requires the disposable E2E database.');
      test.setTimeout(120_000);
      const database = new DatabaseSync(databasePath!);
      database.exec('PRAGMA busy_timeout=10000; PRAGMA foreign_keys=ON;');
      const id = 7_296_000;
      const providerId = id + 1;
      const context = await browser.newContext({ baseURL, javaScriptEnabled, viewport: { width: 390, height: 844 } });
      const clean = () => {
        for (const owner of [id, providerId]) {
          database.prepare('DELETE FROM "MediaItem" WHERE "Id"=?').run(owner);
          database.prepare('DELETE FROM "LibraryPath" WHERE "Id"=?').run(owner);
          database.prepare('DELETE FROM "Library" WHERE "Id"=?').run(owner);
          database.prepare('DELETE FROM "MediaSource" WHERE "Id"=?').run(owner);
        }
      };
      try {
        clean();
        database.exec(`INSERT INTO "MediaSource" ("Id") VALUES (${id});
          INSERT INTO "LocalMediaSource" ("Id") VALUES (${id});
          INSERT INTO "Library" ("Id","MediaKind","MediaSourceId","Name") VALUES (${id},1,${id},'Synthetic date-added Health');
          INSERT INTO "LocalLibrary" ("Id") VALUES (${id});
          INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (${id},${id},'synthetic-date-added');
          INSERT INTO "MediaItem" ("Id","LibraryPathId") VALUES (${id},${id});
          INSERT INTO "Movie" ("Id") VALUES (${id});
          INSERT INTO "MovieMetadata" ("MovieId","MetadataKind","DateAdded","DateUpdated")
          VALUES (${id},3,'0001-01-01T00:00:00Z','2026-01-01');
          INSERT INTO "MediaSource" ("Id") VALUES (${providerId});
          INSERT INTO "JellyfinMediaSource" ("Id") VALUES (${providerId});
          INSERT INTO "Library" ("Id","MediaSourceId","Name","MediaKind") VALUES (${providerId},${providerId},'Synthetic disabled Jellyfin Health',1);
          INSERT INTO "JellyfinLibrary" ("Id","ShouldSyncItems") VALUES (${providerId},0);
          INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (${providerId},${providerId},'synthetic-index-health');
          INSERT INTO "MediaItem" ("Id","LibraryPathId") VALUES (${providerId},${providerId});
          INSERT INTO "Movie" ("Id") VALUES (${providerId});
          INSERT INTO "JellyfinMovie" ("Id") VALUES (${providerId});
          INSERT INTO "MediaVersion" ("Id","MovieId","DateAdded","DateUpdated","Duration","Height","Width","VideoScanKind")
          VALUES (${providerId},${providerId},'2026-01-01','2026-01-01','00:01:00',180,320,0);
          INSERT INTO "MediaStream" ("MediaVersionId","Index","MediaStreamKind","Channels","Default","Forced") VALUES
          (${providerId},0,1,0,0,0),(${providerId},1,2,2,0,0),(${providerId},2,5,0,0,0),(${providerId},3,3,0,0,0);`);
        const streams = () => database.prepare('SELECT * FROM "MediaStream" WHERE "MediaVersionId"=? ORDER BY "Id"').all(providerId);
        const originalStreams = streams();
        const page = await context.newPage();
        const errors: string[] = [];
        page.on('pageerror', error => errors.push(error.message));
        page.on('console', value => { if (value.type() === 'error') errors.push(value.text()); });
        for (const [language, warning, indexWarning, indexTitle, button, warningStatus, passStatus] of [
          ['en-us', '1 local media items have an unset date added.', '1 Jellyfin media versions need stream index repair.', 'Jellyfin stream indexes', 'Refresh', 'Warning', 'Pass'],
          ['nl', 'Bij 1 lokale media-items ontbreekt de toevoegdatum.', 'Bij 1 Jellyfin-mediaversies moeten de streamindexen worden hersteld.', 'Jellyfin-streamindexen', 'Vernieuwen', 'Waarschuwing', 'Geslaagd'],
        ]) {
          database.prepare('UPDATE "MovieMetadata" SET "DateAdded"=? WHERE "MovieId"=?').run('0001-01-01T00:00:00Z', id);
          database.prepare('UPDATE "MediaVersion" SET "ProviderStreamIndexVersion"=0 WHERE "Id"=?').run(providerId);
          database.prepare('DELETE FROM "ProviderStreamIndex" WHERE "MediaVersionId"=?').run(providerId);
          expect((await page.request.post('/admin-api/settings/ui/save', { form: { language, is_dark_mode: String(dark) }, maxRedirects: 0 })).status()).toBe(303);
          expect((await page.request.post('/admin-api/system/health/refresh', { maxRedirects: 0 })).status()).toBe(303);
          await page.goto('/system/health');
          if (javaScriptEnabled) await waitForHydration(page);
          await expect(page.locator('[data-not-found]')).toHaveCount(0);
          const row = page.getByRole('row').filter({ hasText: warning });
          await visibleStatus(row, warningStatus);
          expect(stored(database, 'Search Projection')?.status).toBe('Warning');
          const indexRow = page.getByRole('row').filter({ hasText: indexTitle });
          await visibleStatus(indexRow, warningStatus);
          await expect(indexRow).toContainText(indexWarning);
          expect(stored(database, 'Jellyfin stream indexes')?.status).toBe('Warning');
          expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 2)).toBe(true);
          await row.screenshot({ path: testInfo.outputPath(`date-added-${language}.png`) });
          await indexRow.screenshot({ path: testInfo.outputPath(`jellyfin-index-${language}.png`) });
          database.prepare('UPDATE "MovieMetadata" SET "DateAdded"=? WHERE "MovieId"=?').run('2026-01-01T00:00:00Z', id);
          database.prepare('UPDATE "MediaVersion" SET "ProviderStreamIndexVersion"=1 WHERE "Id"=?').run(providerId);
          for (const [logical, physical] of [[0, 0], [1, 1], [3, 2]]) {
            database.prepare('INSERT INTO "ProviderStreamIndex" ("MediaVersionId","StreamIndex","SourceStreamIndex") VALUES (?,?,?)').run(providerId, logical, physical);
          }
          await refresh(page, javaScriptEnabled, button);
          await expect(row).toHaveCount(0);
          expect(stored(database, 'Search Projection')?.status).toBe('Pass');
          const recovered = page.getByRole('row').filter({ hasText: 'Search Projection' });
          await visibleStatus(recovered, passStatus);
          await visibleStatus(indexRow, passStatus);
          await expect(indexRow).not.toContainText(indexWarning);
          expect(stored(database, 'Jellyfin stream indexes')?.status).toBe('Pass');
          expect(database.prepare('SELECT "ShouldSyncItems" FROM "JellyfinLibrary" WHERE "Id"=?').get(providerId)).toEqual({ ShouldSyncItems: 0 });
          expect(streams()).toEqual(originalStreams);
        }
        expect(errors).toEqual([]);
      } finally {
        clean();
        await context.request.post('/admin-api/settings/ui/save', { form: { language: 'en-us', is_dark_mode: 'true' }, maxRedirects: 0 });
        await context.request.post('/admin-api/system/health/refresh', { maxRedirects: 0 });
        database.close();
        await context.close();
      }
    });

    test(`Next runtime session-cache Health and channel recovery (JavaScript ${javaScriptEnabled}, dark ${dark})`, async ({ browser, baseURL }, testInfo) => {
      test.skip(!databasePath, 'Requires the disposable E2E database.');
      test.setTimeout(120_000);
      const database = new DatabaseSync(databasePath!);
      database.exec('PRAGMA busy_timeout=10000; PRAGMA foreign_keys=ON;');
      const id = 7_296_100;
      if (modernRuntime) expect(applicationDataPath, 'Requires the disposable application-data root.').toBeTruthy();
      const overlayFolder = applicationDataPath ? join(applicationDataPath, 'next', 'channel-config-overlays') : undefined;
      const overlay = overlayFolder ? join(overlayFolder, `${id}.json`) : undefined;
      const originalOverlay = overlay && existsSync(overlay) ? readFileSync(overlay) : undefined;
      let overlayTouched = false;
      const otherEnabled = database.prepare('SELECT "Id" FROM "Channel" WHERE "StreamingEngine"=1 AND "IsEnabled"=1 AND "Id"<>?')
        .all(id) as { Id: number }[];
      const context = await browser.newContext({ baseURL, javaScriptEnabled, viewport: { width: 390, height: 844 } });
      const clean = () => {
        database.prepare('DELETE FROM "Channel" WHERE "Id"=?').run(id);
        database.prepare('DELETE FROM "FFmpegProfile" WHERE "Id"=?').run(id);
      };
      try {
        clean();
        if (modernRuntime) {
          mkdirSync(overlayFolder!, { recursive: true });
          rmSync(overlay!, { force: true });
          overlayTouched = true;
        }
        // The host seeds another enabled Next channel. Isolate this proof's
        // channel, then restore those exact choices before leaving the fixture.
        for (const channel of otherEnabled) {
          database.prepare('UPDATE "Channel" SET "IsEnabled"=0 WHERE "Id"=?').run(channel.Id);
        }
        const profile = database.prepare('SELECT * FROM "FFmpegProfile" ORDER BY "Id" LIMIT 1').get() as Record<string, SQLInputValue>;
        Object.assign(profile, { Id: id, Name: 'Synthetic Next Health', VideoFormat: 1, AudioFormat: 1, VideoBitrate: 2000, AudioBitrate: 128 });
        const columns = Object.keys(profile);
        database.prepare(`INSERT INTO "FFmpegProfile" (${columns.map(value => `"${value}"`).join(',')}) VALUES (${columns.map(() => '?').join(',')})`).run(...Object.values(profile));
        database.prepare(`INSERT INTO "Channel" ("Id","Number","Name","FFmpegProfileId","IdleBehavior","MusicVideoCreditsMode","PlayoutMode","PlayoutSource","SongVideoMode","SortNumber","StreamSelectorMode","StreamingMode","SubtitleMode","TranscodeMode","UniqueId","StreamingEngine","IsEnabled")
          VALUES (?,?,?,?,0,0,0,0,0,?,0,0,0,0,?,1,1)`).run(id, String(id), 'Synthetic Next Health', id, id, String(id));
        const page = await context.newPage();
        const errors: string[] = [];
        page.on('pageerror', error => errors.push(error.message));
        page.on('console', value => { if (value.type() === 'error') errors.push(value.text()); });
        for (const [language, title, unavailable, button, failStatus, passStatus] of [
          ['en-us', 'Next runtime availability', 'Next runtime session controls could not be verified;', 'Refresh', 'Fail', 'Pass'],
          ['nl', 'Beschikbaarheid van Next-runtime', 'De sessie-instellingen van de Next-runtime konden niet worden geverifieerd.', 'Vernieuwen', 'Mislukt', 'Geslaagd'],
        ]) {
          database.prepare('UPDATE "Channel" SET "IsEnabled"=1 WHERE "Id"=?').run(id);
          expect((await page.request.post('/admin-api/settings/ui/save', { form: { language, is_dark_mode: String(dark) }, maxRedirects: 0 })).status()).toBe(303);
          expect((await page.request.post('/admin-api/system/health/refresh', { maxRedirects: 0 })).status()).toBe(303);
          await page.goto('/system/health');
          if (javaScriptEnabled) await waitForHydration(page);
          await expect(page.locator('[data-not-found]')).toHaveCount(0);
          const row = page.getByRole('row').filter({ hasText: title });
          expect(stored(database, 'Next Runtime Availability')?.status).toBe(modernRuntime ? 'Pass' : 'Fail');
          await visibleStatus(row, modernRuntime ? passStatus : failStatus);
          if (!modernRuntime) await expect(row).toContainText(unavailable);
          expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 2)).toBe(true);
          await row.screenshot({ path: testInfo.outputPath(`next-session-${language}.png`) });
          if (modernRuntime) {
            writeFileSync(overlay!, JSON.stringify({ version: 'https://ersatztv.org/channel/version/0.1.7' }));
            await refresh(page, javaScriptEnabled, button);
            await visibleStatus(row, failStatus);
            await expect(row).toContainText(language === 'nl' ? 'ondersteunde versie-URI' : 'supported version URI');
            await expect(row).toContainText('0.1.0–0.1.6');
            expect(stored(database, 'Next Runtime Availability')?.status).toBe('Fail');
            expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 2)).toBe(true);
            await row.screenshot({ path: testInfo.outputPath(`next-overlay-${language}.png`) });
            writeFileSync(overlay!, JSON.stringify({ version: 'https://ersatztv.org/channel/version/0.1.6' }));
            await refresh(page, javaScriptEnabled, button);
            await visibleStatus(row, passStatus);
            expect(stored(database, 'Next Runtime Availability')?.status).toBe('Pass');
          }
          // Disabling an unsupported channel is a real recovery action, not
          // evidence that the older executable gained modern capabilities.
          database.prepare('UPDATE "Channel" SET "IsEnabled"=0 WHERE "Id"=?').run(id);
          await refresh(page, javaScriptEnabled, button);
          await visibleStatus(row, passStatus);
          expect(stored(database, 'Next Runtime Availability')?.status).toBe('Pass');
          database.prepare('UPDATE "Channel" SET "IsEnabled"=1 WHERE "Id"=?').run(id);
          await refresh(page, javaScriptEnabled, button);
          await visibleStatus(row, modernRuntime ? passStatus : failStatus);
          expect(stored(database, 'Next Runtime Availability')?.status).toBe(modernRuntime ? 'Pass' : 'Fail');
          expect(database.prepare('SELECT "StreamingEngine","IsEnabled" FROM "Channel" WHERE "Id"=?').get(id))
            .toEqual({ StreamingEngine: 1, IsEnabled: 1 });
        }
        expect(errors).toEqual([]);
      } finally {
        clean();
        if (overlayTouched) {
          if (originalOverlay) writeFileSync(overlay!, originalOverlay);
          else rmSync(overlay!, { force: true });
        }
        for (const channel of otherEnabled) {
          database.prepare('UPDATE "Channel" SET "IsEnabled"=1 WHERE "Id"=?').run(channel.Id);
        }
        await context.request.post('/admin-api/settings/ui/save', { form: { language: 'en-us', is_dark_mode: 'true' }, maxRedirects: 0 });
        await context.request.post('/admin-api/system/health/refresh', { maxRedirects: 0 });
        database.close();
        await context.close();
      }
    });
  }
}
