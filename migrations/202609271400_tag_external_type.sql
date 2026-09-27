-- Port of ErsatzTV.Infrastructure.Sqlite/Migrations/
-- 20260903214731_Backfill_Tag_ExternalTypeId.cs at 9067cde8 (UCU-08 row 129).
-- ADR 0104 deviation: match collections within the metadata owner's source.
-- Only Plex-owned leftovers can be Plex labels. Preserve ambiguous/unclaimed
-- identities, orphan/multiple-owner tags, ordinary tags and existing types.
-- No tag IDs, names, external IDs or metadata references change.
WITH "Owners" AS (
    SELECT "T"."Id" AS "TagId", "M"."ArtistId" AS "MediaItemId"
    FROM "Tag" AS "T" JOIN "ArtistMetadata" AS "M" ON "M"."Id" = "T"."ArtistMetadataId"
    UNION ALL
    SELECT "T"."Id" AS "TagId", "M"."EpisodeId" AS "MediaItemId"
    FROM "Tag" AS "T" JOIN "EpisodeMetadata" AS "M" ON "M"."Id" = "T"."EpisodeMetadataId"
    UNION ALL
    SELECT "T"."Id" AS "TagId", "M"."ImageId" AS "MediaItemId"
    FROM "Tag" AS "T" JOIN "ImageMetadata" AS "M" ON "M"."Id" = "T"."ImageMetadataId"
    UNION ALL
    SELECT "T"."Id" AS "TagId", "M"."MovieId" AS "MediaItemId"
    FROM "Tag" AS "T" JOIN "MovieMetadata" AS "M" ON "M"."Id" = "T"."MovieMetadataId"
    UNION ALL
    SELECT "T"."Id" AS "TagId", "M"."MusicVideoId" AS "MediaItemId"
    FROM "Tag" AS "T" JOIN "MusicVideoMetadata" AS "M" ON "M"."Id" = "T"."MusicVideoMetadataId"
    UNION ALL
    SELECT "T"."Id" AS "TagId", "M"."OtherVideoId" AS "MediaItemId"
    FROM "Tag" AS "T" JOIN "OtherVideoMetadata" AS "M" ON "M"."Id" = "T"."OtherVideoMetadataId"
    UNION ALL
    SELECT "T"."Id" AS "TagId", "M"."RemoteStreamId" AS "MediaItemId"
    FROM "Tag" AS "T" JOIN "RemoteStreamMetadata" AS "M" ON "M"."Id" = "T"."RemoteStreamMetadataId"
    UNION ALL
    SELECT "T"."Id" AS "TagId", "M"."SeasonId" AS "MediaItemId"
    FROM "Tag" AS "T" JOIN "SeasonMetadata" AS "M" ON "M"."Id" = "T"."SeasonMetadataId"
    UNION ALL
    SELECT "T"."Id" AS "TagId", "M"."ShowId" AS "MediaItemId"
    FROM "Tag" AS "T" JOIN "ShowMetadata" AS "M" ON "M"."Id" = "T"."ShowMetadataId"
    UNION ALL
    SELECT "T"."Id" AS "TagId", "M"."SongId" AS "MediaItemId"
    FROM "Tag" AS "T" JOIN "SongMetadata" AS "M" ON "M"."Id" = "T"."SongMetadataId"
), "ProviderSources" AS (
    SELECT "Id", 'plex' AS "Provider" FROM "PlexMediaSource"
    UNION ALL
    SELECT "Id", 'emby' AS "Provider" FROM "EmbyMediaSource"
    UNION ALL
    SELECT "Id", 'jellyfin' AS "Provider" FROM "JellyfinMediaSource"
    UNION ALL
    SELECT "Id", 'local' AS "Provider" FROM "LocalMediaSource"
), "UniqueProviders" AS (
    SELECT "Id", MIN("Provider") AS "Provider" FROM "ProviderSources"
    GROUP BY "Id" HAVING COUNT(*) = 1
), "Eligible" AS (
    SELECT "T"."Id", "T"."ExternalCollectionId", "L"."MediaSourceId", "P"."Provider"
    FROM "Tag" AS "T"
    JOIN "Owners" AS "O" ON "O"."TagId" = "T"."Id"
    JOIN "MediaItem" AS "MI" ON "MI"."Id" = "O"."MediaItemId"
    JOIN "LibraryPath" AS "LP" ON "LP"."Id" = "MI"."LibraryPathId"
    JOIN "Library" AS "L" ON "L"."Id" = "LP"."LibraryId"
    JOIN "UniqueProviders" AS "P" ON "P"."Id" = "L"."MediaSourceId"
    WHERE "T"."ExternalTypeId" IS NULL
      AND "T"."ExternalCollectionId" IS NOT NULL
      AND LENGTH(TRIM("T"."ExternalCollectionId")) > 0
      AND (
          ("T"."ArtistMetadataId" IS NOT NULL) +
          ("T"."EpisodeMetadataId" IS NOT NULL) +
          ("T"."ImageMetadataId" IS NOT NULL) +
          ("T"."MovieMetadataId" IS NOT NULL) +
          ("T"."MusicVideoMetadataId" IS NOT NULL) +
          ("T"."OtherVideoMetadataId" IS NOT NULL) +
          ("T"."RemoteStreamMetadataId" IS NOT NULL) +
          ("T"."SeasonMetadataId" IS NOT NULL) +
          ("T"."ShowMetadataId" IS NOT NULL) +
          ("T"."SongMetadataId" IS NOT NULL)
      ) = 1
), "Types" AS (
    SELECT "E"."Id", CASE
        WHEN "E"."Provider" = 'plex' AND EXISTS (
            SELECT 1 FROM "PlexCollection" AS "C"
            WHERE "C"."MediaSourceId" = "E"."MediaSourceId"
              AND "C"."Key" = "E"."ExternalCollectionId"
        ) THEN 'plex/collection'
        WHEN "E"."Provider" = 'emby' AND EXISTS (
            SELECT 1 FROM "EmbyCollection" AS "C"
            WHERE "C"."MediaSourceId" = "E"."MediaSourceId"
              AND "C"."ItemId" = "E"."ExternalCollectionId"
        ) THEN 'emby/collection'
        WHEN "E"."Provider" = 'jellyfin' AND EXISTS (
            SELECT 1 FROM "JellyfinCollection" AS "C"
            WHERE "C"."MediaSourceId" = "E"."MediaSourceId"
              AND "C"."ItemId" = "E"."ExternalCollectionId"
        ) THEN 'jellyfin/collection'
        WHEN "E"."Provider" = 'plex' AND NOT EXISTS (
            SELECT 1 FROM "PlexCollection" AS "C"
            WHERE "C"."MediaSourceId" IS NULL
              AND "C"."Key" = "E"."ExternalCollectionId"
        ) THEN 'plex/label'
        ELSE NULL
    END AS "ExternalTypeId"
    FROM "Eligible" AS "E"
)
UPDATE "Tag" SET "ExternalTypeId" = "Types"."ExternalTypeId"
FROM "Types"
WHERE "Tag"."Id" = "Types"."Id" AND "Types"."ExternalTypeId" IS NOT NULL;
