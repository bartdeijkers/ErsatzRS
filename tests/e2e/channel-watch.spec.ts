import { expect, Locator, Page, TestInfo, test } from '@playwright/test';
import { execFileSync } from 'node:child_process';

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
  await page.locator('[role="grid"]').evaluate((element, requested) => {
    const timeline = element as HTMLElement;
    const inlineScrollBehavior = timeline.style.scrollBehavior;
    timeline.style.scrollBehavior = 'auto';
    timeline.scrollLeft = requested;
    timeline.style.scrollBehavior = inlineScrollBehavior;
  }, scrollLeft);
}

async function timelineCenterMinutes(page: Page, zoomPercent: number): Promise<number> {
  return page.locator('[role="grid"]').evaluate((element, zoom) => {
    const timeline = element as HTMLElement;
    const timelineViewport = Math.max(timeline.clientWidth - 352, 0);
    const pixelsPerMinute = (6 * zoom) / 100;
    return (timeline.scrollLeft + timelineViewport / 2) / pixelsPerMinute;
  }, zoomPercent);
}

function makeCalibrationClip(testInfo: TestInfo): string {
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
      'testsrc2=size=640x360:rate=30',
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
DELETE FROM "MediaStream" WHERE "MediaVersionId" = ${mediaVersionId};
DELETE FROM "MediaFile" WHERE "MediaVersionId" = ${mediaVersionId};
DELETE FROM "MediaVersion" WHERE "Id" = ${mediaVersionId};
DELETE FROM "MovieMetadata" WHERE "MovieId" = ${mediaItemId};
DELETE FROM "Movie" WHERE "Id" = ${mediaItemId};
DELETE FROM "MediaItem" WHERE "Id" = ${mediaItemId};
DELETE FROM "LibraryPath" WHERE "Id" = ${libraryPathId};
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

function programmeStartMinutes(title: string): number {
  const match = title.match(/·\s*(\d{2}):(\d{2})/);
  if (!match) throw new Error(`programme title has no start time: ${title}`);
  return Number(match[1]) * 60 + Number(match[2]);
}

async function forceEnglish(page: Page): Promise<void> {
  await page.request.post('/api/settings/ui/save', {
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
  const nextPage = page.getByRole('button', { name: '›', exact: true });

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
    await setTimelineScroll(page, 360);
    const watchCenterBefore = await timelineCenterMinutes(page, 100);
    await zoom.fill('150');
    await expect
      .poll(async () =>
        Math.abs((await timelineCenterMinutes(page, 150)) - watchCenterBefore),
      )
      .toBeLessThan(1);
    await expect(page.getByText('Now Playing').first()).toBeVisible();
    await expect.poll(() => playlistRequests).toBeGreaterThan(modalPlaylistRequests);
    const watchVideo = page.locator('.channel-watch-js [data-channel-preview] video');
    await expect(watchVideo).toHaveAttribute('autoplay', '');
    await expect.poll(() => watchVideo.evaluate((video: HTMLVideoElement) => video.muted)).toBe(true);
    await page.getByRole('button', { name: 'Stop Preview' }).click();
    await expect(
      page.locator('.channel-watch-js [data-preview-status]'),
    ).toContainText('Preview stopped');

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
    await page.route('**/api/get_channel_preview_url*', async (route) => {
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
      response.url().includes('/api/get_channel_preview_url'),
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
    await page.route('**/api/get_channel_watch_data*', async (route) => {
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
      response.url().includes('/api/get_channel_watch_data'),
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
    await page.locator('select[name="playout_source"]').selectOption('1');
    await page
      .locator('select[name="mirror_source_channel_id"]')
      .selectOption(String(source.id));
    await page.locator('input[name="playout_offset_hours"]').fill('1');
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

    const previewRoot = page.locator('[data-channel-preview]');
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
    expect(previewRequests.variant).toBeGreaterThan(0);
    expect(previewRequests.segments).toBeGreaterThan(0);
    expect(
      await page.evaluate(
        () =>
          (window as Window & { __previewFatalStates?: unknown[] }).__previewFatalStates ?? [],
      ),
    ).toEqual([]);
    expect(fatalErrors).toEqual([]);
  });
});
