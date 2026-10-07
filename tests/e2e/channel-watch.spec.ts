import { expect, Locator, Page, TestInfo, test } from '@playwright/test';
import { execFileSync, spawn } from 'node:child_process';
import { existsSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

function sqlite(sql: string): string {
  if (!dbPath) throw new Error('E2E_DB_PATH or ERSATZRS_DB_PATH is required');
  return execFileSync('sqlite3', [dbPath], {
    input: `.timeout 10000\n${sql}`,
    encoding: 'utf8',
  }).trim();
}

function dbTime(value: Date): string {
  return value.toISOString().replace('T', ' ').replace('Z', '');
}

function sqlString(value: string): string {
  return `'${value.replaceAll("'", "''")}'`;
}

async function setTimelineScroll(page: Page, scrollLeft: number): Promise<void> {
  await page.locator('[role="grid"]').evaluate(async (element, requested) => {
    const timeline = element as HTMLElement;
    const inlineScrollBehavior = timeline.style.scrollBehavior;
    timeline.style.scrollBehavior = 'auto';
    timeline.scrollLeft = requested;
    await new Promise<void>((resolve) => {
      requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
    });
    timeline.style.scrollBehavior = inlineScrollBehavior;
  }, scrollLeft);
}

type TimelineGeometry = {
  scrollLeft: number;
  clientWidth: number;
  scrollWidth: number;
  channelColumn: number;
};

async function timelineGeometry(page: Page): Promise<TimelineGeometry> {
  return page.locator('[role="grid"]').evaluate((element) => {
    const timeline = element as HTMLElement;
    // The sticky channel column overlays the left edge of the timeline, so the
    // visible programme viewport excludes it. Measure it from the DOM rather
    // than repeating the app's CHANNEL_COLUMN_PIXELS, so a CSS change cannot
    // silently desync this measurement from what the app scrolls by.
    const columnHeader = timeline.querySelector('[role="columnheader"]');
    return {
      scrollLeft: timeline.scrollLeft,
      clientWidth: timeline.clientWidth,
      scrollWidth: timeline.scrollWidth,
      channelColumn: columnHeader?.getBoundingClientRect().width ?? 0,
    };
  });
}

function centerMinutes(geometry: TimelineGeometry, zoomPercent: number): number {
  const viewport = Math.max(geometry.clientWidth - geometry.channelColumn, 0);
  const pixelsPerMinute = (6 * zoomPercent) / 100;
  return (geometry.scrollLeft + viewport / 2) / pixelsPerMinute;
}

async function timelineCenterMinutes(page: Page, zoomPercent: number): Promise<number> {
  return centerMinutes(await timelineGeometry(page), zoomPercent);
}

// The centre-preservation proof is only meaningful when the timeline is a real
// scroll container. If the layout blows out (the guide sized to its own content),
// scrollLeft is pinned at 0 and the computed centre scales exactly with zoom, so
// the comparison passes without testing anything - while the Now/±2h controls
// silently do nothing. Assert the precondition so that failure mode is loud.
async function expectScrollableTimeline(page: Page): Promise<void> {
  const geometry = await timelineGeometry(page);
  expect(geometry.channelColumn).toBeGreaterThan(0);
  expect(geometry.scrollWidth).toBeGreaterThan(geometry.clientWidth);
}

// Proves the viewport-stepping controls actually move the timeline.
async function expectStepScrollsTimeline(page: Page, button: string): Promise<void> {
  const before = (await timelineGeometry(page)).scrollLeft;
  await page.getByRole('button', { name: button, exact: true }).click();
  await expect
    .poll(async () => (await timelineGeometry(page)).scrollLeft)
    .toBeGreaterThan(before);
}

async function stopFixtureTranscode(page: Page, channelNumber: string): Promise<string> {
  const workingDirectory = join(tmpdir(), 'ersatzrs-hls-preview', channelNumber);
  await expect.poll(() => existsSync(workingDirectory)).toBe(true);
  const preview = page.locator('.channel-watch-js [data-channel-preview]');
  const stopPreview = preview.getByRole('button', { name: 'Stop Preview', exact: true });
  if (await stopPreview.isVisible()) await stopPreview.click();
  // Stop Preview releases the browser, while the server keeps its idle lease.
  // Explicitly stop this fixture through the real administration action so the
  // next browser's channel does not wait behind the previous fixture's lease.
  await page.getByRole('button', { name: 'Stop Transcode Session', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Stop Transcode Session', exact: true });
  await expect(dialog).toBeVisible();
  const stopped = page.waitForResponse((response) =>
    response.request().method() === 'POST'
    && new URL(response.url()).pathname.includes('/admin-api/stop_channel_session'));
  await dialog.getByRole('button', { name: 'Stop Transcode Session', exact: true }).click();
  const response = await stopped;
  expect(response.ok()).toBe(true);
  expect(await response.json()).toBe(true);
  // The response acknowledges cancellation. Directory removal follows process
  // shutdown and proves this fixture no longer occupies a transcode slot.
  await expect.poll(() => existsSync(workingDirectory)).toBe(false);
  await expect(dialog).not.toBeVisible();
  await expect(preview).toHaveAttribute('data-preview-state', 'stopped');
  await expect(preview.locator('video')).not.toHaveAttribute('src');
  return response.url();
}

function makeCalibrationClip(
  testInfo: TestInfo,
  videoSource = 'testsrc2=size=640x360:rate=30',
): string {
  const clipPath = testInfo.outputPath('channel-preview-calibration.mp4');
  const ffmpegPath =
    process.env.ErsatzRS_FFMPEG__PATH ??
    process.env.ERSATZRS_FFMPEG__PATH ??
    process.env.FFMPEG_PATH ??
    'ffmpeg';
  execFileSync(
    ffmpegPath,
    [
      '-hide_banner',
      '-loglevel',
      'error',
      '-y',
      '-f',
      'lavfi',
      '-i',
      videoSource,
      '-f',
      'lavfi',
      '-i',
      'sine=frequency=440:sample_rate=48000',
      '-t',
      '70',
      '-c:v',
      'libx264',
      '-pix_fmt',
      'yuv420p',
      '-preset',
      'ultrafast',
      '-c:a',
      'aac',
      '-b:a',
      '128k',
      clipPath,
    ],
    { stdio: 'inherit' },
  );
  return clipPath;
}

function seedChannel(
  testInfo: TestInfo,
  mediaPath?: string,
  ordinal = 0,
): { id: number; name: string; number: string } {
  const projectOffset = [...testInfo.project.name].reduce(
    (total, character) => total + character.charCodeAt(0),
    0,
  ) * 100;
  const id = 540_000 + projectOffset + ordinal * 1_000;
  const playoutId = id + 1;
  const mediaSourceId = id + 100;
  const libraryId = id + 101;
  const libraryPathId = id + 102;
  const mediaItemId = id + 103;
  const metadataId = id + 104;
  const mediaVersionId = id + 105;
  const mediaFileId = id + 106;
  const videoStreamId = id + 107;
  const audioStreamId = id + 108;
  const previewProfileId = id + 200;
  const number = `${8_000 + projectOffset / 100 + ordinal}`;
  const name = `E2E Preview ${testInfo.project.name} ${ordinal}`;
  const mediaTitle = `E2E Preview Programme ${testInfo.project.name} ${ordinal}`;
  const sourceProfileId = sqlite('SELECT "Id" FROM "FFmpegProfile" ORDER BY "Id" LIMIT 1;');
  if (!sourceProfileId) {
    throw new Error('E2E database needs an FFmpeg profile');
  }

  const now = new Date();
  const currentStart = new Date(now.getTime() - 2_000);
  const currentFinish = new Date(now.getTime() + 58_000);
  const nextFinish = new Date(now.getTime() + 118_000);
  const gapFinish = new Date(now.getTime() + 128_000);

  sqlite(`
DELETE FROM "PlayoutItem"
WHERE "PlayoutId" IN (SELECT "Id" FROM "Playout" WHERE "ChannelId" = ${id});
DELETE FROM "PlayoutGap"
WHERE "PlayoutId" IN (SELECT "Id" FROM "Playout" WHERE "ChannelId" = ${id});
DELETE FROM "Playout" WHERE "ChannelId" = ${id};
DELETE FROM "Artwork" WHERE "ChannelId" = ${id};
DELETE FROM "Channel" WHERE "Id" = ${id};
DELETE FROM "FFmpegProfile" WHERE "Id" = ${previewProfileId};
DELETE FROM "Subtitle" WHERE "MovieMetadataId" = ${metadataId};
DELETE FROM "MediaStream" WHERE "MediaVersionId" = ${mediaVersionId};
DELETE FROM "MediaFile" WHERE "MediaVersionId" = ${mediaVersionId};
DELETE FROM "MediaVersion" WHERE "Id" = ${mediaVersionId};
DELETE FROM "MovieMetadata" WHERE "MovieId" = ${mediaItemId};
DELETE FROM "Movie" WHERE "Id" = ${mediaItemId};
DELETE FROM "MediaItem" WHERE "Id" = ${mediaItemId};
DELETE FROM "LibraryPath" WHERE "Id" = ${libraryPathId};
DELETE FROM "LocalLibrary" WHERE "Id" = ${libraryId};
DELETE FROM "Library" WHERE "Id" = ${libraryId};
DELETE FROM "MediaSource" WHERE "Id" = ${mediaSourceId};

INSERT INTO "FFmpegProfile" (
  "Id", "Name", "ThreadCount", "TranscodeAudio", "TranscodeVideo",
  "HardwareAcceleration", "VaapiDisplay", "VaapiDriver", "VaapiDevice",
  "QsvExtraHardwareFrames", "ResolutionId", "ScalingBehavior", "PadMode",
  "VideoFormat", "VideoProfile", "VideoPreset", "AllowBFrames", "BitDepth",
  "VideoBitrate", "VideoBufferSize", "TonemapAlgorithm", "AudioFormat",
  "AudioBitrate", "AudioBufferSize", "NormalizeLoudnessMode", "TargetLoudness",
  "AudioChannels", "AudioSampleRate", "NormalizeFramerate", "NormalizeColors",
  "DeinterlaceVideo"
)
SELECT
  ${previewProfileId}, 'E2E browser preview AC-3', "ThreadCount", 1, 1,
  "HardwareAcceleration", "VaapiDisplay", "VaapiDriver", "VaapiDevice",
  "QsvExtraHardwareFrames", "ResolutionId", "ScalingBehavior", "PadMode",
  1, 'high', "VideoPreset", "AllowBFrames", 0,
  "VideoBitrate", "VideoBufferSize", "TonemapAlgorithm", 2,
  "AudioBitrate", "AudioBufferSize", "NormalizeLoudnessMode", "TargetLoudness",
  "AudioChannels", "AudioSampleRate", "NormalizeFramerate", "NormalizeColors",
  "DeinterlaceVideo"
FROM "FFmpegProfile" WHERE "Id" = ${sourceProfileId};

INSERT INTO "MediaSource" ("Id") VALUES (${mediaSourceId});
INSERT INTO "Library" ("Id", "MediaKind", "MediaSourceId", "Name")
VALUES (${libraryId}, 1, ${mediaSourceId}, 'E2E Channel Preview Library');
INSERT INTO "LocalLibrary" ("Id") VALUES (${libraryId});
INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId")
VALUES (${libraryPathId}, '/e2e/channel-preview', ${libraryId});
INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State")
VALUES (${mediaItemId}, ${libraryPathId}, 0);
INSERT INTO "Movie" ("Id") VALUES (${mediaItemId});
INSERT INTO "MovieMetadata" (
  "Id", "DateAdded", "DateUpdated", "MetadataKind", "MovieId", "Title"
) VALUES (
  ${metadataId}, '2026-07-26 00:00:00', '2026-07-26 00:00:00',
  0, ${mediaItemId}, '${mediaTitle}'
);
${mediaPath ? `
INSERT INTO "MediaVersion" (
  "Id", "DateAdded", "DateUpdated", "Duration", "Height", "MovieId",
  "Name", "RFrameRate", "VideoScanKind", "Width"
) VALUES (
  ${mediaVersionId}, '${dbTime(now)}', '${dbTime(now)}', '00:01:10', 360,
  ${mediaItemId}, 'Browser preview calibration', '30/1', 1, 640
);
INSERT INTO "MediaFile" ("Id", "MediaVersionId", "Path", "PathHash")
VALUES (${mediaFileId}, ${mediaVersionId}, ${sqlString(mediaPath)}, 'e2e-preview-${mediaFileId}');
INSERT INTO "MediaStream" (
  "Id", "Index", "Codec", "Profile", "MediaStreamKind", "Channels",
  "Default", "Forced", "MediaVersionId", "PixelFormat"
) VALUES (
  ${videoStreamId}, 0, 'h264', 'High', 1, 0, 1, 0, ${mediaVersionId}, 'yuv420p'
);
INSERT INTO "MediaStream" (
  "Id", "Index", "Codec", "MediaStreamKind", "Language", "Channels",
  "Default", "Forced", "MediaVersionId"
) VALUES (
  ${audioStreamId}, 1, 'aac', 2, 'eng', 2, 1, 0, ${mediaVersionId}
);
` : ''}

INSERT INTO "Channel" (
  "Id", "UniqueId", "Number", "SortNumber", "Name", "Group",
  "FFmpegProfileId", "StreamingMode", "StreamSelectorMode",
  "PlayoutSource", "PlayoutMode", "TranscodeMode", "IdleBehavior",
  "MusicVideoCreditsMode", "SongVideoMode", "SubtitleMode", "IsEnabled", "ShowInEpg"
) VALUES (
  ${id}, '00000000-0000-0000-0000-${id.toString().padStart(12, '0')}',
  '${number}', -1000, '${name}', 'ErsatzRS', ${previewProfileId}, 5, 0,
  0, 0, 0, 0, 0, 0, 0, 1, 1
);
INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed")
VALUES (${playoutId}, ${id}, 1, 0);
INSERT INTO "PlayoutItem" (
  "Id", "DisableWatermarks", "FillerKind", "Finish", "GuideGroup",
  "InPoint", "MediaItemId", "OutPoint", "PlayoutId", "Start"
) VALUES
  (${id + 2}, 0, 0, '${dbTime(currentFinish)}', 0, '00:00:00', ${mediaItemId}, '00:30:00', ${playoutId}, '${dbTime(currentStart)}'),
  (${id + 3}, 0, 1, '${dbTime(nextFinish)}', 0, '00:00:00', ${mediaItemId}, '00:30:00', ${playoutId}, '${dbTime(currentFinish)}');
INSERT INTO "PlayoutGap" ("Id", "PlayoutId", "Start", "Finish")
VALUES (${id + 4}, ${playoutId}, '${dbTime(nextFinish)}', '${dbTime(gapFinish)}');
`);

  return { id, name, number };
}

function seedMirrorCandidate(
  testInfo: TestInfo,
  source: { id: number; number: string },
): { id: number; name: string; number: string } {
  const id = source.id + 500;
  const number = `${Number.parseInt(source.number, 10) + 100}`;
  const name = `E2E Delayed Mirror ${testInfo.project.name}`;
  const profileId = sqlite(
    `SELECT "FFmpegProfileId" FROM "Channel" WHERE "Id" = ${source.id};`,
  );
  sqlite(`
DELETE FROM "Channel" WHERE "Id" = ${id};
INSERT INTO "Channel" (
  "Id", "UniqueId", "Number", "SortNumber", "Name", "Group",
  "FFmpegProfileId", "StreamingMode", "StreamSelectorMode",
  "PlayoutSource", "PlayoutMode", "TranscodeMode", "IdleBehavior",
  "MusicVideoCreditsMode", "SongVideoMode", "SubtitleMode", "IsEnabled", "ShowInEpg"
) VALUES (
  ${id}, '00000000-0000-0000-0001-${id.toString().padStart(12, '0')}',
  '${number}', -999, '${name}', 'ErsatzRS', ${profileId}, 5, 0,
  0, 0, 0, 0, 0, 0, 0, 1, 1
);
`);
  return { id, name, number };
}

function seedProgrammeGraphics(testInfo: TestInfo, anchor: Date) {
  const clip = makeCalibrationClip(testInfo, 'color=c=black:size=640x360:rate=30');
  const channels = [20, 21, 22].map((ordinal) => seedChannel(testInfo, clip, ordinal));
  const channel = channels[0];
  const graphicId = channel.id + 400;
  const template = '{{ headline }}|{{ CurrentProgramme.Title | default("", true) }}|'
    + '{{ NextProgramme.Title | default("", true) }}|'
    + '{{ ContinuingProgramme.Title | default("", true) }}|{{ Epg[0].Title }}';
  const config = JSON.stringify({ kind: 'text', renderer: 'drawtext', text: template,
    drawtext: { font_file: null, refresh_interval_milliseconds: 250,
      reload_interval_frames: 1, metadata_refresh_interval_seconds: 5 },
    base_style: 'proof', styles: [{ name: 'proof', font_size: 84, text_color: '#ffffff' }] });
  const titles = ['FIRST PROGRAMME', 'SECOND PROGRAMME', 'JUNCTION'];
  const segments = [
    [0, 5, 1, 1, 2, 0], [5, 25, 1, 0, 0, 0], [25, 30, 1, 2, 2, 0],
    [30, 50, 1, 0, 0, 20], [50, 55, 1, 3, 2, 0],
    [55, 60, 2, 1, 2, 0], [60, 80, 2, 0, 1, 0],
  ];
  sqlite(`
${channels.map((row, index) => `UPDATE "MovieMetadata" SET "Title"=${sqlString(titles[index])}
  WHERE "MovieId"=${row.id + 103};`).join('\n')}
DELETE FROM "PlayoutItem" WHERE "PlayoutId"=${channel.id + 1};
DELETE FROM "PlayoutGap" WHERE "PlayoutId"=${channel.id + 1};
DELETE FROM "PlayoutItemGraphicsElement" WHERE "GraphicsElementId"=${graphicId};
DELETE FROM "GraphicsElement" WHERE "Id"=${graphicId};
UPDATE "FFmpegProfile" SET "HardwareAcceleration"=0, "VideoPreset"='ultrafast'
  WHERE "Id"=${channel.id + 200};
INSERT INTO "GraphicsElement" ("Id", "Name", "Kind", "Config", "EpgEntries")
VALUES (${graphicId}, 'Programme context proof', 1, ${sqlString(config)}, 8);
${segments.map(([begin, end, group, filler, media, source], index) => `
INSERT INTO "PlayoutItem" ("Id", "PlayoutId", "MediaItemId", "Start", "Finish",
  "InPoint", "OutPoint", "GuideGroup", "FillerKind", "DisableWatermarks")
VALUES (${channel.id + 600 + index}, ${channel.id + 1}, ${channels[media].id + 103},
  '${dbTime(new Date(anchor.getTime() + begin * 1000)).replace(/\.000$/, '')}',
  '${dbTime(new Date(anchor.getTime() + end * 1000)).replace(/\.000$/, '')}',
  '00:00:${source.toString().padStart(2, '0')}',
  '00:00:${(source + end - begin).toString().padStart(2, '0')}', ${group}, ${filler}, 0);
INSERT INTO "PlayoutItemGraphicsElement" ("PlayoutItemId", "GraphicsElementId", "Variables")
VALUES (${channel.id + 600 + index}, ${graphicId}, '{"headline":"EXPLICIT"}');
`).join('\n')}
`);
  return { channel, graphicId, template };
}

function programmeTimeline(playoutId: number): string {
  return sqlite(`SELECT json_group_array(json_object('id', pi."Id", 'start', pi."Start",
    'finish', pi."Finish", 'in', pi."InPoint", 'out', pi."OutPoint", 'media', pi."MediaItemId",
    'variables', pg."Variables")) FROM "PlayoutItem" pi
    LEFT JOIN "PlayoutItemGraphicsElement" pg ON pg."PlayoutItemId"=pi."Id"
    WHERE pi."PlayoutId"=${playoutId} ORDER BY pi."Start", pi."Id";`);
}

function recordProgrammeBroadcast(testInfo: TestInfo, number: string) {
  const ffmpeg = process.env.ErsatzRS_FFMPEG__PATH ?? process.env.ERSATZRS_FFMPEG__PATH
    ?? process.env.FFMPEG_PATH ?? 'ffmpeg';
  const path = testInfo.outputPath('iptv-programme-transition.mkv');
  const base = process.env.ERSATZRS_BASE_URL ?? 'http://127.0.0.1:8409';
  const child = spawn(ffmpeg, ['-v', 'error', '-y', '-analyzeduration', '1000000',
    '-probesize', '1000000', '-rw_timeout', '15000000', '-i',
    `${base}/iptv/channel/${number}.ts?mode=ts`, '-t', '30', '-c', 'copy', path]);
  let stderr = '';
  child.stderr.on('data', (data: Buffer) => { stderr = (stderr + data.toString()).slice(-65536); });
  const timer = setTimeout(() => {
    stderr += '\nProgramme capture exceeded its 45-second deadline.';
    child.kill('SIGKILL');
  }, 45_000);
  const result = new Promise<number | null>((resolve) => {
    child.on('error', (error) => { stderr += error.message; clearTimeout(timer); resolve(null); });
    child.on('exit', (code) => { clearTimeout(timer); resolve(code); });
  });
  return {
    async decode() {
      expect(await result, stderr).toBe(0);
      execFileSync(ffmpeg, ['-v', 'error', '-i', path, '-f', 'null', '-'], { timeout: 20_000 });
      for (const [time, label] of [['0.5', 'first'], ['26', 'second']]) {
        const framePath = testInfo.outputPath(`iptv-${label}-programme.png`);
        execFileSync(ffmpeg, ['-v', 'error', '-y', '-ss', time, '-i', path,
          '-frames:v', '1', framePath], { timeout: 15_000 });
        expect(existsSync(framePath), `${label} regular IPTV frame was not decoded`).toBe(true);
      }
    },
    async stop() { child.kill('SIGTERM'); await result; },
  };
}

test.describe('finding 04 programme graphics acceptance', () => {
  test.skip(!dbPath, 'Requires a disposable migrated database');

  test('historical editor preview preserves split programmes, defaults and raw Epg', async ({ page, browser }, testInfo) => {
    test.setTimeout(90_000);
    await forceEnglish(page);
    const anchor = new Date('2026-04-15T10:00:00Z');
    const { channel, graphicId, template } = seedProgrammeGraphics(testInfo, anchor);
    const before = programmeTimeline(channel.id + 1);
    const errors: string[] = [];
    page.on('pageerror', (error) => {
      // Firefox cancels the old document's requests during the intentional reload.
      if (error.name !== 'AbortError' || !/operation was aborted/i.test(error.message)) {
        errors.push(error.message);
      }
    });
    await page.goto(`/graphics-elements/${graphicId}`);
    await waitForHydration(page);
    const preview = page.locator('[data-graphics-context-preview]').filter({ visible: true });
    await preview.locator('[data-graphics-preview-channel]').selectOption(channel.id.toString());
    for (const [seconds, expected] of [
      [1, 'EXPLICIT|FIRST PROGRAMME|SECOND PROGRAMME||JUNCTION'],
      [10, 'EXPLICIT|FIRST PROGRAMME|SECOND PROGRAMME||FIRST PROGRAMME'],
      [26, 'EXPLICIT|FIRST PROGRAMME|SECOND PROGRAMME|FIRST PROGRAMME|JUNCTION'],
      [52, 'EXPLICIT|FIRST PROGRAMME|SECOND PROGRAMME||JUNCTION'],
      [56, 'EXPLICIT|SECOND PROGRAMME|||JUNCTION'],
      [65, 'EXPLICIT|SECOND PROGRAMME|||SECOND PROGRAMME'],
    ] as const) {
      await preview.locator('[data-graphics-preview-time]').fill(
        new Date(anchor.getTime() + seconds * 1000).toISOString());
      await preview.locator('[data-graphics-preview-submit]').click();
      await expect(preview.locator('[data-graphics-preview-text]')).toHaveText(expected);
      await expect(preview.locator('[data-graphics-preview-error]')).toBeHidden();
    }
    await page.locator('[data-graphics-element-renderer-select]').filter({ visible: true }).selectOption('drawvg');
    await preview.locator('[data-graphics-preview-time]').fill('2026-04-15T12:00:26+02:00');
    await preview.locator('[data-graphics-preview-submit]').click();
    await expect(preview.locator('[data-graphics-preview-text]'))
      .toHaveText('EXPLICIT|FIRST PROGRAMME|SECOND PROGRAMME|FIRST PROGRAMME|JUNCTION');
    const editor = page.locator('[data-graphics-element-editor]').filter({ visible: true });
    await editor.locator('[data-graphics-element-text]').fill(`DRAFT|${template}`);
    await preview.locator('[data-graphics-preview-submit]').click();
    await expect(preview.locator('[data-graphics-preview-text]'))
      .toHaveText('DRAFT|EXPLICIT|FIRST PROGRAMME|SECOND PROGRAMME|FIRST PROGRAMME|JUNCTION');
    await editor.locator('[data-graphics-element-text]').fill(template);
    await expect(editor.locator('[data-graphics-metadata-refresh]')).toHaveValue('5');
    await expect(editor.locator('[name="preview_channel"]')).toHaveValue(channel.id.toString());
    await expect(editor.locator('[name="preview_time"]')).toHaveValue('2026-04-15T12:00:26+02:00');
    await editor.locator('[name="preview_width"]').fill('320');
    await editor.locator('[name="preview_height"]').fill('180');
    // Render the changing programme itself, rather than clipping both images to
    // the identical explicit prefix at the saved 84-pixel type size.
    await editor.locator('[data-graphics-element-text]').fill('{{ CurrentProgramme.Title | default("", true) }}');
    await editor.locator('[name="font_size"]').fill('14');
    const nativeOutputs: string[] = [];
    for (const seconds of [10, 65]) {
      await editor.locator('[name="preview_time"]').fill(new Date(anchor.getTime() + seconds * 1000).toISOString());
      const rendered = page.waitForResponse(response => response.url().includes('/admin-api/graphics-elements/author')
        && response.request().postData()?.includes('intent=preview') === true, { timeout: 15000 });
      await editor.locator('[data-graphics-render-submit]').click();
      const response = await rendered;
      expect(response.status()).toBe(200);
      const freshSource = (await response.text()).match(/src="(data:image\/png;base64,[^"]+)"/)?.[1];
      expect(freshSource).toBeTruthy();
      const image = page.frameLocator('[data-graphics-render-frame]').locator('[data-graphics-render-output]');
      await expect(image).toHaveAttribute('src', freshSource!);
      await expect(image).toBeVisible();
      const source = (await image.getAttribute('src'))!;
      const png = Buffer.from(source.split(',')[1], 'base64');
      const pngPath = testInfo.outputPath(`historical-native-${seconds}.png`);
      writeFileSync(pngPath, png);
      const decoded = execFileSync(process.env.ErsatzRS_FFMPEG__PATH ?? 'ffmpeg', ['-v', 'error', '-i', pngPath, '-frames:v', '1', '-f', 'rawvideo', '-pix_fmt', 'rgba', 'pipe:1'], { timeout: 15000 });
      expect(decoded.length).toBe(320 * 180 * 4);
      expect(decoded.some((value, index) => index % 4 === 3 && value > 0)).toBe(true);
      nativeOutputs.push(source);
    }
    expect(nativeOutputs[0]).not.toBe(nativeOutputs[1]);
    await editor.locator('[data-graphics-element-text]').fill(template);
    await editor.locator('[name="font_size"]').fill('84');
    await editor.locator('[name="preview_time"]').fill('2026-04-15T12:00:26+02:00');
    await editor.locator('[data-save-graphics-element]').click();
    await expect(page).toHaveURL(new RegExp(`/graphics-elements/${graphicId}\\?saved=1$`));
    expect(sqlite(`SELECT json_extract("Config", '$.drawtext.metadata_refresh_interval_seconds')
      FROM "GraphicsElement" WHERE "Id"=${graphicId};`)).toBe('5');
    await page.request.post('/admin-api/settings/ui/save', {
      form: { is_dark_mode: 'true', language: 'nl' }, maxRedirects: 0,
    });
    await page.reload();
    await waitForHydration(page);
    await expect(preview.locator('h2')).toHaveText('Voorbeeld van programmatekst');
    await preview.locator('[data-graphics-preview-channel]').selectOption(channel.id.toString());
    await preview.locator('[data-graphics-preview-time]').fill('2026-04-15T12:00:26+02:00');
    await preview.locator('[data-graphics-preview-submit]').click();
    await expect(preview.locator('[data-graphics-preview-text]'))
      .toHaveText('EXPLICIT|FIRST PROGRAMME|SECOND PROGRAMME|FIRST PROGRAMME|JUNCTION');
    await page.setViewportSize({ width: 390, height: 844 });
    const navigation = page.getByRole('button', { name: 'Navigatie omschakelen' });
    await page.keyboard.press('Escape');
    await expect(navigation).toHaveAttribute('aria-expanded', 'false');
    await expect.poll(() => page.locator('[data-nav-drawer]')
      .evaluate((element) => element.getBoundingClientRect().right)).toBeLessThanOrEqual(0);
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
    await preview.screenshot({ path: testInfo.outputPath('historical-editor-mobile.png') });
    const context = await browser.newContext({ baseURL: process.env.ERSATZRS_BASE_URL ?? 'http://127.0.0.1:8409', javaScriptEnabled: false });
    try {
      const noScript = await context.newPage();
      await noScript.goto(`/graphics-elements/${graphicId}?preview_channel=${channel.id}&preview_time=2026-04-15T10%3A00%3A26Z`);
      await expect(noScript.locator('[data-graphics-preview-text]').filter({ visible: true }))
        .toHaveText('EXPLICIT|FIRST PROGRAMME|SECOND PROGRAMME|FIRST PROGRAMME|JUNCTION');
      const nativeEditor = noScript.locator('[data-graphics-element-editor]').filter({ visible: true });
      await expect(nativeEditor.locator('[name="preview_channel"]')).toHaveValue(channel.id.toString());
      await expect(nativeEditor.locator('[name="preview_time"]')).toHaveValue('2026-04-15T10:00:26Z');
      await nativeEditor.locator('[data-save-graphics-element]').click();
      await expect(noScript).toHaveURL(new RegExp(`/graphics-elements/${graphicId}\\?saved=1$`));

    } finally { await context.close(); }
    expect(programmeTimeline(channel.id + 1)).toBe(before);
    expect(errors).toEqual([]);
  });

  test('decodes native programme titles across a served transition', async ({ page }, testInfo) => {
    test.setTimeout(120_000);
    await forceEnglish(page);
    const anchor = new Date(Date.now() - 2000);
    const { channel, graphicId } = seedProgrammeGraphics(testInfo, anchor);
    // Two complete occurrences make the broadcast boundary independent of the
    // editor's historical fixture. The black source contains no glyphs.
    const first = channel.id + 601;
    const second = channel.id + 606;
    sqlite(`
DELETE FROM "PlayoutItem" WHERE "PlayoutId"=${channel.id + 1} AND "Id" NOT IN (${first}, ${second});
UPDATE "PlayoutItem" SET "Start"='${dbTime(anchor)}',
  "Finish"='${dbTime(new Date(anchor.getTime() + 24000))}', "InPoint"='00:00:00', "OutPoint"='00:00:24'
  WHERE "Id"=${first};
UPDATE "PlayoutItem" SET "Start"='${dbTime(new Date(anchor.getTime() + 24000))}',
  "Finish"='${dbTime(new Date(anchor.getTime() + 70000))}', "InPoint"='00:00:00', "OutPoint"='00:00:46'
  WHERE "Id"=${second};
UPDATE "GraphicsElement" SET "Config"=json_set("Config", '$.text',
  'ON AIR {{ CurrentProgramme.Title | default("", true) }}') WHERE "Id"=${graphicId};
`);
    const before = programmeTimeline(channel.id + 1);
    const limitKey = 'ffmpeg.segmenter.active_transcode_limit';
    const originalLimit = sqlite(`SELECT quote("Value") FROM "ConfigElement" WHERE "Key"='${limitKey}';`);
    // Each of the two native outputs needs capacity for its boundary handover.
    sqlite(`UPDATE "ConfigElement" SET "Value"='4' WHERE "Key"='${limitKey}';`);
    const root = page.locator('.channel-watch-js [data-channel-preview]');
    const video = root.locator('video');
    const frame = () => video.evaluate((element: HTMLVideoElement) => {
      const canvas = document.createElement('canvas');
      canvas.width = 640; canvas.height = 360;
      const context = canvas.getContext('2d')!;
      context.drawImage(element, 0, 0, 640, 360);
      const pixels = context.getImageData(0, 0, 640, 360).data;
      let white = 0;
      for (let i = 0; i < pixels.length; i += 4) {
        if (pixels[i] > 180 && pixels[i + 1] > 180 && pixels[i + 2] > 180) white++;
      }
      return { white, png: canvas.toDataURL('image/png'), time: element.currentTime };
    });
    let recording: ReturnType<typeof recordProgrammeBroadcast> | undefined;
    try {
      await page.goto(`/channels/${channel.id}/watch`);
      await waitForHydration(page);
      await expect.poll(() => video.evaluate((element: HTMLVideoElement) => element.readyState),
        { timeout: 45_000 }).toBeGreaterThanOrEqual(2);
      await expect.poll(async () => (await frame()).white).toBeGreaterThan(100);
      const initial = await frame();
      recording = recordProgrammeBroadcast(testInfo, channel.number);
      writeFileSync(testInfo.outputPath('served-first-programme.png'), Buffer.from(initial.png.split(',')[1], 'base64'));
      await expect.poll(async () => Math.abs((await frame()).white - initial.white),
        { timeout: 60_000 }).toBeGreaterThan(150);
      const next = await frame();
      writeFileSync(testInfo.outputPath('served-second-programme.png'), Buffer.from(next.png.split(',')[1], 'base64'));
      expect(next.time).toBeGreaterThan(initial.time + 2);
      expect(next.white).toBeGreaterThan(100);
      await recording.decode();
      expect(programmeTimeline(channel.id + 1)).toBe(before);
      writeFileSync(testInfo.outputPath('served-transition.json'), JSON.stringify({
        expectedTitles: ['FIRST PROGRAMME', 'SECOND PROGRAMME'],
        first: { whitePixels: initial.white, decodedTime: initial.time },
        second: { whitePixels: next.white, decodedTime: next.time }, timelinePreserved: true,
        regularIptvCaptureSeconds: 30, regularIptvFullyDecoded: true,
      }, null, 2));
    } finally {
      try {
        await recording?.stop();
        if (await root.getByRole('button', { name: 'Stop Preview', exact: true }).isVisible()) {
          await root.getByRole('button', { name: 'Stop Preview', exact: true }).click();
          await stopFixtureTranscode(page, channel.number);
        }
      } finally {
        sqlite(`UPDATE "ConfigElement" SET "Value"=${originalLimit} WHERE "Key"='${limitKey}';`);
      }
    }
  });
});

function programmeStartMinutes(title: string): number {
  const match = title.match(/·\s*(\d{2}):(\d{2})/);
  if (!match) throw new Error(`programme title has no start time: ${title}`);
  return Number(match[1]) * 60 + Number(match[2]);
}

async function forceEnglish(page: Page): Promise<void> {
  await page.request.post('/admin-api/settings/ui/save', {
    form: { is_dark_mode: 'true', language: 'en-us' },
    maxRedirects: 0,
  });
}

async function waitForHydration(page: Page): Promise<void> {
  await expect
    .poll(
      () =>
        page.evaluate(
          () =>
            (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated ===
            true,
        ),
      { timeout: 15_000 },
    )
    .toBe(true);
}

async function findChannelRow(page: Page, channelName: string): Promise<Locator> {
  const rows = page.getByRole('row');
  const channelRow = rows.filter({ hasText: channelName });
  const nextPage = page.getByRole('button', { name: 'Next page', exact: true });

  for (let pageIndex = 0; pageIndex < 100; pageIndex += 1) {
    if ((await channelRow.count()) > 0) return channelRow;

    await expect(nextPage).toBeEnabled();
    const firstRow = rows.nth(1);
    const previousFirstRowText = await firstRow.innerText();
    await nextPage.click();
    await expect.poll(() => firstRow.innerText()).not.toBe(previousFirstRowText);
  }

  throw new Error(`channel row was not found after paging: ${channelName}`);
}

test.describe('channel preview modal and watch route', () => {
  test.skip(!dbPath, 'E2E_DB_PATH or ERSATZRS_DB_PATH is required');

  test('both surfaces autoplay muted and share localized HLS failures', async ({
    browser,
    baseURL,
    page,
  }, testInfo) => {
    const channel = seedChannel(testInfo);
    await forceEnglish(page);
    let playlistRequests = 0;
    await page.route('**/iptv/preview/*/master.m3u8*', async (route) => {
      playlistRequests += 1;
      await route.fulfill({ status: 503, contentType: 'text/plain', body: 'at capacity' });
    });

    await page.goto('/channels');
    await waitForHydration(page);
    const row = page.getByRole('row').filter({ hasText: channel.name });
    await row.getByRole('button', { name: 'Preview channel' }).click();
    const dialog = page.getByRole('dialog', { name: 'Channel Preview' });
    await expect(dialog).toBeVisible();
    const modalVideo = dialog.locator('video');
    await expect(modalVideo).toHaveAttribute('autoplay', '');
    await expect.poll(() => modalVideo.evaluate((video: HTMLVideoElement) => video.muted)).toBe(true);
    await expect.poll(() => playlistRequests).toBeGreaterThan(0);
    const firstModalPlaylistRequests = playlistRequests;
    await page.keyboard.press('Escape');
    await expect(dialog).toBeHidden();
    const previewButton = row.getByRole('button', { name: 'Preview channel' });
    await expect(previewButton).toBeFocused();
    await previewButton.click();
    await expect.poll(() => playlistRequests).toBeGreaterThan(firstModalPlaylistRequests);
    await expect(page.locator('[data-preview-status]')).toContainText(
      /transcode slots|network connection/,
    );
    const modalPlaylistRequests = playlistRequests;

    await page.getByRole('button', { name: 'Open Full Player' }).click();
    await expect(page).toHaveURL(new RegExp(`/channels/${channel.id}/watch$`));
    await waitForHydration(page);
    await expect(page.getByRole('heading', { name: new RegExp(channel.name) })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Programme Guide' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Now', exact: true })).toBeVisible();
    const zoom = page.getByRole('slider', { name: 'Zoom' });
    await expect(zoom).toBeVisible();
    await expect(zoom).toHaveValue('100');
    await expect.poll(() => playlistRequests).toBeGreaterThan(modalPlaylistRequests);
    const watchStatus = page.locator('.channel-watch-js [data-preview-status]');
    await expect(watchStatus).toContainText(/transcode slots|network connection/);
    // The guide arrives through a client-side resource and scrolls the timeline
    // to its default position exactly once, when the first snapshot lands. The
    // lane renders from a placeholder before then, so gate on the channel label
    // the loaded snapshot supplies: otherwise that one-shot scroll can fall
    // between the scroll set below and the zoom proof, moving the measured
    // center and failing the comparison.
    await expect(
      page.locator('[data-epg-grid] [role="rowheader"]').first(),
    ).toContainText(channel.name);
    await expectScrollableTimeline(page);
    await setTimelineScroll(page, 360);
    const watchCenterBefore = await timelineCenterMinutes(page, 100);
    await zoom.fill('150');
    await expect
      .poll(async () =>
        Math.abs((await timelineCenterMinutes(page, 150)) - watchCenterBefore),
      )
      .toBeLessThan(1);
    await expectStepScrollsTimeline(page, '+2h');
    await expect(page.getByText('Now Playing').first()).toBeVisible();
    const watchVideo = page.locator('.channel-watch-js [data-channel-preview] video');
    await expect(watchVideo).toHaveAttribute('autoplay', '');
    await expect.poll(() => watchVideo.evaluate((video: HTMLVideoElement) => video.muted)).toBe(true);
    await page.getByRole('button', { name: 'Stop Preview' }).click();
    await expect(watchStatus).toContainText('Preview stopped');

    const noJsContext = await browser.newContext({ javaScriptEnabled: false, baseURL });
    const noJsPage = await noJsContext.newPage();
    await noJsPage.goto(`/channels/${channel.id}/watch`);
    await expect(noJsPage.getByRole('heading', { name: new RegExp(channel.name) })).toBeVisible();
    await noJsContext.close();
  });

  test('closing while preview authorization is pending cannot start hidden playback', async ({
    page,
  }, testInfo) => {
    const channel = seedChannel(testInfo);
    await forceEnglish(page);
    let releasePreviewUrl = () => {};
    const previewUrlBlocked = new Promise<void>((resolve) => {
      releasePreviewUrl = resolve;
    });
    let previewUrlRequests = 0;
    let playlistRequests = 0;
    await page.route('**/admin-api/get_channel_preview_url*', async (route) => {
      previewUrlRequests += 1;
      await previewUrlBlocked;
      await route.continue();
    });
    await page.route('**/iptv/preview/*/master.m3u8*', async (route) => {
      playlistRequests += 1;
      await route.fulfill({ status: 503, body: 'unexpected hidden playback' });
    });

    await page.goto('/channels');
    await waitForHydration(page);
    const row = page.getByRole('row').filter({ hasText: channel.name });
    await row.getByRole('button', { name: 'Preview channel' }).click();
    await expect.poll(() => previewUrlRequests).toBe(1);

    const dialog = page.getByRole('dialog', { name: 'Channel Preview' });
    await dialog.getByRole('button', { name: 'Close', exact: true }).click();
    await expect(dialog).toBeHidden();
    const previewUrlResponse = page.waitForResponse((response) =>
      response.url().includes('/admin-api/get_channel_preview_url'),
    );
    releasePreviewUrl();
    await previewUrlResponse;
    await page.waitForTimeout(250);
    expect(playlistRequests).toBe(0);
  });

  test('late channel projections cannot replace the currently open preview', async ({
    page,
  }, testInfo) => {
    const first = seedChannel(testInfo, undefined, 0);
    const second = seedChannel(testInfo, undefined, 1);
    await forceEnglish(page);
    let releaseFirstProjection = () => {};
    const firstProjectionBlocked = new Promise<void>((resolve) => {
      releaseFirstProjection = resolve;
    });
    let projectionRequests = 0;
    await page.route('**/admin-api/get_channel_watch_data*', async (route) => {
      projectionRequests += 1;
      if (projectionRequests === 1) {
        await firstProjectionBlocked;
      }
      await route.continue();
    });

    await page.goto('/channels');
    await waitForHydration(page);
    await page
      .getByRole('row')
      .filter({ hasText: first.name })
      .getByRole('button', { name: 'Preview channel' })
      .click();
    await expect.poll(() => projectionRequests).toBe(1);
    await page.keyboard.press('Escape');

    await page
      .getByRole('row')
      .filter({ hasText: second.name })
      .getByRole('button', { name: 'Preview channel' })
      .click();
    await expect.poll(() => projectionRequests).toBe(2);
    const dialog = page.getByRole('dialog', { name: 'Channel Preview' });
    await expect(dialog).toContainText(`E2E Preview Programme ${testInfo.project.name} 1`);

    const firstProjectionResponse = page.waitForResponse((response) =>
      response.url().includes('/admin-api/get_channel_watch_data'),
    );
    releaseFirstProjection();
    await firstProjectionResponse;
    await page.waitForTimeout(100);
    await expect(dialog).toContainText(`E2E Preview Programme ${testInfo.project.name} 1`);
    await expect(dialog).not.toContainText(
      `E2E Preview Programme ${testInfo.project.name} 0`,
    );
    await page.getByRole('button', { name: 'Open Full Player' }).click();
    await expect(page).toHaveURL(new RegExp(`/channels/${second.id}/watch$`));
    await waitForHydration(page);
    const stopUrl = await stopFixtureTranscode(page, second.number);
    // The first modal can also have acquired a lease before its delayed
    // projection resolves. Cancel only these two channels owned by this test.
    const firstStopped = await page.request.post(stopUrl, { form: { number: first.number } });
    expect(firstStopped.ok()).toBe(true);
    await expect.poll(() => existsSync(join(tmpdir(), 'ersatzrs-hls-preview', first.number)))
      .toBe(false);
  });

  for (const failure of [
    { status: 401, message: 'Preview authorization failed.' },
    { status: 404, message: 'The live stream is unavailable.' },
  ]) {
    test(`reports localized ${failure.status} preview failures`, async ({ page }, testInfo) => {
      const channel = seedChannel(testInfo);
      await forceEnglish(page);
      await page.route('**/iptv/preview/*/master.m3u8*', async (route) => {
        await route.fulfill({
          status: failure.status,
          contentType: 'text/plain',
          body: failure.message,
        });
      });

      await page.goto(`/channels/${channel.id}/watch`);
      await waitForHydration(page);
      await expect(page.locator('[data-preview-status]')).toHaveText(failure.message);
    });
  }

  test('global EPG navigation renders the shared five-hour lane grid', async ({
    page,
  }, testInfo) => {
    const channel = seedChannel(testInfo);
    await forceEnglish(page);

    await page.goto('/channels');
    await waitForHydration(page);
    await page.getByRole('link', { name: 'EPG Preview', exact: true }).click();
    await expect(page).toHaveURL(/\/epg$/);
    await waitForHydration(page);
    await expect(page.getByRole('heading', { name: 'EPG Preview' })).toBeVisible();
    await expect(
      page.locator('[data-channel-id]').filter({ hasText: channel.name }),
    ).toBeVisible();
    await expect(page.getByRole('button', { name: '-2h' })).toBeVisible();
    await expect(page.getByRole('button', { name: '+4h' })).toBeVisible();
    const zoom = page.getByRole('slider', { name: 'Zoom' });
    await expect(zoom).toBeVisible();
    await expect(zoom).toHaveValue('100');
    const firstProgramme = page.locator('[role="gridcell"]').first();
    const timeline = page.locator('[data-epg-time-track]');
    const widthBefore = await firstProgramme.evaluate((element) =>
      Number.parseFloat((element as HTMLElement).style.width),
    );
    const timelineWidthBefore = await timeline.evaluate(
      (element) => element.getBoundingClientRect().width,
    );
    await expectScrollableTimeline(page);
    await setTimelineScroll(page, 360);
    const centerBefore = await timelineCenterMinutes(page, 100);
    await zoom.fill('150');
    await expect(zoom).toHaveValue('150');
    await expect(page.locator('[data-zoom-value]')).toHaveText('150%');
    await expect
      .poll(() =>
        firstProgramme.evaluate((element) =>
          Number.parseFloat((element as HTMLElement).style.width),
        ),
      )
      .toBeGreaterThan(widthBefore);
    await expect
      .poll(() => timeline.evaluate((element) => element.getBoundingClientRect().width))
      .toBeGreaterThan(timelineWidthBefore);
    await expect
      .poll(async () => Math.abs((await timelineCenterMinutes(page, 150)) - centerBefore))
      .toBeLessThan(1);
    await expectStepScrollsTimeline(page, '+2h');
  });

  test('configures and renders a one-hour delayed mirror through the admin UI', async ({
    page,
  }, testInfo) => {
    const source = seedChannel(testInfo);
    const mirror = seedMirrorCandidate(testInfo, source);
    const now = new Date();
    sqlite(`
UPDATE "PlayoutItem"
SET "Start" = '${dbTime(new Date(now.getTime() - 90 * 60_000))}',
    "Finish" = '${dbTime(new Date(now.getTime() + 30 * 60_000))}'
WHERE "Id" = ${source.id + 2};
UPDATE "PlayoutItem"
SET "Start" = '${dbTime(new Date(now.getTime() + 30 * 60_000))}',
    "Finish" = '${dbTime(new Date(now.getTime() + 90 * 60_000))}'
WHERE "Id" = ${source.id + 3};
`);
    await forceEnglish(page);
    const browserErrors: string[] = [];
    page.on('console', (message) => {
      if (message.type() === 'error') browserErrors.push(message.text());
    });
    page.on('pageerror', (error) => browserErrors.push(error.message));

    await page.goto(`/channels/${mirror.id}`);
    await waitForHydration(page);
    const mirrorSource = page.locator('select[name="mirror_source_channel_id"]');
    const playoutOffset = page.locator('input[name="playout_offset_hours"]');
    await expect(page.locator('select[name="playout_source"]')).toHaveValue('0');
    await expect(mirrorSource).toBeDisabled();
    await expect(playoutOffset).toBeDisabled();
    await expect(
      page.getByText('Available only when Playout Source is set to Mirror.'),
    ).toBeVisible();
    await page.locator('select[name="playout_source"]').selectOption('1');
    await expect(mirrorSource).toBeEnabled();
    await expect(playoutOffset).toBeEnabled();
    await mirrorSource.selectOption(String(source.id));
    await playoutOffset.fill('1');
    await page.locator('select[name="playout_source"]').selectOption('0');
    await expect(mirrorSource).toBeDisabled();
    await expect(playoutOffset).toBeDisabled();
    await page.locator('select[name="playout_source"]').selectOption('1');
    await expect(mirrorSource).toBeEnabled();
    await expect(playoutOffset).toBeEnabled();
    await expect(mirrorSource).toHaveValue(String(source.id));
    await expect(playoutOffset).toHaveValue('1');
    await Promise.all([
      page.waitForURL(/\/channels$/),
      page.getByRole('button', { name: 'Save Channel', exact: true }).click(),
    ]);
    await waitForHydration(page);

    await page.goto(`/channels/${mirror.id}`);
    await waitForHydration(page);
    await expect(page.locator('select[name="playout_source"]')).toHaveValue('1');
    await expect(page.locator('select[name="mirror_source_channel_id"]')).toHaveValue(
      String(source.id),
    );
    await expect(page.locator('input[name="playout_offset_hours"]')).toHaveValue('1');

    await page.goto('/channels');
    await waitForHydration(page);
    const mirrorRow = await findChannelRow(page, mirror.name);
    await expect(mirrorRow.getByRole('button', { name: 'Preview channel' })).toBeVisible();

    await page.goto('/playouts/add');
    await waitForHydration(page);
    await expect(
      page.locator(`select[name="channel_id"] option[value="${mirror.id}"]`),
    ).toHaveAttribute('disabled', '');

    await page.goto(`/channels/${mirror.id}/watch`);
    await waitForHydration(page);
    await expect(page.getByRole('heading', { name: new RegExp(mirror.name) })).toBeVisible();
    await expect(page.getByRole('gridcell').filter({ hasText: 'E2E Preview Programme' }).first())
      .toBeVisible();

    await stopFixtureTranscode(page, mirror.number);

    await page.goto('/epg');
    await waitForHydration(page);
    const sourceLane = page.locator(`[data-channel-id="${source.id}"]`);
    const mirrorLane = page.locator(`[data-channel-id="${mirror.id}"]`);
    const sourceProgramme = sourceLane
      .getByRole('gridcell')
      .filter({ hasText: 'E2E Preview Programme' })
      .first();
    const mirrorProgramme = mirrorLane
      .getByRole('gridcell')
      .filter({ hasText: 'E2E Preview Programme' })
      .first();
    await expect(sourceProgramme).toBeVisible();
    await expect(mirrorProgramme).toBeVisible();
    const sourceTitle = await sourceProgramme.getAttribute('title');
    const mirrorTitle = await mirrorProgramme.getAttribute('title');
    expect(sourceTitle).not.toBeNull();
    expect(mirrorTitle).not.toBeNull();
    expect(
      (programmeStartMinutes(mirrorTitle!) - programmeStartMinutes(sourceTitle!) + 24 * 60) %
        (24 * 60),
    ).toBe(60);
    expect(browserErrors).toEqual([]);
  });

  test('preview video and modal stay contained on desktop and mobile screens', async ({
    page,
  }, testInfo) => {
    const channel = seedChannel(testInfo);
    await forceEnglish(page);
    await page.route('**/iptv/preview/*/master.m3u8*', async (route) => {
      await route.fulfill({ status: 503, contentType: 'text/plain', body: 'at capacity' });
    });

    await page.goto('/channels');
    await waitForHydration(page);
    const row = page.getByRole('row').filter({ hasText: channel.name });
    await row.getByRole('button', { name: 'Preview channel' }).click();
    const dialog = page.getByRole('dialog', { name: 'Channel Preview' });
    const panel = dialog.locator('[data-modal-panel]');

    for (const viewport of [
      { width: 1440, height: 900 },
      { width: 390, height: 844 },
      { width: 844, height: 390 },
    ]) {
      await page.setViewportSize(viewport);
      await expect(dialog).toBeVisible();
      const geometry = await panel.evaluate((element) => {
        const panelRect = element.getBoundingClientRect();
        const frame = element.querySelector('[data-channel-preview] video')?.parentElement;
        const frameRect = frame?.getBoundingClientRect();
        return {
          panel: {
            left: panelRect.left,
            top: panelRect.top,
            right: panelRect.right,
            bottom: panelRect.bottom,
            scrollHeight: element.scrollHeight,
            clientHeight: element.clientHeight,
          },
          frame: frameRect
            ? { width: frameRect.width, height: frameRect.height }
            : undefined,
        };
      });
      expect(geometry.panel.left).toBeGreaterThanOrEqual(0);
      expect(geometry.panel.top).toBeGreaterThanOrEqual(0);
      expect(geometry.panel.right).toBeLessThanOrEqual(viewport.width);
      expect(geometry.panel.bottom).toBeLessThanOrEqual(viewport.height);
      expect(geometry.panel.scrollHeight).toBeLessThanOrEqual(geometry.panel.clientHeight + 1);
      expect(geometry.frame).toBeDefined();
      expect(geometry.frame!.width).toBeLessThanOrEqual(viewport.width);
      expect(geometry.frame!.height).toBeLessThanOrEqual(viewport.height);
      expect(
        geometry.frame!.width / geometry.frame!.height,
        `modal geometry ${JSON.stringify({ viewport, geometry })}`,
      ).toBeCloseTo(16 / 9, 1);
    }

    await dialog.getByRole('button', { name: 'Open Full Player' }).click();
    await waitForHydration(page);
    for (const viewport of [
      { width: 1440, height: 900 },
      { width: 390, height: 844 },
    ]) {
      await page.setViewportSize(viewport);
      const frame = page.locator('.channel-watch-js [data-channel-preview] video').locator('..');
      const bounds = await frame.boundingBox();
      const regionBounds = await page.locator('[data-watch-preview-region]').boundingBox();
      expect(bounds).not.toBeNull();
      expect(regionBounds).not.toBeNull();
      expect(bounds!.x).toBeGreaterThanOrEqual(0);
      expect(Math.abs(bounds!.x - regionBounds!.x)).toBeLessThanOrEqual(1);
      expect(bounds!.width).toBeLessThanOrEqual(viewport.width);
      expect(bounds!.height).toBeLessThanOrEqual(viewport.height * 0.72 + 1);
      expect(bounds!.width / bounds!.height).toBeCloseTo(16 / 9, 1);
    }
  });

  test('Next burns explicitly pinned unextracted text independently of native extraction settings', async ({ page }, testInfo) => {
    test.skip(!process.env.ETV_NEXT_FOLDER, 'Requires the real Next engine');
    test.setTimeout(120_000);
    const clipPath = makeCalibrationClip(testInfo, 'color=c=blue:size=640x360:rate=30');
    const subtitlePath = testInfo.outputPath('next-burn.nl.srt');
    const embeddedPath = testInfo.outputPath('next-burn.mkv');
    writeFileSync(subtitlePath, '1\n00:00:00,000 --> 00:01:10,000\nNEXT BURN GLYPH PROOF\n');
    const ffmpeg = process.env.ErsatzRS_FFMPEG__PATH ?? process.env.ERSATZRS_FFMPEG__PATH
      ?? process.env.FFMPEG_PATH ?? 'ffmpeg';
    execFileSync(ffmpeg, ['-v', 'error', '-y', '-i', clipPath, '-i', subtitlePath,
      '-map', '0:v:0', '-map', '0:a:0', '-map', '1:0', '-c', 'copy',
      '-metadata:s:s:0', 'language=nld', '-metadata:s:s:0', 'title=Dutch',
      '-disposition:s:0', 'default', embeddedPath], { stdio: 'inherit', timeout: 30_000 });
    const channel = seedChannel(testInfo, embeddedPath, 10);
    const keys = ['ffmpeg.use_embedded_subtitles', 'ffmpeg.extract_embedded_subtitles'];
    const restoreSettings = sqlite(`SELECT 'INSERT OR REPLACE INTO "ConfigElement" ("Key", "Value") VALUES ('
      || quote("Key") || ',' || quote("Value") || ');' FROM "ConfigElement"
      WHERE "Key" IN (${keys.map(sqlString).join(',')});`);
    sqlite(`
UPDATE "Channel" SET "StreamingEngine"=1, "StreamingMode"=4,
  "NextEngineTextSubtitleMode"=0, "SubtitleMode"=2,
  "PreferredSubtitleLanguageCode"='nld' WHERE "Id"=${channel.id};
UPDATE "PlayoutItem" SET "OutPoint"='00:01:00', "MediaVersionId"=${channel.id + 105},
  "VideoStreamIndex"=0, "AudioStreamIndex"=1, "SubtitleStreamIndex"=2
  WHERE "PlayoutId"=${channel.id + 1};
INSERT INTO "MediaStream" ("MediaVersionId", "Index", "MediaStreamKind", "Codec",
  "Language", "Title", "Default", "Forced", "Channels")
VALUES (${channel.id + 105}, 2, 3, 'subrip', 'nld', 'Dutch', 1, 0, 0);
INSERT INTO "Subtitle" ("MovieMetadataId", "SubtitleKind", "Title", "StreamIndex",
  "Codec", "Default", "Forced", "SDH", "Language", "IsExtracted", "DateAdded", "DateUpdated")
VALUES (${channel.id + 104}, 0, 'Dutch', 2, 'subrip', 1, 0, 0, 'nld', 0,
  '2026-09-23 00:00:00', '2026-09-23 00:00:00');
${keys.map((key) => `INSERT OR REPLACE INTO "ConfigElement" ("Key", "Value") VALUES (${sqlString(key)}, 'false');`).join('\n')}
`);
    try {
      await forceEnglish(page);
      const errors: string[] = [];
      page.on('pageerror', (error) => errors.push(error.message));
      const masterResponse = page.waitForResponse((response) =>
        response.url().includes(`/iptv/preview/${channel.number}/master.m3u8`)
        && response.status() === 200);
      await page.goto(`/channels/${channel.id}/watch`);
      await waitForHydration(page);
      expect(await (await masterResponse).text()).not.toContain('TYPE=SUBTITLES');
      const root = page.locator('.channel-watch-js [data-channel-preview]');
      const video = root.locator('video');
      await expect(root).toHaveAttribute('data-preview-engine', 'hlsjs');
      await expect.poll(() => video.evaluate((element: HTMLVideoElement) => element.readyState),
        { timeout: 60_000 }).toBeGreaterThanOrEqual(2);
      const time = await video.evaluate((element: HTMLVideoElement) => element.currentTime);
      await expect.poll(() => video.evaluate((element: HTMLVideoElement) => element.currentTime),
        { timeout: 20_000 }).toBeGreaterThan(time + 1);
      // drawImage samples decoded video pixels, not DOM text-track overlays.
      // The synthetic blue source contains no white pixels; white glyphs prove burn-in.
      await expect.poll(() => video.evaluate((element: HTMLVideoElement) => {
        const canvas = document.createElement('canvas');
        canvas.width = 640;
        canvas.height = 360;
        const context = canvas.getContext('2d')!;
        context.drawImage(element, 0, 0, 640, 360);
        const pixels = context.getImageData(0, 240, 640, 120).data;
        let white = 0;
        for (let i = 0; i < pixels.length; i += 4) {
          if (pixels[i] > 180 && pixels[i + 1] > 180 && pixels[i + 2] > 180) white++;
        }
        return white;
      }), { timeout: 20_000 }).toBeGreaterThan(20);
      await expect(root.getByRole('button', { name: 'Captions', exact: true })).toBeDisabled();
      expect(sqlite(`SELECT "IsExtracted", "Path" IS NULL FROM "Subtitle"
        WHERE "MovieMetadataId"=${channel.id + 104};`)).toBe('0|1');
      expect(errors).toEqual([]);
    } finally {
      try {
        await stopFixtureTranscode(page, channel.number);
      } finally {
        sqlite(`DELETE FROM "ConfigElement" WHERE "Key" IN (${keys.map(sqlString).join(',')});\n${restoreSettings}`);
      }
    }
  });

  test('Next converted captions are authenticated and toggle real browser cues', async ({ page }, testInfo) => {
    // Run the disposable host and this spec with ETV_NEXT_FOLDER and a synthetic
    // JWT__IssuerSigningKey so tokenless media requests exercise authentication.
    test.skip(!process.env.ETV_NEXT_FOLDER || !process.env.JWT__IssuerSigningKey,
      'Requires the real Next engine and JWT authentication on the disposable host');
    test.setTimeout(120_000);
    const clipPath = makeCalibrationClip(testInfo);
    const subtitlePath = testInfo.outputPath('next-preview.nl.srt');
    writeFileSync(subtitlePath, '1\n00:00:00,000 --> 00:01:10,000\nNext browser caption proof\n');
    const channel = seedChannel(testInfo, clipPath, 9);
    sqlite(`
UPDATE "Channel" SET "StreamingEngine"=1, "StreamingMode"=4,
  "NextEngineTextSubtitleMode"=1, "SubtitleMode"=2,
  "PreferredSubtitleLanguageCode"='nld' WHERE "Id"=${channel.id};
UPDATE "PlayoutItem" SET "OutPoint"='00:01:00' WHERE "PlayoutId"=${channel.id + 1};
INSERT INTO "Subtitle" ("MovieMetadataId", "SubtitleKind", "Title", "StreamIndex",
  "Codec", "Default", "Forced", "SDH", "Language", "IsExtracted", "Path", "DateAdded", "DateUpdated")
VALUES (${channel.id + 104}, 1, 'Dutch', 0, 'subrip', 1, 0, 0, 'nld', 0,
  ${sqlString(subtitlePath)}, '2026-09-23 00:00:00', '2026-09-23 00:00:00');
`);
    await forceEnglish(page);
    const errors: string[] = [];
    page.on('pageerror', (error) => errors.push(error.message));
    const masterResponse = page.waitForResponse((response) =>
      response.url().includes(`/iptv/preview/${channel.number}/master.m3u8`)
      && response.status() === 200);
    await page.goto(`/channels/${channel.id}/watch`);
    await waitForHydration(page);
    const master = await masterResponse;
    const masterText = await master.text();
    expect(masterText).toContain('SUBTITLES="subs"');
    const rendition = masterText.split('\n').find((line) => line.startsWith('#EXT-X-MEDIA:TYPE=SUBTITLES'));
    const renditionPath = rendition?.match(/URI="([^"]+)"/)?.[1];
    expect(renditionPath).toBeTruthy();
    const renditionUrl = new URL(renditionPath!, master.url());
    expect(renditionUrl.searchParams.has('access_token')).toBe(true);
    const unauthenticated = new URL(renditionUrl);
    unauthenticated.search = '';
    expect((await page.request.get(unauthenticated.href)).status()).toBe(401);
    let subtitlePlaylist = '';
    await expect.poll(async () => {
      const response = await page.request.get(renditionUrl.href);
      if (!response.ok()) return false;
      subtitlePlaylist = await response.text();
      return subtitlePlaylist.split('\n').some((line) => line.trim() && !line.startsWith('#'));
    }, { timeout: 30_000 }).toBe(true);
    const segment = subtitlePlaylist.split('\n').find((line) => line.trim() && !line.startsWith('#'))!;
    const segmentUrl = new URL(segment.trim(), renditionUrl);
    expect(segmentUrl.searchParams.has('access_token')).toBe(true);
    const vtt = await page.request.get(segmentUrl.href);
    expect(vtt.ok()).toBe(true);
    expect(await vtt.text()).toContain('Next browser caption proof');
    segmentUrl.search = '';
    expect((await page.request.get(segmentUrl.href)).status()).toBe(401);

    const root = page.locator('.channel-watch-js [data-channel-preview]');
    const video = root.locator('video');
    const captions = root.getByRole('button', { name: 'Captions', exact: true });
    await expect(root).toHaveAttribute('data-preview-engine', 'hlsjs');
    await expect(captions).toBeEnabled({ timeout: 30_000 });
    if (await captions.getAttribute('aria-pressed') !== 'true') await captions.click();
    await expect.poll(() => video.evaluate((element: HTMLVideoElement) =>
      Array.from(element.textTracks).some((track) => track.mode === 'showing'
        && Array.from(track.activeCues ?? []).some((cue) =>
          (cue as VTTCue).text.includes('Next browser caption proof')))),
    { timeout: 30_000 }).toBe(true);
    await captions.click();
    await expect(captions).toHaveAttribute('aria-pressed', 'false');
    await expect.poll(() => video.evaluate((element: HTMLVideoElement) =>
      Array.from(element.textTracks).some((track) => track.mode === 'showing'))).toBe(false);
    await captions.click();
    await expect(captions).toHaveAttribute('aria-pressed', 'true');
    await expect.poll(() => video.evaluate((element: HTMLVideoElement) =>
      Array.from(element.textTracks).some((track) => track.mode === 'showing'
        && (track.activeCues?.length ?? 0) > 0))).toBe(true);
    await root.getByRole('button', { name: 'Stop Preview', exact: true }).click();
    await expect(captions).toBeDisabled();
    await expect(captions).toHaveAttribute('aria-pressed', 'false');
    await stopFixtureTranscode(page, channel.number);
    expect(errors).toEqual([]);
  });

  test('shows the real transcode error when decoded preview video is an error slate', async ({ page }, testInfo) => {
    test.setTimeout(120_000);
    const clipPath = makeCalibrationClip(testInfo, 'color=c=blue:s=640x360:r=30');
    const channel = seedChannel(testInfo, clipPath, 5);
    const elementId = channel.id + 500;
    const config = JSON.stringify({ kind: 'script', command: "return 'deliberately_invalid_vgs_command'", args: [] });
    sqlite(`
      INSERT INTO "GraphicsElement" ("Id", "Kind", "Name", "Config")
      VALUES (${elementId}, 4, 'Synthetic invalid drawvg', ${sqlString(config)});
      INSERT INTO "PlayoutItemGraphicsElement" ("PlayoutItemId", "GraphicsElementId")
      SELECT "Id", ${elementId} FROM "PlayoutItem" WHERE "PlayoutId" = ${channel.id + 1};
    `);
    await forceEnglish(page);
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => {
      if (/failed_to_cast_marker_node|hydration/i.test(message.text()) && message.type() === 'error') errors.push(message.text());
    });
    try {
      await page.goto(`/channels/${channel.id}/watch`);
      await waitForHydration(page);
      const root = page.locator('[data-channel-preview]');
      await expect(root).toHaveAttribute('data-preview-state', 'live', { timeout: 60_000 });
      await expect(root).toHaveAttribute('data-preview-output', 'error-slate', { timeout: 20_000 });
      const warning = root.locator('[data-preview-output-error]');
      await expect(warning).toBeVisible();
      await expect(warning).toContainText('Error slate: programme playback failed.');
      await expect(warning).toContainText(/drawvg|deliberately_invalid_vgs_command|Invalid argument/i);
      await expect(root.locator('[data-preview-output-unknown]')).not.toBeVisible();
      expect(await root.locator('video').evaluate((video: HTMLVideoElement) => video.readyState)).toBeGreaterThanOrEqual(2);
      // Decode/playback success is intentionally simultaneous with a failed programme.
      await warning.scrollIntoViewIfNeeded();
      await page.screenshot({ path: testInfo.outputPath('channel-error-slate.png'), fullPage: true, animations: 'disabled' });
      const statusRoute = '**/admin-api/get_channel_preview_output*';
      await page.route(statusRoute, route => route.abort());
      await expect(root).toHaveAttribute('data-preview-output', 'unknown', { timeout: 10_000 });
      await expect(root.locator('[data-preview-output-unknown]')).toBeVisible();
      await expect(warning).not.toBeVisible();
      await page.unroute(statusRoute);
      await expect(root).toHaveAttribute('data-preview-output', 'error-slate', { timeout: 10_000 });
      await page.request.post('/admin-api/settings/ui/save', {
        form: { is_dark_mode: 'true', language: 'nl' }, maxRedirects: 0,
      });
      await page.reload();
      await waitForHydration(page);
      await expect(root).toHaveAttribute('data-preview-output', 'error-slate', { timeout: 30_000 });
      await expect(warning).toContainText('Foutbeeld: het programma kan niet worden afgespeeld.');
      await page.setViewportSize({ width: 390, height: 844 });
      await expect.poll(() => warning.evaluate(element => {
        const rect = element.getBoundingClientRect();
        return rect.left >= 0 && rect.right <= window.innerWidth
          && document.documentElement.scrollWidth <= window.innerWidth;
      })).toBe(true);
      await warning.scrollIntoViewIfNeeded();
      await page.screenshot({ path: testInfo.outputPath('channel-error-slate-mobile.png'), fullPage: true, animations: 'disabled' });
      expect(errors).toEqual([]);
    } finally {
      await forceEnglish(page);
      await page.goto(`/channels/${channel.id}/watch`);
      await waitForHydration(page);
      await page.getByRole('button', { name: 'Stop Preview', exact: true }).click();
      await stopFixtureTranscode(page, channel.number);
    }
  });

  test('plays a real H.264/AAC preview derivative through Hls.js', async ({
    page,
  }, testInfo) => {
    test.setTimeout(120_000);
    const clipPath = makeCalibrationClip(testInfo);
    const channel = seedChannel(testInfo, clipPath);
    await forceEnglish(page);

    const fatalErrors: string[] = [];
    const previewRequests = { variant: 0, segments: 0 };
    page.on('console', (message) => {
      if (message.type() === 'error') fatalErrors.push(message.text());
    });
    page.on('pageerror', (error) => fatalErrors.push(error.message));
    page.on('response', (response) => {
      const url = response.url();
      if (url.includes(`/iptv/preview/${channel.number}/hls.m3u8`)) {
        previewRequests.variant += 1;
      } else if (
        url.includes(`/iptv/preview/${channel.number}/`) &&
        !url.includes('master.m3u8')
      ) {
        previewRequests.segments += 1;
      }
    });

    const previewRoot = page.locator('[data-channel-preview]');
    try {
      await page.goto(`/channels/${channel.id}/watch`);
      await waitForHydration(page);
      await page.evaluate(() => {
        const root = document.querySelector('[data-channel-preview]');
        (window as Window & { __previewFatalStates?: unknown[] }).__previewFatalStates = [];
        root?.addEventListener('ersatzrs-preview-state', ((event: CustomEvent) => {
          if (['network', 'media', 'unsupported'].includes(event.detail.state)) {
            (window as Window & { __previewFatalStates: unknown[] }).__previewFatalStates.push(
              event.detail,
            );
          }
        }) as EventListener);
      });

      const video = previewRoot.locator('video');
      await expect(video).toHaveAttribute('autoplay', '');
      await expect.poll(() => video.evaluate((element: HTMLVideoElement) => element.muted)).toBe(true);
      await expect(previewRoot).toHaveAttribute('data-preview-engine', 'hlsjs', {
        timeout: 30_000,
      });
      await expect
        .poll(() => video.evaluate((element: HTMLVideoElement) => element.readyState), {
          timeout: 60_000,
        })
        .toBeGreaterThanOrEqual(2);
      const startTime = await video.evaluate((element: HTMLVideoElement) => element.currentTime);
      await expect
        .poll(() => video.evaluate((element: HTMLVideoElement) => element.currentTime), {
          timeout: 20_000,
        })
        .toBeGreaterThanOrEqual(startTime + 2);
      await expect(previewRoot).toHaveAttribute('data-preview-state', 'live');
      await expect(previewRoot).toHaveAttribute('data-preview-output', 'stream', { timeout: 15_000 });
      await expect(previewRoot.locator('[data-preview-output-error]')).not.toBeVisible();
      expect(previewRequests.variant).toBeGreaterThan(0);
      expect(previewRequests.segments).toBeGreaterThan(0);
      expect(
        await page.evaluate(
          () =>
            (window as Window & { __previewFatalStates?: unknown[] }).__previewFatalStates ?? [],
        ),
      ).toEqual([]);
      expect(fatalErrors).toEqual([]);
    } finally {
      await previewRoot.getByRole('button', { name: 'Stop Preview', exact: true }).click();
      await stopFixtureTranscode(page, channel.number);
    }
  });
});

