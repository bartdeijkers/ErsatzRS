import { expect, Locator, Page, TestInfo, test, type APIRequestContext } from '@playwright/test';
import { execFileSync } from 'child_process';
import { createHash } from 'crypto';
import {
  chmodSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from 'fs';
import { get as httpGet } from 'http';
import { dirname, join, relative } from 'path';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;
const NSF_URL = 'https://www.youtube.com/watch?v=mhJRzQsLZGg';
const ADDON_ID = 'org.ersatzrs.e2e.nsf-live';
const LIST_NAME = 'NSF live fixture';

type NsfFixture = {
  packageRoot: string;
  seekLogPath: string;
};

function sqlEscape(value: string): string {
  return value.replace(/'/g, "''");
}

function sqlite(sql: string): string {
  if (!dbPath) {
    throw new Error('E2E_DB_PATH or ERSATZRS_DB_PATH is required for the NSF live fixture');
  }
  return execFileSync('sqlite3', [dbPath], {
    input: `.timeout 10000\n${sql}`,
    encoding: 'utf8',
  }).trim();
}

function addonContentHash(root: string): string {
  const files: string[] = [];
  const visit = (directory: string): void => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) {
        visit(path);
      } else if (entry.isFile()) {
        files.push(path);
      } else {
        throw new Error(`Unsupported add-on fixture entry: ${path}`);
      }
    }
  };
  visit(root);
  files.sort();

  const hash = createHash('sha256');
  hash.update(Buffer.from('ersatzrs-addon-content-v1\0'));
  for (const path of files) {
    const encodedPath = Buffer.from(relative(root, path).split('\\').join('/'));
    const pathLength = Buffer.alloc(8);
    pathLength.writeBigUInt64LE(BigInt(encodedPath.length));
    const contents = readFileSync(path);
    const fileLength = Buffer.alloc(8);
    fileLength.writeBigUInt64LE(BigInt(contents.length));
    hash.update(pathLength);
    hash.update(encodedPath);
    hash.update(fileLength);
    hash.update(contents);
    hash.update(Buffer.from([0]));
  }
  return hash.digest('hex');
}

function makeCalibrationClip(path: string): void {
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
      'testsrc2=size=320x180:rate=30',
      '-f',
      'lavfi',
      '-i',
      'sine=frequency=1000:sample_rate=48000',
      '-t',
      '45',
      '-c:v',
      'mpeg2video',
      '-g',
      '15',
      '-bf',
      '0',
      '-c:a',
      'mp2',
      '-b:a',
      '128k',
      '-shortest',
      '-f',
      'mpegts',
      path,
    ],
    { stdio: 'inherit' },
  );
}

