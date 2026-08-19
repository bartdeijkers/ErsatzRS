-- ErsatzRS extension: project an add-on list into the normal Song hierarchy
-- while retaining its Remote Stream playback source. Songs have no parent
-- container, so this mirrors the Movie and Other Video projections.

CREATE TABLE "AddonMediaListItemSong" (
    "AddonMediaListItemId" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListItemSong" PRIMARY KEY,
    "SongId" INTEGER NOT NULL,
    CONSTRAINT "FK_AddonMediaListItemSong_AddonMediaListItem_AddonMediaListItemId"
        FOREIGN KEY ("AddonMediaListItemId") REFERENCES "AddonMediaListItem" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_AddonMediaListItemSong_Song_SongId"
        FOREIGN KEY ("SongId") REFERENCES "Song" ("Id") ON DELETE CASCADE
);

CREATE UNIQUE INDEX "IX_AddonMediaListItemSong_SongId"
    ON "AddonMediaListItemSong" ("SongId");
