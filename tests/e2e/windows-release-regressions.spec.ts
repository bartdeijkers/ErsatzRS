import { expect, Page, TestInfo, test } from '@playwright/test';
import { execFileSync } from 'child_process';

const dbPath = process.env.E2E_DB_PATH ?? process.env.ERSATZRS_DB_PATH;

type Seed = {
  mediaSourceId: number;
  libraryId: number;
  libraryPathId: number;
  showId: number;
  seasonId: number;
  episodeId: number;
  episodeTwoId: number;
  showMetadataId: number;
  seasonMetadataId: number;
  episodeMetadataId: number;
  episodeTwoMetadataId: number;
  mediaVersionId: number;
  mediaFileId: number;
  videoStreamId: number;
  subtitleStreamId: number;
  subtitleId: number;
  chapterId: number;
};

function sqlite(sql: string): string {
  if (!dbPath) {
    throw new Error('E2E_DB_PATH or ERSATZRS_DB_PATH is required for seeding');
  }

  return execFileSync('sqlite3', [dbPath], {
    input: `.timeout 10000\n${sql}`,
    encoding: 'utf8',
  }).trim();
}

function seedForProject(testInfo: TestInfo): Seed {
  const projectOffset = Array.from(testInfo.project.name).reduce(
    (total, char) => total + char.charCodeAt(0),
    0,
  ) * 100;
  const base = 150_000 + projectOffset;
  return {
    mediaSourceId: base + 1,
    libraryId: base + 2,
    libraryPathId: base + 3,
    showId: base + 4,
    seasonId: base + 5,
    episodeId: base + 6,
    episodeTwoId: base + 7,
    showMetadataId: base + 8,
    seasonMetadataId: base + 9,
    episodeMetadataId: base + 10,
    episodeTwoMetadataId: base + 11,
    mediaVersionId: base + 12,
    mediaFileId: base + 13,
    videoStreamId: base + 14,
    subtitleStreamId: base + 15,
    subtitleId: base + 16,
    chapterId: base + 17,
  };
}