async function captureJpegXlStream(ffmpeg: string, url: string, output: string) {
  const started = Date.now();
  const samples: { wallSeconds: number; mediaSeconds: number }[] = [];
  const child = spawn(ffmpeg, ['-nostdin', '-v', 'error', '-y', '-stats_period', '0.1',
    '-progress', 'pipe:1', '-analyzeduration', '1000000', '-probesize', '1000000',
    '-rw_timeout', '15000000', '-i', url, '-t', '8', '-c', 'copy', output],
    { stdio: ['ignore', 'pipe', 'pipe'] });
  let pending = '', stderr = '', expired = false;
  child.stdout.setEncoding('utf8'); child.stderr.setEncoding('utf8');
  child.stdout.on('data', (data: string) => {
    pending += data;
    let newline: number;
    while ((newline = pending.indexOf('\n')) >= 0) {
      const line = pending.slice(0, newline); pending = pending.slice(newline + 1);
      if (line.startsWith('out_time_us=')) {
        const mediaSeconds = Number(line.slice('out_time_us='.length)) / 1_000_000;
        if (Number.isFinite(mediaSeconds) && mediaSeconds > 0) {
          samples.push({ wallSeconds: (Date.now() - started) / 1000, mediaSeconds });
        }
      }
    }
    if (pending.length > 65536 || samples.length > 512) child.kill('SIGKILL');
  });
  child.stderr.on('data', (data: string) => { stderr = (stderr + data).slice(-65536); });
  await new Promise<void>((resolve, reject) => {
    const timer = setTimeout(() => { expired = true; child.kill('SIGKILL'); }, 15000);
    child.once('error', (error) => { clearTimeout(timer); reject(error); });
    child.once('close', (code) => {
      clearTimeout(timer);
      if (expired) reject(new Error('native served JPEG XL capture exceeded 15 seconds'));
      else if (code !== 0) reject(new Error(`native capture exited ${code}: ${stderr}`));
      else resolve();
    });
  });
  return { wallSeconds: (Date.now() - started) / 1000, samples };
}

