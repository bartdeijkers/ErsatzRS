// Opt-in actual-host M02 viewing proof; synthetic content only, no provider network.
// Saved frame sheets require visual inspection for the exact words and cut timing.
import { expect, test, type Page } from '@playwright/test';
import { execFileSync } from 'child_process';
import { randomUUID } from 'crypto';
import { readFileSync, writeFileSync } from 'fs';
import { addonId, sourceUrl, hydrated, installPreparedFixture, preparedSqlite } from './addon-prepared-fixture';
import { captureTs } from './sponsorblock-ts-capture';
import { captureHls } from './hls-playback-capture';
import { beginHostLogWindow } from './sponsorblock-host-log';

const dbPath = process.env.E2E_DB_PATH;
const quote = (text: string) => `'${text.replace(/'/g, "''")}'`;
const sqlite = (sql: string) => preparedSqlite(dbPath!, `.bail on\n${sql}`);

async function publish(page: Page, delivery: 'stream' | 'download_ahead'): Promise<number> {
  await hydrated(page, '/media/addon-lists/add');
  await page.getByLabel('Add-on list URL').fill(`${sourceUrl}?delivery=${delivery}`);
  await page.getByLabel('Add-on', { exact: true }).selectOption(addonId);
  await page.getByLabel('Delivery', { exact: true }).selectOption(delivery);
  await page.getByLabel('Collection Type', { exact: true }).selectOption('remote_streams');
  await page.getByLabel('Storage folder').fill(`subtitle-${delivery}`);
  await page.getByRole('combobox', { name: 'Automatic Refresh' }).selectOption('never');
  await page.getByRole('checkbox', { name: 'Create playlist in original order' }).check();
  await page.getByRole('checkbox', { name: 'Load item details automatically' }).uncheck();
  await page.getByRole('button', { name: 'Import overview' }).click();
  await expect(page).toHaveURL(/\/media\/addon-lists\/\d+\?import=1$/);
  const id = Number(page.url().match(/addon-lists\/(\d+)/)?.[1]);
  expect(id).toBeGreaterThan(0);
  await expect.poll(() => sqlite(`SELECT "State" FROM "AddonMediaListImportJob" WHERE "AddonMediaListId"=${id} ORDER BY "Generation" DESC LIMIT 1;`), { timeout: 60000 }).toBe('3');
  await hydrated(page, `/media/addon-lists/${id}/playback`);
  await page.locator('[data-option-key="processed"] select').selectOption('true');
  await Promise.all([
    page.waitForEvent('framenavigated', frame => frame === page.mainFrame()),
    page.getByRole('button', { name: 'Save settings', exact: true }).click(),
  ]);
  await hydrated(page);
  await expect(page.locator('#workspace-errors')).toHaveCount(0);
  await hydrated(page, `/media/addon-lists/${id}`);
  // Streaming needs both interval metadata and captions. They share the provider
  // lease, so a queued caption job can outlive the first operation's completion.
  const completedKinds = delivery === 'stream' ? '2,3' : '1';
  await expect.poll(() => sqlite(`SELECT COUNT(*) FROM "AddonMediaListItemOperationJob" WHERE "AddonMediaListId"=${id} AND "Kind" IN (${completedKinds}) AND "State"=2;`), { timeout: 120000 }).toBe(delivery === 'stream' ? '2' : '1');
  await expect.poll(() => sqlite(`SELECT COUNT(*) FROM "AddonMediaListRevision" r JOIN "AddonMediaList" l
    ON l."Id"=r."AddonMediaListId" AND l."PublishedRevision"=r."Revision"
    WHERE l."Id"=${id} AND r."CompletedAt" IS NOT NULL;`), { timeout: 90000 }).toBe('1');
  return id;
}