function installFrozenNsfAddon(): NsfFixture {
  if (!dbPath) {
    throw new Error('E2E database path is required');
  }

  const packageRoot = join(dirname(dbPath), 'addon-nsf-live-fixture');
  const managedRoot = join(dirname(dbPath), 'addon-nsf-live-media');
  const seekLogPath = join(dirname(packageRoot), 'addon-nsf-live-producer-seeks.log');
  mkdirSync(packageRoot, { recursive: true });
  mkdirSync(managedRoot, { recursive: true });
  rmSync(seekLogPath, { force: true });
  makeCalibrationClip(join(packageRoot, 'calibration.ts'));

  const pageRecord = JSON.stringify({
    record_type: 'page',
    complete: true,
    total_hint: 1,
    revision_token: 'nsf-live-frozen-v1',
  });
  const listRecord = JSON.stringify({
    record_type: 'list',
    provider_id: 'youtube/direct/mhJRzQsLZGg',
    name: LIST_NAME,
    description: 'Frozen direct YouTube live-video acceptance fixture',
  });
  const itemRecord = JSON.stringify({
    record_type: 'item',
    provider_id: 'mhJRzQsLZGg',
    rank: 0,
    display_title: 'NSF Live',
    title: 'NSF Live',
    kind: 'remote_stream',
    guids: ['youtube://mhJRzQsLZGg'],
    source_url: NSF_URL,
    availability: 'available',
    content_kind: 'other_video',
    liveness: 'live',
  });
  const checkRecord = JSON.stringify({
    status: 'ready',
    code: 'ready',
    message: 'Frozen NSF live fixture is ready.',
  });

  const shell = [
    '#!/bin/sh',
    'set -eu',
    'case "${1:-}" in',
    '  check)',
    `    printf '%s\\n' '${checkRecord}'`,
    '    ;;',
    '  discover)',
    '    cat >/dev/null',
    `    printf '%s\\n' '${pageRecord}' '${listRecord}' '${itemRecord}'`,
    '    ;;',
    '  list)',
    `    printf '%s\\n' '${listRecord}' '${itemRecord}'`,
    '    ;;',
    '  play)',
    '    printf \'%s\\n\' "$ERSATZRS_REMOTE_STREAM_SEEK" >> ../addon-nsf-live-producer-seeks.log',
    '    cat ./calibration.ts',
    '    ;;',
    '  *) exit 2 ;;',
    'esac',
    '',
  ].join('\n');
  writeFileSync(join(packageRoot, 'addon.sh'), shell);
  chmodSync(join(packageRoot, 'addon.sh'), 0o755);

  const batch = [
    '@echo off',
    'if /i "%~1"=="check" (',
    `  echo ${checkRecord}`,
    '  exit /b 0',
    ')',
    'if /i "%~1"=="discover" (',
    `  echo ${pageRecord}`,
    `  echo ${listRecord}`,
    `  echo ${itemRecord}`,
    '  exit /b 0',
    ')',
    'if /i "%~1"=="list" (',
    `  echo ${listRecord}`,
    `  echo ${itemRecord}`,
    '  exit /b 0',
    ')',
    'if /i "%~1"=="play" (',
    '  >>"%~dp0..\\addon-nsf-live-producer-seeks.log" echo(%ERSATZRS_REMOTE_STREAM_SEEK%',
    '  type "%~dp0calibration.ts"',
    '  exit /b 0',
    ')',
    'exit /b 2',
    '',
  ].join('\r\n');
  writeFileSync(join(packageRoot, 'addon.cmd'), batch);

  const manifest = JSON.stringify({
    manifest_version: 3,
    id: ADDON_ID,
    version: '1.0.0',
    name: { 'en-US': 'Frozen NSF live fixture' },
    summary: { 'en-US': 'Network-free AMM-G07 direct-live acceptance fixture.' },
    icon: null,
    license: 'Zlib',
    source_url: 'https://example.invalid/ersatzrs-e2e/nsf-live',
    host_version: '*',
    capabilities: [
      { id: 'addon.check.v1' },
      { id: 'media-list.import.v1' },
      { id: 'media-list.list.v5' },
      { id: 'remote-stream.list.v4' },
      { id: 'remote-stream.play.v1' },
    ],
    dependencies: [],
    entrypoints: {
      any: { path: 'addon.sh', kind: 'posix_shell' },
      'win-x64': { path: 'addon.cmd', kind: 'windows_batch' },
    },
    permissions: {
      network_hosts: ['www.youtube.com'],
      media_read: false,
      managed_ffmpeg: false,
      external_commands: ['cat'],
    },
    settings: [
      {
        key: 'MEDIA_STORAGE_PATH',
        label: { 'en-US': 'Media storage location' },
        kind: 'path',
        required: false,
        options: [],
      },
    ],
    media_list_storage: {
      path_setting: 'MEDIA_STORAGE_PATH',
      default_subdirectory: 'nsf-live-media',
      source_name: 'Frozen NSF live fixture',
    },
    panels: [],
  });
  const contentSha256 = addonContentHash(packageRoot);

  sqlite(`
DELETE FROM "AddonInstallation" WHERE "AddonId" = '${ADDON_ID}';
INSERT INTO "AddonInstallation" (
  "AddonId", "RepositoryId", "Version", "Enabled", "PackageSha256", "ContentSha256",
  "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt", "TrustTier"
) VALUES (
  '${ADDON_ID}', NULL, '1.0.0', 1, '${'0'.repeat(64)}', '${contentSha256}',
  '${sqlEscape(manifest)}', '${sqlEscape(packageRoot)}',
  '2026-08-25T00:00:00Z', '2026-08-25T00:00:00Z', 'verified'
);
INSERT INTO "MediaSource" DEFAULT VALUES;
INSERT INTO "LocalMediaSource" ("Id") VALUES (last_insert_rowid());
INSERT INTO "AddonMediaSource" ("AddonId", "MediaSourceId", "Name", "RootPath")
VALUES (
  '${ADDON_ID}', last_insert_rowid(), 'Frozen NSF live fixture',
  '${sqlEscape(managedRoot)}'
);
INSERT INTO "ConfigElement" ("Key", "Value")
VALUES ('playout.days_to_build', '1')
ON CONFLICT("Key") DO UPDATE SET "Value" = excluded."Value";
INSERT INTO "ConfigElement" ("Key", "Value")
VALUES ('ffmpeg.segmenter.initial_segment_count', '1')
ON CONFLICT("Key") DO UPDATE SET "Value" = excluded."Value";
`);

  return { packageRoot, seekLogPath };
}

