import { waitForHydration } from './hydration';
import { expect, type Page } from '@playwright/test';
import { execFileSync } from 'child_process';
import { createHash } from 'crypto';
import { chmodSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'fs';
import { dirname, join, relative } from 'path';
import { subtitleProviderSource } from './addon-subtitle-provider';

export const addonId = 'org.ersatzrs.e2e.prepared-playback';
export const sourceUrl = 'https://prepared.example.invalid/video';
/** Host admission is shared across add-ons, so package isolation also needs host isolation. */
export function preparedSourceUrl(variant: PreparedFixtureVariant = {}): string {
  if (!variant.addonId) return sourceUrl;
  const identity = createHash('sha256').update(variant.addonId).digest('hex').slice(0, 24);
  return `https://${identity}.prepared.example.invalid/video`;
}
const quote = (value: string): string => value.replace(/'/g, "''");
export function preparedSqlite(dbPath: string, sql: string): string {
  return execFileSync('sqlite3', [dbPath], { input: `.timeout 10000\n${sql}`, encoding: 'utf8' }).trim();
}

/** Only safe state/counters/timestamps: never provider diagnostics, URLs or payloads. */
export async function waitForPreparedImport(dbPath: string, listId: number, discoveredCount = 1): Promise<void> {
  // objectContaining retains diagnostic fields in the failure diff; toMatchObject hides them.
  await expect.poll(() => JSON.parse(preparedSqlite(dbPath, `SELECT json_object(
    'state', "State", 'discoveredCount', "DiscoveredCount", 'generation', "Generation",
    'attempts', "AttemptCount", 'nextEligibleAt', "NextEligibleAt",
    'heartbeatAt', "HeartbeatAt", 'lastProgressAt', "LastProgressAt",
    'leaseExpiresAt', "LeaseExpiresAt", 'completedAt', "CompletedAt")
    FROM "AddonMediaListImportJob" WHERE "AddonMediaListId"=${listId}
    ORDER BY "Generation" DESC LIMIT 1;`) || 'null'), {
    timeout: 60000, message: 'Synthetic overview import completes with the exact item count',
  }).toEqual(expect.objectContaining({ state: 3, discoveredCount }));
}

/** DB completion precedes the landing page's own reload; let that navigation finish. */
export async function preparedWorkspaceReady(page: Page): Promise<void> {
  await expect(page.getByRole('heading', { level: 1, name: 'Prepared playback fixture' })).toBeVisible({ timeout: 30000 });
  await hydrated(page);
}

function contentHash(root: string): string {
  const files: string[] = [];
  const visit = (directory: string): void => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) visit(path);
      else if (entry.isFile()) files.push(path);
      else throw new Error('Unexpected fixture entry');
    }
  };
  visit(root);
  const hash = createHash('sha256').update(Buffer.from('ersatzrs-addon-content-v1\0'));
  for (const path of files.sort()) {
    const name = Buffer.from(relative(root, path).split('\\').join('/'));
    const body = readFileSync(path);
    const size = (length: number): Buffer => {
      const result = Buffer.alloc(8);
      result.writeBigUInt64LE(BigInt(length));
      return result;
    };
    hash.update(size(name.length)).update(name).update(size(body.length)).update(body).update(Buffer.from([0]));
  }
  return hash.digest('hex');
}

/** Separate package identity for one database. Omitted fields keep the shared fixture unchanged. */
export interface PreparedFixtureVariant {
  addonId?: string;
  processedDefault?: boolean;
  /** Delay before discovery output, so an import completes after its landing page renders. */
  discoverDelayMs?: number;
  /** Number of distinct finite items; the first keeps the shared fixture identity. */
  itemCount?: number;
  /** Adds a second editable string setting, DISPLAY_NOTE, for sibling-edit coverage. */
  extraSetting?: boolean;
  /** Rich source metadata for automatic publication and parent derivation. */
  richMetadata?: boolean;
  /** Synthetic provider failure for one candidate, before a file is written. */
  failPreparationFor?: string;
}

export function preparedInvocationLog(dbPath: string, variant: PreparedFixtureVariant = {}): string {
  const suffix = variant.addonId ? `-${variant.addonId}` : '';
  return join(dirname(dbPath), `addon-prepared-playback-invocations${suffix}.jsonl`);
}

