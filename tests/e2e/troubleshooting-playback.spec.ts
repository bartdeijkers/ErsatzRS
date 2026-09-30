import { expect, test } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';

const db = process.env.E2E_DB_PATH;
const id = 935_001;
const musicId = id + 10;
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
    const musicClip = path.join(root, 'music-video.mkv');
    execFileSync(ffmpeg, ['-hide_banner', '-loglevel', 'error', '-y',
      '-f', 'lavfi', '-i', 'color=black:s=320x180:r=25:d=16',
      '-f', 'lavfi', '-i', 'sine=frequency=880:sample_rate=48000:duration=16',
      '-map', '0:v', '-map', '1:a', '-c:v', 'libx264', '-preset', 'ultrafast',
      '-pix_fmt', 'yuv420p', '-c:a', 'aac', '-metadata:s:a:0', 'language=nld',
      '-metadata:s:a:0', 'title=Dutch', musicClip]);
    writeFileSync(subtitle, '1\n00:00:01,000 --> 00:00:06,000\nPreview subtitle proof\n');
    for (const color of ['red', 'lime', 'blue']) {
      execFileSync(ffmpeg, ['-hide_banner', '-loglevel', 'error', '-y',
        '-f', 'lavfi', '-i', `color=${color}:s=40x20`, '-frames:v', '1',
        '-threads', '1', path.join(root, `${color}.png`)]);
    }
    const motion = path.join(root, 'timed-motion.mkv');
    execFileSync(ffmpeg, ['-hide_banner', '-loglevel', 'error', '-y',
      '-f', 'lavfi', '-i', 'color=red:s=40x20:r=25:d=1',
      '-f', 'lavfi', '-i', 'color=blue:s=40x20:r=25:d=1',
      '-filter_complex', '[0:v][1:v]concat=n=2:v=1:a=0[v]',
      '-map', '[v]', '-c:v', 'ffv1', motion]);
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
INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (${musicId},${id},0),(${musicId + 1},${id},0);
INSERT INTO "Artist" ("Id") VALUES (${musicId + 1});
INSERT INTO "ArtistMetadata" ("Id","ArtistId","Title","MetadataKind","DateAdded","DateUpdated")
VALUES (${musicId + 1},${musicId + 1},'Synthetic credits artist',0,'2026-09-30 00:00:00','2026-09-30 00:00:00');
INSERT INTO "MusicVideo" ("Id","ArtistId") VALUES (${musicId},${musicId + 1});
INSERT INTO "MusicVideoMetadata" ("Id","MusicVideoId","Title","MetadataKind","DateAdded","DateUpdated")
VALUES (${musicId},${musicId},'Synthetic credits song',0,'2026-09-30 00:00:00','2026-09-30 00:00:00');
INSERT INTO "MediaVersion" ("Id","MusicVideoId","Duration","Width","Height","VideoScanKind","DateAdded","DateUpdated")
VALUES (${musicId},${musicId},'00:00:16',320,180,0,'2026-09-30 00:00:00','2026-09-30 00:00:00');
INSERT INTO "MediaFile" ("Id","MediaVersionId","Path","PathHash") VALUES (${musicId},${musicId},'${quote(musicClip)}','music-preview');
INSERT INTO "MediaStream" ("Id","Index","Codec","MediaStreamKind","Language","Channels","Title","Default","Forced","MediaVersionId")
VALUES (${musicId},0,'h264',1,NULL,0,NULL,1,0,${musicId}),(${musicId + 1},1,'aac',2,'nld',1,'Dutch',1,0,${musicId});
INSERT INTO "ChannelWatermark" ("Id","Name","DurationSeconds","FrequencyMinutes","HorizontalMarginPercent","ImageSource","Image","Location","Mode","Opacity","PlaceWithinSourceContent","Size","VerticalMarginPercent","WidthPercent","ZIndex")
VALUES (${id},'Preview watermark A',15,15,5,0,'${quote(path.join(root, 'red.png'))}',3,1,100,0,1,5,15,0),
(${id + 1},'Preview watermark B',15,15,5,0,'${quote(path.join(root, 'lime.png'))}',2,1,100,0,1,5,15,1);
INSERT INTO "GraphicsElement" ("Id","Name","Kind","ZIndex","Location","Scale","PlaceWithinSourceContent","EpgEntries","Config","HorizontalMarginPercent","VerticalMarginPercent","OpacityPercent")
VALUES (${id},'Preview image with programme information and broadcast branding',0,2,4,0,0,0,'${quote(JSON.stringify({ kind: 'image', image: path.join(root, 'blue.png') }))}',0,5,100);
INSERT INTO "GraphicsElement" ("Id","Name","Kind","ZIndex","Location","Scale","PlaceWithinSourceContent","EpgEntries","Config","HorizontalMarginPercent","VerticalMarginPercent","OpacityPercent")
VALUES (${id + 20},'Timed motion hold',3,2,3,0,0,0,'${quote(JSON.stringify({ kind: 'motion', video_path: motion, start_seconds: 1, hold_seconds: 2, end_behavior: 'Hold' }))}',0,0,100);
INSERT INTO "GraphicsElement" ("Id","Name","Kind","ZIndex","Location","Scale","PlaceWithinSourceContent","EpgEntries","Config","HorizontalMarginPercent","VerticalMarginPercent","OpacityPercent")
VALUES (${id + 21},'Timed drawvg script',4,3,3,0,0,0,'${quote(JSON.stringify({ kind: 'script', command: "return 'setcolor lime\\nrect 64 0 20 20 fill\\n'", start_seconds: 2, duration_seconds: 2 }))}',0,0,100);
`);
  });

  test.afterAll(() => {
    if (db) sql(`DELETE FROM "MediaSource" WHERE "Id"=${id};
