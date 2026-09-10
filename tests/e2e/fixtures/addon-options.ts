import { randomUUID } from 'crypto';
import { execFileSync } from 'child_process';
import { chmodSync, mkdirSync, writeFileSync } from 'fs';
import { dirname, join } from 'path';
import { addonContentHash } from './addon-metadata';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

export function sqlite(sql: string): string {
  if (!dbPath) throw new Error('The isolated E2E database path is required');
  const input = `.timeout 10000\n${sql}`;
  try {
    return execFileSync('sqlite3', [dbPath], { input, encoding: 'utf8' }).trim();
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
    // Match the existing metadata-editor fixture's Windows SQLite fallback.
    return execFileSync('python', ['-c', [
      'import sqlite3, sys',
      'connection = sqlite3.connect(sys.argv[1], timeout=10)',
      'statement = sys.stdin.read().split("\\n", 1)[1].strip()',
      'if statement.upper().startswith("SELECT") and statement.count(";") <= 1:',
      '    for row in connection.execute(statement).fetchall():',
      '        print("|".join("" if value is None else str(value) for value in row))',
      'else:',
      '    connection.executescript(statement)',
      'connection.commit()',
      'connection.close()',
    ].join('\n'), dbPath], { input, encoding: 'utf8' }).trim();
  }
}

function quote(value: string): string {
  return `'${value.replace(/'/g, "''")}'`;
}

export function seedOptions(access = false) {
  if (!dbPath) throw new Error('The isolated E2E database path is required');
  // Every invocation has its own numeric and provider namespace. No reseeding
  // or deletion can collide with another spec's live background operation.
  const suffix = randomUUID().replace(/-/g, '');
  const base = 2_000_000_000 + Number.parseInt(suffix.slice(0, 8), 16) * 10;
  const listId = base + 1;
  const firstItemId = base + 2;
  const secondItemId = base + 3;
  const revisionId = base + 4;
  const addonId = `org.example.e2e.options.${suffix}`;
  const packageRoot = join(dirname(dbPath), `e2e-addon-options-${suffix}`);
  mkdirSync(packageRoot, { recursive: true });
  // Only local readiness and runtime identity checks succeed. Provider operations fail
  // closed; this fixture never opens browser profiles or performs network work.
  writeFileSync(join(packageRoot, 'addon.sh'), `#!/bin/sh\nif [ "\${1:-}" = "check" ]; then\n  printf '%s\\n' '{"status":"ready","code":"ready","message":"Synthetic option fixture is ready."}'\n  exit 0\nfi\nif [ \"\${1:-}\" = \"runtime-info\" ]; then\n  printf '%s\\n' '{\"tool_version\":\"1.0\"}'\n  exit 0\nfi\nexit 1\n`);
  chmodSync(join(packageRoot, 'addon.sh'), 0o755);
  writeFileSync(join(packageRoot, 'addon.cmd'), [
    '@echo off',
    'if /i "%~1"=="check" (',
    '  echo {"status":"ready","code":"ready","message":"Synthetic option fixture is ready."}',
    '  exit /b 0',
    ')',
    'if /i "%~1"=="runtime-info" (',
    '  echo {"tool_version":"1.0"}',
    '  exit /b 0',
    ')',
    'exit /b 1',
    '',
  ].join('\r\n'));
  const manifest = {
    manifest_version: 5, id: addonId, version: '1.0.0',
    name: { 'en-US': 'Synthetic option provider' },
    summary: { 'en-US': 'Native draft option fixture' },
    license: 'MIT', source_url: 'https://example.invalid/options-fixture', host_version: '*',
    capabilities: [
      { id: 'addon.check.v1' },
      { id: 'media-list.list.v6' },
      ...(access ? [{id:'media-list.item-runtime.v1'},{id:'media-list.access-check.v1'}] : []),
      { id: 'media-list.options.v1', options: [
        {
          key: 'browser', label: { 'en-US': 'Browser' },
          description: { 'en-US': 'Synthetic browser selection; no profiles are opened.' },
          scope: 'both', operation_targets: access ? ['playback','test_access'] : ['playback'], applicability: 'all',
          kind: 'enum', default: 'none',
          choices: ['none', 'firefox', 'chrome', 'chromium'].map((value) => ({
            value, label: { 'en-US': value === 'none' ? 'None' : value },
          })),
        },
        {
          key: 'sponsorblock_remove', label: { 'en-US': 'Remove SponsorBlock segments' },
          description: { 'en-US': 'Prepare finite media before playback.' },
          scope: 'both', operation_targets: ['preparation', 'playback'],
          applicability: 'finite_only', kind: 'boolean', default: true,
          requires_preparation: true,
        },
      ] },
    ],
    dependencies: [],
    entrypoints: {
      any: { path: 'addon.sh', kind: 'posix_shell' },
      'win-x64': { path: 'addon.cmd', kind: 'windows_batch' },
    },
    permissions: { network_hosts: [], media_read: false, managed_ffmpeg: false, external_commands: [] },
    settings: [], panels: [],
  };
  sqlite(`
INSERT INTO "AddonInstallation" (
  "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256", "ManifestJson",
  "InstallPath", "InstalledAt", "UpdatedAt", "TrustTier"
) VALUES (${quote(addonId)}, '1.0.0', 1, 'synthetic-unused-package', ${quote(addonContentHash(packageRoot))},
  ${quote(JSON.stringify(manifest))}, ${quote(packageRoot)},
  '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z', 'verified');
INSERT INTO "AddonMediaList" (
  "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "ItemCount", "AutoRefresh",
  "RefreshCadence", "GeneratePlaylist", "ReviewState", "PresentationKind",
  "RequestedPresentationKind", "PublishedRevision"
) VALUES (${listId}, ${quote(addonId)}, ${quote(`options-${suffix}`)},
  'https://example.invalid/options-list', 'Synthetic options list', 2, 0, 4, 0, 1, 0, 0, 1);
INSERT INTO "AddonMediaListItem" (
  "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind", "Rank", "Title",
  "SourceUrl", "DurationSeconds", "Availability", "ContentKind", "Included", "Published",
  "ProviderLiveness", "EffectiveLiveness"
) VALUES
  (${firstItemId}, ${listId}, 'first', 'First option item', 4, 0, 'First option item',
   'https://example.invalid/options-first', 60, 0, 0, 1, 1, 1, 1),
  (${secondItemId}, ${listId}, 'second', 'Second option item', 4, 1, 'Second option item',
   'https://example.invalid/options-second', 60, 0, 0, 1, 1, 1, 1);
INSERT INTO "AddonMediaListRevision" (
  "Id", "AddonMediaListId", "Revision", "SourceDraftRevision", "OptionValuesJson"
) VALUES (${revisionId}, ${listId}, 1, 0, '{"browser":"none","sponsorblock_remove":false}');
INSERT INTO "AddonMediaListRevisionItemOption" (
  "AddonMediaListRevisionId", "ProviderId", "OptionOverridesJson"
) VALUES (${revisionId}, 'first', '{}'), (${revisionId}, 'second', '{}');
`);
  return { listId, firstItemId, secondItemId, revisionId };
}


