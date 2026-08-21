ALTER TABLE "AddonMediaList" ADD COLUMN "MetadataConflictJson" TEXT NULL;
ALTER TABLE "AddonMediaList" ADD COLUMN "PublicationJournalJson" TEXT NULL;
ALTER TABLE "AddonMediaList" ADD COLUMN "PublicationStartedAt" TEXT NULL;

CREATE TABLE "AddonMediaListArtworkUpload" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListArtworkUpload" PRIMARY KEY AUTOINCREMENT,
    "AddonMediaListId" INTEGER NOT NULL,
    "AddonMediaListItemId" INTEGER NULL,
    "ScopeItemId" INTEGER NOT NULL DEFAULT 0,
    "Role" INTEGER NOT NULL,
    "ContentType" TEXT NOT NULL,
    "Extension" TEXT NOT NULL,
    "Bytes" BLOB NOT NULL,
    "Sha256" TEXT NOT NULL,
    "CreatedAt" TEXT NOT NULL,
    "UpdatedAt" TEXT NOT NULL,
    CONSTRAINT "FK_AddonMediaListArtworkUpload_AddonMediaList_AddonMediaListId"
        FOREIGN KEY ("AddonMediaListId") REFERENCES "AddonMediaList" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_AddonMediaListArtworkUpload_AddonMediaListItem_AddonMediaListItemId"
        FOREIGN KEY ("AddonMediaListItemId") REFERENCES "AddonMediaListItem" ("Id") ON DELETE CASCADE
);

CREATE UNIQUE INDEX "IX_AddonMediaListArtworkUpload_ListScopeRole"
    ON "AddonMediaListArtworkUpload" ("AddonMediaListId", "ScopeItemId", "Role");
CREATE INDEX "IX_AddonMediaListArtworkUpload_ItemId"
    ON "AddonMediaListArtworkUpload" ("AddonMediaListItemId");

CREATE TABLE "AddonMediaListArtworkMaterialization" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListArtworkMaterialization" PRIMARY KEY AUTOINCREMENT,
    "AddonMediaListId" INTEGER NOT NULL,
    "AddonMediaListItemId" INTEGER NULL,
    "ScopeItemId" INTEGER NOT NULL DEFAULT 0,
    "Role" INTEGER NOT NULL,
    "Source" TEXT NOT NULL,
    "FileName" TEXT NOT NULL,
    "Sha256" TEXT NOT NULL,
    "PublishedRevision" INTEGER NOT NULL,
    CONSTRAINT "FK_AddonMediaListArtworkMaterialization_AddonMediaList_AddonMediaListId"
        FOREIGN KEY ("AddonMediaListId") REFERENCES "AddonMediaList" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_AddonMediaListArtworkMaterialization_AddonMediaListItem_AddonMediaListItemId"
        FOREIGN KEY ("AddonMediaListItemId") REFERENCES "AddonMediaListItem" ("Id") ON DELETE CASCADE
);

CREATE UNIQUE INDEX "IX_AddonMediaListArtworkMaterialization_ListScopeRole"
    ON "AddonMediaListArtworkMaterialization" ("AddonMediaListId", "ScopeItemId", "Role");
CREATE INDEX "IX_AddonMediaListArtworkMaterialization_ItemId"
    ON "AddonMediaListArtworkMaterialization" ("AddonMediaListItemId");
