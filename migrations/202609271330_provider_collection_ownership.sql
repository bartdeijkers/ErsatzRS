-- ErsatzRS extension (ADR 0104): provider IDs belong to one media source.
-- Keep legacy Collection IDs and references. Empty, mixed-source, invalid-key
-- and conflicting identities stay unclaimed; scan order must not choose owners.
-- Source deletion releases ownership rather than cascading into user schedules.

ALTER TABLE "PlexCollection" ADD COLUMN "MediaSourceId" INTEGER NULL
    CONSTRAINT "FK_PlexCollection_PlexMediaSource_MediaSourceId"
    REFERENCES "PlexMediaSource" ("Id") ON DELETE SET NULL;

WITH "Candidates" AS (
    SELECT "C"."Id", "C"."Key" AS "ExternalId", MIN("S"."Id") AS "SourceId"
    FROM "PlexCollection" AS "C"
    JOIN "CollectionItem" AS "CI" ON "CI"."CollectionId" = "C"."Id"
    LEFT JOIN "MediaItem" AS "MI" ON "MI"."Id" = "CI"."MediaItemId"
    LEFT JOIN "LibraryPath" AS "LP" ON "LP"."Id" = "MI"."LibraryPathId"
    LEFT JOIN "Library" AS "L" ON "L"."Id" = "LP"."LibraryId"
    LEFT JOIN "PlexMediaSource" AS "S" ON "S"."Id" = "L"."MediaSourceId"
    WHERE "C"."Key" IS NOT NULL AND LENGTH(TRIM("C"."Key")) > 0
    GROUP BY "C"."Id", "C"."Key"
    HAVING COUNT(*) = COUNT("S"."Id") AND COUNT(DISTINCT "S"."Id") = 1
), "Unambiguous" AS (
    SELECT "Id", "SourceId",
        COUNT(*) OVER (PARTITION BY "SourceId", "ExternalId") AS "IdentityCount"
    FROM "Candidates"
)
UPDATE "PlexCollection"
SET "MediaSourceId" = (
    SELECT "SourceId" FROM "Unambiguous"
    WHERE "Unambiguous"."Id" = "PlexCollection"."Id" AND "IdentityCount" = 1
);

CREATE UNIQUE INDEX "IX_PlexCollection_MediaSourceId_Key"
    ON "PlexCollection" ("MediaSourceId", "Key");

ALTER TABLE "JellyfinCollection" ADD COLUMN "MediaSourceId" INTEGER NULL
    CONSTRAINT "FK_JellyfinCollection_JellyfinMediaSource_MediaSourceId"
    REFERENCES "JellyfinMediaSource" ("Id") ON DELETE SET NULL;

WITH "Candidates" AS (
    SELECT "C"."Id", "C"."ItemId" AS "ExternalId", MIN("S"."Id") AS "SourceId"
    FROM "JellyfinCollection" AS "C"
    JOIN "CollectionItem" AS "CI" ON "CI"."CollectionId" = "C"."Id"
    LEFT JOIN "MediaItem" AS "MI" ON "MI"."Id" = "CI"."MediaItemId"
    LEFT JOIN "LibraryPath" AS "LP" ON "LP"."Id" = "MI"."LibraryPathId"
    LEFT JOIN "Library" AS "L" ON "L"."Id" = "LP"."LibraryId"
    LEFT JOIN "JellyfinMediaSource" AS "S" ON "S"."Id" = "L"."MediaSourceId"
    WHERE "C"."ItemId" IS NOT NULL AND LENGTH(TRIM("C"."ItemId")) > 0
    GROUP BY "C"."Id", "C"."ItemId"
    HAVING COUNT(*) = COUNT("S"."Id") AND COUNT(DISTINCT "S"."Id") = 1
), "Unambiguous" AS (
    SELECT "Id", "SourceId",
        COUNT(*) OVER (PARTITION BY "SourceId", "ExternalId") AS "IdentityCount"
    FROM "Candidates"
)
UPDATE "JellyfinCollection"
SET "MediaSourceId" = (
    SELECT "SourceId" FROM "Unambiguous"
    WHERE "Unambiguous"."Id" = "JellyfinCollection"."Id" AND "IdentityCount" = 1
);

CREATE UNIQUE INDEX "IX_JellyfinCollection_MediaSourceId_ItemId"
    ON "JellyfinCollection" ("MediaSourceId", "ItemId");

ALTER TABLE "EmbyCollection" ADD COLUMN "MediaSourceId" INTEGER NULL
    CONSTRAINT "FK_EmbyCollection_EmbyMediaSource_MediaSourceId"
    REFERENCES "EmbyMediaSource" ("Id") ON DELETE SET NULL;

WITH "Candidates" AS (
    SELECT "C"."Id", "C"."ItemId" AS "ExternalId", MIN("S"."Id") AS "SourceId"
    FROM "EmbyCollection" AS "C"
    JOIN "CollectionItem" AS "CI" ON "CI"."CollectionId" = "C"."Id"
    LEFT JOIN "MediaItem" AS "MI" ON "MI"."Id" = "CI"."MediaItemId"
    LEFT JOIN "LibraryPath" AS "LP" ON "LP"."Id" = "MI"."LibraryPathId"
    LEFT JOIN "Library" AS "L" ON "L"."Id" = "LP"."LibraryId"
    LEFT JOIN "EmbyMediaSource" AS "S" ON "S"."Id" = "L"."MediaSourceId"
    WHERE "C"."ItemId" IS NOT NULL AND LENGTH(TRIM("C"."ItemId")) > 0
    GROUP BY "C"."Id", "C"."ItemId"
    HAVING COUNT(*) = COUNT("S"."Id") AND COUNT(DISTINCT "S"."Id") = 1
), "Unambiguous" AS (
    SELECT "Id", "SourceId",
        COUNT(*) OVER (PARTITION BY "SourceId", "ExternalId") AS "IdentityCount"
    FROM "Candidates"
)
UPDATE "EmbyCollection"
SET "MediaSourceId" = (
    SELECT "SourceId" FROM "Unambiguous"
    WHERE "Unambiguous"."Id" = "EmbyCollection"."Id" AND "IdentityCount" = 1
);

CREATE UNIQUE INDEX "IX_EmbyCollection_MediaSourceId_ItemId"
    ON "EmbyCollection" ("MediaSourceId", "ItemId");
