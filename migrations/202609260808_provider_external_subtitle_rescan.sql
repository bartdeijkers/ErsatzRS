-- Recover provider sidecars deleted by cleanup or collapsed by path matching.
-- Preserve subtitle rows and extraction caches; the next ordinary scan repairs them.

UPDATE "JellyfinMovie" SET "Etag" = NULL
WHERE "Id" IN (
    SELECT V."MovieId" FROM "MediaVersion" V
    INNER JOIN "MediaStream" S ON S."MediaVersionId" = V."Id"
    WHERE V."MovieId" IS NOT NULL AND S."MediaStreamKind" = 5
);

UPDATE "JellyfinEpisode" SET "Etag" = NULL
WHERE "Id" IN (
    SELECT V."EpisodeId" FROM "MediaVersion" V
    INNER JOIN "MediaStream" S ON S."MediaVersionId" = V."Id"
    WHERE V."EpisodeId" IS NOT NULL AND S."MediaStreamKind" = 5
);

UPDATE "EmbyMovie" SET "Etag" = NULL
WHERE "Id" IN (
    SELECT V."MovieId" FROM "MediaVersion" V
    INNER JOIN "MediaStream" S ON S."MediaVersionId" = V."Id"
    WHERE V."MovieId" IS NOT NULL AND S."MediaStreamKind" = 5
);

UPDATE "EmbyEpisode" SET "Etag" = NULL
WHERE "Id" IN (
    SELECT V."EpisodeId" FROM "MediaVersion" V
    INNER JOIN "MediaStream" S ON S."MediaVersionId" = V."Id"
    WHERE V."EpisodeId" IS NOT NULL AND S."MediaStreamKind" = 5
);
