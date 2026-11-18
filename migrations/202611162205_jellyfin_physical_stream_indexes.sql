-- ADR 0137: retain saved provider track identities while routing physical inputs.
-- Existing rows remain unqualified until an ordinary claimed Jellyfin scan
-- publishes a complete map transactionally. No saved pins or tracks are rewritten.
ALTER TABLE "MediaVersion" ADD COLUMN "ProviderStreamIndexVersion" INTEGER NOT NULL DEFAULT 0
    CHECK ("ProviderStreamIndexVersion" >= 0);

CREATE TABLE "ProviderStreamIndex" (
    "MediaVersionId" INTEGER NOT NULL,
    "StreamIndex" INTEGER NOT NULL CHECK ("StreamIndex" >= 0),
    "SourceStreamIndex" INTEGER NOT NULL CHECK ("SourceStreamIndex" >= 0),
    CONSTRAINT "PK_ProviderStreamIndex" PRIMARY KEY ("MediaVersionId", "StreamIndex"),
    CONSTRAINT "FK_ProviderStreamIndex_MediaVersion_MediaVersionId"
        FOREIGN KEY ("MediaVersionId") REFERENCES "MediaVersion" ("Id") ON DELETE CASCADE
);
CREATE UNIQUE INDEX "IX_ProviderStreamIndex_MediaVersionId_SourceStreamIndex"
    ON "ProviderStreamIndex" ("MediaVersionId", "SourceStreamIndex");

-- Reuse ordinary scan scheduling. Disabled/sync choices, ETags, cached subtitles
-- and published playout/scheduling state remain intact until successful scans.
UPDATE "Library" SET "LastScan" = '0001-01-01 00:00:00'
WHERE "Id" IN (SELECT "Id" FROM "JellyfinLibrary");