/** Database-only local candidate; these tests do not start playback or a scan. */
export function seedItemReplacement(itemId: number): number {
  const id = itemId + 100;
  sqlite(`
INSERT INTO "MediaSource" ("Id") VALUES (${id});
INSERT INTO "LocalMediaSource" ("Id") VALUES (${id});
INSERT INTO "Library" ("Id","MediaKind","MediaSourceId","Name") VALUES (${id},0,${id},'Synthetic replacement library');
INSERT INTO "LibraryPath" ("Id","Path","LibraryId") VALUES (${id},'/synthetic/item-form/${id}',${id});
INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (${id},${id},0);
INSERT INTO "Movie" ("Id") VALUES (${id});
INSERT INTO "MovieMetadata" ("MovieId","MetadataKind","Title","DateAdded","DateUpdated") VALUES (${id},1,'Synthetic replacement ${id}','2026-09-01T00:00:00Z','2026-09-01T00:00:00Z');
INSERT INTO "MediaVersion" ("Id","DateAdded","DateUpdated","Duration","Height","Width","VideoScanKind","MovieId") VALUES (${id},'2026-09-01T00:00:00Z','2026-09-01T00:00:00Z','00:01:00',1080,1920,0,${id});
INSERT INTO "MediaFile" ("MediaVersionId","Path","PathHash") VALUES (${id},'/synthetic/item-form/${id}/movie.mkv','synthetic-item-form-${id}');
INSERT INTO "MediaStream" ("Index","Codec","MediaStreamKind","Channels","Default","Forced","MediaVersionId","AttachedPic") VALUES (0,'h264',1,0,1,0,${id},0);
`);
  return id;
}
