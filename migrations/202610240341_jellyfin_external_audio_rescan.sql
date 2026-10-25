-- ErsatzTV.Infrastructure.Sqlite/Migrations/20261022024401_Rescan_JellyfinNegativeStreamIndex.cs
-- Revisit legacy negative stream indexes through the ordinary Jellyfin scanner.
-- Track/subtitle rows and cached files survive until a successful replacement scan.

UPDATE "JellyfinMovie" SET "Etag" = NULL
WHERE "Id" IN (
    SELECT V."MovieId" FROM "MediaVersion" V
    INNER JOIN "MediaStream" S ON S."MediaVersionId" = V."Id"
    WHERE S."Index" < 0 AND V."MovieId" IS NOT NULL
);

UPDATE "JellyfinEpisode" SET "Etag" = NULL
WHERE "Id" IN (
    SELECT V."EpisodeId" FROM "MediaVersion" V
    INNER JOIN "MediaStream" S ON S."MediaVersionId" = V."Id"
    WHERE S."Index" < 0 AND V."EpisodeId" IS NOT NULL
);

UPDATE "Library" SET "LastScan" = '0001-01-01 00:00:00'
WHERE "Id" IN (SELECT "Id" FROM "JellyfinLibrary");
