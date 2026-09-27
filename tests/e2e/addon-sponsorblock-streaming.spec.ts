// Manual AMM-M-01A host probe. Opt in with E2E_SPONSORBLOCK_ADDON_CHECKOUT.
// Browser frame cadence and captured packets do not prove decoded content or audible continuity.
import { expect, test, type Page } from '@playwright/test';
import { execFileSync } from 'child_process';
import { randomUUID } from 'crypto';
import { cpSync, mkdirSync, writeFileSync } from 'fs';
import { join, resolve } from 'path';
import { hydrated, preparedSqlite } from './addon-prepared-fixture';
import { captureTs } from './sponsorblock-ts-capture';
import { captureHls } from './hls-playback-capture';
import { beginHostLogWindow, type HostLogEvidence } from './sponsorblock-host-log';

const checkout = process.env.E2E_SPONSORBLOCK_ADDON_CHECKOUT;
const dbPath = process.env.E2E_DB_PATH;
const stateDirectory = process.env.E2E_APP_STATE_DIR;
const addonId = 'org.ersatzrs.addon.yt-dlp';
const sourceUrl = 'https://www.youtube.com/watch?v=nealYSA39fA';
const startupLimit = 20_000;
const quote = (value: string): string => `'${value.replace(/'/g, "''")}'`;
const sqlite = (sql: string): string => preparedSqlite(dbPath!, sql);
type Interval = { start: number; end: number };
type Plan = {
  version: number;
  source_duration_milliseconds: number;
  source_window_milliseconds: Interval;
  excluded_intervals_milliseconds: Interval[];
};

async function confirm(page: Page, id: string): Promise<void> {
  const dialog = page.locator(`#${id}`);
  await expect(dialog).toBeVisible();
  await dialog.getByRole('button', { name: 'Confirm', exact: true }).click();
}

async function installCheckout(page: Page): Promise<void> {
  const root = join(stateDirectory!, 'addons', 'checkouts', `sponsorblock-${randomUUID()}`);
  mkdirSync(join(root, 'addons'), { recursive: true });
  // Copy public package content, never the checkout's .git or operator profiles.
  cpSync(join(resolve(checkout!), 'repository.toml'), join(root, 'repository.toml'));
  cpSync(join(resolve(checkout!), 'addons', addonId), join(root, 'addons', addonId), { recursive: true });
  await hydrated(page, '/settings/addons?view=sources');
  await page.getByRole('button', { name: 'Enable', exact: true }).click();
  await confirm(page, 'addon-unknown-sources-dialog');
  await expect.poll(() => sqlite(`SELECT "Value" FROM "ConfigElement" WHERE "Key"='addons.allow_unverified_sources';`)).toBe('true');
  await page.getByRole('button', { name: 'Refresh', exact: true }).click();
  await expect.poll(() => sqlite(`SELECT COUNT(*) FROM "AddonSourceOffer" WHERE "AddonId"=${quote(addonId)};`), { timeout: 30_000 }).toBe('1');
  const sourceId = sqlite(`SELECT "SourceId" FROM "AddonSourceOffer" WHERE "AddonId"=${quote(addonId)};`);
  await hydrated(page, `/settings/addons/${addonId}?source=${encodeURIComponent(sourceId)}`);
  await page.getByRole('button', { name: 'Install', exact: true }).click();
  await Promise.all([
    page.waitForEvent('framenavigated', frame => frame === page.mainFrame()),
    confirm(page, 'addon-install-dialog'),
  ]);
  await hydrated(page);
  await expect.poll(() => sqlite(`SELECT "TrustTier" FROM "AddonInstallation" WHERE "AddonId"=${quote(addonId)};`), { timeout: 30_000 }).toBe('local');
  await page.getByRole('button', { name: 'Enable', exact: true }).click();
  await Promise.all([
    page.waitForEvent('framenavigated', frame => frame === page.mainFrame()),
    confirm(page, 'addon-toggle-dialog'),
  ]);
  await hydrated(page);
  await expect.poll(() => sqlite(`SELECT "Enabled" FROM "AddonInstallation" WHERE "AddonId"=${quote(addonId)};`), { timeout: 30_000 }).toBe('1');
  expect(sqlite(`SELECT COUNT(*) FROM "AddonMediaSource" a JOIN "LocalMediaSource" l ON l."Id"=a."MediaSourceId" WHERE a."AddonId"=${quote(addonId)};`)).toBe('1');
}

