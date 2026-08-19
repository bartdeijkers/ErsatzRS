-- ErsatzRS extension: project an add-on list into the normal Song hierarchy
-- while retaining its Remote Stream playback source. Songs have no parent
-- container, so this mirrors the Movie and Other Video projections.
--
-- The first foreign key is 65 characters spelled out, one over MySQL's
-- 64-character identifier limit, so it carries the same EF-style truncation the
-- rest of the MySQL schema uses. The SQLite spelling stays complete.

CREATE TABLE `AddonMediaListItemSong` (
    `AddonMediaListItemId` INT NOT NULL,
    `SongId` INT NOT NULL,
    CONSTRAINT `PK_AddonMediaListItemSong` PRIMARY KEY (`AddonMediaListItemId`),
    CONSTRAINT `FK_AddonMediaListItemSong_AddonMediaListItem_AddonMediaListItem~`
        FOREIGN KEY (`AddonMediaListItemId`) REFERENCES `AddonMediaListItem` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_AddonMediaListItemSong_Song_SongId`
        FOREIGN KEY (`SongId`) REFERENCES `Song` (`Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE UNIQUE INDEX `IX_AddonMediaListItemSong_SongId`
    ON `AddonMediaListItemSong` (`SongId`);
