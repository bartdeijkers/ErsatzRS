import { expect, test } from '@playwright/test';
import { execFileSync } from 'child_process';
import { existsSync, readFileSync } from 'fs';
import { get as httpGet } from 'http';
import { addonId, sourceUrl, hydrated, installPreparedFixture, preparedSqlite } from './addon-prepared-fixture';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;
function sqlite(sql: string): string {
  if (!dbPath) throw new Error('The isolated E2E database is required');
  return preparedSqlite(dbPath, sql);
}

function streamBytes(url: string): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    let length = 0;
    const request = httpGet(url, response => {
      if (response.statusCode !== 200) {
        response.resume();
        request.destroy(new Error(`Stream HTTP ${response.statusCode}`));
        return;
      }
      response.on('data', (chunk: Buffer) => {
        chunks.push(chunk);
        length += chunk.length;
        if (length >= 188 * 100) {
          clearTimeout(timer);
          resolve(Buffer.concat(chunks));
          response.destroy();
          request.destroy();
        }
      });
      response.on('end', () => {
        if (length < 188 * 100) {
          clearTimeout(timer);
          reject(new Error('Stream ended before sufficient TS packets'));
        }
      });
      response.on('error', error => { clearTimeout(timer); reject(error); });
    });
    const timer = setTimeout(() => request.destroy(new Error('Stream timeout')), 60000);
    request.on('error', error => { clearTimeout(timer); reject(error); });
  });
}