function watchRuntimeErrors(page: Page): string[] {
  const runtimeErrors: string[] = [];
  page.on('pageerror', (error) => {
    if (error.name === 'AbortError' || error.message.includes('operation was aborted')) return;
    runtimeErrors.push(error.stack ?? error.message);
  });
  page.on('console', (message) => {
    if (message.type() !== 'error') return;
    const text = message.text();
    if (
      text.includes('failed_to_cast_marker_node') ||
      text.includes('rust_panic') ||
      text.includes('unreachable executed') ||
      text.toLowerCase().includes('hydration')
    ) {
      runtimeErrors.push(text);
    }
  });
  return runtimeErrors;
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
          () => (window as Window & { __ersatzrs_hydrated?: boolean }).__ersatzrs_hydrated === true,
        ),
      { timeout: 15_000 },
    )
    .toBe(true);
}

async function gotoHydrated(page: Page, route: string): Promise<void> {
  const response = await page.goto(route, { waitUntil: 'domcontentloaded' });
  expect(response?.status(), `GET ${route}`).toBe(200);
  await expect(page.locator('[data-not-found]')).toHaveCount(0);
  await waitForHydration(page);
}

function scheduleItemSelect(page: Page, label: string): Locator {
  return page
    .locator('form[class^="item-editor"] label')
    .filter({
      has: page.locator('span').filter({ hasText: new RegExp(`^${label}$`) }),
    })
    .locator('select')
    .first();
}

function readStreamBytes(url: string, minimumBytes: number, timeoutMs: number): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    let settled = false;
    let total = 0;
    const chunks: Buffer[] = [];
    const finish = (error?: Error): void => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      request.destroy();
      if (error) reject(error);
      else resolve(Buffer.concat(chunks));
    };
    const request = httpGet(url, (response) => {
      const status = response.statusCode ?? 0;
      if (status < 200 || status >= 300) {
        response.resume();
        finish(new Error(`GET ${url} returned HTTP ${status}`));
        return;
      }
      response.on('data', (chunk: Buffer) => {
        chunks.push(chunk);
        total += chunk.length;
        if (total >= minimumBytes) {
          response.destroy();
          finish();
        }
      });
      response.on('end', () => {
        if (total < minimumBytes) {
          finish(new Error(`GET ${url} ended after ${total} bytes; expected ${minimumBytes}`));
        }
      });
      response.on('error', (error) => {
        if (!settled) finish(error);
      });
    });
    const timer = setTimeout(
      () => finish(new Error(`GET ${url} did not produce ${minimumBytes} bytes in ${timeoutMs}ms`)),
      timeoutMs,
    );
    request.on('error', (error) => {
      if (!settled) finish(error);
    });
  });
}

async function pollText(
  request: APIRequestContext,
  url: string,
  predicate: (body: string) => boolean,
  timeout = 60_000,
): Promise<string> {
  let body = '';
  await expect
    .poll(
      async () => {
        try {
          const response = await request.get(url, { timeout: 10_000 });
          body = response.ok() ? await response.text() : '';
          return response.ok() && predicate(body);
        } catch {
          return false;
        }
      },
      { timeout },
    )
    .toBe(true);
  return body;
}

function producerSeeks(path: string): string[] {
  if (!existsSync(path)) return [];
  return readFileSync(path, 'utf8')
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
}