test('finding 09 decodes compact JPEG XL in a full native broadcast graph', async ({ page }, testInfo) => {
  test.skip(!dbPath, 'Requires an isolated migrated database');
  test.setTimeout(120_000);
  const ffmpeg = process.env.ErsatzRS_FFMPEG__PATH ?? 'ffmpeg';
  const clip = testInfo.outputPath('jxl-programme-1080p.mp4');
  const animation = testInfo.outputPath('compact-alpha-animation.jxl');
  // Real 1080p H.264/AAC decoding and three full-frame alpha carriers exercise
  // the complete production graph, including the area that made geq expensive.
  execFileSync(ffmpeg, ['-v', 'error', '-y', '-f', 'lavfi', '-i',
    'testsrc2=s=1920x1080:r=25,drawbox=x=0:y=0:w=300:h=150:color=black:t=fill',
    '-f', 'lavfi', '-i', 'sine=frequency=440:sample_rate=48000', '-t', '15',
    '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p', '-c:a', 'aac', clip],
    { timeout: 15000 });
  execFileSync(ffmpeg, ['-v', 'error', '-y', '-f', 'lavfi', '-i',
    'color=c=black@0:s=1920x1080:r=25:duration=1,format=rgba', '-vf',
    "drawtext=fontfile=assets/fonts/Roboto-Regular.ttf:text='%{n}':x=30:y=30:fontsize=90:fontcolor=white,colorchannelmixer=aa=0.5",
    '-c:v', 'libjxl_anim', '-effort', '1', '-distance', '0', '-f', 'image2pipe', animation],
    { timeout: 15000 });
  const channel = seedChannel(testInfo, clip, 29);
  const anchor = new Date(Date.now() - 2450);
  const playout = channel.id + 1;
  const graphic = channel.id + 500;
  const config = JSON.stringify({ kind: 'motion', video_path: animation,
    start_seconds: 0, hold_seconds: 0, end_behavior: 'Loop' });
  sqlite(`
INSERT INTO "Resolution" ("Id","Width","Height","Name") VALUES (${channel.id + 201},1920,1080,'JPEG XL proof');
UPDATE "Channel" SET "StreamingMode"=1 WHERE "Id"=${channel.id};
UPDATE "FFmpegProfile" SET "ResolutionId"=${channel.id + 201},"HardwareAcceleration"=0,"VideoPreset"='ultrafast',"VideoBitrate"=8000,"NormalizeFramerate"=1
 WHERE "Id"=${channel.id + 200};
UPDATE "MediaVersion" SET "Width"=1920,"Height"=1080,"RFrameRate"='25/1',"Duration"='00:00:15' WHERE "Id"=${channel.id + 105};
UPDATE "PlayoutItem" SET "Start"='${dbTime(anchor)}',"Finish"='${dbTime(new Date(anchor.getTime()+60000))}',"InPoint"='00:00:00',"OutPoint"='00:01:00' WHERE "Id"=${channel.id + 2};
UPDATE "PlayoutItem" SET "Start"='${dbTime(new Date(anchor.getTime()+60000))}',"Finish"='${dbTime(new Date(anchor.getTime()+120000))}' WHERE "Id"=${channel.id + 3};
${[0,1,2].map((index) => `
INSERT INTO "GraphicsElement" ("Id","Name","Kind","Config","Location","HorizontalMarginPercent","VerticalMarginPercent","OpacityExpression","ZIndex")
 VALUES (${graphic+index},'Native JXL carrier ${index}',3,${sqlString(config)},0,0,0,'content_seconds < 7.5',${index});
INSERT INTO "PlayoutItemGraphicsElement" ("PlayoutItemId","GraphicsElementId") VALUES (${channel.id+2},${graphic+index});`).join('\n')}
`);
  await page.goto(`/channels/${channel.id}`);
  await expect(page.locator('input[name="name"]')).toHaveValue(channel.name);
  // The existing MPEG troubleshooting report identifies the exact private
  // HTTP producer used by ordinary IPTV. Keep public playback proof too: its
  // wrapper deliberately paces delivery at 1x and cannot measure spare capacity.
  const diagnostic = await page.request.get(`/system/troubleshooting/mpeg?channel=${channel.number}&mode=internal-legacy&duration=1`);
  expect(diagnostic.ok()).toBe(true);
  const producerUrl = (await diagnostic.text()).match(/http:\/\/127\.0\.0\.1:\d+\/ffmpeg\/stream\/\d+/)?.[0];
  expect(producerUrl, 'existing native MPEG report must expose its real sample URL').toBeTruthy();
  const base = process.env.ERSATZRS_BASE_URL ?? 'http://127.0.0.1:8409';
  for (const [label, url] of [
    ['producer', producerUrl!],
    ['iptv', `${base}/iptv/channel/${channel.number}.ts?mode=ts-legacy`],
  ]) {
    const actualAnchor = new Date(Date.now() - 2450);
    sqlite(`UPDATE "PlayoutItem" SET "Start"='${dbTime(actualAnchor)}',"Finish"='${dbTime(new Date(actualAnchor.getTime()+60000))}' WHERE "Id"=${channel.id+2};
  UPDATE "PlayoutItem" SET "Start"='${dbTime(new Date(actualAnchor.getTime()+60000))}',"Finish"='${dbTime(new Date(actualAnchor.getTime()+120000))}' WHERE "Id"=${channel.id+3};`);
    const before = programmeTimeline(playout);
    const capture = testInfo.outputPath(`served-jxl-${label}-1080p.mkv`);
    const { wallSeconds, samples } = await captureJpegXlStream(
      ffmpeg, url, capture,
    );
    writeFileSync(testInfo.outputPath(`served-jxl-${label}-progress.json`), JSON.stringify(samples, null, 2));
    execFileSync(ffmpeg, ['-v', 'error', '-i', capture, '-f', 'null', '-'], { timeout: 15000 });
    const frames = execFileSync(ffmpeg, ['-v', 'error', '-i', capture,
      '-vf', 'crop=300:150:0:0', '-pix_fmt', 'rgb24', '-f', 'rawvideo', 'pipe:1'],
      { timeout: 15000, maxBuffer: 64*1024*1024 });
    const frameBytes = 300*150*3;
    expect(frames.length % frameBytes).toBe(0);
    const count = frames.length / frameBytes;
    expect(count).toBeGreaterThanOrEqual(195);
    const peaks: number[] = [], white: number[] = [];
    for (let index=0; index<count; index++) {
      const data = frames.subarray(index*frameBytes,(index+1)*frameBytes);
      let peak=0, bright=0;
      for (let pixel=0;pixel<data.length;pixel+=3) { peak=Math.max(peak,data[pixel]); if(data[pixel]>170&&data[pixel+1]>170&&data[pixel+2]>170) bright++; }
      peaks.push(peak); white.push(bright);
    }
    const sourceFrames = execFileSync(ffmpeg,['-v','error','-i',animation,
      '-vf','crop=300:150:0:0','-pix_fmt','rgba','-f','rawvideo','pipe:1'],{timeout:15000,maxBuffer:8*1024*1024});
    expect(sourceFrames.length).toBe(25*300*150*4);
    const scores = Array.from({length:25},(_,frame) => {
      let error=0;
      for(let pixel=0;pixel<300*150;pixel++) {
        const sourcePixel=frame*300*150*4+pixel*4;
        const alpha=sourceFrames[sourcePixel+3]/255;
        const predicted=sourceFrames[sourcePixel]*(1-Math.pow(1-alpha,3));
        error+=Math.abs(frames[pixel*3]-predicted);
      }
      return error/(300*150);
    });
    const firstSourceFrame = scores.indexOf(Math.min(...scores));
    expect(firstSourceFrame, 'ordinary native playback must join the running JXL phase instead of restarting frame zero').not.toBe(0);
    expect(firstSourceFrame).toBeGreaterThanOrEqual(10);
    expect(Math.min(...scores), 'decoded served glyph must match an actual source animation frame').toBeLessThan(5);
    expect(Math.max(...peaks.slice(0,40))).toBeGreaterThan(180);
    // H.264 ringing can brighten an isolated pixel. Check the matched white
    // glyph's mean compositing error to distinguish transparency from opacity.
    let foregroundPixels = 0, foregroundError = 0;
    for (let pixel=0;pixel<300*150;pixel++) {
      const sourcePixel = firstSourceFrame*300*150*4+pixel*4;
      if (sourceFrames[sourcePixel] > 250 && sourceFrames[sourcePixel+1] > 250 &&
          sourceFrames[sourcePixel+2] > 250 && sourceFrames[sourcePixel+3] >= 127) {
        const alpha = sourceFrames[sourcePixel+3]/255;
        const predicted = sourceFrames[sourcePixel]*(1-Math.pow(1-alpha,3));
        foregroundError += Math.abs(frames[pixel*3]-predicted);
        foregroundPixels++;
      }
    }
    expect(foregroundPixels, 'matched source frame must contain a white glyph').toBeGreaterThan(0);
    expect(foregroundError/foregroundPixels, 'served glyph must preserve the three-layer alpha blend').toBeLessThan(5);
    expect(new Set(white.slice(0,40)).size).toBeGreaterThan(5);
    expect(Math.max(...peaks.slice(-40))).toBeLessThan(35);
    // Measure live delivery while all three carriers are active. The total
    // request time separately includes bounded probes and legacy demuxer startup;
    // it is startup latency, not the running graph's sustainable throughput.
    const activeSamples = samples.filter((sample) => sample.mediaSeconds >= 0.5 && sample.mediaSeconds <= 4.5);
    expect(activeSamples.length).toBeGreaterThanOrEqual(6);
    const firstSample = activeSamples[0], lastSample = activeSamples.at(-1)!;
    const measuredMediaSeconds = lastSample.mediaSeconds - firstSample.mediaSeconds;
    expect(measuredMediaSeconds).toBeGreaterThanOrEqual(2.5);
    const speed = measuredMediaSeconds / (lastSample.wallSeconds - firstSample.wallSeconds);
    if (label === 'producer') {
      expect(speed, 'actual full native HTTP graph capacity with all three carriers active').toBeGreaterThanOrEqual(1);
    } else {
      // The public wrapper uses -readrate 1.0. At 25 fps, reported media progress
      // is quantized to 40 ms; require realtime delivery within one output frame.
      expect(measuredMediaSeconds + 1 / 25).toBeGreaterThanOrEqual(lastSample.wallSeconds - firstSample.wallSeconds);
    }
    expect(programmeTimeline(playout)).toBe(before);
    for(const [time,state] of [['0.4','visible'],['7.2','clear']]) {
      const image = testInfo.outputPath(`served-jxl-${label}-${state}.png`);
      execFileSync(ffmpeg,['-v','error','-y','-ss',time,'-i',capture,'-frames:v','1',image],{timeout:15000});
      expect(existsSync(image)).toBe(true);
    }
    writeFileSync(testInfo.outputPath(`served-jxl-${label}-proof.json`),JSON.stringify({
      route:label, resolution:'1920x1080', carriers:3, sourceFormat:'compact JPEG XL', requestedJoinSeconds:2.45,
      decodedFrames:count, wallSeconds, coldRequestMediaRatio:(count/25)/wallSeconds,
      initialMediaLatencySeconds:samples[0]?.wallSeconds, measuredMediaSeconds, completeGraphSpeed:speed, progress:samples, firstSourceFrame, sourceFrameScores:scores, firstPeaks:peaks.slice(0,40),
      lastPeaks:peaks.slice(-40), timelinePreserved:true },null,2));
  }
});
