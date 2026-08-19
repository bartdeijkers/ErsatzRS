-- ErsatzRS extension: project an add-on list into the normal Music Video
-- hierarchy while retaining its Remote Stream playback source.
--
-- Unlike the other projected kinds, `MusicVideo.ArtistId` is NOT NULL, so a
-- music-video list needs one managed Artist above it the way a television list
-- needs a Show.

CREATE TABLE "AddonMediaListArtist" (
    "AddonMediaListId" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListArtist" PRIMARY KEY,
    "ArtistId" INTEGER NOT NULL,
    CONSTRAINT "FK_AddonMediaListArtist_AddonMediaList_AddonMediaListId"
        FOREIGN KEY ("AddonMediaListId") REFERENCES "AddonMediaList" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_AddonMediaListArtist_Artist_ArtistId"
        FOREIGN KEY ("ArtistId") REFERENCES "Artist" ("Id") ON DELETE CASCADE
);

CREATE UNIQUE INDEX "IX_AddonMediaListArtist_ArtistId"
    ON "AddonMediaListArtist" ("ArtistId");

CREATE TABLE "AddonMediaListItemMusicVideo" (
    "AddonMediaListItemId" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListItemMusicVideo" PRIMARY KEY,
    "MusicVideoId" INTEGER NOT NULL,
    CONSTRAINT "FK_AddonMediaListItemMusicVideo_AddonMediaListItem_AddonMediaListItemId"
        FOREIGN KEY ("AddonMediaListItemId") REFERENCES "AddonMediaListItem" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_AddonMediaListItemMusicVideo_MusicVideo_MusicVideoId"
        FOREIGN KEY ("MusicVideoId") REFERENCES "MusicVideo" ("Id") ON DELETE CASCADE
);

CREATE UNIQUE INDEX "IX_AddonMediaListItemMusicVideo_MusicVideoId"
    ON "AddonMediaListItemMusicVideo" ("MusicVideoId");