test.describe('AMM-G07 NSF live acceptance', () => {
  test.skip(!dbPath, 'E2E_DB_PATH or ERSATZRS_DB_PATH is required');

  test('publishes and plays the exact NSF live URL through the real admin path', async ({
    baseURL,
    page,
    request,
  }, testInfo: TestInfo) => {
    test.setTimeout(300_000);
    test.skip(
      testInfo.project.name !== 'chromium',
      'The acceptance mutates one shared database and is intentionally Chromium-only.',
    );
    if (!baseURL) throw new Error('Playwright baseURL is required');

    const runtimeErrors = watchRuntimeErrors(page);
    const fixture = installFrozenNsfAddon();
    expect(existsSync(join(fixture.packageRoot, 'calibration.ts'))).toBe(true);
    await forceEnglish(page);

    await gotoHydrated(page, '/media/addon-lists/add');
    await page.getByLabel('Add-on list URL').fill(NSF_URL);
    await page.getByLabel('Add-on', { exact: true }).selectOption(ADDON_ID);
    await page.getByLabel('Collection Type', { exact: true }).selectOption('remote_streams');
    await page.getByLabel('Storage folder').fill('nsf-live');
    await page
      .getByRole('checkbox', { name: 'Update list from the provider once each day' })
      .uncheck();
    await page
      .getByRole('checkbox', { name: 'Create playlist in original order' })
      .check();
    await page
      .getByRole('checkbox', { name: 'Load item details after publishing' })
      .uncheck();
    await page.getByRole('button', { name: /^import$/i }).click();
    await expect(page).toHaveURL(/\/media\/addon-lists\/\d+\?import=1$/);
    await waitForHydration(page);

    const listId = Number(page.url().match(/\/media\/addon-lists\/(\d+)/)?.[1] ?? 0);
    expect(listId).toBeGreaterThan(0);
    await expect
      .poll(
        () =>
          sqlite(`
SELECT "State" || ',' || "DiscoveredCount" || ',' || "OverviewComplete"
FROM "AddonMediaListImportJob"
WHERE "AddonMediaListId" = ${listId}
ORDER BY "Generation" DESC LIMIT 1;
`),
        { timeout: 60_000 },
      )
      .toBe('3,1,0');
    expect(
      sqlite(`
SELECT "SourceUrl" || '|' || "GeneratePlaylist" || '|' || "ItemCount"
FROM "AddonMediaList" WHERE "Id" = ${listId};
`),
    ).toBe(`${NSF_URL}|1|1`);
    expect(
      sqlite(`
SELECT "SourceUrl" || '|' || "ProviderLiveness" || '|' || "EffectiveLiveness"
FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ${listId};
`),
    ).toBe(`${NSF_URL}|2|2`);

    await page.reload({ waitUntil: 'domcontentloaded' });
    await waitForHydration(page);
    await expect(page.getByText('NSF Live', { exact: true }).first()).toBeVisible();
    const publish = page.getByRole('button', { name: /^publish$/i });
    await expect(publish).toBeEnabled();
    const publishNavigation = page.waitForEvent('framenavigated', { timeout: 120_000 });
    await publish.click();
    await publishNavigation;
    await waitForHydration(page);

    await expect
      .poll(
        () =>
          sqlite(`
SELECT rs."Liveness" || '|' || rs."IsLive" || '|' || rs."Url" || '|' || rs."AddonId"
FROM "AddonMediaListItem" item
JOIN "RemoteStream" rs ON rs."Id" = item."MediaItemId"
WHERE item."AddonMediaListId" = ${listId};
`),
        { timeout: 60_000 },
      )
      .toBe(`2|1|${NSF_URL}|${ADDON_ID}`);
    expect(
      sqlite(`
SELECT "PublishedLiveness" FROM "AddonMediaListItem"
WHERE "AddonMediaListId" = ${listId};
`),
    ).toBe('2');

    const playlistId = Number(
      sqlite(`SELECT "PlaylistId" FROM "AddonMediaList" WHERE "Id" = ${listId};`),
    );
    expect(playlistId).toBeGreaterThan(0);
    expect(
      sqlite(`
SELECT COUNT(*) FROM "PlaylistItem" pi
JOIN "AddonMediaListItem" item ON item."MediaItemId" = pi."MediaItemId"
WHERE pi."PlaylistId" = ${playlistId} AND item."AddonMediaListId" = ${listId};
`),
    ).toBe('1');

    await gotoHydrated(page, '/channels/add');
    await page.locator('input[name="name"]').fill('nsf live');
    await page.getByRole('button', { name: 'Add Channel', exact: true }).click();
    await expect(page).toHaveURL(/\/channels$/);
    const channelRow = sqlite(`
SELECT "Id" || '|' || "Number" FROM "Channel" WHERE "Name" = 'nsf live';
`);
    const [channelIdText, channelNumber] = channelRow.split('|');
    const channelId = Number(channelIdText);
    expect(channelId).toBeGreaterThan(0);
    expect(channelNumber).not.toBe('');

    await gotoHydrated(page, '/schedules/add');
    await page.locator('input[name="name"]').fill('nsf live schedule');
    await page.getByRole('button', { name: /add schedule/i }).click();
    await expect(page).toHaveURL(/\/schedules\/\d+\/items$/);
    await waitForHydration(page);
    const scheduleId = Number(page.url().match(/\/schedules\/(\d+)\/items$/)?.[1] ?? 0);
    expect(scheduleId).toBeGreaterThan(0);
    await page.getByRole('button', { name: /add schedule item/i }).click();
    const collectionType = scheduleItemSelect(page, 'Collection Type');
    await expect(collectionType).toBeVisible();
    await collectionType.selectOption('6');
    const playlist = scheduleItemSelect(page, 'Playlist');
    await expect(playlist.locator(`option[value="${playlistId}"]`)).toHaveCount(1);
    await playlist.selectOption(String(playlistId));
    await scheduleItemSelect(page, 'Playout Mode').selectOption('1');
    await page.getByRole('button', { name: /save schedule items/i }).click();
    await expect(page.locator('tbody tr')).toHaveCount(1);
    expect(
      sqlite(`
SELECT item."CollectionType" || '|' || item."PlaylistId" || '|' ||
       CASE WHEN flood."Id" IS NOT NULL THEN 'Flood' ELSE 'Other' END
FROM "ProgramScheduleItem" item
LEFT JOIN "ProgramScheduleFloodItem" flood ON flood."Id" = item."Id"
WHERE item."ProgramScheduleId" = ${scheduleId};
`),
    ).toBe(`6|${playlistId}|Flood`);

    await gotoHydrated(page, '/playouts/add');
    await page.locator('select[name="channel_id"]').selectOption(String(channelId));
    await page.locator('select[name="program_schedule_id"]').selectOption(String(scheduleId));
    await page.getByRole('button', { name: 'Add Playout', exact: true }).click();
    await expect(page).toHaveURL(/\/playouts$/);

    const mediaItemId = Number(
      sqlite(`
SELECT "MediaItemId" FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ${listId};
`),
    );
    await expect
      .poll(
        () =>
          sqlite(`
SELECT COUNT(*)
FROM "PlayoutItem" item
JOIN "Playout" playout ON playout."Id" = item."PlayoutId"
WHERE playout."ChannelId" = ${channelId}
  AND item."MediaItemId" = ${mediaItemId}
  AND julianday(item."Start") <= julianday('now')
  AND julianday(item."Finish") > julianday('now');
`),
        { timeout: 90_000 },
      )
      .toBe('1');

    const tsBody = await readStreamBytes(
      new URL(`/iptv/channel/${channelNumber}.ts?mode=ts`, baseURL).toString(),
      188 * 10,
      60_000,
    );
    expect(tsBody.length).toBeGreaterThanOrEqual(188 * 10);
    expect(tsBody.includes(0x47)).toBe(true);

    const variantUrl = new URL(
      `/iptv/channel/${channelNumber}.m3u8?mode=segmenter`,
      baseURL,
    ).toString();
    const variant = await pollText(request, variantUrl, (body) => body.includes('#EXTM3U'));
    expect(variant).toContain('#EXTM3U');
    const sessionUrl = new URL(`/iptv/session/${channelNumber}/hls.m3u8`, baseURL).toString();
    const session = await pollText(request, sessionUrl, (body) => body.includes('#EXTINF'));
    const segment = session
      .split(/\r?\n/)
      .map((line) => line.trim())
      .find((line) => line !== '' && !line.startsWith('#'));
    expect(segment).toBeTruthy();
    const segmentUrl = segment!.includes('://')
      ? segment!
      : segment!.startsWith('/')
        ? new URL(segment!, baseURL).toString()
        : new URL(`/iptv/session/${channelNumber}/${segment!}`, baseURL).toString();
    const segmentResponse = await request.get(segmentUrl, { timeout: 30_000 });
    expect(segmentResponse.ok(), `GET ${segmentUrl}`).toBe(true);
    expect((await segmentResponse.body()).length).toBeGreaterThan(0);

    await expect
      .poll(() => producerSeeks(fixture.seekLogPath).length, { timeout: 30_000 })
      .toBeGreaterThanOrEqual(1);
    const seeks = producerSeeks(fixture.seekLogPath);
    expect(seeks.every((seek) => seek === '0'), `producer seeks: ${seeks.join(', ')}`).toBe(true);
    expect(runtimeErrors).toEqual([]);
  });
});
