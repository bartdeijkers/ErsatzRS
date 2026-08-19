-- ErsatzRS extension: project an add-on list into the normal Music Video
-- hierarchy while retaining its Remote Stream playback source.
--
-- Unlike the other projected kinds, `MusicVideo.ArtistId` is NOT NULL, so a
-- music-video list needs one managed Artist above it the way a television list
-- needs a Show.
--
-- The item foreign key is 71 characters spelled out, seven over MySQL's
-- 64-character identifier limit, so it carries the same EF-style truncation the
-- rest of the MySQL schema uses. The SQLite spelling stays complete.

CREATE TABLE `AddonMediaListArtist` (
    `AddonMediaListId` INT NOT NULL,
    `ArtistId` INT NOT NULL,
    CONSTRAINT `PK_AddonMediaListArtist` PRIMARY KEY (`AddonMediaListId`),
    CONSTRAINT `FK_AddonMediaListArtist_AddonMediaList_AddonMediaListId`
        FOREIGN KEY (`AddonMediaListId`) REFERENCES `AddonMediaList` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_AddonMediaListArtist_Artist_ArtistId`
        FOREIGN KEY (`ArtistId`) REFERENCES `Artist` (`Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE UNIQUE INDEX `IX_AddonMediaListArtist_ArtistId`
    ON `AddonMediaListArtist` (`ArtistId`);

CREATE TABLE `AddonMediaListItemMusicVideo` (
    `AddonMediaListItemId` INT NOT NULL,
    `MusicVideoId` INT NOT NULL,
    CONSTRAINT `PK_AddonMediaListItemMusicVideo` PRIMARY KEY (`AddonMediaListItemId`),
    CONSTRAINT `FK_AddonMediaListItemMusicVideo_AddonMediaListItem_AddonMediaLi~`
        FOREIGN KEY (`AddonMediaListItemId`) REFERENCES `AddonMediaListItem` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_AddonMediaListItemMusicVideo_MusicVideo_MusicVideoId`
        FOREIGN KEY (`MusicVideoId`) REFERENCES `MusicVideo` (`Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE UNIQUE INDEX `IX_AddonMediaListItemMusicVideo_MusicVideoId`
    ON `AddonMediaListItemMusicVideo` (`MusicVideoId`);