function assertCaptionOwnership(listId: number, prepared: boolean): number {
  const rows = JSON.parse(sqlite(`SELECT json_group_array(json_object('id',a."Id",'clock',a."SubtitleTimeline",'language',a."SubtitleLanguage"))
    FROM "AddonMediaListRevisionItem" ri JOIN "AddonMediaListRevision" r ON r."Id"=ri."AddonMediaListRevisionId"
    JOIN json_each(ri."SubtitleArtifactIdsJson") ids
    JOIN "AddonMediaListArtifact" a ON a."Id"=ids.value
    JOIN "AddonMediaListRevisionArtifact" pin ON pin."AddonMediaListRevisionId"=r."Id" AND pin."AddonMediaListArtifactId"=a."Id"
    WHERE r."AddonMediaListId"=${listId} AND r."CompletedAt" IS NOT NULL AND a."Kind"=0 AND a."State"=2;`));
  expect(rows).toHaveLength(1);
  expect(rows[0].language).toBe('en');
  expect(rows[0].clock).toBe(prepared ? 1 : 0);
  const completion = JSON.parse(sqlite(`SELECT "OutcomeJson" FROM "AddonMediaListItemOperationJob"
    WHERE "AddonMediaListId"=${listId} AND "Kind"=${prepared ? 1 : 3} AND "State"=2 ORDER BY "Id" DESC LIMIT 1;`));
  expect(prepared ? completion.Prepared.subtitle_artifact_ids : completion.Subtitles.artifact_ids).toEqual([rows[0].id]);
  const itemId = Number(sqlite(`SELECT "MediaItemId" FROM "AddonMediaListItem" WHERE "AddonMediaListId"=${listId};`));
  expect(itemId).toBeGreaterThan(0);
  expect(sqlite(`SELECT COUNT(*) FROM "Subtitle" s JOIN "RemoteStreamMetadata" m ON m."Id"=s."RemoteStreamMetadataId"
    WHERE m."RemoteStreamId"=${itemId} AND s."SubtitleKind"=1 AND s."Language"='eng';`)).toBe('1');
  const path = sqlite(`SELECT s."Path" FROM "Subtitle" s JOIN "RemoteStreamMetadata" m ON m."Id"=s."RemoteStreamMetadataId"
    WHERE m."RemoteStreamId"=${itemId} AND s."SubtitleKind"=1 AND s."Language"='eng';`);
  const content = readFileSync(path,'utf8');
  // The immutable source artifact is still source-clock, but publication maps
  // its scanner-owned sidecar through the frozen plan before playback.
  expect(content.replace(/\r\n/g,'\n').trim()).toBe(
    '1\n00:00:08,000 --> 00:00:40,000\nMANUAL BEFORE\n\n2\n00:00:40,000 --> 00:01:20,000\nMANUAL AFTER',
  );
  return itemId;
}

function schedule(itemId: number, fallback: boolean): { number: string; started: number } {
  const number = sqlite('SELECT MAX(890000,COALESCE(MAX(CAST("Number" AS INTEGER)),0))+1 FROM "Channel";');
  const resolution = Number(sqlite('SELECT "Id" FROM "Resolution" WHERE "Width"=320 AND "Height"=180 LIMIT 1;') ||
    sqlite('INSERT INTO "Resolution" ("Width","Height") VALUES(320,180); SELECT last_insert_rowid();'));
  const profile = Number(sqlite(`INSERT INTO "FFmpegProfile" (
    "Name","ThreadCount","TranscodeAudio","TranscodeVideo","HardwareAcceleration",
    "VaapiDisplay","VaapiDriver","VaapiDevice","QsvExtraHardwareFrames","ResolutionId",
    "ScalingBehavior","PadMode","VideoFormat","VideoProfile","VideoPreset","AllowBFrames",
    "BitDepth","VideoBitrate","VideoBufferSize","TonemapAlgorithm","AudioFormat",
    "AudioBitrate","AudioBufferSize","NormalizeLoudnessMode","TargetLoudness","AudioChannels",
    "AudioSampleRate","NormalizeFramerate","NormalizeColors","DeinterlaceVideo")
    SELECT 'M02 subtitle software',1,1,1,0,"VaapiDisplay","VaapiDriver","VaapiDevice","QsvExtraHardwareFrames",${resolution},
    "ScalingBehavior","PadMode",1,'high','veryfast',0,0,1000,2000,"TonemapAlgorithm",1,
    128,256,"NormalizeLoudnessMode","TargetLoudness",2,"AudioSampleRate","NormalizeFramerate","NormalizeColors","DeinterlaceVideo"
    FROM "FFmpegProfile" ORDER BY "Id" LIMIT 1; SELECT last_insert_rowid();`));
  sqlite(`BEGIN IMMEDIATE;
    INSERT INTO "Channel" ("UniqueId","Number","SortNumber","Name","Group","FFmpegProfileId",
    "StreamingMode","StreamSelectorMode","PlayoutSource","PlayoutMode","TranscodeMode","IdleBehavior","PreferredSubtitleLanguageCode",
    "MusicVideoCreditsMode","SongVideoMode","SubtitleMode","IsEnabled","ShowInEpg")
    VALUES (${quote(randomUUID())},'${number}',${Number(number)},'M02 subtitles','E2E',${profile},5,0,0,0,0,0,${fallback ? "'nld'" : 'NULL'},0,0,3,1,1);
    INSERT INTO "Playout" ("ChannelId","ScheduleKind","Seed") VALUES(last_insert_rowid(),20,1);
    INSERT INTO "PlayoutItem" ("DisableWatermarks","FillerKind","Finish","GuideGroup","InPoint","MediaItemId","OutPoint","PlayoutId","Start")
    VALUES(1,0,strftime('%Y-%m-%d %H:%M:%f','now','+81 seconds'),0,'00:00:05',${itemId},'00:01:26',last_insert_rowid(),strftime('%Y-%m-%d %H:%M:%f','now'));
    COMMIT;`);
  const started = Date.parse(sqlite(`SELECT strftime('%Y-%m-%dT%H:%M:%fZ',i."Start") FROM "PlayoutItem" i JOIN "Playout" p ON p."Id"=i."PlayoutId" JOIN "Channel" c ON c."Id"=p."ChannelId" WHERE c."Number"='${number}';`));
  expect(Number.isFinite(started)).toBe(true);
  return { number, started };
}

