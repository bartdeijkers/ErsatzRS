ALTER TABLE "AddonMediaList" ADD COLUMN "ReviewState" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "AddonMediaList" ADD COLUMN "ProviderMetadataJson" TEXT NOT NULL DEFAULT '{}';
ALTER TABLE "AddonMediaList" ADD COLUMN "MetadataOverrideJson" TEXT NOT NULL DEFAULT '{}';
ALTER TABLE "AddonMediaList" ADD COLUMN "PublishedRevision" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "AddonMediaList" ADD COLUMN "ReviewError" TEXT NULL;

UPDATE "AddonMediaList"
SET "ReviewState" = 2, "PublishedRevision" = 1;

ALTER TABLE "AddonMediaListItem" ADD COLUMN "Included" INTEGER NOT NULL DEFAULT 1;
ALTER TABLE "AddonMediaListItem" ADD COLUMN "Published" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "AddonMediaListItem" ADD COLUMN "ProviderMetadataJson" TEXT NOT NULL DEFAULT '{}';
ALTER TABLE "AddonMediaListItem" ADD COLUMN "MetadataOverrideJson" TEXT NOT NULL DEFAULT '{}';
ALTER TABLE "AddonMediaListItem" ADD COLUMN "MetadataConflictJson" TEXT NULL;
ALTER TABLE "AddonMediaListItem" ADD COLUMN "CollectionRole" INTEGER NOT NULL DEFAULT 0;

UPDATE "AddonMediaListItem" SET "Published" = 1;

CREATE INDEX "IX_AddonMediaList_ReviewState" ON "AddonMediaList" ("ReviewState");
CREATE INDEX "IX_AddonMediaListItem_ListIncluded"
    ON "AddonMediaListItem" ("AddonMediaListId", "Included", "Rank");
