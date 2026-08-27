import { expect, Page, TestInfo, test } from '@playwright/test';
import { createHash } from 'crypto';
import { execFileSync } from 'child_process';
import { chmodSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'fs';
import { dirname, join, relative } from 'path';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

type MetadataSeed = {
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

function projectBase(testInfo: TestInfo): number {
  const projectOffset = Array.from(testInfo.project.name).reduce(
    (total, character) => total + character.charCodeAt(0),
    0,
  ) * 1_000;
  return 760_000 + projectOffset;
}

function sqlEscape(value: string): string {
  return value.replace(/'/g, "''");
}

function sqlite(sql: string): string {
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

function watchRuntimeErrors(page: Page): string[] {
  const runtimeErrors: string[] = [];
  page.on('pageerror', (error) => {
    const text = error.stack ?? error.message;
    if (
      !(
        text.includes('AbortError') &&
        text.toLowerCase().includes('operation was aborted')
      ) &&
      !text.includes('NetworkError when attempting to fetch resource')
    ) {
      runtimeErrors.push(text);
    }
  });
  page.on('console', (message) => {
    if (message.type() !== 'error') {
      return;
    }
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
    form: {
      is_dark_mode: 'true',
      language: 'en-us',
    },
    maxRedirects: 0,
  });
}

async function contrastAgainstPageBackground(page: Page, selector: string): Promise<number> {
  return page.locator(selector).first().evaluate((element) => {
    const parseColor = (value: string): [number, number, number, number] => {
      const channels = value.match(/[\d.]+/g)?.map(Number) ?? [];
      if (channels.length < 3) {
        throw new Error(`Could not parse computed color: ${value}`);
      }
      return [channels[0], channels[1], channels[2], channels[3] ?? 1];
    };
    const probe = document.createElement('span');
    probe.style.color = 'var(--color-bg)';
    element.appendChild(probe);
    const background = parseColor(getComputedStyle(probe).color);
    probe.remove();
    const foreground = parseColor(getComputedStyle(element).color);
    const composited = foreground.slice(0, 3).map(
      (channel, index) => channel * foreground[3] + background[index] * (1 - foreground[3]),
    );
    const luminance = (channels: number[]): number => channels
      .map((channel) => channel / 255)
      .map((channel) => (channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4))
      .reduce((total, channel, index) => total + channel * [0.2126, 0.7152, 0.0722][index], 0);
    const foregroundLuminance = luminance(composited);
    const backgroundLuminance = luminance(background.slice(0, 3));
    return (
      (Math.max(foregroundLuminance, backgroundLuminance) + 0.05)
      / (Math.min(foregroundLuminance, backgroundLuminance) + 0.05)
    );
  });
}

async function gotoHydrated(page: Page, route: string): Promise<void> {
  const response = await page.goto(route, { waitUntil: 'domcontentloaded' });
  expect(response?.status(), `GET ${route}`).toBe(200);
  await expect(page.locator('[data-not-found]')).toHaveCount(0);
  await waitUntilHydrated(page);
}

async function waitUntilHydrated(page: Page): Promise<void> {
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

function seedMetadataEditor(testInfo: TestInfo): MetadataSeed {
  if (!dbPath) {
    throw new Error('E2E database path is required');
  }
  const base = projectBase(testInfo);
  const suffix = testInfo.project.name.toLowerCase().replace(/[^a-z0-9]+/g, '-');
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

function seedPaginationItems(seed: MetadataSeed, testInfo: TestInfo): void {
  const suffix = testInfo.project.name.toLowerCase().replace(/[^a-z0-9]+/g, '-');
  const values = Array.from({ length: 26 }, (_, offset) => {
    const rank = offset + 2;
    const itemId = seed.secondItemId + 100 + rank;
    const title = `E2E Metadata Extra ${rank} ${testInfo.project.name}`;
    const releaseDate = `2026-07-${String((rank % 28) + 1).padStart(2, '0')}`;
    const metadata = JSON.stringify({ title, release_date: releaseDate });
    return `
  (${itemId}, ${seed.listId}, 'item:extra-${suffix}-${rank}', '${sqlEscape(title)}', 4, ${rank},
   '${sqlEscape(title)}', 'https://example.invalid/e2e/${suffix}/extra-${rank}', 600, 0, 1, 1, 0,
   '${sqlEscape(metadata)}', '{}', 1, 1)`;
  }).join(',');

  sqlite(`
UPDATE "AddonMediaList" SET "ItemCount" = 28 WHERE "Id" = ${seed.listId};
INSERT INTO "AddonMediaListItem" (
  "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind", "Rank", "Title",
  "SourceUrl", "DurationSeconds", "Availability", "ContentKind", "Included", "Published",
  "ProviderMetadataJson", "MetadataOverrideJson", "ProviderLiveness", "EffectiveLiveness"
) VALUES${values};
`);
}

function seedHighCardinalityNumberingBlockers(seed: MetadataSeed, testInfo: TestInfo): void {
  const suffix = testInfo.project.name.toLowerCase().replace(/[^a-z0-9]+/g, '-');
  const values = Array.from({ length: 48 }, (_, rank) => {
    const itemId = seed.secondItemId + 1_000 + rank;
    const title = `Glamourland episode ${rank + 1}`;
    const metadata = JSON.stringify({ title, release_date: `1986-09-${String((rank % 28) + 1).padStart(2, '0')}` });
    return `
  (${itemId}, ${seed.listId}, 'item:numbering-${suffix}-${rank}', '${sqlEscape(title)}', 4, ${rank},
   '${sqlEscape(title)}', 'https://example.invalid/e2e/${suffix}/numbering-${rank}', 1800, 0, 1, 1, 0,
   '${sqlEscape(metadata)}', '{}', 1, 1)`;
  }).join(',');

  sqlite(`
DELETE FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ${seed.listId};
UPDATE "AddonMediaList"
SET "Name" = 'Glamourland', "ItemCount" = 48, "ReviewState" = 1,
    "PresentationKind" = 2, "RequestedPresentationKind" = 2,
    "ProviderMetadataJson" = '{"title":"Glamourland","plot":"Provider collection summary"}',
    "MetadataOverrideJson" = '{}', "MetadataConflictJson" = NULL
WHERE "Id" = ${seed.listId};
INSERT INTO "AddonMediaListItem" (
  "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind", "Rank", "Title",
  "SourceUrl", "DurationSeconds", "Availability", "ContentKind", "Included", "Published",
  "ProviderMetadataJson", "MetadataOverrideJson", "ProviderLiveness", "EffectiveLiveness"
) VALUES${values};
`);
}

function seedDetailProgress(seed: MetadataSeed, testInfo: TestInfo): void {
  const suffix = testInfo.project.name.toLowerCase().replace(/[^a-z0-9]+/g, '-');
  const providerHost = `e2e-${suffix}.invalid`;
  sqlite(`
UPDATE "AddonMediaList"
SET "ReviewState" = 2, "PublishedRevision" = 1, "DetailImportPolicy" = 1,
    "DetailImportAuthorizedAt" = '2026-08-21T00:02:00Z'
WHERE "Id" = ${seed.listId};

INSERT INTO "AddonMediaListImportJob" (
  "AddonMediaListId", "Generation", "State", "RecordCapability", "DiscoveredCount",
  "TotalHint", "OverviewComplete", "LastProgressAt", "CreatedAt", "UpdatedAt", "CompletedAt"
) VALUES (
  ${seed.listId}, 1, 3, 'media-list.list.v4', 2, 2, 1,
  '2026-08-21T00:02:00Z', '2026-08-21T00:00:00Z',
  '2026-08-21T00:02:00Z', '2026-08-21T00:02:00Z'
);

INSERT INTO "AddonMediaListDetailJob" (
  "AddonMediaListItemId", "AddonMediaListId", "ProviderHost", "State", "ManualPriority",
  "OverviewFingerprint", "OverviewRecordJson", "LeaseOwner", "LeaseExpiresAt",
  "StartedAt", "UpdatedAt"
) VALUES
  (${seed.firstItemId}, ${seed.listId}, '${providerHost}', 3, 0,
   'first-overview', '{}', 'e2e-browser-fixture', '2099-01-01T00:00:00Z',
   '2026-08-21T00:02:00Z', '2026-08-21T00:02:00Z'),
  (${seed.secondItemId}, ${seed.listId}, '${providerHost}', 0, 0,
   'second-overview', '{}', NULL, NULL, NULL, '2026-08-21T00:02:00Z');

DELETE FROM "AddonProviderHostCooldown" WHERE "ProviderHost" = '${providerHost}';
INSERT INTO "AddonProviderHostCooldown" (
  "ProviderHost", "NextEligibleAt", "SampledDelaySeconds", "LastAttemptAt",
  "LeaseOwner", "LeaseExpiresAt", "UpdatedAt"
) VALUES (
  '${providerHost}', '2099-01-01T00:00:00Z', 16, '2026-08-21T00:02:00Z',
  'e2e-browser-fixture', '2099-01-01T00:00:00Z', '2026-08-21T00:02:00Z'
);
`);
}

test.describe('shared add-on metadata editor', () => {
  test.skip(!dbPath, 'E2E_DB_PATH or ERSATZRS_DB_PATH is required');

  test('keeps source and import metadata together on the add page', async ({ page }) => {
    const runtimeErrors = watchRuntimeErrors(page);
    await forceEnglish(page);
    await gotoHydrated(page, '/media/addon-lists/add');

    await expect(page.getByRole('heading', { name: 'Add add-on list' })).toBeVisible();
    await expect(page.getByLabel('Add-on list URL')).toBeVisible();
    await expect(page.getByLabel('Add-on', { exact: true })).toHaveValue('');

    const collectionType = page.getByLabel('Collection Type', { exact: true });
    await expect(collectionType).toHaveValue('television_show');
    await collectionType.selectOption('movies');
    await expect(collectionType).toHaveValue('movies');

    await expect(page.getByLabel('Storage folder')).toBeVisible();
    await expect(
      page.getByRole('checkbox', { name: 'Update list from the provider once each day' }),
    ).toBeChecked();
    await expect(
      page.getByRole('checkbox', { name: 'Create playlist in original order' }),
    ).not.toBeChecked();
    const loadDetails = page.getByRole('checkbox', {
      name: 'Load item details automatically',
    });
    await expect(loadDetails).toBeChecked();
    await expect(page.getByText(/playback-required technical details.*before the first publication/i)).toBeVisible();
    await loadDetails.uncheck();
    await expect(
      page.getByText(/request descriptive details manually.*before publication/i),
    ).toBeVisible();
    await expect(page.getByRole('button', { name: 'Import' })).toBeEnabled();
    expect(runtimeErrors).toEqual([]);
  });

  test('materializes the exact committed overview when discovery becomes ready', async ({
    page,
  }, testInfo) => {
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedMetadataEditor(testInfo);
    const firstMetadata = JSON.stringify({
      title: seed.firstTitle,
      plot: 'Primary item provider summary',
      release_date: '2026-08-20',
    });
    const secondMetadata = JSON.stringify({
      title: seed.secondTitle,
      plot: 'Review item provider summary',
      release_date: '2026-08-21',
    });
    sqlite(`
DELETE FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ${seed.listId};
UPDATE "AddonMediaList" SET "ItemCount" = 0 WHERE "Id" = ${seed.listId};
INSERT INTO "AddonMediaListImportJob" (
  "AddonMediaListId", "Generation", "State", "RecordCapability", "DiscoveredCount",
  "OverviewComplete", "NextEligibleAt", "CreatedAt", "UpdatedAt", "LastProgressAt"
) VALUES (
  ${seed.listId}, 1, 1, 'media-list.list.v4', 0, 0, '2099-01-01T00:00:00Z',
  '2026-08-26T10:00:00Z', '2026-08-26T10:00:00Z', '2026-08-26T10:00:00Z'
);
`);
    await forceEnglish(page);
    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);
    const workflowSteps = page.locator(
      'section[aria-labelledby="addon-list-readiness-heading"] nav',
    );
    await expect(workflowSteps.locator('[aria-current="step"]')).toHaveCount(1);
    await expect(workflowSteps.locator('[aria-current="step"]')).toContainText('Import');
    const collection = page.getByTestId('addon-metadata-items');
    await expect(collection.locator('li[data-item-id]')).toHaveCount(0);

    sqlite(`
INSERT INTO "AddonMediaListItem" (
  "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind", "Rank", "Title",
  "SourceUrl", "DurationSeconds", "Availability", "ContentKind", "Included", "Published",
  "ProviderMetadataJson", "MetadataOverrideJson", "ProviderLiveness", "EffectiveLiveness"
) VALUES
  (${seed.firstItemId}, ${seed.listId}, 'item:primary-ready', '${sqlEscape(seed.firstTitle)}', 4, 0,
   '${sqlEscape(seed.firstTitle)}', 'https://example.invalid/ready/primary', 1200, 0, 0, 1, 0,
   '${sqlEscape(firstMetadata)}', '{}', 1, 1),
  (${seed.secondItemId}, ${seed.listId}, 'item:review-ready', '${sqlEscape(seed.secondTitle)}', 4, 1,
   '${sqlEscape(seed.secondTitle)}', 'https://example.invalid/ready/review', 600, 2, 0, 1, 0,
   '${sqlEscape(secondMetadata)}', '{}', 1, 1);
UPDATE "AddonMediaList"
SET "ItemCount" = 2,
    "ProviderMetadataJson" = '{"title":"Glamourland","plot":"Imported collection summary"}',
    "MetadataOverrideJson" = '{}', "MetadataConflictJson" = NULL
WHERE "Id" = ${seed.listId};
UPDATE "AddonMediaListImportJob"
SET "State" = 3, "DiscoveredCount" = 2, "TotalHint" = 2, "OverviewComplete" = 1,
    "UpdatedAt" = '2026-08-26T10:00:04Z', "LastProgressAt" = '2026-08-26T10:00:04Z',
    "CompletedAt" = '2026-08-26T10:00:04Z'
WHERE "AddonMediaListId" = ${seed.listId};
`);

    await expect(collection.locator('li[data-item-id]')).toHaveCount(2, { timeout: 15_000 });
    await expect(workflowSteps.locator('[aria-current="step"]')).toContainText('Ready to review');
    await expect(collection.getByText(seed.firstTitle)).toBeVisible();
    await expect(collection.getByText(seed.secondTitle)).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Glamourland', exact: true })).toBeVisible();
    const ids = await collection
      .locator('li[data-item-id]')
      .evaluateAll((items) => items.map((item) => item.getAttribute('data-item-id')));
    expect(ids).toEqual([String(seed.firstItemId), String(seed.secondItemId)]);
    expect(runtimeErrors).toEqual([]);
  });

  test('keeps import, save, item review, and explicit publication usable without JavaScript', async ({
    browser,
    baseURL,
  }, testInfo) => {
    test.setTimeout(120_000);
    const seed = seedMetadataEditor(testInfo);
    const invalidCollectionMetadata = JSON.stringify({
      title: '',
      artwork: [{ url: 'http://invalid.example/poster.jpg', role: 'poster' }],
    });
    sqlite(`
UPDATE "AddonMediaList"
SET "ProviderMetadataJson" = '${sqlEscape(invalidCollectionMetadata)}',
    "MetadataOverrideJson" = '{}'
WHERE "Id" = ${seed.listId};
UPDATE "AddonMediaListItem" SET "Included" = 0 WHERE "Id" = ${seed.secondItemId};
`);
    const context = await browser.newContext({ javaScriptEnabled: false, baseURL });
    const noJsPage = await context.newPage();
    try {
      await noJsPage.goto(`/media/addon-lists/add?addon_id=${encodeURIComponent(seed.addonId)}`);
      await expect(noJsPage.getByRole('heading', { name: 'Add add-on list' })).toBeVisible();
      await expect(noJsPage.getByLabel('Add-on', { exact: true })).toHaveValue(seed.addonId);
      await noJsPage.getByLabel('Add-on list URL').fill(seed.noJsSourceUrl);
      await noJsPage.getByRole('button', { name: 'Import overview' }).click();
      await expect(noJsPage).toHaveURL(/\/media\/addon-lists\/\d+\?import=1$/, {
        timeout: 60_000,
      });
      await expect
        .poll(() => sqlite(`
SELECT COUNT(*) FROM "AddonMediaList"
WHERE "AddonId" = '${sqlEscape(seed.addonId)}'
  AND "ProviderId" = '${sqlEscape(seed.noJsProviderId)}'
  AND "Name" = '${sqlEscape(seed.noJsListName)}';
`), { timeout: 60_000 })
        .toBe('1');

      await noJsPage.goto(`/media/addon-lists/${seed.listId}`);
      await expect(noJsPage.getByRole('link', { name: /collection needs a title/i })).toBeVisible();
      await expect(
        noJsPage.getByRole('link', { name: /artwork must use a valid bounded HTTPS URL/i }),
      ).toBeVisible();
      await noJsPage.getByLabel('Title', { exact: true }).fill('No-JS collection title');
      await noJsPage.getByLabel('Poster', { exact: true }).fill('https://example.invalid/poster.jpg');
      await noJsPage.getByRole('button', { name: 'Save Changes', exact: true }).click();
      await expect(noJsPage).toHaveURL(/\?status=saved$/);
      expect(
        sqlite(`
SELECT json_extract("MetadataOverrideJson", '$.title') || ',' ||
       json_extract("MetadataOverrideJson", '$.artwork[0].url')
FROM "AddonMediaList" WHERE "Id" = ${seed.listId};
`),
      ).toBe('No-JS collection title,https://example.invalid/poster.jpg');

      const firstItem = noJsPage
        .getByTestId('addon-metadata-items')
        .locator(`li[data-item-id="${seed.firstItemId}"]`);
      await firstItem.getByRole('link', { name: 'Review item' }).click();
      await expect(noJsPage).toHaveURL(
        new RegExp(`/media/addon-lists/${seed.listId}/items/${seed.firstItemId}$`),
      );
      await noJsPage.getByLabel('Title', { exact: true }).fill('No-JS reviewed item');
      await noJsPage.getByLabel('Tags', { exact: true }).fill('native\nreviewed');
      await noJsPage.getByRole('button', { name: 'Save', exact: true }).click();
      await expect(noJsPage).toHaveURL(/\?status=saved$/);
      expect(
        sqlite(`
SELECT json_extract("MetadataOverrideJson", '$.title') || ',' ||
       json_extract("MetadataOverrideJson", '$.tags[1]')
FROM "AddonMediaListItem" WHERE "Id" = ${seed.firstItemId};
`),
      ).toBe('No-JS reviewed item,reviewed');
      expect(
        sqlite(`
SELECT json_extract("MetadataOverrideJson", '$.title')
FROM "AddonMediaList" WHERE "Id" = ${seed.listId};
`),
      ).toBe('No-JS collection title');

      await noJsPage.goto(`/media/addon-lists/${seed.listId}`);
      await noJsPage
        .getByRole('checkbox', { name: 'Update list from the provider once each day' })
        .check();
      expect(
        await noJsPage
          .locator('form#addon_media_lists-list-editor-form :invalid')
          .evaluateAll((controls) =>
            controls.map((control) => ({
              name: control.getAttribute('name'),
              value: (control as HTMLInputElement).value,
              validationMessage: (control as HTMLInputElement).validationMessage,
            })),
          ),
      ).toEqual([]);
      await noJsPage.getByRole('button', { name: 'Save Changes', exact: true }).click();
      await expect(noJsPage).toHaveURL(/\?status=saved$/);
      expect(
        sqlite(`SELECT "AutoRefresh" FROM "AddonMediaList" WHERE "Id" = ${seed.listId};`),
      ).toBe('1');

      await noJsPage.getByRole('button', { name: 'Publish list', exact: true }).click();
      await expect(noJsPage).toHaveURL(/\?status=published$/, { timeout: 60_000 });
      expect(
        sqlite(`SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id" = ${seed.listId};`),
      ).toBe('1');
    } finally {
      await context.close();
    }
  });

  test('keeps grouped blockers in a non-sticky responsive readiness rail', async ({
    page,
  }, testInfo) => {
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedMetadataEditor(testInfo);
    seedHighCardinalityNumberingBlockers(seed, testInfo);
    await page.setViewportSize({ width: 1440, height: 900 });
    await forceEnglish(page);
    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);

    const readinessShell = page.locator(
      'section[aria-labelledby="addon-list-readiness-heading"]',
    );
    const collectionHero = page.locator('#addon-list-description').locator('xpath=ancestor::header');
    const itemHeading = page.locator('#addon-list-items-heading');
    const [shellBox, heroBox, itemHeadingBox] = await Promise.all([
      readinessShell.boundingBox(),
      collectionHero.boundingBox(),
      itemHeading.boundingBox(),
    ]);
    expect(await readinessShell.evaluate((element) => getComputedStyle(element).position)).toBe(
      'static',
    );
    expect(heroBox?.y ?? Number.POSITIVE_INFINITY).toBeLessThan(
      shellBox?.y ?? Number.NEGATIVE_INFINITY,
    );
    expect(shellBox?.x ?? 0).toBeGreaterThan(itemHeadingBox?.x ?? Number.POSITIVE_INFINITY);

    const blockerPanel = page.locator('#addon-list-publication-blockers');
    const groupedBlockers = blockerPanel.getByRole('list', { name: 'Blockers' }).locator(':scope > li');
    await expect(groupedBlockers).toHaveCount(1);
    const numberingBlocker = groupedBlockers.getByRole('link', {
      name: /48 items.*valid season and episode number/i,
    });
    await expect(numberingBlocker).toBeVisible();

    const firstItem = page.getByTestId('addon-metadata-items').locator('li[data-item-id]').first();
    await expect(firstItem.getByText('Enter a valid season and episode number.', { exact: true })).toHaveCount(2);

    await page.setViewportSize({ width: 1180, height: 900 });
    const [stackedShellBox, stackedItemHeadingBox] = await Promise.all([
      readinessShell.boundingBox(),
      itemHeading.boundingBox(),
    ]);
    expect(stackedShellBox?.y ?? Number.POSITIVE_INFINITY).toBeLessThan(
      stackedItemHeadingBox?.y ?? Number.NEGATIVE_INFINITY,
    );
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= document.documentElement.clientWidth + 1,
      ),
    ).toBe(true);

    await numberingBlocker.click();
    await expect(page.locator('#episode-numbering-panel')).toBeVisible();
    await expect(page.getByRole('button', { name: /^number episodes$/i })).toHaveAttribute(
      'aria-expanded',
      'true',
    );
    expect(runtimeErrors).toEqual([]);
  });

  test('reflows review cards and keeps the mobile navigation keyboard-safe', async ({
    page,
  }, testInfo) => {
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedMetadataEditor(testInfo);
    const longDescription = Array.from(
      { length: 16 },
      (_, index) => `Localized provider description sentence ${index + 1}.`,
    ).join(' ');
    sqlite(`
UPDATE "AddonMediaList"
SET "ProviderMetadataJson" = json_set("ProviderMetadataJson", '$.plot', '${sqlEscape(longDescription)}')
WHERE "Id" = ${seed.listId};
`);
    await page.setViewportSize({ width: 320, height: 800 });
    await forceEnglish(page);
    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);

    const toggle = page.getByRole('button', { name: 'Toggle navigation' });
    const drawer = page.locator('[data-nav-drawer]');
    await expect(toggle).toHaveAttribute('aria-expanded', 'false');
    await expect(drawer).toHaveAttribute('aria-hidden', 'true');
    await expect(drawer).toHaveAttribute('inert', '');
    await toggle.focus();
    await page.keyboard.press('Enter');
    await expect(toggle).toHaveAttribute('aria-expanded', 'true');
    await expect(drawer).toHaveAttribute('aria-hidden', 'false');
    await expect(drawer).not.toHaveAttribute('inert', '');
    expect(await drawer.evaluate((element) => element.contains(document.activeElement))).toBe(true);
    await page.keyboard.press('Escape');
    await expect(toggle).toBeFocused();
    await expect(drawer).toHaveAttribute('inert', '');

    const collection = page.getByTestId('addon-metadata-items');
    const firstItem = collection.locator('li[data-item-id]').first();
    for (const label of [
      'First aired',
      'Season, Episode',
      'Media Kind',
      'Playback behavior',
      'Status',
    ]) {
      await expect(firstItem.getByText(label, { exact: true })).toBeVisible();
    }
    await expect(firstItem.getByRole('checkbox', { name: 'Include in publication' })).toBeVisible();
    const review = firstItem.getByRole('link', { name: 'Review item' });
    const reviewBox = await review.boundingBox();
    expect(reviewBox?.height).toBeGreaterThanOrEqual(44);
    expect(reviewBox?.width).toBeGreaterThanOrEqual(44);
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= document.documentElement.clientWidth + 1,
      ),
    ).toBe(true);
    expect(await collection.evaluate((element) => element.scrollWidth <= element.clientWidth + 1)).toBe(
      true,
    );

    const descriptionToggle = page.locator('button[aria-controls="addon-list-description"]');
    await expect(descriptionToggle).toHaveAttribute('aria-expanded', 'false');
    await descriptionToggle.click();
    await expect(descriptionToggle).toHaveAttribute('aria-expanded', 'true');

    for (const width of [390, 900, 1440, 1920, 2560, 3840]) {
      await page.setViewportSize({ width, height: 900 });
      expect(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= document.documentElement.clientWidth + 1,
        ),
      ).toBe(true);
      const mainGeometry = await page.locator('main').first().evaluate((main) => ({
        clientWidth: main.clientWidth,
        scrollWidth: main.scrollWidth,
        scrollLeft: main.scrollLeft,
      }));
      expect(mainGeometry.scrollLeft).toBe(0);
      expect(mainGeometry.scrollWidth).toBeLessThanOrEqual(mainGeometry.clientWidth + 1);
    }
    await page.setViewportSize({ width: 1440, height: 900 });
    const readinessShell = page.locator(
      'section[aria-labelledby="addon-list-readiness-heading"]',
    );
    const readinessHeading = page.locator('#addon-list-readiness-heading');
    await expect(readinessHeading).toHaveCSS('position', 'relative');
    const readinessHeadingType = await readinessHeading.evaluate((heading) => {
      const style = getComputedStyle(heading);
      return {
        fontSize: Number.parseFloat(style.fontSize),
        fontWeight: Number.parseInt(style.fontWeight, 10),
      };
    });
    expect(readinessHeadingType.fontSize).toBeLessThanOrEqual(16);
    expect(readinessHeadingType.fontWeight).toBeLessThanOrEqual(700);
    expect(await contrastAgainstPageBackground(page, '#addon-list-readiness-heading')).toBeGreaterThanOrEqual(4.5);
    await readinessShell.screenshot({
      path: testInfo.outputPath('addon-list-readiness-wide.png'),
    });
    await page.setViewportSize({ width: 3840, height: 2160 });
    const mainBox = await page.locator('main').first().boundingBox();
    const editorBox = await page.locator('form#addon_media_lists-list-editor-form').boundingBox();
    const wideReadinessBox = await readinessShell.boundingBox();
    expect(editorBox?.x ?? Number.POSITIVE_INFINITY).toBeLessThan(
      wideReadinessBox?.x ?? Number.NEGATIVE_INFINITY,
    );
    expect((editorBox?.width ?? 0) + (wideReadinessBox?.width ?? 0)).toBeGreaterThanOrEqual(
      (mainBox?.width ?? 0) - 120,
    );
    const wideFirstItem = page.getByTestId('addon-metadata-items').locator('li[data-item-id]').first();
    expect((await wideFirstItem.boundingBox())?.height ?? Number.POSITIVE_INFINITY).toBeLessThanOrEqual(58);
    await expect(wideFirstItem.getByRole('link', { name: 'Review item' })).toHaveCount(1);
    await expect(page.getByTestId('addon-item-column-header')).toBeVisible();
    await page.getByRole('link', { name: 'Edit metadata' }).click();
    await expect(page.getByTestId('addon-general-metadata')).toBeVisible();
    await expect(page.getByTestId('addon-general-metadata')).toBeFocused();
    expect(await contrastAgainstPageBackground(page, '#addon-list-description')).toBeGreaterThanOrEqual(4.5);
    expect(await contrastAgainstPageBackground(page, '[data-testid="addon-metadata-conflict-summary"]')).toBeGreaterThanOrEqual(4.5);
    await page.screenshot({
      path: testInfo.outputPath('addon-metadata-editor-wide-review.png'),
      fullPage: false,
    });
    await page.getByRole('button', { name: 'Edit metadata' }).click();
    await page.getByTestId('addon-metadata-items').scrollIntoViewIfNeeded();
    await page.screenshot({
      path: testInfo.outputPath('addon-metadata-items-wide-review.png'),
      fullPage: false,
    });
    await page.setViewportSize({ width: 390, height: 844 });
    const mobileWorkflow = page.locator(
      'section[aria-labelledby="addon-list-readiness-heading"]',
    );
    const [mobileWorkflowBox, mobileMainBox] = await Promise.all([
      mobileWorkflow.boundingBox(),
      page.locator('main').first().boundingBox(),
    ]);
    await expect(mobileWorkflow).toHaveCSS('position', 'static');
    expect(mobileWorkflowBox?.width ?? Number.POSITIVE_INFINITY).toBeLessThanOrEqual(
      (mobileMainBox?.width ?? 0) + 1,
    );
    await expect(
      mobileWorkflow.locator('[role="status"]').first(),
    ).toBeVisible();
    await mobileWorkflow.screenshot({
      path: testInfo.outputPath('addon-list-readiness-mobile.png'),
    });
    await page.getByTestId('addon-metadata-items').locator('li[data-item-id]').first().scrollIntoViewIfNeeded();
    await page.screenshot({
      path: testInfo.outputPath('addon-metadata-items-mobile-review.png'),
      fullPage: false,
    });
    await page.setViewportSize({ width: 640, height: 900 });
    await page.evaluate(() => {
      document.body.style.zoom = '2';
    });
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= document.documentElement.clientWidth + 1,
      ),
    ).toBe(true);
    const zoomedMain = await page.locator('main').first().evaluate((main) => ({
      clientWidth: main.clientWidth,
      scrollWidth: main.scrollWidth,
      scrollLeft: main.scrollLeft,
    }));
    expect(zoomedMain.scrollLeft).toBe(0);
    expect(zoomedMain.scrollWidth).toBeLessThanOrEqual(zoomedMain.clientWidth + 1);
    await page.evaluate(() => {
      document.body.style.zoom = '';
    });

    await page.request.post('/api/settings/ui/save', {
      form: { is_dark_mode: 'false', language: 'en-us' },
      maxRedirects: 0,
    });
    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);
    await expect(page.locator('[data-theme="light"]')).toBeVisible();
    expect(await contrastAgainstPageBackground(page, '#addon-list-description')).toBeGreaterThanOrEqual(4.5);
    expect(await contrastAgainstPageBackground(page, '[data-testid="addon-metadata-conflict-summary"]')).toBeGreaterThanOrEqual(4.5);
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= document.documentElement.clientWidth + 1,
      ),
    ).toBe(true);

    await forceEnglish(page);
    await gotoHydrated(page, `/settings/addons/${encodeURIComponent(seed.addonId)}`);
    await expect(page.getByRole('link', { name: 'Import a list' })).toHaveAttribute(
      'href',
      `/media/addon-lists/add?addon_id=${encodeURIComponent(seed.addonId)}`,
    );
    expect(runtimeErrors).toEqual([]);
  });

  test('shows durable detail progress and accessible per-item actions', async ({
    page,
  }, testInfo) => {
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedMetadataEditor(testInfo);
    seedDetailProgress(seed, testInfo);
    await forceEnglish(page);
    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);

    const progress = page.locator('section[data-import-state="Ready"]');
    await expect(progress).toBeVisible();
    await expect(progress.getByText('Loading item details', { exact: true })).toBeVisible();
    await expect(progress.getByRole('progressbar')).toHaveAttribute('aria-valuemax', '2');
    await expect(progress.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '0');
    await expect(progress.locator('[aria-live="polite"]').first()).toContainText(
      'Loading item details',
    );

    const itemsTable = page.getByTestId('addon-metadata-items');
    const running = itemsTable.getByRole('button', {
      name: `Loading details: ${seed.firstTitle}`,
    });
    await expect(running).toBeDisabled();
    await expect(running).toHaveAttribute('aria-busy', 'true');

    const fetch = itemsTable.getByRole('button', {
      name: `Fetch details: ${seed.secondTitle}`,
    });
    await fetch.click();
    await expect
      .poll(() =>
        sqlite(`
SELECT "State" || ',' || "ManualPriority"
FROM "AddonMediaListDetailJob" WHERE "AddonMediaListItemId" = ${seed.secondItemId};
`),
      )
      .toMatch(/^[12],1$/);
    await expect(
      itemsTable
        .locator('li[data-item-id]')
        .filter({ hasText: seed.secondTitle })
        .first()
        .locator('button[aria-busy="true"]'),
    ).toBeVisible();
    expect(runtimeErrors).toEqual([]);
  });

  test('queues a missing finite duration before publication instead of failing save', async ({
    page,
  }, testInfo) => {
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedMetadataEditor(testInfo);
    const suffix = testInfo.project.name.toLowerCase().replace(/[^a-z0-9]+/g, '-');
    const providerHost = `e2e-duration-${suffix}.invalid`;
    sqlite(`
UPDATE "AddonMediaListItem" SET "DurationSeconds" = NULL WHERE "Id" = ${seed.firstItemId};
UPDATE "AddonMediaListItem" SET "Included" = 0 WHERE "Id" = ${seed.secondItemId};
INSERT INTO "AddonMediaListImportJob" (
  "AddonMediaListId", "Generation", "State", "RecordCapability", "DiscoveredCount",
  "TotalHint", "OverviewComplete", "LastProgressAt", "CreatedAt", "UpdatedAt", "CompletedAt"
) VALUES (
  ${seed.listId}, 1, 3, 'media-list.list.v4', 2, 2, 1,
  '2026-08-26T10:00:00Z', '2026-08-26T10:00:00Z',
  '2026-08-26T10:00:00Z', '2026-08-26T10:00:00Z'
);
INSERT INTO "AddonMediaListDetailJob" (
  "AddonMediaListItemId", "AddonMediaListId", "ProviderHost", "State", "ManualPriority",
  "OverviewFingerprint", "OverviewRecordJson", "UpdatedAt"
) VALUES (
  ${seed.firstItemId}, ${seed.listId}, '${providerHost}', 0, 0,
  'duration-overview', '{}', '2026-08-26T10:00:00Z'
);
INSERT INTO "AddonProviderHostCooldown" (
  "ProviderHost", "NextEligibleAt", "SampledDelaySeconds", "LastAttemptAt", "UpdatedAt"
) VALUES (
  '${providerHost}', '2099-01-01T00:00:00Z', 16,
  '2026-08-26T10:00:00Z', '2026-08-26T10:00:00Z'
);
`);
    await forceEnglish(page);
    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);

    await page.getByRole('button', { name: 'Prepare publication', exact: true }).click();
    await expect(page.getByText(/playback details queued: 1/i)).toBeVisible();
    await expect(page.getByText('Preparing publication', { exact: true })).toBeVisible();
    await expect(
      page.locator(
        'section[aria-labelledby="addon-list-readiness-heading"] nav [aria-current="step"]',
      ),
    ).toContainText('Publish list');
    await expect
      .poll(() =>
        sqlite(`
SELECT "State" || ',' || "ManualPriority"
FROM "AddonMediaListDetailJob" WHERE "AddonMediaListItemId" = ${seed.firstItemId};
`),
      )
      .toMatch(/^[12],1$/);
    expect(
      sqlite(`SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id" = ${seed.listId};`),
    ).toBe('0');
    expect(runtimeErrors).toEqual([]);
  });

  test('previews exact numbering across pages with mobile and reduced-motion cues', async ({
    page,
  }, testInfo) => {
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedMetadataEditor(testInfo);
    sqlite(`
UPDATE "AddonMediaList"
SET "PresentationKind" = 1, "RequestedPresentationKind" = 1
WHERE "Id" = ${seed.listId};
UPDATE "AddonMediaListItem" SET "ContentKind" = 1 WHERE "AddonMediaListId" = ${seed.listId};
UPDATE "AddonMediaListItem"
SET "ProviderMetadataJson" = json_set(
      "ProviderMetadataJson", '$.season', 4, '$.episode', 9
    ), "Season" = 4, "Episode" = 9
WHERE "Id" = ${seed.firstItemId};
`);
    seedPaginationItems(seed, testInfo);
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await forceEnglish(page);
    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);

    const itemsTable = page.getByTestId('addon-metadata-items');
    const firstRow = itemsTable
      .locator('li[data-item-id]')
      .filter({ hasText: seed.firstTitle })
      .first();
    await expect(firstRow.locator('[aria-label="Number 1"]')).toHaveText('1');
    await expect(firstRow.getByText('First aired', { exact: true })).toBeVisible();
    await expect(firstRow.getByText('Season, Episode', { exact: true })).toBeVisible();
    await expect(firstRow.getByText('2026-08-20', { exact: true })).toBeVisible();
    await expect(firstRow.getByRole('spinbutton', { name: 'Season', exact: true })).toHaveValue('4');
    await expect(firstRow.getByRole('spinbutton', { name: 'Episode', exact: true })).toHaveValue('9');

    await page.getByRole('button', { name: /^number episodes$/i }).click();
    const panel = page.locator('#episode-numbering-panel');
    await expect(panel.getByText(/27 episodes · s01e01–s01e27/i)).toBeVisible();
    await expect(
      panel.getByText(/3 additional affected items are on other pages/i),
    ).toBeVisible();
    await expect(itemsTable.getByText('S01E01', { exact: true })).toBeVisible();
    await expect(itemsTable.getByText('S01E24', { exact: true })).toBeVisible();
    const firstPageIds = await itemsTable
      .locator('li[data-item-id]')
      .evaluateAll((rows) => rows.map((row) => row.getAttribute('data-item-id')));
    expect(new Set(firstPageIds).size).toBe(firstPageIds.length);

    const nextPage = page.getByRole('button', { name: 'Next page' });
    await nextPage.focus();
    await page.keyboard.press('Enter');
    await expect(itemsTable.getByText('S01E25', { exact: true })).toBeVisible();
    await expect(itemsTable.getByText('S01E27', { exact: true })).toBeVisible();
    const secondPageIds = await itemsTable
      .locator('li[data-item-id]')
      .evaluateAll((rows) => rows.map((row) => row.getAttribute('data-item-id')));
    expect(new Set(secondPageIds).size).toBe(secondPageIds.length);

    await page.setViewportSize({ width: 390, height: 844 });
    await expect(itemsTable).toBeVisible();
    await expect(page.getByRole('button', { name: 'Previous page' })).toBeVisible();
    const itemTransition = await itemsTable
      .locator('li[data-item-id]')
      .first()
      .evaluate((element) => getComputedStyle(element).transitionDuration);
    expect(itemTransition).toBe('0s');
    expect(runtimeErrors).toEqual([]);
  });

  test('edits and undoes media kind and playback behavior in explicit bulk scopes', async ({
    page,
  }, testInfo) => {
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedMetadataEditor(testInfo);
    sqlite(`
UPDATE "AddonMediaListItem"
SET "ProviderLiveness" = 2, "EffectiveLiveness" = 2,
    "LivenessOverride" = NULL, "LivenessConflict" = 0
WHERE "Id" = ${seed.firstItemId};
UPDATE "AddonMediaListItem"
SET "ProviderLiveness" = 2, "EffectiveLiveness" = 1,
    "LivenessOverride" = 1, "LivenessConflict" = 1
WHERE "Id" = ${seed.secondItemId};
`);
    await forceEnglish(page);
    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);

    const itemsTable = page.getByTestId('addon-metadata-items');
    const firstRow = itemsTable
      .locator('li[data-item-id]')
      .filter({ hasText: seed.firstTitle })
      .first();
    const secondRow = itemsTable
      .locator('li[data-item-id]')
      .filter({ hasText: seed.secondTitle })
      .first();
    await expect(itemsTable.getByRole('checkbox', { name: 'Include in publication' })).toHaveCount(2);
    await expect(itemsTable.getByRole('checkbox', { name: 'Select item' })).toHaveCount(0);
    const firstBehavior = firstRow.getByLabel('Playback behavior');
    await expect(firstBehavior).toHaveValue('provider');
    await expect(firstBehavior.locator('option').first()).toHaveText(
      'Use provider value · Live source',
    );
    await expect(secondRow.getByText('Provider value changed')).toBeVisible();

    await firstBehavior.selectOption('finite');
    await expect
      .poll(() =>
        sqlite(`
SELECT COALESCE("LivenessOverride", -1) || ',' || "EffectiveLiveness" || ',' || "LivenessConflict"
FROM "AddonMediaListItem" WHERE "Id" = ${seed.firstItemId};
`),
      )
      .toBe('1,1,0');
    const bulk = page.locator('section[aria-label="Bulk edit"]');
    await bulk.getByRole('button', { name: 'Undo: Playback behavior' }).click();
    await expect
      .poll(() =>
        sqlite(`
SELECT COALESCE("LivenessOverride", -1) || ',' || "EffectiveLiveness"
FROM "AddonMediaListItem" WHERE "Id" = ${seed.firstItemId};
`),
      )
      .toBe('-1,2');

    await bulk.getByLabel('Scope').selectOption('all');
    await bulk.getByLabel('Bulk edit · Media kind').selectOption('television_episode');
    await bulk.getByLabel('Bulk edit · Playback behavior').selectOption('live');
    const applyAll = bulk.getByRole('button', { name: /apply \(2\)/i });
    await applyAll.focus();
    await page.keyboard.press('Enter');
    await expect
      .poll(() =>
        sqlite(`
SELECT group_concat("ContentKind" || ',' || COALESCE("LivenessOverride", -1) || ',' || "EffectiveLiveness", ';')
FROM (SELECT * FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ${seed.listId} ORDER BY "Rank");
`),
      )
      .toBe('1,2,2;1,2,2');

    await bulk.getByRole('button', { name: 'Undo: Bulk edit' }).click();
    await expect
      .poll(() =>
        sqlite(`
SELECT group_concat("ContentKind" || ',' || COALESCE("LivenessOverride", -1) || ',' || "EffectiveLiveness", ';')
FROM (SELECT * FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ${seed.listId} ORDER BY "Rank");
`),
      )
      .toBe('0,-1,2;0,1,1');

    await page.setViewportSize({ width: 390, height: 844 });
    const bulkLayout = await bulk.evaluate((element) => getComputedStyle(element).gridTemplateColumns);
    expect(bulkLayout.trim().split(/\s+/)).toHaveLength(1);
    await expect(applyAll).toBeVisible();
    expect(runtimeErrors).toEqual([]);
  });

  test('keeps row persistence explicit during latency and restores failed edits', async ({
    page,
  }, testInfo) => {
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedMetadataEditor(testInfo);
    sqlite(`
UPDATE "AddonMediaListItem"
SET "Included" = 0
WHERE "Id" = ${seed.secondItemId};
UPDATE "AddonMediaListItem"
SET "ProviderLiveness" = 2, "EffectiveLiveness" = 2,
    "LivenessOverride" = NULL, "LivenessConflict" = 0
WHERE "Id" = ${seed.firstItemId};
`);

    let failNextMutation = false;
    await page.route(/\/api\/apply_addon_media_list_item_review_edits/, async (route) => {
      await new Promise((resolve) => setTimeout(resolve, 750));
      if (failNextMutation) {
        failNextMutation = false;
        await route.fulfill({ status: 500, contentType: 'text/plain', body: 'injected mutation failure' });
      } else {
        await route.continue();
      }
    });

    await forceEnglish(page);
    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);
    const row = page
      .getByTestId('addon-metadata-items')
      .locator(`li[data-item-id="${seed.firstItemId}"]`);
    const behavior = row.getByLabel('Playback behavior');
    const publish = page.getByRole('button', { name: 'Publish list', exact: true });

    await behavior.selectOption('finite');
    await expect(row.getByText('Saving...', { exact: true })).toBeVisible();
    await expect(publish).toBeDisabled();
    await expect(row.getByText('Saved.', { exact: true })).toBeVisible();
    await expect(behavior).toHaveValue('finite');

    failNextMutation = true;
    await behavior.selectOption('live');
    await expect(row.getByText('Saving...', { exact: true })).toBeVisible();
    await expect(row.getByText('Error', { exact: true })).toBeVisible();
    await expect(behavior).toHaveValue('finite');
    expect(
      sqlite(`SELECT "LivenessOverride" FROM "AddonMediaListItem" WHERE "Id" = ${seed.firstItemId};`),
    ).toBe('1');
    expect(runtimeErrors).toEqual([]);
  });

  test('assigns missing episode numbers in original order and allows a bounded overwrite', async ({
    page,
  }, testInfo) => {
    test.setTimeout(60_000);
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedMetadataEditor(testInfo);
    sqlite(`
UPDATE "AddonMediaList"
SET "PresentationKind" = 1, "RequestedPresentationKind" = 1
WHERE "Id" = ${seed.listId};
UPDATE "AddonMediaListItem"
SET "ContentKind" = 1
WHERE "AddonMediaListId" = ${seed.listId};
UPDATE "AddonMediaListItem"
SET "ProviderMetadataJson" = json_set(
      "ProviderMetadataJson", '$.season', 4, '$.episode', 9
    ),
    "Season" = 4,
    "Episode" = 9
WHERE "Id" = ${seed.firstItemId};
UPDATE "AddonMediaListItem"
SET "ProviderMetadataJson" = json_set(
      "ProviderMetadataJson", '$.release_date', '2027-01-20'
    )
WHERE "Id" = ${seed.secondItemId};
`);
    await forceEnglish(page);
    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);

    const numberingButton = page.getByRole('button', { name: /^number episodes$/i });
    await expect(numberingButton).toBeVisible();
    const itemsTable = page.getByTestId('addon-metadata-items');
    const secondRow = itemsTable
      .locator('li[data-item-id]')
      .filter({ hasText: seed.secondTitle })
      .first();
    await secondRow.getByRole('spinbutton', { name: 'Season', exact: true }).fill('3');
    await secondRow.getByRole('spinbutton', { name: 'Episode', exact: true }).fill('7');
    await secondRow.getByRole('button', { name: 'Save season and episode' }).click();
    await expect
      .poll(() =>
        sqlite(`
SELECT json_extract("MetadataOverrideJson", '$.season') || ',' ||
       json_extract("MetadataOverrideJson", '$.episode')
FROM "AddonMediaListItem" WHERE "Id" = ${seed.secondItemId};
`),
      )
      .toBe('3,7');
    await page.getByRole('button', { name: 'Undo: Season, Episode' }).click();
    await expect
      .poll(() =>
        sqlite(`
SELECT COALESCE(json_extract("MetadataOverrideJson", '$.season'), -1) || ',' ||
       COALESCE(json_extract("MetadataOverrideJson", '$.episode'), -1)
FROM "AddonMediaListItem" WHERE "Id" = ${seed.secondItemId};
`),
      )
      .toBe('-1,-1');

    await numberingButton.click();
    const panel = page.locator('#episode-numbering-panel');
    await expect(panel).toBeVisible();
    await expect(panel.getByText(/1 episodes · s01e01–s01e01/i)).toBeVisible();
    await panel.getByRole('button', { name: /^apply$/i }).click();
    await expect
      .poll(() =>
        sqlite(`
SELECT json_extract("MetadataOverrideJson", '$.season') || ',' ||
       json_extract("MetadataOverrideJson", '$.episode')
FROM "AddonMediaListItem" WHERE "Id" = ${seed.secondItemId};
`),
      )
      .toBe('1,1');
    const firstRow = itemsTable
      .locator('li[data-item-id]')
      .filter({ hasText: seed.firstTitle })
      .first();
    await expect(firstRow.getByRole('spinbutton', { name: 'Season', exact: true })).toHaveValue('4');
    await expect(firstRow.getByRole('spinbutton', { name: 'Episode', exact: true })).toHaveValue('9');
    await expect(secondRow.getByRole('spinbutton', { name: 'Season', exact: true })).toHaveValue('1');
    await expect(secondRow.getByRole('spinbutton', { name: 'Episode', exact: true })).toHaveValue('1');

    await numberingButton.click();
    await panel.getByLabel(/^season$/i).fill('1');
    await panel.getByLabel(/^first episode$/i).fill('1');
    await panel.getByLabel(/^number to assign$/i).fill('1');
    await panel.getByRole('checkbox', { name: /replace existing episode numbers/i }).check();
    await expect(panel.getByText(/1 episodes · s01e02–s01e02/i)).toBeVisible();
    await panel.getByRole('button', { name: /^apply$/i }).click();
    await expect
      .poll(() =>
        sqlite(`
SELECT json_extract("MetadataOverrideJson", '$.season') || ',' ||
       json_extract("MetadataOverrideJson", '$.episode')
FROM "AddonMediaListItem" WHERE "Id" = ${seed.firstItemId};
`),
      )
      .toBe('1,2');
    await expect(firstRow.getByRole('spinbutton', { name: 'Season', exact: true })).toHaveValue('1');
    await expect(firstRow.getByRole('spinbutton', { name: 'Episode', exact: true })).toHaveValue('2');
    expect(
      sqlite(`
SELECT json_extract("MetadataOverrideJson", '$.season') || ',' ||
       json_extract("MetadataOverrideJson", '$.episode')
FROM "AddonMediaListItem" WHERE "Id" = ${seed.secondItemId};
`),
    ).toBe('1,1');

    await numberingButton.click();
    await panel
      .getByRole('checkbox', { name: /auto-detect seasons from first-air dates/i })
      .check();
    await panel.getByLabel(/^number to assign$/i).fill('');
    await panel.getByLabel('Minimum gap between seasons (days)').fill('90');
    await expect(panel.getByText(/2 episodes · s01e01–s02e01/i)).toBeVisible();
    await secondRow.getByRole('button', { name: 'Start here' }).click();
    await expect(panel.getByText(new RegExp(`#2 · ${seed.secondTitle}`))).toBeVisible();
    await expect(panel.getByText(/1 episodes · s01e01–s01e01/i)).toBeVisible();
    await panel.getByRole('button', { name: 'Start with first item' }).click();
    await expect(panel.getByText(/2 episodes · s01e01–s02e01/i)).toBeVisible();
    await panel.getByRole('button', { name: /^apply$/i }).click();
    await expect
      .poll(() =>
        sqlite(`
SELECT group_concat(
         json_extract("MetadataOverrideJson", '$.season') || ',' ||
         json_extract("MetadataOverrideJson", '$.episode'),
         ';'
       )
FROM (SELECT * FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ${seed.listId} ORDER BY "Rank");
`),
      )
      .toBe('1,1;2,1');
    expect(runtimeErrors).toEqual([]);
  });

  test('reviews, edits, uploads, rejects stale ETags, publishes, and reincludes', async ({
    page,
  }, testInfo) => {
    test.setTimeout(180_000);
    const runtimeErrors = watchRuntimeErrors(page);
    const seed = seedMetadataEditor(testInfo);
    await forceEnglish(page);
    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);

    const editorForm = page.locator('#addon_media_lists-list-editor-form');
    const hero = page.locator('#addon-list-description').locator('xpath=ancestor::header');
    await expect(hero.getByRole('heading', { name: 'Operator draft' })).toBeVisible();
    await expect(hero.getByText(/1 metadata conflicts/i)).toBeVisible();
    await hero.getByRole('button', { name: /^edit metadata$/i }).click();
    await expect(
      editorForm.getByRole('heading', { name: /^metadata conflicts$/i }),
    ).toBeVisible();
    await expect(editorForm.getByText('Provider original')).toBeVisible();
    await expect(editorForm.getByText('Provider refreshed')).toBeVisible();

    const reviewFilter = page.getByRole('button', { name: /1 review recommended/i });
    await reviewFilter.click();
    await expect(reviewFilter).toHaveAttribute('aria-pressed', 'true');
    const itemsTable = page.getByTestId('addon-metadata-items');
    await expect(itemsTable.getByText(seed.secondTitle)).toBeVisible();
    await expect(itemsTable.getByText(seed.firstTitle)).toHaveCount(0);

    let reviewRow = itemsTable
      .locator('li[data-item-id]')
      .filter({ hasText: seed.secondTitle })
      .first();
    await reviewRow.getByRole('link', { name: 'Review item', exact: true }).click();
    const inlineEditor = itemsTable
      .locator('li[data-item-id]')
      .filter({ has: page.getByLabel(/^tags/i) })
      .first();
    await expect(inlineEditor).toBeVisible();
    const manyTags = Array.from({ length: 120 }, (_, index) => `tag-${index + 1}`).join('\n');
    await inlineEditor.getByLabel(/^tags/i).fill(manyTags);
    await inlineEditor.getByRole('button', { name: /^save$/i }).click();
    await expect
      .poll(() =>
        sqlite(`
SELECT json_extract("MetadataOverrideJson", '$.tags[119]')
FROM "AddonMediaListItem" WHERE "Id" = ${seed.secondItemId};
`),
      )
      .toBe('tag-120');

    reviewRow = itemsTable
      .locator('li[data-item-id]')
      .filter({ hasText: seed.secondTitle })
      .first();
    await reviewRow.getByRole('checkbox', { name: 'Include in publication' }).uncheck();
    await expect
      .poll(() =>
        sqlite(`SELECT "Included" FROM "AddonMediaListItem" WHERE "Id" = ${seed.secondItemId};`),
      )
      .toBe('0');

    const generalPanel = editorForm.getByTestId('addon-general-metadata');
    const posterUpload = generalPanel.getByLabel(/^upload image: poster$/i);
    await posterUpload.setInputFiles({
      name: 'poster.png',
      mimeType: 'image/png',
      buffer: Buffer.from(
        'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Y9Wl9sAAAAASUVORK5CYII=',
        'base64',
      ),
    });
    const posterValue = generalPanel.getByLabel(/^poster$/i);
    await expect(posterValue).toHaveValue(/^ersatzrs-upload:\/\//);

    sqlite(`
UPDATE "AddonMediaList"
SET "MetadataOverrideJson" = json_set("MetadataOverrideJson", '$.sort_title', 'Concurrent writer')
WHERE "Id" = ${seed.listId};
`);
    await page.getByRole('button', { name: /^save changes$/i }).click();
    await expect(page.getByText(/metadata changed since the editor was loaded/i)).toBeVisible();

    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);
    const refreshedForm = page.locator('#addon_media_lists-list-editor-form');
    const refreshedHero = page
      .locator('#addon-list-description')
      .locator('xpath=ancestor::header');
    await refreshedHero.getByRole('button', { name: /^edit metadata$/i }).click();
    const refreshedPanel = refreshedForm.getByTestId('addon-general-metadata');
    await refreshedPanel.getByRole('textbox', { name: /^title$/i }).fill('Reviewed collection');
    const saveNavigation = page.waitForEvent('framenavigated');
    await page.getByRole('button', { name: /^save changes$/i }).click();
    await saveNavigation;
    await waitUntilHydrated(page);
    await expect
      .poll(() =>
        sqlite(`
SELECT json_extract("MetadataOverrideJson", '$.title')
FROM "AddonMediaList" WHERE "Id" = ${seed.listId};
`),
      )
      .toBe('Reviewed collection');

    const firstPublishNavigation = page.waitForEvent('framenavigated');
    await page.getByRole('button', { name: /^publish list$/i }).click();
    await firstPublishNavigation;
    await waitUntilHydrated(page);
    await expect
      .poll(
        () =>
          sqlite(`
SELECT "ReviewState" || ',' || "PublishedRevision"
FROM "AddonMediaList" WHERE "Id" = ${seed.listId};
`),
        { timeout: 30_000 },
      )
      .toBe('2,1');
    expect(
      sqlite(`SELECT "Published" FROM "AddonMediaListItem" WHERE "Id" = ${seed.firstItemId};`),
    ).toBe('1');
    expect(
      sqlite(`SELECT "Published" FROM "AddonMediaListItem" WHERE "Id" = ${seed.secondItemId};`),
    ).toBe('0');

    sqlite(`
UPDATE "AddonMediaListItem" SET "Availability" = 0, "AvailabilityReason" = NULL
WHERE "Id" = ${seed.secondItemId};
`);
    await gotoHydrated(page, `/media/addon-lists/${seed.listId}`);
    const reincludeRow = page
      .getByTestId('addon-metadata-items')
      .locator('li[data-item-id]')
      .filter({ hasText: seed.secondTitle })
      .first();
    await reincludeRow.getByRole('checkbox', { name: 'Include in publication' }).check();
    await expect
      .poll(() =>
        sqlite(`SELECT "Included" FROM "AddonMediaListItem" WHERE "Id" = ${seed.secondItemId};`),
      )
      .toBe('1');
    const secondPublishNavigation = page.waitForEvent('framenavigated');
    await page.getByRole('button', { name: /^publish list$/i }).click();
    await secondPublishNavigation;
    await waitUntilHydrated(page);
    await expect
      .poll(
        () =>
          sqlite(`
SELECT "PublishedRevision" || ',' || (
  SELECT "Published" FROM "AddonMediaListItem" WHERE "Id" = ${seed.secondItemId}
)
FROM "AddonMediaList" WHERE "Id" = ${seed.listId};
`),
        { timeout: 30_000 },
      )
      .toBe('2,1');

    const providerStateBeforeReclassification = sqlite(`
SELECT group_concat(
  "ProviderId" || '|' || "SourceUrl" || '|' || "Rank" || '|' ||
  "Included" || '|' || "ProviderMetadataJson" || '|' || "MetadataOverrideJson",
  char(10)
)
FROM (
  SELECT * FROM "AddonMediaListItem"
  WHERE "AddonMediaListId" = ${seed.listId}
  ORDER BY "Id"
);
`);
    const mediaType = page.getByLabel('Collection Type', { exact: true });
    await expect(mediaType.locator('option')).toHaveText([
      'Determine from the items',
      'TV Shows',
      'Movies',
      'Music Video',
      'Other Videos',
      'Songs',
      'Images',
      'Remote Streams',
    ]);
    const typeChangeHelp = page.getByTestId('addon-media-type-change-help');
    await expect(typeChangeHelp).toContainText('Current library type: Remote Streams');
    await expect(typeChangeHelp).toContainText('without importing the add-on again');

    await mediaType.selectOption('other_videos');
    await expect(typeChangeHelp).toContainText('Remote Streams → Other Videos');
    await expect(page.getByTestId('addon-media-type-change-route')).toBeVisible();
    const typeSaveNavigation = page.waitForEvent('framenavigated');
    await page.getByRole('button', { name: /^save changes$/i }).click();
    await typeSaveNavigation;
    await waitUntilHydrated(page);
    await expect(page.getByLabel('Collection Type', { exact: true })).toHaveValue('other_videos');
    await expect(page.getByTestId('addon-media-type-change-help')).toContainText(
      'Remote Streams → Other Videos',
    );

    const typePublishNavigation = page.waitForEvent('framenavigated');
    await page.getByRole('button', { name: /^publish list$/i }).click();
    await typePublishNavigation;
    await waitUntilHydrated(page);
    await expect(page.getByTestId('addon-media-type-change-help')).toContainText(
      'Current library type: Other Videos',
    );
    await expect(page.getByTestId('addon-media-type-change-route')).not.toBeVisible();
    await expect
      .poll(
        () =>
          sqlite(`
SELECT (SELECT COUNT(*) FROM "AddonMediaListItemOtherVideo" AS "link"
        JOIN "AddonMediaListItem" AS "item" ON "item"."Id" = "link"."AddonMediaListItemId"
        WHERE "item"."AddonMediaListId" = ${seed.listId}) || ',' ||
       (SELECT COUNT(*) FROM "RemoteStream" AS "stream"
        JOIN "AddonMediaListItem" AS "item" ON "item"."MediaItemId" = "stream"."Id"
        WHERE "item"."AddonMediaListId" = ${seed.listId}) || ',' ||
       "PublishedRevision"
FROM "AddonMediaList" WHERE "Id" = ${seed.listId};
`),
        { timeout: 30_000 },
      )
      .toBe('2,2,3');
    expect(
      sqlite(`
SELECT group_concat(
  "ProviderId" || '|' || "SourceUrl" || '|' || "Rank" || '|' ||
  "Included" || '|' || "ProviderMetadataJson" || '|' || "MetadataOverrideJson",
  char(10)
)
FROM (
  SELECT * FROM "AddonMediaListItem"
  WHERE "AddonMediaListId" = ${seed.listId}
  ORDER BY "Id"
);
`),
    ).toBe(providerStateBeforeReclassification);

    expect(runtimeErrors).toEqual([]);
  });
});
