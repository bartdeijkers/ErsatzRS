-- ErsatzRS extension; no direct ErsatzTV migration exists.
-- ADR 0048 follow-up: allow the same media item to appear in one collection
-- more than once when each row carries a different playback override (for
-- example one row per commentary or alternate audio track). The composite
-- primary key ("CollectionId", "MediaItemId") cannot represent that, so the
-- table is rebuilt around a surrogate "Id" key, mirroring the "PlaylistItem"
-- row shape. Row-tuple uniqueness (no two rows with an identical override
-- set) is enforced by the add-to-collection flows.
CREATE TABLE "CollectionItem_new" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_CollectionItem" PRIMARY KEY AUTOINCREMENT,
    "CollectionId" INTEGER NOT NULL,
    "MediaItemId" INTEGER NOT NULL,
    "CustomIndex" INTEGER NULL,
    "PreferredAudioLanguageCode" TEXT NULL,
    "PreferredAudioTitle" TEXT NULL,
    "PreferredSubtitleLanguageCode" TEXT NULL,
    "SubtitleMode" INTEGER NULL,
    CONSTRAINT "FK_CollectionItem_Collection_CollectionId" FOREIGN KEY ("CollectionId") REFERENCES "Collection" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_CollectionItem_MediaItem_MediaItemId" FOREIGN KEY ("MediaItemId") REFERENCES "MediaItem" ("Id") ON DELETE CASCADE
);

INSERT INTO "CollectionItem_new" (
    "CollectionId", "MediaItemId", "CustomIndex",
    "PreferredAudioLanguageCode", "PreferredAudioTitle",
    "PreferredSubtitleLanguageCode", "SubtitleMode"
)
SELECT "CollectionId", "MediaItemId", "CustomIndex",
       "PreferredAudioLanguageCode", "PreferredAudioTitle",
       "PreferredSubtitleLanguageCode", "SubtitleMode"
FROM "CollectionItem"
ORDER BY "CollectionId", "MediaItemId";

DROP TABLE "CollectionItem";

ALTER TABLE "CollectionItem_new" RENAME TO "CollectionItem";

CREATE INDEX "IX_CollectionItem_CollectionId_MediaItemId" ON "CollectionItem" ("CollectionId", "MediaItemId");
