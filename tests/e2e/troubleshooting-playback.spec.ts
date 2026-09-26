import { expect, test } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';

const db = process.env.E2E_DB_PATH;
const id = 935_001;
const quote = (value: string) => value.replaceAll("'", "''");
function sql(statement: string): string {
  if (!db) throw new Error('Use the isolated Playwright wrapper (E2E_DB_PATH required)');
  return execFileSync('sqlite3', [db], {
    input: `.timeout 10000\nPRAGMA foreign_keys=ON;\n${statement}`, encoding: 'utf8',
  }).trim();
}

test.describe('playback troubleshooting engines', () => {
  test.describe.configure({ mode: 'serial' });
  let root: string;
  let profile: string;
  let selectorFile: string;
  test.beforeAll(() => {
    if (!process.env.E2E_APP_STATE_DIR) throw new Error('Isolated E2E_APP_STATE_DIR required');
    root = mkdtempSync(path.join(tmpdir(), 'ersatzrs-preview-e2e-'));
    const selectorFolder = path.join(process.env.E2E_APP_STATE_DIR, 'data', 'scripts', 'channel-stream-selectors');
    mkdirSync(selectorFolder, { recursive: true });
    selectorFile = path.join(selectorFolder, 'preview-fixture.yml');
    writeFileSync(selectorFile, 'items:\n  - audio_language: [nld]\n    subtitle_language: [nld]\n');
    const clip = path.join(root, 'preview.mkv');
    const subtitle = path.join(root, 'preview.nl.srt');
    const ffmpeg = process.env.ErsatzRS_FFMPEG__PATH ?? process.env.ERSATZRS_FFMPEG__PATH
      ?? process.env.FFMPEG_PATH ?? 'ffmpeg';
    execFileSync(ffmpeg, ['-hide_banner', '-loglevel', 'error', '-y',
      '-f', 'lavfi', '-i', 'color=black:s=320x180:r=25:d=6',
      '-f', 'lavfi', '-i', 'sine=frequency=880:sample_rate=48000:duration=6',
      '-map', '0:v', '-map', '1:a', '-c:v', 'libx264', '-preset', 'ultrafast',
      '-pix_fmt', 'yuv420p', '-c:a', 'aac', '-metadata:s:a:0', 'language=nld',
      '-metadata:s:a:0', 'title=Dutch', clip]);
    writeFileSync(subtitle, '1\n00:00:01,000 --> 00:00:06,000\nPreview subtitle proof\n');
    for (const color of ['red', 'lime']) {
      execFileSync(ffmpeg, ['-hide_banner', '-loglevel', 'error', '-y',
        '-f', 'lavfi', '-i', `color=${color}:s=40x20`, '-frames:v', '1',
        '-threads', '1', path.join(root, `${color}.png`)]);
    }
    profile = `${id}`;
    sql(`
INSERT INTO "Resolution" ("Id","Name","Width","Height") VALUES (${id},'Preview fixture',320,180);
INSERT INTO "FFmpegProfile" (
 "Id","Name","ThreadCount","TranscodeAudio","TranscodeVideo","HardwareAcceleration",
 "VaapiDisplay","VaapiDriver","VaapiDevice","QsvExtraHardwareFrames","ResolutionId",
 "ScalingBehavior","PadMode","VideoFormat","VideoProfile","VideoPreset","AllowBFrames",
 "BitDepth","VideoBitrate","VideoBufferSize","TonemapAlgorithm","AudioFormat",
 "AudioBitrate","AudioBufferSize","NormalizeLoudnessMode","TargetLoudness","AudioChannels",
 "AudioSampleRate","NormalizeFramerate","NormalizeColors","DeinterlaceVideo")
SELECT ${id},'Preview fixture',1,1,1,0,
 "VaapiDisplay","VaapiDriver","VaapiDevice","QsvExtraHardwareFrames",${id},
 "ScalingBehavior","PadMode",1,'high','ultrafast',0,
 0,500,1000,"TonemapAlgorithm",1,
 128,256,0,NULL,2,48,0,0,0
FROM "FFmpegProfile" ORDER BY "Id" LIMIT 1;
INSERT INTO "MediaSource" ("Id") VALUES (${id});
INSERT INTO "LocalMediaSource" ("Id") VALUES (${id});
INSERT INTO "Library" ("Id","Name","MediaKind","MediaSourceId") VALUES (${id},'Preview fixture',1,${id});
INSERT INTO "LocalLibrary" ("Id") VALUES (${id});
INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (${id},${id},'${quote(root)}');
INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (${id},${id},0);
INSERT INTO "Movie" ("Id") VALUES (${id});
INSERT INTO "MovieMetadata" ("Id","MovieId","Title","MetadataKind","DateAdded","DateUpdated")
VALUES (${id},${id},'Preview fixture',0,'2026-09-26 00:00:00','2026-09-26 00:00:00');
INSERT INTO "MediaVersion" ("Id","MovieId","Duration","Width","Height","VideoScanKind","DateAdded","DateUpdated")
VALUES (${id},${id},'00:00:06',320,180,0,'2026-09-26 00:00:00','2026-09-26 00:00:00');
INSERT INTO "MediaFile" ("Id","MediaVersionId","Path","PathHash") VALUES (${id},${id},'${quote(clip)}','preview-fixture');
INSERT INTO "MediaStream" ("Index","Codec","MediaStreamKind","Language","Channels","Title","Default","Forced","MediaVersionId")
VALUES (0,'h264',1,NULL,0,NULL,1,0,${id}),(1,'aac',2,'nld',1,'Dutch',1,0,${id});
INSERT INTO "Subtitle" ("Id","MovieMetadataId","SubtitleKind","Title","StreamIndex","Codec","Default","Forced","SDH","Language","IsExtracted","Path","DateAdded","DateUpdated")
VALUES (${id},${id},1,'Dutch',9,'subrip',1,0,0,'nld',0,'${quote(subtitle)}','2026-09-26 00:00:00','2026-09-26 00:00:00');
INSERT INTO "ChannelWatermark" ("Id","Name","DurationSeconds","FrequencyMinutes","HorizontalMarginPercent","ImageSource","Image","Location","Mode","Opacity","PlaceWithinSourceContent","Size","VerticalMarginPercent","WidthPercent","ZIndex")
VALUES (${id},'Preview watermark A',15,15,5,0,'${quote(path.join(root, 'red.png'))}',3,1,100,0,1,5,15,0),
(${id + 1},'Preview watermark B',15,15,5,0,'${quote(path.join(root, 'lime.png'))}',2,1,100,0,1,5,15,1);
`);
  });

  test.afterAll(() => {
    if (db) sql(`DELETE FROM "MediaSource" WHERE "Id"=${id};
DELETE FROM "ChannelWatermark" WHERE "Id" IN (${id},${id + 1});
DELETE FROM "FFmpegProfile" WHERE "Id"=${id};
DELETE FROM "Resolution" WHERE "Id"=${id};`);
    if (root) rmSync(root, { recursive: true, force: true });
    if (selectorFile) rmSync(selectorFile, { force: true });
  });

  test.beforeEach(async ({ page }) => {
    await page.request.post('/api/settings/ui/save', {
      form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0,
    });
  });

  test('switches engine controls, preserves all watermarks and fits narrow screens', async ({ page }, info) => {
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => {
      if (message.type() === 'error') errors.push(message.text());
    });
    await page.goto(`/system/troubleshooting/playback?mediaItem=${id}`);
    await page.waitForFunction(() =>
      (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true);
    const engine = page.getByRole('combobox', { name: 'Streaming Engine', exact: true });
    const watermarks = page.locator('select[name="watermark"]');
    const selector = page.locator('select[name="streamSelector"]');
    const disabled = await selector.isDisabled();
    expect(disabled).toBe(false);
    await selector.selectOption('preview-fixture.yml');
    await expect(engine).toHaveValue('Legacy');
    await expect(page.locator('select[name="graphicsElement"]')).toBeVisible();
    await watermarks.selectOption([`${id}`, `${id + 1}`]);
    await engine.selectOption('Next');
    await expect(selector).toBeVisible();
    expect(await selector.isDisabled()).toBe(disabled);
    await expect(selector).toHaveValue('preview-fixture.yml');
    await expect(page.locator('select[name="graphicsElement"]')).toBeHidden();
    await expect(watermarks).toHaveAttribute('multiple');
    await expect(watermarks).toHaveValues([`${id}`, `${id + 1}`]);
    expect(await page.locator('form[data-playback-troubleshooting]').evaluate(form =>
      new FormData(form as HTMLFormElement).getAll('watermark'),
    )).toEqual([`${id}`, `${id + 1}`]);
    expect(await page.locator('form[data-playback-troubleshooting]').evaluate(form =>
      Array.from(new FormData(form as HTMLFormElement).keys()),
    )).not.toContain('graphicsElement');
    await watermarks.selectOption('');
    await expect(watermarks).toHaveValue('');
    await engine.selectOption('Legacy');
    await expect(watermarks).toHaveAttribute('multiple');
    expect(await selector.isDisabled()).toBe(disabled);
    await expect(page.locator('select[name="graphicsElement"]')).toBeVisible();
    await page.setViewportSize({ width: 320, height: 720 });
    const navigation = page.getByRole('button', { name: 'Toggle navigation' });
    // Crossing the mobile breakpoint closes the drawer asynchronously. Clicking
    // from its old desktop state can reopen it after that media-query event.
    await expect(navigation).toHaveAttribute('aria-expanded', 'false');
    await expect.poll(async () => {
      const drawer = await page.locator('[data-nav-drawer]').boundingBox();
      return drawer ? drawer.x + drawer.width : 0;
    }).toBeLessThanOrEqual(0);
    for (const field of await page.locator('form[data-playback-troubleshooting] select').all()) {
      const box = await field.boundingBox();
      expect(box).not.toBeNull();
      expect(box!.x).toBeGreaterThanOrEqual(0);
      expect(box!.x + box!.width).toBeLessThanOrEqual(320);
    }
    await page.screenshot({ path: `test-results/troubleshooting-controls-${info.project.name}.png`, fullPage: true });
    expect(errors).toEqual([]);
  });

  for (const engine of ['Legacy', 'Next']) {
    test(`${engine} plays in the page and provides reports`, async ({ page }, info) => {
      test.skip(engine === 'Next' && !process.env.ETV_NEXT_FOLDER,
        'Official Next runtime acceptance requires ETV_NEXT_FOLDER.');
      const errors: string[] = [];
      page.on('pageerror', error => errors.push(error.message));
      await page.goto(`/system/troubleshooting/playback?mediaItem=${id}&preferredAudioTitle=Dutch&subtitleId=${id}`);
      await page.getByRole('combobox', { name: 'Streaming Engine', exact: true }).selectOption(engine);
      await page.locator('select[name="ffmpegProfile"]').selectOption(profile);
      await page.locator('select[name="streamSelector"]').selectOption('preview-fixture.yml');
      await page.locator('input[name="seekSeconds"]').fill('1');
      await page.locator('select[name="watermark"]').selectOption([`${id}`, `${id + 1}`]);
      const requests: string[] = [];
      page.on('request', request => {
        if (new URL(request.url()).pathname === '/api/troubleshoot/playback.m3u8') requests.push(request.url());
      });
      await page.getByRole('button', { name: 'Play', exact: true }).click();
      await expect.poll(() => page.locator('video').evaluate((video: HTMLVideoElement) => video.currentTime)).toBeGreaterThan(0);
      await expect(page.locator('[data-channel-preview]')).toHaveAttribute('data-preview-engine', 'hlsjs');
      expect(requests).toHaveLength(1);
      const query = new URL(requests[0]).searchParams;
      expect(query.get('streamingEngine')).toBe(engine);
      expect(query.get('streamSelector')).toBe('preview-fixture.yml');
      expect(query.get('subtitleId')).toBe(`${id}`);
      expect(query.get('preferredAudioTitle')).toBe('Dutch');
      expect(query.get('seekSeconds')).toBe('1');
      expect(query.getAll('watermark')).toEqual([`${id}`, `${id + 1}`]);
      await expect.poll(() => page.locator('video').evaluate((video: HTMLVideoElement) => {
        const canvas = document.createElement('canvas');
        canvas.width = 320;
        canvas.height = 180;
        const context = canvas.getContext('2d')!;
        context.drawImage(video, 0, 0, 320, 180);
        const red = context.getImageData(25, 15, 1, 1).data;
        const green = context.getImageData(285, 15, 1, 1).data;
        return {
          red: red[0] > 180 && red[1] < 50 && red[2] < 50,
          green: green[1] > 180 && green[0] < 50 && green[2] < 50,
        };
      })).toEqual({ red: true, green: true });
      await expect(page).toHaveURL(/\/system\/troubleshooting\/playback/);
      const playlist = engine === 'Next' ? 'ffmpeg.m3u8' : 'live.m3u8';
      await expect.poll(async () => {
        const response = await page.request.get(`/iptv/session/.troubleshooting/${playlist}`);
        return (await response.text()).includes('#EXT-X-ENDLIST');
      }).toBe(true);
      const archive = await page.request.get('/api/troubleshoot/playback/archive');
      expect(archive.status()).toBe(200);
      expect(archive.headers()['content-type']).toContain('application/zip');
      expect((await archive.body()).subarray(0, 2).toString()).toBe('PK');
      await expect(page.locator('textarea')).toHaveValue(
        engine === 'Next' ? /Next troubleshooting completed/ : /exit status:/,
      );
      if (engine === 'Next') {
        await expect(page.locator('textarea')).toHaveValue(/\[selection\]/);
        await expect(page.locator('textarea')).toHaveValue(/Custom selector rule 1: audio matched=true, subtitle matched=true/);
      }
      await expect(page.getByRole('button', { name: 'Play', exact: true })).toBeEnabled();
      await expect(page.locator('[data-preview-speed]')).toBeVisible();
      expect(Number(await page.locator('[data-preview-speed] output').textContent())).toBeGreaterThan(0);
      await page.locator('[data-preview-speed]').scrollIntoViewIfNeeded();
      await page.screenshot({
        path: `test-results/troubleshooting-speed-${engine.toLowerCase()}-${info.project.name}.png`,
        fullPage: true,
      });
      expect(errors).toEqual([]);
    });
  }

  test('no-JavaScript form submits the selected engine, tracks and all watermarks', async ({ browser, baseURL }) => {
    const context = await browser.newContext({ baseURL, javaScriptEnabled: false });
    try {
      const page = await context.newPage();
      let submitted: URL | undefined;
      // Capture the native browser's GET serialization; runtime execution is
      // covered above without interception.
      await page.route('**/api/troubleshoot/playback.m3u8?**', route => {
        submitted = new URL(route.request().url());
        return route.fulfill({ contentType: 'text/plain', body: 'Captured preview submission' });
      });
      await page.goto(`/system/troubleshooting/playback?mediaItem=${id}&subtitleId=${id}`);
      await page.getByRole('combobox', { name: 'Streaming Engine', exact: true }).selectOption('Next');
      await page.locator('select[name="streamSelector"]').selectOption('preview-fixture.yml');
      await page.locator('input[name="startFromBeginning"]').check();
      await page.locator('select[name="watermark"]').selectOption([`${id}`, `${id + 1}`]);
      await page.getByRole('button', { name: 'Play', exact: true }).click();
      await expect(page.locator('body')).toContainText('Captured preview submission');
      expect(submitted?.searchParams.get('streamingEngine')).toBe('Next');
      expect(submitted?.searchParams.get('streamSelector')).toBe('preview-fixture.yml');
      expect(submitted?.searchParams.get('subtitleId')).toBe(`${id}`);
      expect(submitted?.searchParams.get('startFromBeginning')).toBe('true');
      expect(submitted?.searchParams.getAll('watermark')).toEqual([`${id}`, `${id + 1}`]);
    } finally {
      await context.close();
    }
  });
});