function programmePosition(plan: Plan, sourceMilliseconds: number): number {
  const window = plan.source_window_milliseconds;
  expect(sourceMilliseconds).toBeGreaterThanOrEqual(window.start);
  expect(sourceMilliseconds).toBeLessThanOrEqual(window.end);
  return sourceMilliseconds - window.start - plan.excluded_intervals_milliseconds.reduce(
    (sum, range) => sum + Math.max(0, Math.min(range.end, sourceMilliseconds) - range.start), 0,
  );
}

function timeSpan(milliseconds: number): string {
  const hours = Math.floor(milliseconds / 3_600_000);
  const minutes = Math.floor(milliseconds / 60_000) % 60;
  const seconds = Math.floor(milliseconds / 1000) % 60;
  return `${String(hours).padStart(2, '0')}:${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}.${String(milliseconds % 1000).padStart(3, '0')}`;
}

function seedChannel(mediaItemId: number, joinMilliseconds: number, durationMilliseconds: number) {
  // Only schedule/profile rows are seeded. Media and publication come from the host.
  // Use AAC (1), not the machine's seeded audio default, for Chromium playback.
  const resolutionId = Number(sqlite('SELECT "Id" FROM "Resolution" WHERE "Width"=1280 AND "Height"=720 ORDER BY "Id" LIMIT 1;'));
  expect(resolutionId, 'The bounded diagnostic uses an existing 1280x720 resolution').toBeGreaterThan(0);
  const profileId = Number(sqlite(`
INSERT INTO "FFmpegProfile" (
 "Name","ThreadCount","TranscodeAudio","TranscodeVideo","HardwareAcceleration",
 "VaapiDisplay","VaapiDriver","VaapiDevice","QsvExtraHardwareFrames","ResolutionId",
 "ScalingBehavior","PadMode","VideoFormat","VideoProfile","VideoPreset","AllowBFrames",
 "BitDepth","VideoBitrate","VideoBufferSize","TonemapAlgorithm","AudioFormat",
 "AudioBitrate","AudioBufferSize","NormalizeLoudnessMode","TargetLoudness","AudioChannels",
 "AudioSampleRate","NormalizeFramerate","NormalizeColors","DeinterlaceVideo")
SELECT 'Manual SponsorBlock software',1,1,1,0,
 "VaapiDisplay","VaapiDriver","VaapiDevice","QsvExtraHardwareFrames",${resolutionId},
 "ScalingBehavior","PadMode",1,'high','veryfast',0,
 0,2000,4000,"TonemapAlgorithm",1,
 128,256,"NormalizeLoudnessMode","TargetLoudness",2,
 "AudioSampleRate","NormalizeFramerate","NormalizeColors","DeinterlaceVideo"
FROM "FFmpegProfile" ORDER BY "Id" LIMIT 1;
SELECT last_insert_rowid();`));
  expect(profileId).toBeGreaterThan(0);
  const number = sqlite('SELECT MAX(870000, COALESCE(MAX(CAST("Number" AS INTEGER)),0))+1 FROM "Channel";');
  const remaining = (durationMilliseconds - joinMilliseconds) / 1000;
  sqlite(`
.bail on
BEGIN IMMEDIATE;
INSERT INTO "Channel" ("UniqueId","Number","SortNumber","Name","Group","FFmpegProfileId",
 "StreamingMode","StreamSelectorMode","PlayoutSource","PlayoutMode","TranscodeMode","IdleBehavior",
 "MusicVideoCreditsMode","SongVideoMode","SubtitleMode","IsEnabled","ShowInEpg")
VALUES (${quote(randomUUID())},'${number}',${Number(number)},'Manual SponsorBlock','E2E',${profileId},5,0,0,0,0,0,0,0,0,1,1);
INSERT INTO "Playout" ("ChannelId","ScheduleKind","Seed") VALUES (last_insert_rowid(),20,1);
INSERT INTO "PlayoutItem" ("DisableWatermarks","FillerKind","Finish","GuideGroup","InPoint",
 "MediaItemId","OutPoint","PlayoutId","Start")
VALUES (1,0,strftime('%Y-%m-%d %H:%M:%f','now','+${remaining} seconds'),0,${quote(timeSpan(joinMilliseconds))},
 ${mediaItemId},${quote(timeSpan(durationMilliseconds))},last_insert_rowid(),strftime('%Y-%m-%d %H:%M:%f','now'));
COMMIT;
`);
  const profile = JSON.parse(sqlite(`SELECT json_object('id',p."Id",'threads',p."ThreadCount",'hardwareAcceleration',p."HardwareAcceleration",'width',r."Width",'height',r."Height",'videoFormat',p."VideoFormat",'videoBitrate',p."VideoBitrate",'audioFormat',p."AudioFormat",'audioChannels',p."AudioChannels",'audioSampleRate',p."AudioSampleRate") FROM "FFmpegProfile" p JOIN "Resolution" r ON r."Id"=p."ResolutionId" WHERE p."Id"=${profileId};`));
  const scheduledStartMilliseconds = Date.parse(sqlite(`SELECT strftime('%Y-%m-%dT%H:%M:%fZ',i."Start") FROM "PlayoutItem" i JOIN "Playout" p ON p."Id"=i."PlayoutId" JOIN "Channel" c ON c."Id"=p."ChannelId" WHERE c."Number"='${number}';`));
  expect(Number.isFinite(scheduledStartMilliseconds)).toBe(true);
  return { number, profile, scheduledStartMilliseconds };
}



