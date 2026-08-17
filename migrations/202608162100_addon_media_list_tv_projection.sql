-- ErsatzRS extension: project selected add-on lists into normal television
-- hierarchy while retaining their Remote Stream playback source.

ALTER TABLE "AddonMediaList" ADD COLUMN "PresentationKind" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "AddonMediaList" ADD COLUMN "RequestedPresentationKind" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "AddonMediaList" ADD COLUMN "PresentationError" TEXT NULL;

ALTER TABLE "AddonMediaListItem" ADD COLUMN "ChapterInput" TEXT NULL;

CREATE TABLE "AddonMediaListTelevisionShow" (
    "AddonMediaListId" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListTelevisionShow" PRIMARY KEY,
    "ShowId" INTEGER NOT NULL,
    CONSTRAINT "FK_AddonMediaListTelevisionShow_AddonMediaList_AddonMediaListId"
        FOREIGN KEY ("AddonMediaListId") REFERENCES "AddonMediaList" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_AddonMediaListTelevisionShow_Show_ShowId"
        FOREIGN KEY ("ShowId") REFERENCES "Show" ("Id") ON DELETE CASCADE
);

CREATE UNIQUE INDEX "IX_AddonMediaListTelevisionShow_ShowId"
    ON "AddonMediaListTelevisionShow" ("ShowId");

CREATE TABLE "AddonMediaListItemEpisode" (
    "AddonMediaListItemId" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListItemEpisode" PRIMARY KEY,
    "EpisodeId" INTEGER NOT NULL,
    CONSTRAINT "FK_AddonMediaListItemEpisode_AddonMediaListItem_AddonMediaListItemId"
        FOREIGN KEY ("AddonMediaListItemId") REFERENCES "AddonMediaListItem" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_AddonMediaListItemEpisode_Episode_EpisodeId"
        FOREIGN KEY ("EpisodeId") REFERENCES "Episode" ("Id") ON DELETE CASCADE
);

CREATE UNIQUE INDEX "IX_AddonMediaListItemEpisode_EpisodeId"
    ON "AddonMediaListItemEpisode" ("EpisodeId");

-- The former list-wide value was valid only for one source item. Move every
-- unambiguous value to that item; ambiguous legacy values remain on the list
-- for the health/admin migration warning.
UPDATE "AddonMediaListItem"
SET "ChapterInput" = (
    SELECT "list"."ChapterInput"
    FROM "AddonMediaList" AS "list"
    WHERE "list"."Id" = "AddonMediaListItem"."AddonMediaListId"
)
WHERE "ParentProviderId" IS NULL
  AND 1 = (
      SELECT COUNT(*)
      FROM "AddonMediaListItem" AS "root"
      WHERE "root"."AddonMediaListId" = "AddonMediaListItem"."AddonMediaListId"
        AND "root"."ParentProviderId" IS NULL
  )
  AND EXISTS (
      SELECT 1
      FROM "AddonMediaList" AS "list"
      WHERE "list"."Id" = "AddonMediaListItem"."AddonMediaListId"
        AND COALESCE("list"."ChapterInput", '') <> ''
  );

UPDATE "AddonMediaList"
SET "ChapterInput" = NULL
WHERE COALESCE("ChapterInput", '') <> ''
  AND 1 = (
      SELECT COUNT(*)
      FROM "AddonMediaListItem" AS "root"
      WHERE "root"."AddonMediaListId" = "AddonMediaList"."Id"
        AND "root"."ParentProviderId" IS NULL
  );