function captionFrames(capture: string): { visibleSeconds: number[]; blueStartsAtSample: number; frameSheet: string } {
  const ffmpeg = process.env.ErsatzRS_FFMPEG__PATH ?? process.env.ERSATZRS_FFMPEG__PATH;
  if (!ffmpeg) throw new Error('Verified FFmpeg 9 path is required');
  const decoded = execFileSync(ffmpeg, ['-hide_banner','-loglevel','error','-threads','1','-i',capture,
    '-an','-vf','fps=1,scale=320:180','-t','50','-threads','1','-pix_fmt','rgb24','-f','rawvideo','pipe:1'],
  { timeout: 15000, maxBuffer: 10 * 1024 * 1024 });
  const size = 320 * 180 * 3;
  expect(decoded.length % size).toBe(0);
  const visibleSeconds: number[] = [];
  const phases: ('before' | 'after')[] = [];
  for (let frame = 0; frame < decoded.length / size; frame++) {
    let white = 0;
    for (let pixel = 320 * 110; pixel < 320 * 180; pixel++) {
      const offset = frame * size + pixel * 3;
      if (decoded[offset] > 180 && decoded[offset + 1] > 180 && decoded[offset + 2] > 180) white++;
    }
    if (white > 30) visibleSeconds.push(frame);
    const background = frame * size;
    const red = decoded[background] > 180 && decoded[background + 1] < 60 && decoded[background + 2] < 60;
    const blue = decoded[background] < 60 && decoded[background + 1] < 60 && decoded[background + 2] > 180;
    expect(red || blue, 'Decoded source must be retained red/blue, never excluded green or an error slate').toBe(true);
    phases.push(red ? 'before' : 'after');
  }
  expect(visibleSeconds.length, 'Decoded output must visibly render captions, not just contain rows').toBeGreaterThan(10);
  const blueStartsAtSample = phases.indexOf('after');
  expect(blueStartsAtSample, 'Capture must cover the retained cut after its nonzero join').toBeGreaterThan(0);
  expect(phases.slice(blueStartsAtSample).every(phase => phase === 'after')).toBe(true);
  expect(visibleSeconds.some(second => second < blueStartsAtSample)).toBe(true);
  expect(visibleSeconds.some(second => second >= blueStartsAtSample)).toBe(true);
  const frameSheet = `${capture}.frames.png`;
  execFileSync(ffmpeg, ['-hide_banner','-loglevel','error','-y','-threads','1','-i',capture,
    '-an','-vf','fps=1,scale=320:180,tile=8x7','-frames:v','1','-threads','1',frameSheet],
  { timeout: 15000, maxBuffer: 1024 * 1024 });
  return { visibleSeconds, blueStartsAtSample, frameSheet };
}