export function installPreparedFixture(dbPath: string, pauseFirst = false, subtitleFixture = false, variant: PreparedFixtureVariant = {}): { invocationLog: string } {
  if (!dbPath) throw new Error('The isolated E2E database is required');
  const sqlite = (sql: string): string => preparedSqlite(dbPath, sql);
  const fixtureAddonId = variant.addonId ?? addonId;
  const fixtureSourceUrl = preparedSourceUrl(variant);
  const suffix = variant.addonId ? `-${variant.addonId}` : '';
  const root = join(dirname(dbPath), `addon-prepared-playback-package${suffix}`);
  const storage = join(dirname(dbPath), `addon-prepared-playback-library${suffix}`);
  const invocationLog = preparedInvocationLog(dbPath, variant);
  mkdirSync(root, { recursive: true });
  mkdirSync(storage, { recursive: true });
  const itemCount = Math.max(1, variant.itemCount ?? 1);
  const records = [
    { record_type: 'page', complete: true, total_hint: itemCount, revision_token: 'prepared-fixture-v1' },
    { record_type: 'list', provider_id: 'prepared-list', name: 'Prepared playback fixture' },
    ...Array.from({ length: itemCount }, (_, index) => ({
      record_type: 'item', provider_id: index === 0 ? 'prepared-video' : `prepared-video-${index + 1}`, rank: index,
      display_title: index === 0 ? 'Prepared calibration' : `Prepared calibration ${index + 1}`,
      title: index === 0 ? 'Prepared calibration' : `Prepared calibration ${index + 1}`, kind: 'remote_stream',
      guids: [], source_url: index === 0 ? fixtureSourceUrl : `${fixtureSourceUrl}/${index + 1}`, availability: 'available', content_kind: 'other_video',
      liveness: 'finite', duration_seconds: subtitleFixture ? 90 : 120,
      ...(variant.richMetadata ? { metadata: {
        title: `Automatic episode ${index + 1}`, plot: `Full description ${index + 1}`,
        season: 1, episode: index + 1, release_date: '2026-05-05',
        genres: ['Documentary'], tags: ['Archive'], languages: ['en'],
        studios: ['Fixture Channel'], content_ratings: [index === 12 ? 'age:18' : 'age:0'],
      } } : {}),
    })),
  ];
  // No network client exists in this provider. A raw play attempt is recorded and fails.
  // The only playable source is generated by the real preparation IPC invocation.
  writeFileSync(join(root, 'provider.cjs'), `
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const { execFileSync, spawn } = require('child_process');
const command = process.argv[2];
const log = ${JSON.stringify(invocationLog)};
fs.appendFileSync(log, JSON.stringify({ command }) + '\\n');
const output = value => process.stdout.write(JSON.stringify(value) + '\\n');
if (command === 'check') output({ status: 'ready', code: 'ready', message: 'Synthetic fixture ready' });
else if (command === 'discover' || command === 'list') {
  if (command === 'discover') fs.readFileSync(0, 'utf8');${(variant.discoverDelayMs ?? 0) > 0 ? `
  if (command === 'discover') Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ${variant.discoverDelayMs});` : ''}
  const records = ${JSON.stringify(records)};
  for (const record of records) if (command === 'discover' || record.record_type !== 'page') output(record);
} else if (command === 'runtime-info') {
  fs.readFileSync(0, 'utf8');
  output({ tool_version: 'fixture-1' });
} else if (command === 'prepare') {
  const request = JSON.parse(fs.readFileSync(0, 'utf8'));
  if (request.options.values.processed !== true) throw new Error('Saved preparation option was not forwarded');
  if (request.provider_id === ${JSON.stringify(variant.failPreparationFor ?? '')}) throw new Error('Synthetic preparation failure');
  if (${pauseFirst} && fs.readFileSync(log, 'utf8').split('\\n').filter(line => line && JSON.parse(line).command === 'prepare').length === 1) {
    const descendant = spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], { stdio: 'ignore' });
    fs.appendFileSync(log, JSON.stringify({ command: 'paused', provider: process.pid, descendant: descendant.pid }) + '\\n');
    setInterval(() => {}, 1000);
    return;
  }
  const media = path.join(request.staging_directory, 'prepared.mkv');
  execFileSync(process.env.FFMPEG_BIN, [
    '-hide_banner', '-loglevel', 'error', '-y',
    '-f', 'lavfi', '-i', 'color=c=red:size=320x180:rate=25',
    '-f', 'lavfi', '-i', 'sine=frequency=1000:sample_rate=48000',
    '-t', '60', '-c:v', 'mpeg2video', '-g', '12', '-bf', '0', '-c:a', 'mp2', '-threads', '1', media
  ], { timeout: 60000, stdio: ['ignore', 'ignore', 'pipe'] });
  const bytes = fs.readFileSync(media);
  output({ provider_id: request.provider_id, context_fingerprint: request.context_fingerprint,
    tool_version: request.tool_version, selected_format: 'synthetic-mpeg2', relative_path: 'prepared.mkv',
    byte_length: bytes.length, sha256: crypto.createHash('sha256').update(bytes).digest('hex'),
    duration_milliseconds: 60000 });
} else process.exit(77);
`);
  if (subtitleFixture) writeFileSync(join(root, 'provider.cjs'), subtitleProviderSource(invocationLog, records));
  const shellQuote = (value: string): string => `'${value.replace(/'/g, `'\\''`)}'`;
  writeFileSync(join(root, 'addon.sh'), `#!/bin/sh\nexec ${shellQuote(process.execPath)} ./provider.cjs "$@"\n`);
  chmodSync(join(root, 'addon.sh'), 0o755);
  writeFileSync(join(root, 'addon.cmd'), `@echo off\r\n"${process.execPath}" "%~dp0provider.cjs" %*\r\n`);
  const manifest = {
    manifest_version: 5, id: fixtureAddonId, version: '1.0.0', name: { 'en-US': 'Prepared playback fixture' },
    summary: { 'en-US': 'Network-free processed-file acceptance' }, icon: null, license: 'Zlib',
    source_url: 'https://prepared.example.invalid/addon', host_version: '*',
    capabilities: [
      ...['addon.check.v1', 'media-list.import.v1', 'media-list.list.v5', 'remote-stream.list.v4',
        'remote-stream.play.v1', 'media-list.item-runtime.v1', 'media-list.prepare.v2'].map(id => ({ id })),
      { id: 'media-list.options.v1', options: [{ key: 'processed', label: { 'en-US': 'Prepare calibration' },
        description: { 'en-US': 'Generate a synthetic finite file' }, scope: 'both',
        operation_targets: subtitleFixture ? ['preparation', 'playback'] : ['preparation'], applicability: 'finite_only', kind: 'boolean',
        default: variant.processedDefault ?? false, requires_preparation: true }] },
    ],
    dependencies: [], entrypoints: { any: { path: 'addon.sh', kind: 'posix_shell' },
      'win-x64': { path: 'addon.cmd', kind: 'windows_batch' } },
    permissions: { network_hosts: [new URL(fixtureSourceUrl).hostname], media_read: false,
      managed_ffmpeg: true, external_commands: ['node'] },
    settings: [{ key: 'MEDIA_STORAGE_PATH', label: { 'en-US': 'Media storage location' },
      kind: 'path', required: false, options: [] },
      ...(variant.extraSetting ? [{ key: 'DISPLAY_NOTE', label: { 'en-US': 'Display note' },
        kind: 'string', required: false, options: [] }] : [])],
    media_list_storage: { path_setting: 'MEDIA_STORAGE_PATH', default_subdirectory: 'prepared-playback',
      source_name: 'Prepared playback fixture' }, panels: [],
  };
  if (subtitleFixture) manifest.capabilities.push(...[
    'media-list.subtitles.v1', 'media-list.interval-metadata.v1', 'remote-stream.play.v4',
  ].map(id => ({ id })));
  sqlite(`
INSERT INTO "AddonInstallation" ("AddonId","Version","Enabled","PackageSha256","ContentSha256",
 "ManifestJson","InstallPath","InstalledAt","UpdatedAt","TrustTier")
VALUES ('${fixtureAddonId}','1.0.0',1,'${'0'.repeat(64)}','${contentHash(root)}',
 '${quote(JSON.stringify(manifest))}','${quote(root)}','2026-09-05T00:00:00Z','2026-09-05T00:00:00Z','verified');
INSERT INTO "MediaSource" DEFAULT VALUES;
INSERT INTO "LocalMediaSource" ("Id") VALUES (last_insert_rowid());
INSERT INTO "AddonMediaSource" ("AddonId","MediaSourceId","Name","RootPath")
VALUES ('${fixtureAddonId}',last_insert_rowid(),'Prepared playback fixture','${quote(storage)}');
INSERT INTO "ConfigElement" ("Key","Value") VALUES ('playout.days_to_build','1')
ON CONFLICT("Key") DO UPDATE SET "Value"=excluded."Value";
`);
  return { invocationLog };
}

export async function hydrated(page: Page, route?: string): Promise<void> {
  // Finish navigation before applying the separate bounded hydration check.
  if (route) expect((await page.goto(route, { waitUntil: 'load', timeout: 30000 }))?.status()).toBe(200);
  await waitForHydration(page);
  await expect(page.locator('[data-not-found]')).toHaveCount(0);
}
