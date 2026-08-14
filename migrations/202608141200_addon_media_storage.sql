CREATE TABLE "AddonMediaSource" (
    "AddonId" TEXT NOT NULL CONSTRAINT "PK_AddonMediaSource" PRIMARY KEY,
    "MediaSourceId" INTEGER NOT NULL,
    "Name" TEXT NOT NULL COLLATE NOCASE,
    "RootPath" TEXT NOT NULL,
    CONSTRAINT "FK_AddonMediaSource_AddonInstallation_AddonId"
        FOREIGN KEY ("AddonId") REFERENCES "AddonInstallation" ("AddonId") ON DELETE CASCADE,
    CONSTRAINT "FK_AddonMediaSource_MediaSource_MediaSourceId"
        FOREIGN KEY ("MediaSourceId") REFERENCES "MediaSource" ("Id") ON DELETE CASCADE
);

CREATE UNIQUE INDEX "IX_AddonMediaSource_MediaSourceId"
    ON "AddonMediaSource" ("MediaSourceId");

CREATE TABLE "AddonMediaListLibrary" (
    "AddonMediaListId" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListLibrary" PRIMARY KEY,
    "LibraryId" INTEGER NOT NULL,
    "FolderName" TEXT NOT NULL COLLATE NOCASE,
    "ManifestPath" TEXT NOT NULL,
    CONSTRAINT "FK_AddonMediaListLibrary_AddonMediaList_AddonMediaListId"
        FOREIGN KEY ("AddonMediaListId") REFERENCES "AddonMediaList" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_AddonMediaListLibrary_Library_LibraryId"
        FOREIGN KEY ("LibraryId") REFERENCES "Library" ("Id") ON DELETE CASCADE
);

CREATE UNIQUE INDEX "IX_AddonMediaListLibrary_LibraryId"
    ON "AddonMediaListLibrary" ("LibraryId");

ALTER TABLE "AddonMediaListItem" ADD COLUMN "SourceUrl" TEXT NULL;