DELETE FROM "GraphicsElement" WHERE "Id" IN (${id},${id + 20},${id + 21});
DELETE FROM "ChannelWatermark" WHERE "Id" IN (${id},${id + 1});
DELETE FROM "FFmpegProfile" WHERE "Id"=${id};
DELETE FROM "Resolution" WHERE "Id"=${id};`);
    if (root) rmSync(root, { recursive: true, force: true });
    if (selectorFile) rmSync(selectorFile, { force: true });
  });

  test.beforeEach(async ({ page }) => {
    await page.request.post('/admin-api/settings/ui/save', {
      form: { is_dark_mode: 'true', language: 'en-us' }, maxRedirects: 0,
    });
  });

  test('switches engine controls, preserves watermarks and graphics and fits narrow screens', async ({ page }, info) => {
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => {
      if (message.type() === 'error') errors.push(message.text());
    });
    await page.goto(`/system/troubleshooting/playback?mediaItem=${id}`);
    await page.waitForFunction(() =>
      (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true);
    await expect(page.locator('select[name="musicVideoCreditsTemplate"]')).toHaveCount(0);
    const engine = page.getByRole('combobox', { name: 'Streaming Engine', exact: true });
    const watermarks = page.locator('select[name="watermark"]');
    const selector = page.locator('select[name="streamSelector"]');
    const disabled = await selector.isDisabled();
    expect(disabled).toBe(false);
    await selector.selectOption('preview-fixture.yml');
    await expect(engine).toHaveValue('Legacy');
    await expect(page.locator('select[name="graphicsElement"]')).toBeVisible();
    await watermarks.selectOption([`${id}`, `${id + 1}`]);
    await page.locator('select[name="graphicsElement"]').selectOption(`${id}`);
    await engine.selectOption('Next');
    await expect(selector).toBeVisible();
    expect(await selector.isDisabled()).toBe(disabled);
    await expect(selector).toHaveValue('preview-fixture.yml');
    await expect(page.locator('select[name="graphicsElement"]')).toBeVisible();
    await expect(page.locator('select[name="graphicsElement"]')).toHaveValues([`${id}`]);
    await expect(watermarks).toHaveAttribute('multiple');
    await expect(watermarks).toHaveValues([`${id}`, `${id + 1}`]);
    expect(await page.locator('form[data-playback-troubleshooting]').evaluate(form =>
      new FormData(form as HTMLFormElement).getAll('watermark'),
    )).toEqual([`${id}`, `${id + 1}`]);
    expect(await page.locator('form[data-playback-troubleshooting]').evaluate(form =>
      Array.from(new FormData(form as HTMLFormElement).keys()),
    )).toContain('graphicsElement');
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

  test('failed native playback completes promptly and leaves diagnostics for the next request', async ({ page }) => {
    const invalid = path.join(root, 'invalid-preview.mkv');
    writeFileSync(invalid, 'synthetic invalid media');
    sql(`UPDATE "MediaFile" SET "Path"='${quote(invalid)}' WHERE "Id"=${id};`);
    try {
      const response = await page.request.get(
        `/api/troubleshoot/playback.m3u8?mediaItem=${id}&ffmpegProfile=${profile}&streamingEngine=Legacy`,
        { timeout: 10_000, maxRedirects: 0 },
      );
      expect(response.status()).toBe(500);
      const log = await page.request.get('/api/troubleshoot/playback/log');
      expect(log.status()).toBe(200);
      expect((await log.json()).text).toMatch(/exit status:/);
    } finally {
      sql(`UPDATE "MediaFile" SET "Path"='${quote(path.join(root, 'preview.mkv'))}' WHERE "Id"=${id};`);
    }
    // The following normal engine cases must still acquire the same preview lock.
  });

  test('navigation during a native preview retires browser callbacks and retains worker cleanup', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(`/system/troubleshooting/playback?mediaItem=${id}`);
    await page.getByRole('combobox', { name: 'Streaming Engine', exact: true }).selectOption('Legacy');
    await page.locator('select[name="ffmpegProfile"]').selectOption(profile);
    await page.getByRole('button', { name: 'Play', exact: true }).click();
    await expect.poll(() => page.locator('video').evaluate((video: HTMLVideoElement) => video.currentTime)).toBeGreaterThan(0);
    await page.goto('/ffmpeg');
    await expect.poll(async () => {
      const response = await page.request.get('/api/troubleshoot/playback/log');
      return (await response.json())?.text ?? '';
    }, { timeout: 15_000 }).toMatch(/exit status:/);
    expect(errors).toEqual([]);
  });

  test('media sample ZIP preserves alternate audio and embedded subtitles', async ({ page }) => {
    const ffmpeg = process.env.ErsatzRS_FFMPEG__PATH ?? process.env.FFMPEG_PATH ?? 'ffmpeg';
    const ffprobe = process.env.ErsatzRS_FFPROBE__PATH ?? process.env.FFPROBE_PATH ?? 'ffprobe';
    const clip = path.join(root, 'sample-tracks.mkv');
    const subtitle = path.join(root, 'sample-tracks.srt');
    writeFileSync(subtitle, '1\n00:00:00,000 --> 00:00:02,000\nSynthetic Dutch subtitle\n');
    execFileSync(ffmpeg, ['-hide_banner', '-loglevel', 'error', '-y',
      '-f', 'lavfi', '-i', 'color=blue:s=64x48:r=5:d=2',
      '-f', 'lavfi', '-i', 'sine=frequency=440:duration=2',
      '-f', 'lavfi', '-i', 'sine=frequency=880:duration=2', '-i', subtitle,
      '-map', '0:v', '-map', '1:a', '-map', '2:a', '-map', '3:s',
      '-c:v', 'ffv1', '-c:a', 'pcm_s16le', '-c:s', 'srt',
      '-metadata:s:a:0', 'language=eng', '-metadata:s:a:1', 'language=nld',
      '-metadata:s:s:0', 'language=nld', clip]);
    sql(`UPDATE "MediaFile" SET "Path"='${quote(clip)}' WHERE "Id"=${id};`);
    try {
      const response = await page.request.get(`/api/troubleshoot/playback/sample/${id}`);
      expect(response.status()).toBe(200);
      expect(response.headers()['content-type']).toContain('application/zip');
      const archive = path.join(root, 'sample.zip');
      const extracted = path.join(root, 'captured-sample.mkv');
      writeFileSync(archive, await response.body());
      execFileSync('python3', ['-c',
        'import pathlib,sys,zipfile; z=zipfile.ZipFile(sys.argv[1]); assert z.testzip() is None; names=z.namelist(); assert len(names)==1 and names[0].endswith(".mkv"); pathlib.Path(sys.argv[2]).write_bytes(z.read(names[0]))',
        archive, extracted]);
      const streams = JSON.parse(execFileSync(ffprobe,
        ['-v', 'error', '-show_streams', '-of', 'json', extracted], { encoding: 'utf8' })).streams;
      expect(streams).toHaveLength(4);
      expect(streams.filter((stream: any) => stream.codec_type === 'audio')
        .map((stream: any) => [stream.codec_name, stream.tags.language])).toEqual([
          ['pcm_s16le', 'eng'], ['pcm_s16le', 'nld'],
        ]);
      expect(streams.find((stream: any) => stream.codec_type === 'subtitle').tags.language).toBe('nld');
      expect(streams.find((stream: any) => stream.codec_type === 'video').codec_name).toBe('ffv1');
    } finally {
      sql(`UPDATE "MediaFile" SET "Path"='${quote(path.join(root, 'preview.mkv'))}' WHERE "Id"=${id};`);
    }
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
      await page.locator('select[name="graphicsElement"]').selectOption(`${id}`);
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
      expect(query.getAll('graphicsElement')).toEqual([`${id}`]);
      await expect.poll(() => page.locator('video').evaluate((video: HTMLVideoElement) => {
        const canvas = document.createElement('canvas');
        canvas.width = 320;
        canvas.height = 180;
        const context = canvas.getContext('2d')!;
        context.drawImage(video, 0, 0, 320, 180);
        const red = context.getImageData(25, 15, 1, 1).data;
        const green = context.getImageData(285, 15, 1, 1).data;
        const blue = context.getImageData(160, 15, 1, 1).data;
        return {
          blue: blue[2] > 180 && blue[0] < 50 && blue[1] < 50,
          red: red[0] > 180 && red[1] < 50 && red[2] < 50,
          green: green[1] > 180 && green[0] < 50 && green[2] < 50,
        };
      })).toEqual({ red: true, green: true, blue: true });
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

  for (const engine of ['Legacy', 'Next']) {
    test(`${engine} motion holds the sought final frame and expires on programme time`, async ({ page, baseURL }) => {
      test.skip(engine === 'Next' && !process.env.ETV_NEXT_FOLDER,
        'Official Next runtime acceptance requires ETV_NEXT_FOLDER.');
      await page.goto(`/system/troubleshooting/playback?mediaItem=${id}&preferredAudioTitle=Dutch&subtitleId=${id}`);
      await page.getByRole('combobox', { name: 'Streaming Engine', exact: true }).selectOption(engine);
      await page.locator('select[name="ffmpegProfile"]').selectOption(profile);
      await page.locator('input[name="seekSeconds"]').fill('3');
      await page.locator('select[name="graphicsElement"]').selectOption([`${id + 20}`, `${id + 21}`]);
      await page.getByRole('button', { name: 'Play', exact: true }).click();
      const playlist = engine === 'Next' ? 'ffmpeg.m3u8' : 'live.m3u8';
      const url = `${baseURL}/iptv/session/.troubleshooting/${playlist}`;
      await expect.poll(async () => (await (await page.request.get(url)).text()).includes('#EXT-X-ENDLIST')).toBe(true);
      const ffmpeg = process.env.ErsatzRS_FFMPEG__PATH ?? process.env.ERSATZRS_FFMPEG__PATH
        ?? process.env.FFMPEG_PATH ?? 'ffmpeg';
      for (const [seek, blue] of [['0', true], ['2.25', false]] as const) {
        const frame = execFileSync(ffmpeg, ['-nostdin', '-v', 'error', '-i', url,
          '-ss', seek, '-map', '0:v:0', '-frames:v', '1', '-pix_fmt', 'rgb24', '-f', 'rawvideo', 'pipe:1']);
        expect(frame.length).toBe(320 * 180 * 3);
        const pixel = [...frame.subarray((8 * 320 + 8) * 3, (8 * 320 + 8) * 3 + 3)];
        const script = [...frame.subarray((8 * 320 + 72) * 3, (8 * 320 + 72) * 3 + 3)];
        if (blue) {
          expect(pixel[2]).toBeGreaterThan(180);
          expect(pixel[0]).toBeLessThan(40);
          expect(pixel[1]).toBeLessThan(40);
          expect(script[1]).toBeGreaterThan(180);
          expect(script[0]).toBeLessThan(40);
          expect(script[2]).toBeLessThan(40);
        } else {
          expect(pixel.every(component => component < 40)).toBe(true);
          expect(script.every(component => component < 40)).toBe(true);
        }
      }
      const archive = await page.request.get('/api/troubleshoot/playback/archive');
      expect(archive.status()).toBe(200);
      await expect(page.getByRole('button', { name: 'Play', exact: true })).toBeEnabled();
    });
  }

  for (const engine of ['Legacy', 'Next']) {
    test(`${engine} music-video credits are visible after seeking and archived`, async ({ page }) => {
      test.skip(engine === 'Next' && !process.env.ETV_NEXT_FOLDER,
        'Official Next runtime acceptance requires ETV_NEXT_FOLDER.');
      const errors: string[] = [];
      page.on('pageerror', error => errors.push(error.message));
      await page.goto(`/system/troubleshooting/playback?mediaItem=${musicId}&preferredAudioTitle=Dutch`);
      const credits = page.locator('select[name="musicVideoCreditsTemplate"]');
      await expect(credits).toBeVisible();
      await expect(credits).toHaveValue('');
      // This preset renders white text at source 9–16s; seeking to 9s must
      // rebase it into the preview. The Music Factory preset uses yellow/black.
      await credits.selectOption('_default.ass');
      await page.locator('select[name="streamingEngine"]').selectOption(engine);
      await page.locator('select[name="ffmpegProfile"]').selectOption(profile);
      await page.locator('input[name="seekSeconds"]').fill('9');
      const request = page.waitForRequest(request =>
        new URL(request.url()).pathname === '/api/troubleshoot/playback.m3u8');
      await page.getByRole('button', { name: 'Play', exact: true }).click();
      expect(new URL((await request).url()).searchParams.get('musicVideoCreditsTemplate'))
        .toBe('_default.ass');
      await expect.poll(() => page.locator('video').evaluate((video: HTMLVideoElement) => {
        if (video.readyState < 2) return 0;
        const canvas = document.createElement('canvas');
        canvas.width = 320;
        canvas.height = 180;
        const context = canvas.getContext('2d')!;
        context.drawImage(video, 0, 0, 320, 180);
        const pixels = context.getImageData(0, 0, 320, 180).data;
        let white = 0;
        for (let i = 0; i < pixels.length; i += 4) {
          if (pixels[i] > 180 && pixels[i + 1] > 180 && pixels[i + 2] > 180) white++;
        }
        return white;
      })).toBeGreaterThan(20);
      await expect(page.locator('textarea')).toHaveValue(
        engine === 'Next' ? /Next troubleshooting completed/ : /exit status:/);
      const archive = await page.request.get('/api/troubleshoot/playback/archive');
      expect(archive.status()).toBe(200);
      const bytes = await archive.body();
      for (const member of ['music-video-credits.ass', 'music-video-credits-template.sbntxt',
        'Synthetic credits song', '{{ title }}']) {
        expect(bytes.includes(Buffer.from(member))).toBe(true);
      }
      expect(errors).toEqual([]);
    });
  }

  test('music-video credits selector is responsive and submits without JavaScript', async ({ browser, baseURL }, info) => {
    const context = await browser.newContext({ baseURL, javaScriptEnabled: false, viewport: { width: 320, height: 720 } });
    try {
      const page = await context.newPage();
      let submitted: URL | undefined;
      await page.route('**/api/troubleshoot/playback.m3u8?**', route => {
        submitted = new URL(route.request().url());
        return route.fulfill({ contentType: 'text/plain', body: 'Captured credits submission' });
      });
      await page.goto(`/system/troubleshooting/playback?mediaItem=${musicId}`);
      const credits = page.locator('select[name="musicVideoCreditsTemplate"]');
      await expect(credits).toBeVisible();
      const box = await credits.boundingBox();
      expect(box).not.toBeNull();
      expect(box!.x).toBeGreaterThanOrEqual(0);
      expect(box!.x + box!.width).toBeLessThanOrEqual(320);
      await credits.selectOption('_MusicFactory_ArtistTitle');
      await page.locator('select[name="streamingEngine"]').selectOption('Next');
      await credits.scrollIntoViewIfNeeded();
      await page.screenshot({ path: `test-results/troubleshooting-credits-${info.project.name}.png`, fullPage: true });
      await page.getByRole('button', { name: 'Play', exact: true }).click();
      await expect(page.locator('body')).toContainText('Captured credits submission');
      expect(submitted?.searchParams.get('musicVideoCreditsTemplate')).toBe('_MusicFactory_ArtistTitle');
      expect(submitted?.searchParams.get('streamingEngine')).toBe('Next');
    } finally { await context.close(); }
  });

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
      await page.locator('select[name="graphicsElement"]').selectOption(`${id}`);
      await page.getByRole('button', { name: 'Play', exact: true }).click();
      await expect(page.locator('body')).toContainText('Captured preview submission');
      expect(submitted?.searchParams.get('streamingEngine')).toBe('Next');
      expect(submitted?.searchParams.get('streamSelector')).toBe('preview-fixture.yml');
      expect(submitted?.searchParams.get('subtitleId')).toBe(`${id}`);
      expect(submitted?.searchParams.get('startFromBeginning')).toBe('true');
      expect(submitted?.searchParams.getAll('watermark')).toEqual([`${id}`, `${id + 1}`]);
      expect(submitted?.searchParams.getAll('graphicsElement')).toEqual([`${id}`]);
    } finally {
      await context.close();
    }
  });
});