test('prepared publication pins its generated file and plays it instead of raw provider output', async ({ page, baseURL }, info) => {
  test.setTimeout(300000);
  test.skip(!dbPath, 'Requires the wrapper-owned isolated E2E database');
  test.skip(info.project.name !== 'chromium', 'Shared database acceptance runs once');
  if (!baseURL) throw new Error('Playwright baseURL is required');
  const errors: string[] = [];
  page.on('pageerror', error => { if (error.name !== 'AbortError') errors.push(error.message); });
  page.on('console', message => {
    if (message.type() === 'error' && /hydration|failed_to_cast_marker_node|rust_panic/i.test(message.text())) errors.push(message.text());
  });
  const fixture = installPreparedFixture(dbPath!);
  await page.request.post('/api/settings/ui/save', { form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0 });
  await hydrated(page, '/media/addon-lists/add');
  await page.getByLabel('Add-on list URL').fill(sourceUrl);
  await page.getByLabel('Add-on', { exact: true }).selectOption(addonId);
  await page.getByLabel('Delivery', { exact: true }).selectOption('download_ahead');
  await page.getByLabel('Collection Type', { exact: true }).selectOption('remote_streams');
  await page.getByLabel('Storage folder').fill('prepared-playback');
  await page.getByRole('combobox', { name: 'Automatic Refresh' }).selectOption('never');
  await page.getByRole('checkbox', { name: 'Create playlist in original order' }).check();
  await page.getByRole('checkbox', { name: 'Load item details automatically' }).uncheck();
  await page.getByRole('button', { name: 'Import overview' }).click();
  await expect(page).toHaveURL(/\/media\/addon-lists\/\d+\?import=1$/);
  const listId = Number(page.url().match(/addon-lists\/(\d+)/)?.[1]);
  expect(listId).toBeGreaterThan(0);
  await expect.poll(() => sqlite(`SELECT "State"||'|'||"DiscoveredCount" FROM "AddonMediaListImportJob" WHERE "AddonMediaListId"=${listId} ORDER BY "Generation" DESC LIMIT 1;`), { timeout: 60000 }).toBe('3|1');
  await hydrated(page, `/media/addon-lists/${listId}`);
  const option = page.locator('form[data-option-key="processed"]');
  await option.getByRole('checkbox').check();
  const saved = page.waitForResponse(response => response.url().includes('/api/addon-lists/options/change') && response.request().method() === 'POST');
  await option.getByRole('button', { name: 'Save Changes', exact: true }).click();
  expect((await saved).ok()).toBe(true);
  await hydrated(page, `/media/addon-lists/${listId}`);
  await expect(page.getByRole('button', { name: 'Prepare publication', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: 'Prepare publication', exact: true }).click();
  await expect.poll(() => sqlite(`SELECT COUNT(*) FROM "AddonMediaListItemOperationJob" WHERE "AddonMediaListId"=${listId} AND "Kind"=1 AND "State"=2;`), { timeout: 90000 }).toBe('1');
  // Do not reload: this proves the live preparation poll changes the CTA back to Publish.
  const publish = page.getByRole('button', { name: 'Publish list', exact: true });
  await expect(publish).toBeVisible({ timeout: 20000 });
  await publish.click();
  // PublishedRevision advances before verification; wait for retained completion.
  await expect.poll(() => sqlite(`SELECT COUNT(*) FROM "AddonMediaListRevision" r
JOIN "AddonMediaList" list ON list."Id"=r."AddonMediaListId" AND list."PublishedRevision"=r."Revision"
WHERE list."Id"=${listId} AND r."Revision"=1 AND r."CompletedAt" IS NOT NULL;`), { timeout: 90000 }).toBe('1');
  const pinned = `FROM "AddonMediaListRevisionItem" ri
JOIN "AddonMediaListRevision" r ON r."Id"=ri."AddonMediaListRevisionId"
JOIN "AddonMediaListArtifact" a ON a."Id"=ri."PreparedArtifactId"
JOIN "AddonMediaListRevisionArtifact" pin ON pin."AddonMediaListRevisionId"=r."Id" AND pin."AddonMediaListArtifactId"=a."Id"
WHERE r."AddonMediaListId"=${listId} AND r."CompletedAt" IS NOT NULL
AND r."Revision"=(SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id"=${listId})`;
  expect(sqlite(`SELECT COUNT(*) ${pinned} AND a."State"=2 AND a."DurationMilliseconds" BETWEEN 59000 AND 61000;`)).toBe('1');
  const playlistId = Number(sqlite(`SELECT "PlaylistId" FROM "AddonMediaList" WHERE "Id"=${listId};`));
  expect(playlistId).toBeGreaterThan(0);
  await expect.poll(() => sqlite(`SELECT COUNT(*) FROM "PlaylistItem" WHERE "PlaylistId"=${playlistId};`), { timeout: 60000 }).toBe('1');
  expect(sqlite(`SELECT "Duration" FROM "RemoteStream" WHERE "Id"=(SELECT "MediaItemId" FROM "AddonMediaListItem" WHERE "AddonMediaListId"=${listId});`)).toMatch(/^00:01:00(?:\.0+)?$/);

  // A saved delivery change must leave the published file pinned and playable.
  await hydrated(page, `/media/addon-lists/${listId}`);
  await page.getByLabel('Delivery', { exact: true }).selectOption('stream');
  await Promise.all([
    page.waitForEvent('framenavigated', frame => frame === page.mainFrame()),
    page.locator('button[form="addon_media_lists-list-editor-form"]:visible').filter({ hasText: 'Save Changes' }).click(),
  ]);
  await expect.poll(() => sqlite(`SELECT "DeliveryMode" || '|' || "PublishedRevision" FROM "AddonMediaList" WHERE "Id"=${listId};`)).toBe('0|1');
  expect(sqlite(`SELECT COUNT(*) ${pinned} AND a."State"=2;`)).toBe('1');

  // Scheduling UI is covered separately. Seed only one current entry referencing
  // the scanner-produced media; all preparation/publication writes above are real host actions.
  const channelNumber = String(800000 + listId);
  const profileId = Number(sqlite(`
INSERT INTO "FFmpegProfile" (
 "Name","ThreadCount","TranscodeAudio","TranscodeVideo","HardwareAcceleration",
 "VaapiDisplay","VaapiDriver","VaapiDevice","QsvExtraHardwareFrames","ResolutionId",
 "ScalingBehavior","PadMode","VideoFormat","VideoProfile","VideoPreset","AllowBFrames",
 "BitDepth","VideoBitrate","VideoBufferSize","TonemapAlgorithm","AudioFormat",
 "AudioBitrate","AudioBufferSize","NormalizeLoudnessMode","TargetLoudness","AudioChannels",
 "AudioSampleRate","NormalizeFramerate","NormalizeColors","DeinterlaceVideo")
SELECT 'E2E prepared playback software',1,1,1,0,
 "VaapiDisplay","VaapiDriver","VaapiDevice","QsvExtraHardwareFrames","ResolutionId",
 "ScalingBehavior","PadMode",1,'high','veryfast',0,
 0,1000,2000,"TonemapAlgorithm","AudioFormat",
 128,256,"NormalizeLoudnessMode","TargetLoudness",2,
 "AudioSampleRate","NormalizeFramerate","NormalizeColors","DeinterlaceVideo"
FROM "FFmpegProfile" ORDER BY "Id" LIMIT 1;
SELECT last_insert_rowid();
`));
  expect(profileId).toBeGreaterThan(0);
  sqlite(`
INSERT INTO "Channel" ("UniqueId","Number","SortNumber","Name","Group","FFmpegProfileId",
 "StreamingMode","StreamSelectorMode","PlayoutSource","PlayoutMode","TranscodeMode","IdleBehavior",
 "MusicVideoCreditsMode","SongVideoMode","SubtitleMode","IsEnabled","ShowInEpg")
VALUES ('80000000-0000-0000-0000-${listId.toString().padStart(12, '0')}','${channelNumber}',${Number(channelNumber)},
 'prepared playback','E2E',${profileId},5,0,0,0,0,0,0,0,0,1,1);
INSERT INTO "Playout" ("ChannelId","ScheduleKind","Seed") VALUES (last_insert_rowid(),20,1);
INSERT INTO "PlayoutItem" ("DisableWatermarks","FillerKind","Finish","GuideGroup","InPoint",
 "MediaItemId","OutPoint","PlayoutId","Start")
VALUES (1,0,strftime('%Y-%m-%d %H:%M:%f','now','+55 seconds'),0,'00:00:00',
 (SELECT "MediaItemId" FROM "AddonMediaListItem" WHERE "AddonMediaListId"=${listId}),
 '00:01:00',last_insert_rowid(),strftime('%Y-%m-%d %H:%M:%f','now','-5 seconds'));
`);
  const bytes = await streamBytes(new URL(`/iptv/channel/${channelNumber}.ts?mode=ts`, baseURL).toString());
  expect(bytes.length).toBeGreaterThanOrEqual(18800);
  expect(bytes[0]).toBe(0x47);
  expect(bytes[188]).toBe(0x47);
  // Decode actual output, so an error/slate stream cannot masquerade as successful playback.
  const ffmpeg = process.env.ErsatzRS_FFMPEG__PATH ?? process.env.ERSATZRS_FFMPEG__PATH ?? process.env.FFMPEG_PATH ?? 'ffmpeg';
  const pixel = execFileSync(ffmpeg, ['-hide_banner', '-loglevel', 'error', '-i', 'pipe:0',
    '-frames:v', '1', '-vf', 'scale=1:1', '-f', 'rawvideo', '-pix_fmt', 'rgb24', 'pipe:1'],
  { input: bytes, timeout: 15000 });
  expect(pixel.length).toBe(3);
  expect(pixel[0]).toBeGreaterThan(180);
  expect(pixel[1]).toBeLessThan(80);
  expect(pixel[2]).toBeLessThan(80);
  expect(existsSync(fixture.invocationLog)).toBe(true);
  const calls = readFileSync(fixture.invocationLog, 'utf8').trim().split(/\r?\n/).map(line => JSON.parse(line).command);
  expect(calls).toContain('prepare');
  expect(calls).not.toContain('play');
  expect(errors).toEqual([]);
});