function packetSpans(path: string): { video: number; audio: number } {
  const probe = process.env.ErsatzRS_FFPROBE__PATH ?? process.env.ERSATZRS_FFPROBE__PATH ?? 'ffprobe';
  const result = JSON.parse(execFileSync(probe, ['-v', 'error', '-show_streams', '-show_packets', '-show_entries',
    'stream=index,codec_type:packet=stream_index,pts_time', '-of', 'json', path], { encoding: 'utf8', timeout: 15_000, maxBuffer: 16 * 1024 * 1024 }));
  const span = (kind: string): number => {
    const index = result.streams.find((stream: { codec_type: string }) => stream.codec_type === kind)?.index;
    const timestamps = result.packets.filter((packet: { stream_index: number }) => packet.stream_index === index)
      .map((packet: { pts_time?: string }) => Number(packet.pts_time)).filter(Number.isFinite);
    expect(timestamps.length).toBeGreaterThan(1);
    return Math.max(...timestamps) - Math.min(...timestamps);
  };
  return { video: span('video'), audio: span('audio') };
}

test('manual real SponsorBlock publication streams across its frozen late cut', async ({ page, request, baseURL }, info) => {
  test.skip(!checkout, 'Opt-in real-provider acceptance; never runs in default CI');
  test.skip(info.project.name !== 'chromium', 'One provider run against the isolated shared database');
  test.setTimeout(360_000);
  page.setDefaultTimeout(15_000);
  if (!dbPath || !stateDirectory || !baseURL) throw new Error('Use the isolated Playwright wrapper');
  const hostLog = process.env.E2E_SPONSORBLOCK_HOST_LOG;
  if (!hostLog) throw new Error('E2E_SPONSORBLOCK_HOST_LOG is required for the manual host gate');
  const errors: string[] = [];
  page.on('pageerror', error => { if (error.name !== 'AbortError') errors.push(error.name); });
  // Resume only an explicitly selected completed publication in the isolated
  // probe database; do not repeat provider discovery to diagnose a transport.
  const publishedListId = process.env.E2E_SPONSORBLOCK_PUBLISHED_LIST_ID;
  let listId: number;
  if (publishedListId) {
    listId = Number(publishedListId);
    expect(Number.isSafeInteger(listId) && listId > 0).toBe(true);
  } else {
  await request.post('/admin-api/settings/ui/save', { form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0 });
  await installCheckout(page);
  await hydrated(page, '/media/addon-lists/add');
  await page.getByLabel('Add-on list URL').fill(sourceUrl);
  await page.getByLabel('Add-on', { exact: true }).selectOption(addonId);
  await page.getByLabel('Collection Type', { exact: true }).selectOption('remote_streams');
  await page.getByLabel('Storage folder').fill('sponsorblock-manual');
  await page.getByRole('combobox', { name: 'Automatic Refresh' }).selectOption('never');
  await page.getByRole('checkbox', { name: 'Create playlist in original order' }).check();
  await page.getByRole('checkbox', { name: 'Load item details automatically' }).uncheck();
  await page.getByRole('button', { name: 'Import overview' }).click();
  await expect(page).toHaveURL(/\/media\/addon-lists\/\d+\?import=1$/);
  listId = Number(page.url().match(/addon-lists\/(\d+)/)?.[1]);
  expect(listId).toBeGreaterThan(0);
  await expect.poll(() => sqlite(`SELECT "State"||'|'||"DiscoveredCount" FROM "AddonMediaListImportJob" WHERE "AddonMediaListId"=${listId} ORDER BY "Generation" DESC LIMIT 1;`), { timeout: 60_000 }).toBe('3|1');
  await hydrated(page, `/media/addon-lists/${listId}/playback`);
  await expect(page.locator('[data-option-key="cookies_from_browser"] select')).toHaveValue('');
  const cookieOption = page.locator('[data-option-key="cookies_from_browser"]');
  const noneLabel = await cookieOption.locator('select').evaluate((select: HTMLSelectElement) =>
    Array.from(select.options).find(option => option.value === JSON.stringify('none'))?.label ?? '');
  expect(noneLabel).not.toBe('');
  await expect(cookieOption.locator('.option-effective')).toContainText(noneLabel);
  await page.locator('[data-option-key="sponsorblock_remove"] select').selectOption('true');
  await Promise.all([
    page.waitForEvent('framenavigated', frame => frame === page.mainFrame()),
    page.getByRole('button', { name: 'Save settings', exact: true }).click(),
  ]);
  await hydrated(page);
  await expect(page.locator('#workspace-errors')).toHaveCount(0);
  await hydrated(page, `/media/addon-lists/${listId}`);
  await expect.poll(() => sqlite(`SELECT COUNT(*) FROM "AddonMediaListItemOperationJob" WHERE "AddonMediaListId"=${listId} AND "Kind"=2 AND "State"=2;`), { timeout: 90_000 }).toBe('1');
  }
  expect(sqlite(`SELECT "AddonId" FROM "AddonMediaList" WHERE "Id"=${listId};`)).toBe(addonId);
  expect(sqlite(`SELECT "SourceUrl" FROM "AddonMediaListItem" WHERE "AddonMediaListId"=${listId};`)).toBe(sourceUrl);
  await expect.poll(() => sqlite(`SELECT COUNT(*) FROM "AddonMediaListRevision" r JOIN "AddonMediaList" l ON l."Id"=r."AddonMediaListId" AND l."PublishedRevision"=r."Revision" WHERE l."Id"=${listId} AND r."CompletedAt" IS NOT NULL;`), { timeout: 90_000 }).toBe('1');
  const plan: Plan = JSON.parse(sqlite(`SELECT i."PlaybackTimelineJson" FROM "AddonMediaListRevisionItem" i JOIN "AddonMediaListRevision" r ON r."Id"=i."AddonMediaListRevisionId" WHERE r."AddonMediaListId"=${listId} AND r."CompletedAt" IS NOT NULL ORDER BY r."Revision" DESC LIMIT 1;`));
  expect(plan.version).toBe(1);
  expect(plan.excluded_intervals_milliseconds.length).toBeGreaterThan(0);
  expect(sqlite(`SELECT COUNT(*) FROM "AddonMediaListItemOperationJob" WHERE "AddonMediaListId"=${listId} AND "Kind"=1;`)).toBe('0');
  expect(sqlite(`SELECT COUNT(*) FROM "AddonMediaListRevisionItem" i JOIN "AddonMediaListRevision" r ON r."Id"=i."AddonMediaListRevisionId" WHERE r."AddonMediaListId"=${listId} AND i."PreparedArtifactId" IS NOT NULL;`)).toBe('0');
  const mediaItemId = Number(sqlite(`SELECT "MediaItemId" FROM "AddonMediaListItem" WHERE "AddonMediaListId"=${listId};`));
  expect(mediaItemId).toBeGreaterThan(0);
  const duration = programmePosition(plan, plan.source_window_milliseconds.end);
  const projectedDuration = sqlite(`SELECT "Duration" FROM "RemoteStream" WHERE "Id"=${mediaItemId};`).split(':').map(Number);
  expect(projectedDuration).toHaveLength(3);
  expect(Math.round((projectedDuration[0] * 3600 + projectedDuration[1] * 60 + projectedDuration[2]) * 1000)).toBe(duration);
  // Keep the cut observable even when source selection takes the full budget.
  const sourceJoin = 944_000 - (startupLimit + 5_000);
  const joinPosition = programmePosition(plan, sourceJoin);
  const cut = plan.excluded_intervals_milliseconds.find(range => range.start > sourceJoin);
  expect(cut, 'Observed publication must retain the intended late-cut scenario').toBeTruthy();
  expect(cut!.start).toBeLessThanOrEqual(944_000);
  expect(cut!.end).toBeGreaterThanOrEqual(1_033_000);
  const cutOffsetSeconds = (programmePosition(plan, cut!.start) - joinPosition) / 1000;
  expect(cutOffsetSeconds).toBeGreaterThan(startupLimit / 1000);
  expect(cutOffsetSeconds).toBeLessThan(startupLimit / 1000 + 5);
  const evidence: Record<string, unknown> = { plan, sourceJoin, joinPosition, duration, cutOffsetSeconds,
    reusedCompletedPublication: Boolean(publishedListId),
    scope: 'Actual publication, browser HLS presented-frame startup and cadence, and captured production segments; decoded source-reference and audible continuity require separate validation' };
  const failedPaths: string[] = [];
  try {
    try { await test.step('Capture direct MPEG-TS across the frozen cut', async () => {
    const tsFile = info.outputPath('retained.ts');
    const tsChannel = seedChannel(mediaItemId, joinPosition, duration);
    evidence.tsProfile = tsChannel.profile;
    const finishLog = beginHostLogWindow(hostLog);
    let log: HostLogEvidence;
    let intentionalCutoffWallMilliseconds: number | undefined;
    try {
    const ts = await captureTs(new URL(`/iptv/channel/${tsChannel.number}.ts?mode=ts-legacy`, baseURL), tsFile);
    intentionalCutoffWallMilliseconds = ts.intentionalCutoffWallMilliseconds;
    const tsSpans = packetSpans(tsFile);
    evidence.ts = { ...ts, packetSpans: tsSpans };
    expect(ts.startupMilliseconds).toBeLessThanOrEqual(startupLimit);
    expect(ts.firstDecodedVideoMilliseconds).not.toBeNull();
    expect(ts.firstDecodedAudioMilliseconds).not.toBeNull();
    expect(ts.firstDecodedVideoMilliseconds!).toBeLessThanOrEqual(startupLimit);
    expect(ts.firstDecodedAudioMilliseconds!).toBeLessThanOrEqual(startupLimit);
    expect(Math.min(...ts.decodedSpans)).toBeGreaterThan(cutOffsetSeconds + 3);
    expect(Math.min(tsSpans.video, tsSpans.audio)).toBeGreaterThan(cutOffsetSeconds + 3);
    } finally {
      log = finishLog(intentionalCutoffWallMilliseconds);
      evidence.tsHostLog = log;
    }
    expect(log.error, 'TS host log observation must remain complete').toBeUndefined();
    expect(log.failures, 'TS host emitted an inner playback failure').toEqual([]);
    }); } catch (error) {
      failedPaths.push('TS');
      evidence.tsFailure = error instanceof Error ? error.message : 'Capture failed';
    }
    try { await test.step('Capture HLS across the same frozen cut', async () => {
    const hlsFile = info.outputPath('retained-hls.ts');
    const hlsChannel = seedChannel(mediaItemId, joinPosition, duration);
    evidence.hlsProfile = hlsChannel.profile;
    const finishLog = beginHostLogWindow(hostLog);
    let log: HostLogEvidence;
    try {
    const hls = await captureHls(page, baseURL, hlsChannel.number, hlsFile, cutOffsetSeconds, hlsChannel.scheduledStartMilliseconds);
    const hlsSpans = packetSpans(hlsFile);
    evidence.hls = { ...hls, packetSpans: hlsSpans };
    expect(hls.startupMilliseconds).toBeLessThanOrEqual(startupLimit);
    expect(Math.min(hlsSpans.video, hlsSpans.audio)).toBeGreaterThan(cutOffsetSeconds + 3);
    } finally {
      log = finishLog();
      evidence.hlsHostLog = log;
    }
    expect(log.error, 'HLS host log observation must remain complete').toBeUndefined();
    expect(log.failures, 'HLS host emitted an inner playback failure').toEqual([]);
    }); } catch (error) {
      failedPaths.push('HLS');
      evidence.hlsFailure = error instanceof Error ? error.message : 'Capture failed';
    }
    evidence.failedPaths = failedPaths;
    expect(failedPaths, 'Both serial transports must pass; inspect partial captures and step failures').toEqual([]);
    expect(errors).toEqual([]);
  } finally {
    writeFileSync(info.outputPath('sponsorblock-host-evidence.json'), JSON.stringify(evidence, null, 2));
  }
});
