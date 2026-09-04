-- AMM-K-04 immutable add-on-list publication snapshots and reference pins.
-- Existing AMM-K-01 revision rows intentionally remain without a snapshot
-- digest: their historical draft and generated files were never captured and
-- therefore cannot be restored truthfully.

-- Revision blobs live beside this database rather than below a mutable media
-- root. The persisted namespace prevents two databases in the same profile
-- directory from claiming each other's list-scoped blobs.
INSERT INTO "ConfigElement" ("Key", "Value")
SELECT 'addon_media_list.revision_blob_store_id', lower(hex(randomblob(16)))
WHERE NOT EXISTS (
    SELECT 1 FROM "ConfigElement"
    WHERE "Key" = 'addon_media_list.revision_blob_store_id'
);

ALTER TABLE "AddonMediaListRevision" ADD COLUMN "SnapshotVersion" INTEGER NULL;
ALTER TABLE "AddonMediaListRevision" ADD COLUMN "SnapshotDigest" TEXT NULL;
ALTER TABLE "AddonMediaListRevision" ADD COLUMN "Name" TEXT NULL COLLATE NOCASE;
ALTER TABLE "AddonMediaListRevision" ADD COLUMN "Description" TEXT NULL;
ALTER TABLE "AddonMediaListRevision" ADD COLUMN "GeneratePlaylist" INTEGER NULL;
ALTER TABLE "AddonMediaListRevision" ADD COLUMN "ContentKindOverride" INTEGER NULL;
ALTER TABLE "AddonMediaListRevision" ADD COLUMN "PresentationKind" INTEGER NULL;
ALTER TABLE "AddonMediaListRevision" ADD COLUMN "RequestedPresentationKind" INTEGER NULL;
ALTER TABLE "AddonMediaListRevision" ADD COLUMN "MetadataJson" TEXT NULL;

CREATE INDEX "IX_AddonMediaListRevision_ListCompleted"
    ON "AddonMediaListRevision" ("AddonMediaListId", "CompletedAt" DESC, "Revision" DESC);

CREATE TABLE "AddonMediaListRevisionItem" (
    "AddonMediaListRevisionId" INTEGER NOT NULL,
    "ProviderId" TEXT NOT NULL,
    "GuidsJson" TEXT NOT NULL DEFAULT '[]',
    "Rank" INTEGER NOT NULL,
    "Kind" INTEGER NOT NULL,
    "ContentKind" INTEGER NOT NULL,
    "Included" INTEGER NOT NULL,
    "Published" INTEGER NOT NULL,
    "DisplayTitle" TEXT NOT NULL,
    "Title" TEXT NOT NULL,
    "Year" INTEGER NULL,
    "Season" INTEGER NULL,
    "Episode" INTEGER NULL,
    "SourceUrl" TEXT NULL,
    "AdditionalImageUrls" TEXT NULL,
    "DurationSeconds" INTEGER NULL,
    "ParentProviderId" TEXT NULL,
    "FragmentStartSeconds" INTEGER NULL,
    "FragmentEndSeconds" INTEGER NULL,
    "ChapterInput" TEXT NOT NULL,
    "ProviderChapterInput" TEXT NOT NULL,
    "CollectionRole" INTEGER NOT NULL,
    "Availability" INTEGER NOT NULL,
    "AvailabilityReason" TEXT NULL,
    "AvailabilityReasonCode" INTEGER NULL,
    "ProviderLiveness" INTEGER NOT NULL,
    "LivenessOverride" INTEGER NULL,
    "EffectiveLiveness" INTEGER NOT NULL,
    "MetadataJson" TEXT NOT NULL,
    "MetadataDigest" TEXT NOT NULL,
    "ReplacementMediaItemId" INTEGER NULL,
    "ReplacementMediaVersionId" INTEGER NULL,
    "PublishedMediaItemId" INTEGER NULL,
    CONSTRAINT "PK_AddonMediaListRevisionItem"
        PRIMARY KEY ("AddonMediaListRevisionId", "ProviderId"),
    CONSTRAINT "FK_AddonMediaListRevisionItem_AddonMediaListRevision_RevisionId"
        FOREIGN KEY ("AddonMediaListRevisionId") REFERENCES "AddonMediaListRevision" ("Id") ON DELETE CASCADE
);

CREATE INDEX "IX_AddonMediaListRevisionItem_PublishedMediaItemId"
    ON "AddonMediaListRevisionItem" ("PublishedMediaItemId");
CREATE INDEX "IX_AddonMediaListRevisionItem_ReplacementMediaItemId"
    ON "AddonMediaListRevisionItem" ("ReplacementMediaItemId");

-- Immutable content blobs own the retained bytes. Revision rows link a blob to
-- the path it occupied in that publication, so equal content is shared without
-- turning the later processed-artifact cache into revision storage.
CREATE TABLE "AddonMediaListRevisionFileBlob" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListRevisionFileBlob" PRIMARY KEY AUTOINCREMENT,
    "AddonMediaListId" INTEGER NOT NULL,
    "StorageRelativePath" TEXT NOT NULL,
    "Sha256" TEXT NOT NULL,
    "ByteLength" INTEGER NOT NULL,
    "CreatedAt" TEXT NOT NULL,
    CONSTRAINT "FK_AddonMediaListRevisionFileBlob_AddonMediaList_ListId"
        FOREIGN KEY ("AddonMediaListId") REFERENCES "AddonMediaList" ("Id") ON DELETE CASCADE,
    CONSTRAINT "CK_AddonMediaListRevisionFileBlob_ByteLength"
        CHECK ("ByteLength" >= 0)
);

CREATE UNIQUE INDEX "IX_AddonMediaListRevisionFileBlob_ListContent"
    ON "AddonMediaListRevisionFileBlob" ("AddonMediaListId", "Sha256");
CREATE UNIQUE INDEX "IX_AddonMediaListRevisionFileBlob_StorageRelativePath"
    ON "AddonMediaListRevisionFileBlob" ("StorageRelativePath");

CREATE TABLE "AddonMediaListRevisionFile" (
    "AddonMediaListRevisionId" INTEGER NOT NULL,
    "AddonMediaListRevisionFileBlobId" INTEGER NOT NULL,
    "PublishedRelativePath" TEXT NOT NULL,
    "ProviderId" TEXT NULL,
    CONSTRAINT "PK_AddonMediaListRevisionFile"
        PRIMARY KEY ("AddonMediaListRevisionId", "PublishedRelativePath"),
    CONSTRAINT "FK_AddonMediaListRevisionFile_AddonMediaListRevision_RevisionId"
        FOREIGN KEY ("AddonMediaListRevisionId") REFERENCES "AddonMediaListRevision" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_AddonMediaListRevisionFile_AddonMediaListRevisionFileBlob_BlobId"
        FOREIGN KEY ("AddonMediaListRevisionFileBlobId") REFERENCES "AddonMediaListRevisionFileBlob" ("Id") ON DELETE CASCADE
);

CREATE INDEX "IX_AddonMediaListRevisionFile_BlobId"
    ON "AddonMediaListRevisionFile" ("AddonMediaListRevisionFileBlobId");

-- NULL is a normal publication. Restore jobs persist the exact retained source
-- revision so retries cannot silently switch to the mutable current draft.
ALTER TABLE "AddonMediaListPublicationJob" ADD COLUMN "SourcePublishedRevision" INTEGER NULL;
ALTER TABLE "AddonMediaListPublicationJob" ADD COLUMN "RestoreMaterializedAt" TEXT NULL;