function seedReleaseRegressionFixture(testInfo: TestInfo): Seed {
  const seed = seedForProject(testInfo);
  const now = '2026-06-17T00:00:00Z';
  sqlite(`
PRAGMA foreign_keys=OFF;
DELETE FROM "MediaChapter" WHERE "Id" = ${seed.chapterId};
DELETE FROM "Subtitle" WHERE "Id" = ${seed.subtitleId};
DELETE FROM "MediaStream" WHERE "Id" IN (${seed.videoStreamId}, ${seed.subtitleStreamId});
DELETE FROM "MediaFile" WHERE "Id" = ${seed.mediaFileId};
DELETE FROM "MediaVersion" WHERE "Id" = ${seed.mediaVersionId};
DELETE FROM "EpisodeMetadata" WHERE "Id" IN (${seed.episodeMetadataId}, ${seed.episodeTwoMetadataId});
DELETE FROM "SeasonMetadata" WHERE "Id" = ${seed.seasonMetadataId};
DELETE FROM "ShowMetadata" WHERE "Id" = ${seed.showMetadataId};
DELETE FROM "Episode" WHERE "Id" IN (${seed.episodeId}, ${seed.episodeTwoId});
DELETE FROM "Season" WHERE "Id" = ${seed.seasonId};
DELETE FROM "Show" WHERE "Id" = ${seed.showId};
DELETE FROM "MediaItem" WHERE "Id" IN (${seed.showId}, ${seed.seasonId}, ${seed.episodeId}, ${seed.episodeTwoId});
DELETE FROM "LibraryPath" WHERE "Id" = ${seed.libraryPathId};
DELETE FROM "LocalLibrary" WHERE "Id" = ${seed.libraryId};
DELETE FROM "Library" WHERE "Id" = ${seed.libraryId};
DELETE FROM "LocalMediaSource" WHERE "Id" = ${seed.mediaSourceId};
DELETE FROM "MediaSource" WHERE "Id" = ${seed.mediaSourceId};

INSERT INTO "MediaSource" ("Id") VALUES (${seed.mediaSourceId});
INSERT INTO "LocalMediaSource" ("Id") VALUES (${seed.mediaSourceId});
INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
  VALUES (${seed.libraryId}, 'E2E Windows TV', 2, ${seed.mediaSourceId});
INSERT INTO "LocalLibrary" ("Id") VALUES (${seed.libraryId});
INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId")
  VALUES (${seed.libraryPathId}, '/tmp/ersatzrs-e2e-windows-release', ${seed.libraryId});

INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES
  (${seed.showId}, ${seed.libraryPathId}, 0),
  (${seed.seasonId}, ${seed.libraryPathId}, 0),
  (${seed.episodeId}, ${seed.libraryPathId}, 1),
  (${seed.episodeTwoId}, ${seed.libraryPathId}, 0);
INSERT INTO "Show" ("Id") VALUES (${seed.showId});
INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber")
  VALUES (${seed.seasonId}, ${seed.showId}, 1);
INSERT INTO "Episode" ("Id", "SeasonId")
  VALUES (${seed.episodeId}, ${seed.seasonId}), (${seed.episodeTwoId}, ${seed.seasonId});

INSERT INTO "ShowMetadata" (
  "Id", "ShowId", "MetadataKind", "Title", "SortTitle", "Year", "DateAdded", "DateUpdated"
) VALUES (
  ${seed.showMetadataId}, ${seed.showId}, 0, 'E2E Windows Show', 'e2e windows show', 2026, '${now}', '${now}'
);
INSERT INTO "SeasonMetadata" (
  "Id", "SeasonId", "MetadataKind", "Title", "SortTitle", "DateAdded", "DateUpdated"
) VALUES (
  ${seed.seasonMetadataId}, ${seed.seasonId}, 0, 'Season 1', 'season 1', '${now}', '${now}'
);
INSERT INTO "EpisodeMetadata" (
  "Id", "EpisodeId", "MetadataKind", "Title", "SortTitle", "ReleaseDate",
  "DateAdded", "DateUpdated", "EpisodeNumber", "Plot"
) VALUES
  (${seed.episodeMetadataId}, ${seed.episodeId}, 0, 'Pilot', 'pilot', '2026-06-01T00:00:00Z', '${now}', '${now}', 1, 'The beginning.'),
  (${seed.episodeTwoMetadataId}, ${seed.episodeTwoId}, 0, 'Finale', 'finale', NULL, '${now}', '${now}', 2, 'The ending.');

INSERT INTO "MediaVersion" (
  "Id", "DateAdded", "DateUpdated", "Duration", "EpisodeId",
  "Height", "Width", "VideoScanKind", "SampleAspectRatio",
  "DisplayAspectRatio", "RFrameRate"
) VALUES (
  ${seed.mediaVersionId}, '${now}', '${now}', '00:22:00', ${seed.episodeId},
  1080, 1920, 1, '1:1', '16:9', '24000/1001'
);
INSERT INTO "MediaFile" ("Id", "MediaVersionId", "Path", "PathHash")
  VALUES (${seed.mediaFileId}, ${seed.mediaVersionId}, '/tmp/ersatzrs-e2e-windows-release/show/s01e01.mkv', 'e2e-windows-${seed.mediaFileId}');
INSERT INTO "MediaStream" (
  "Id", "Index", "Codec", "Profile", "MediaStreamKind", "Language",
  "Channels", "Title", "Default", "Forced", "MediaVersionId"
) VALUES
  (${seed.videoStreamId}, 0, 'h264', 'High', 1, NULL, 0, NULL, 1, 0, ${seed.mediaVersionId}),
  (${seed.subtitleStreamId}, 1, 'srt', NULL, 3, 'eng', 0, 'English', 1, 0, ${seed.mediaVersionId});
INSERT INTO "Subtitle" (
  "Id", "EpisodeMetadataId", "StreamIndex", "SubtitleKind", "Title",
  "Codec", "Language", "Default", "Forced", "Path", "IsExtracted",
  "DateAdded", "DateUpdated"
) VALUES (
  ${seed.subtitleId}, ${seed.episodeMetadataId}, 1, 0, 'English', 'srt', 'eng', 1, 0,
  '/cache/sub.srt', 1, '${now}', '${now}'
);
INSERT INTO "MediaChapter" (
  "Id", "MediaVersionId", "ChapterId", "StartTime", "EndTime", "Title"
) VALUES (
  ${seed.chapterId}, ${seed.mediaVersionId}, 1, '00:00:00', '00:01:00', 'Opening'
);
`);
  return seed;
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

test.describe('Windows release interaction regressions', () => {
  test.skip(!dbPath, 'E2E_DB_PATH or ERSATZRS_DB_PATH is required to seed media rows');

  test('library scan, logs pager, and season episode media info stay interactive', async ({
    page,
  }, testInfo) => {
    test.skip(
      testInfo.project.name !== 'chromium',
      'This regression proof uses one browser project to avoid app-state churn.',
    );

    const seed = seedReleaseRegressionFixture(testInfo);
    await forceEnglish(page);

    // Prove the seeded episode interaction before the scan worker is allowed to
    // reconcile the deliberately nonexistent fixture path in the background.
    await page.goto(`/media/tv/seasons/${seed.seasonId}`);
    await expect(page.getByText('E2E Windows Show')).toBeVisible();
    await page.getByRole('button', { name: 'Show Media Info' }).first().click();
    await expect(page.getByText('"Kind": "Episode"')).toBeVisible();
    await expect(page.getByText('"Title": "E2E Windows Show - s01e01 - Pilot"')).toBeVisible();

    await page.goto('/media/libraries');
    const libraryRow = page.getByRole('row').filter({ hasText: 'E2E Windows TV' });
    await expect(libraryRow).toBeVisible();
    await libraryRow.getByRole('button', { name: 'Scan Library' }).click();
    await expect(page.getByRole('status')).toHaveText(/Scan queued/);
    await expect(page.locator('body')).not.toContainText('Scan failed');

    await page.goto('/system/logs');
    await page.getByRole('combobox', { name: 'Rows per page' }).selectOption('25');
    await expect(page.getByRole('button', { name: 'Next page' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Last page' })).toBeVisible();
    await expect(page.locator('body')).not.toContainText('last_page on');
    await expect(page.locator('body')).not.toContainText('u32::try_from');

  });
});