test('manual primary and fallback subtitles survive worker publication and scheduled TS/HLS cut playback', async ({ page, baseURL }, info) => {
  test.skip(process.env.E2E_SUBTITLE_PLAYBACK !== '1', 'Opt-in decoded media acceptance');
  test.skip(info.project.name !== 'chromium', 'One serial shared database acceptance');
  test.setTimeout(600000);
  const hostLog = process.env.E2E_SPONSORBLOCK_HOST_LOG;
  if (!dbPath || !baseURL || !hostLog) throw new Error('Isolated database, base URL and bounded host log are required');
  const fixture = installPreparedFixture(dbPath, false, true);
  // Both preferences request the same English artifact once. Fallback channels
  // override the primary to unavailable Dutch, so visible English proves fallback.
  sqlite(`INSERT INTO "ConfigElement" ("Key","Value") VALUES('ffmpeg.preferred_subtitle_language_code','eng'),('ffmpeg.fallback_subtitle_language_code','eng')
    ON CONFLICT("Key") DO UPDATE SET "Value"=excluded."Value";`);
  await page.request.post('/api/settings/ui/save', { form: { is_dark_mode:'true',language:'en-us' }, maxRedirects:0 });
  for (const delivery of ['stream','download_ahead'] as const) {
    const list = await publish(page, delivery);
    const item = assertCaptionOwnership(list, delivery === 'download_ahead');
    for (const selection of ['primary','fallback'] as const) {
      for (const transport of ['ts','hls'] as const) {
        // A stopped HLS player can leave its server session in the watch window.
        // Await real reservation release before starting the next short playout;
        // otherwise the capture measures a capacity slate instead of subtitles.
        await expect.poll(async () => {
          const response = await page.request.get('/settings/ffmpeg');
          expect(response.ok()).toBe(true);
          return /Streams in use: 0 \//.test(await response.text());
        }, { timeout: 90_000, intervals: [1_000] }).toBe(true);
        const channel = schedule(item, selection === 'fallback');
        const output = info.outputPath(`${delivery}-${selection}-${transport}.ts`);
        const finishLog = beginHostLogWindow(hostLog);
        let cutoff: number | undefined;
        let failure: unknown;
        try {
          if (transport === 'ts') {
            const result = await captureTs(new URL(`/iptv/channel/${channel.number}.ts?mode=ts-legacy`,baseURL),output);
            cutoff = result.intentionalCutoffWallMilliseconds;
          } else await captureHls(page,baseURL,channel.number,output,35,channel.started);
        } catch (error) { failure = error; }
        const log = finishLog(cutoff);
        writeFileSync(`${output}.host-log.json`,JSON.stringify(log,null,2));
        if (failure) throw failure;
        expect(log.error).toBeUndefined();
        expect(log.failures).toEqual([]);
        const frames = captionFrames(output);
        await info.attach(`${delivery}-${selection}-${transport}-caption-frames`,{path:frames.frameSheet,contentType:'image/png'});
        writeFileSync(`${output}.subtitle-proof.json`,JSON.stringify({delivery,selection,transport,...frames,
          expectedText:['MANUAL BEFORE','MANUAL AFTER'],forbiddenText:'REMOVED CUE',
          exactTextAndCutTiming:'Requires inspection of attached decoded frame sheet; pixel test proves visibility only'},null,2));
      }
    }
  }
  const calls = readFileSync(fixture.invocationLog,'utf8').trim().split('\n').map(line => JSON.parse(line));
  expect(calls.filter(call => call.command === 'subtitles')).toHaveLength(1);
  expect(calls.filter(call => call.command === 'prepare')).toHaveLength(1);
  const ranges = calls.filter(call => call.command === 'ranges');
  expect(ranges).toHaveLength(4);
  for (const call of ranges) {
    expect(call.ranges[0].start).toBeGreaterThanOrEqual(5000);
    expect(call.ranges[0].end).toBe(40000);
    expect(call.ranges[1].start).toBe(44000);
  }
  expect(calls.some(call => call.command === 'play')).toBe(false);
});
