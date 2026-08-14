-- Trakt lists are now owned by repository-installed add-ons. This migration is
-- intentionally destructive: legacy Trakt rows are not imported or retained.

DELETE FROM "Playlist"
WHERE "IsSystem" = 1
  AND "Id" IN (
      SELECT "PlaylistId" FROM "TraktList" WHERE "PlaylistId" IS NOT NULL
  );

DELETE FROM "ConfigElement" WHERE "Key" = 'pages.trakt.lists_page_size';
DELETE FROM "ConfigElement" WHERE "Key" = 'pages.addon_media_lists.page_size';

DROP TABLE "TraktListItemGuid";
DROP TABLE "TraktListItem";
DROP TABLE "TraktList";

CREATE TABLE "AddonMediaList" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaList" PRIMARY KEY AUTOINCREMENT,
    "AddonId" TEXT NOT NULL,
    "ProviderId" TEXT NOT NULL,
    "SourceUrl" TEXT NOT NULL,
    "Name" TEXT NOT NULL COLLATE NOCASE,
    "Description" TEXT NULL,
    "ItemCount" INTEGER NOT NULL DEFAULT 0,
    "AutoRefresh" INTEGER NOT NULL DEFAULT 0,
    "GeneratePlaylist" INTEGER NOT NULL DEFAULT 0,
    "PlaylistId" INTEGER NULL,
    "LastUpdate" TEXT NULL,
    "LastMatch" TEXT NULL,
    "LastError" TEXT NULL,
    CONSTRAINT "FK_AddonMediaList_AddonInstallation_AddonId"
        FOREIGN KEY ("AddonId") REFERENCES "AddonInstallation" ("AddonId") ON DELETE RESTRICT,
    CONSTRAINT "FK_AddonMediaList_Playlist_PlaylistId"
        FOREIGN KEY ("PlaylistId") REFERENCES "Playlist" ("Id") ON DELETE SET NULL
);

CREATE UNIQUE INDEX "IX_AddonMediaList_AddonId_ProviderId"
    ON "AddonMediaList" ("AddonId", "ProviderId");
CREATE INDEX "IX_AddonMediaList_Name" ON "AddonMediaList" ("Name");
CREATE INDEX "IX_AddonMediaList_PlaylistId" ON "AddonMediaList" ("PlaylistId");

CREATE TABLE "AddonMediaListItem" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListItem" PRIMARY KEY AUTOINCREMENT,
    "AddonMediaListId" INTEGER NOT NULL,
    "ProviderId" TEXT NOT NULL,
    "DisplayTitle" TEXT NOT NULL,
    "Kind" INTEGER NOT NULL,
    "MediaItemId" INTEGER NULL,
    "Rank" INTEGER NOT NULL,
    "Title" TEXT NOT NULL,
    "Year" INTEGER NULL,
    "Season" INTEGER NULL,
    "Episode" INTEGER NULL,
    CONSTRAINT "FK_AddonMediaListItem_AddonMediaList_AddonMediaListId"
        FOREIGN KEY ("AddonMediaListId") REFERENCES "AddonMediaList" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_AddonMediaListItem_MediaItem_MediaItemId"
        FOREIGN KEY ("MediaItemId") REFERENCES "MediaItem" ("Id") ON DELETE SET NULL
);

CREATE UNIQUE INDEX "IX_AddonMediaListItem_AddonMediaListId_ProviderId"
    ON "AddonMediaListItem" ("AddonMediaListId", "ProviderId");
CREATE INDEX "IX_AddonMediaListItem_MediaItemId" ON "AddonMediaListItem" ("MediaItemId");
CREATE INDEX "IX_AddonMediaListItem_ListRank"
    ON "AddonMediaListItem" ("AddonMediaListId", "Rank");

CREATE TABLE "AddonMediaListItemGuid" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListItemGuid" PRIMARY KEY AUTOINCREMENT,
    "AddonMediaListItemId" INTEGER NOT NULL,
    "Guid" TEXT NOT NULL,
    CONSTRAINT "FK_AddonMediaListItemGuid_AddonMediaListItem_AddonMediaListItemId"
        FOREIGN KEY ("AddonMediaListItemId") REFERENCES "AddonMediaListItem" ("Id") ON DELETE CASCADE
);

CREATE INDEX "IX_AddonMediaListItemGuid_AddonMediaListItemId"
    ON "AddonMediaListItemGuid" ("AddonMediaListItemId");
CREATE INDEX "IX_AddonMediaListItemGuid_Guid" ON "AddonMediaListItemGuid" ("Guid");
