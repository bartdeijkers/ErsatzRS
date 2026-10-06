import { TestInfo } from '@playwright/test';
import { createHash } from 'crypto';
import { execFileSync } from 'child_process';
import { chmodSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'fs';
import { dirname, join, relative } from 'path';
const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

export type MetadataSeed = {
  addonId: string;
  listId: number;
  firstItemId: number;
  secondItemId: number;
  firstTitle: string;
  secondTitle: string;
  noJsProviderId: string;
  noJsSourceUrl: string;
  noJsListName: string;
};

export function projectBase(testInfo: TestInfo): number {
  const projectOffset = Array.from(testInfo.project.name).reduce(
    (total, character) => total + character.charCodeAt(0),
    0,
  ) * 1_000;
  return 760_000 + projectOffset;
}

export function sqlEscape(value: string): string {
  return value.replace(/'/g, "''");
}

export function sqlite(sql: string): string {
  if (!dbPath) {
    throw new Error('E2E_DB_PATH or ERSATZRS_DB_PATH is required for metadata editor seeding');
  }
  const input = `.timeout 10000\n${sql}`;
  try {
    return execFileSync('sqlite3', [dbPath], {
      input,
      encoding: 'utf8',
    }).trim();
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') {
      throw error;
    }

    const pythonSqlite = [
      'import sqlite3, sys',
      'path = sys.argv[1]',
      'script = sys.stdin.read().split("\\n", 1)[1]',
      'connection = sqlite3.connect(path, timeout=10)',
      'statement = script.strip()',
      'if statement.lstrip().upper().startswith(("SELECT", "WITH", "PRAGMA")) and statement.count(";") <= 1:',
      '    cursor = connection.execute(statement)',
      '    for row in cursor.fetchall():',
      '        print("|".join("" if value is None else str(value) for value in row))',
      'else:',
      '    connection.executescript(script)',
      'connection.commit()',
      'connection.close()',
    ].join('\n');
    return execFileSync('python', ['-c', pythonSqlite, dbPath], {
      input,
      encoding: 'utf8',
    }).trim();
  }
}


export function addonContentHash(root: string): string {
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


export function seedMetadataEditor(
  testInfo: TestInfo,
  namespace: 'shared' | 'reset' | 'no-js' = 'shared',
): MetadataSeed {
  if (!dbPath) {
    throw new Error('E2E database path is required');
  }
  // Queue-only reset scenarios keep their live work separate from later fixture seeding.
  const namespaceOffset = { shared: 0, reset: 10_000_000, 'no-js': 20_000_000 }[namespace];
  const base = projectBase(testInfo) + namespaceOffset;
  const projectSuffix = testInfo.project.name.toLowerCase().replace(/[^a-z0-9]+/g, '-');
  const suffix = namespace === 'shared' ? projectSuffix : `${projectSuffix}-${namespace}`;
  const addonId = `org.example.e2e.metadata.${suffix}`;
  const listId = base + 1;
  const firstItemId = base + 2;
  const secondItemId = base + 3;
  const sourceId = base + 4;
  const libraryId = base + 5;
  const libraryPathId = base + 6;
  const firstTitle = `E2E Metadata Primary ${testInfo.project.name}`;
  const secondTitle = `E2E Metadata Review ${testInfo.project.name}`;
  const noJsProviderId = `fixture/no-js-${suffix}`;
  const noJsSourceUrl = `https://example.invalid/e2e/${suffix}/no-js-list`;
  const noJsListName = `E2E No-JS Imported List ${testInfo.project.name}`;
  const folder = join(dirname(dbPath), `e2e-addon-metadata-${suffix}`);
  const packageRoot = join(dirname(dbPath), `e2e-addon-package-${suffix}`);
  mkdirSync(folder, { recursive: true });
  mkdirSync(packageRoot, { recursive: true });
  writeFileSync(join(packageRoot, 'unused'), 'fixture package marker\n');
  const noJsListRecord = JSON.stringify({
    record_type: 'list',
    provider_id: noJsProviderId,
    name: noJsListName,
    description: 'Created through the native HTML import route',
  });
  const noJsItemRecord = JSON.stringify({
    record_type: 'item',
    provider_id: `remote:no-js-${suffix}`,
    rank: 0,
    display_title: `No-JS imported item ${testInfo.project.name}`,
    title: `No-JS imported item ${testInfo.project.name}`,
    kind: 'remote_stream',
    guids: [`fixture://no-js-${suffix}`],
    source_url: `https://example.invalid/e2e/${suffix}/no-js-item`,
    duration_seconds: 300,
    availability: 'available',
    content_kind: 'other_video',
  });
  const noJsPageRecord = JSON.stringify({
    record_type: 'page',
    complete: true,
    total_hint: 1,
    revision_token: `no-js-${suffix}-v1`,
  });
  writeFileSync(
    join(packageRoot, 'addon.sh'),
    `#!/bin/sh\nset -eu\nif [ "\${1:-}" = "check" ]; then\n  printf '%s\\n' '{"status":"ready","code":"ready","message":"E2E list fixture is ready."}'\n  exit 0\nfi\nif [ "\${1:-}" = "discover" ]; then\n  cat >/dev/null\n  printf '%s\\n' '${noJsPageRecord}' '${noJsListRecord}' '${noJsItemRecord}'\n  exit 0\nfi\nprintf '%s\\n' '${noJsListRecord}' '${noJsItemRecord}'\n`,
  );
  chmodSync(join(packageRoot, 'addon.sh'), 0o755);
  writeFileSync(
    join(packageRoot, 'addon.cmd'),
    [
      '@echo off',
      'if /i "%~1"=="check" (',
      '  echo {"status":"ready","code":"ready","message":"E2E list fixture is ready."}',
      '  exit /b 0',
      ')',
      'if /i "%~1"=="discover" (',
      `  echo ${noJsPageRecord}`,
      `  echo ${noJsListRecord}`,
      `  echo ${noJsItemRecord}`,
      '  exit /b 0',
      ')',
      `echo ${noJsListRecord}`,
      `echo ${noJsItemRecord}`,
      'exit /b 0',
      '',
    ].join('\r\n'),
  );
  const contentSha256 = addonContentHash(packageRoot);
  writeFileSync(
    join(folder, 'primary.yml'),
    [
      '# Generated by ErsatzRS from a Remote Stream playlist manifest. Do not edit.',
      `provider_id: item:primary-${suffix}`,
      `url: https://example.invalid/e2e/${suffix}/primary`,
      `title: ${firstTitle}`,
      'duration: "00:20:00"',
      'is_live: false',
      'content_kind: auto',
      '',
    ].join('\n'),
  );
  writeFileSync(
    join(folder, 'review.yml'),
    [
      '# Generated by ErsatzRS from a Remote Stream playlist manifest. Do not edit.',
      `provider_id: item:review-${suffix}`,
      `url: https://example.invalid/e2e/${suffix}/review`,
      `title: ${secondTitle}`,
      'duration: "00:10:00"',
      'is_live: false',
      'content_kind: auto',
      '',
    ].join('\n'),
  );

  const manifest = JSON.stringify({
    manifest_version: 3,
    id: addonId,
    version: '1.0.0',
    name: { 'en-US': `E2E Metadata Provider ${testInfo.project.name}` },
    summary: { 'en-US': 'Browser metadata editor proof' },
    icon: null,
    license: 'Zlib',
    source_url: 'https://example.invalid/e2e-metadata-addon',
    host_version: '*',
    capabilities: [
      { id: 'addon.check.v1' },
      { id: 'media-list.import.v1' },
      { id: 'media-list.list.v4' },
    ],
    dependencies: [],
    entrypoints: {
      any: { path: 'addon.sh', kind: 'posix_shell' },
      'win-x64': { path: 'addon.cmd', kind: 'windows_batch' },
    },
    permissions: {
      network_hosts: [],
      media_read: false,
      managed_ffmpeg: false,
      external_commands: [],
    },
    settings: [],
    panels: [],
  });
  const providerMetadata = JSON.stringify({
    title: 'Provider refreshed',
    plot: 'Provider collection summary',
    genres: ['Documentary'],
    tags: ['archive', 'television'],
  });
  const metadataOverride = JSON.stringify({ title: 'Operator draft' });
  const metadataConflict = JSON.stringify({
    title: {
      provider_before: 'Provider original',
      provider_after: 'Provider refreshed',
      operator_value: 'Operator draft',
    },
  });
  const firstProviderMetadata = JSON.stringify({
    title: firstTitle,
    plot: 'Primary item provider summary',
    release_date: '2026-08-20',
  });
  const secondProviderMetadata = JSON.stringify({
    title: secondTitle,
    plot: 'Review item provider summary',
    release_date: '2026-08-21',
  });

  sqlite(`
DELETE FROM "AddonMediaListPublicationJob" WHERE "AddonMediaListId" = ${listId};
DELETE FROM "AddonMediaListSourceProbeJob" WHERE "AddonMediaListId" = ${listId};
DELETE FROM "AddonMediaListLifecycleJob" WHERE "AddonMediaListId" = ${listId};
DELETE FROM "AddonMediaListSelection" WHERE "AddonMediaListId" = ${listId};
-- Cascade retained snapshot children before reusing this fixture's list identity.
PRAGMA foreign_keys = ON;
DELETE FROM "AddonMediaListRevision" WHERE "AddonMediaListId" = ${listId};
DELETE FROM "AddonMediaListRevisionFileBlob" WHERE "AddonMediaListId" = ${listId};
PRAGMA foreign_keys = OFF;
DELETE FROM "ManagedLibraryLease"
WHERE "OperationId" IN (
  SELECT "Id" FROM "ManagedLibraryOperation" WHERE "LibraryId" = ${libraryId}
);
DELETE FROM "ManagedLibraryOperation" WHERE "LibraryId" = ${libraryId};
DELETE FROM "AddonMediaListArtworkMaterialization" WHERE "AddonMediaListId" = ${listId};
DELETE FROM "AddonMediaListArtworkUpload" WHERE "AddonMediaListId" = ${listId};
DELETE FROM "AddonMediaListDetailJob" WHERE "AddonMediaListId" = ${listId};
DELETE FROM "AddonMediaListImportItem"
WHERE "AddonMediaListImportJobId" IN (
  SELECT "Id" FROM "AddonMediaListImportJob" WHERE "AddonMediaListId" = ${listId}
);
DELETE FROM "AddonMediaListImportCursor"
WHERE "AddonMediaListImportJobId" IN (
  SELECT "Id" FROM "AddonMediaListImportJob" WHERE "AddonMediaListId" = ${listId}
);
DELETE FROM "AddonMediaListImportJob" WHERE "AddonMediaListId" = ${listId};
DELETE FROM "AddonMediaListLibrary" WHERE "AddonMediaListId" = ${listId};
DELETE FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ${listId};
DELETE FROM "AddonMediaList" WHERE "Id" = ${listId};
DELETE FROM "LibraryPath" WHERE "Id" = ${libraryPathId};
DELETE FROM "LocalLibrary" WHERE "Id" = ${libraryId};
DELETE FROM "Library" WHERE "Id" = ${libraryId};
DELETE FROM "LocalMediaSource" WHERE "Id" = ${sourceId};
DELETE FROM "MediaSource" WHERE "Id" = ${sourceId};

INSERT INTO "AddonInstallation" (
  "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
  "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt", "TrustTier"
) VALUES (
  '${sqlEscape(addonId)}', '1.0.0', 1, 'e2e-package', 'e2e-content',
  '${sqlEscape(manifest)}', '${sqlEscape(packageRoot)}',
  '2026-08-21T00:00:00Z', '2026-08-21T00:00:00Z', 'verified'
) ON CONFLICT("AddonId") DO UPDATE SET
  "Enabled" = 1,
  "ContentSha256" = excluded."ContentSha256",
  "ManifestJson" = excluded."ManifestJson",
  "InstallPath" = excluded."InstallPath",
  "UpdatedAt" = excluded."UpdatedAt";

INSERT INTO "MediaSource" ("Id") VALUES (${sourceId});
INSERT INTO "LocalMediaSource" ("Id") VALUES (${sourceId});
INSERT INTO "Library" ("Id", "MediaSourceId", "MediaKind", "Name")
  VALUES (${libraryId}, ${sourceId}, 7, 'E2E metadata remote streams');
INSERT INTO "LocalLibrary" ("Id") VALUES (${libraryId});
INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path")
  VALUES (${libraryPathId}, ${libraryId}, '${sqlEscape(folder)}');

INSERT INTO "AddonMediaList" (
  "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "Description", "ItemCount",
  "AutoRefresh", "GeneratePlaylist", "ProviderMetadataJson", "MetadataOverrideJson",
  "MetadataConflictJson", "ReviewState", "PresentationKind", "RequestedPresentationKind"
) VALUES (
  ${listId}, '${sqlEscape(addonId)}', 'e2e/metadata-${suffix}',
  'https://example.invalid/e2e/${suffix}/list', 'Operator draft',
  'Provider collection summary', 2, 0, 0,
  '${sqlEscape(providerMetadata)}', '${sqlEscape(metadataOverride)}',
  '${sqlEscape(metadataConflict)}', 1, 0, 0
);
INSERT INTO "AddonMediaListLibrary" (
  "AddonMediaListId", "LibraryId", "FolderName", "ManifestPath"
) VALUES (${listId}, ${libraryId}, 'e2e-addon-metadata-${suffix}', '${sqlEscape(folder)}');

INSERT INTO "AddonMediaListItem" (
  "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind", "Rank", "Title",
  "SourceUrl", "DurationSeconds", "Availability", "ContentKind", "Included", "Published",
  "ProviderMetadataJson", "MetadataOverrideJson", "ProviderLiveness", "EffectiveLiveness"
) VALUES
  (${firstItemId}, ${listId}, 'item:primary-${suffix}', '${sqlEscape(firstTitle)}', 4, 0,
   '${sqlEscape(firstTitle)}', 'https://example.invalid/e2e/${suffix}/primary', 1200, 0, 0, 1, 0,
   '${sqlEscape(firstProviderMetadata)}', '{}', 1, 1),
  (${secondItemId}, ${listId}, 'item:review-${suffix}', '${sqlEscape(secondTitle)}', 4, 1,
   '${sqlEscape(secondTitle)}', 'https://example.invalid/e2e/${suffix}/review', 600, 2, 0, 1, 0,
   '${sqlEscape(secondProviderMetadata)}', '{}', 1, 1);
`);

  sqlite(`UPDATE "AddonInstallation" SET "ContentSha256" = '${contentSha256}' WHERE "AddonId" = '${sqlEscape(addonId)}';`);

  return {
    addonId,
    listId,
    firstItemId,
    secondItemId,
    firstTitle,
    secondTitle,
    noJsProviderId,
    noJsSourceUrl,
    noJsListName,
  };
}
