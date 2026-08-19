-- ErsatzRS extension: project an add-on list into the normal Other Video
-- hierarchy while retaining its Remote Stream playback source. Other Videos
-- have no parent container, so this mirrors the Movie projection exactly.
--
-- The first foreign key is 71 characters spelled out, seven over MySQL's
-- 64-character identifier limit, so it carries the same EF-style truncation the
-- rest of the MySQL schema uses. The SQLite spelling stays complete.

CREATE TABLE `AddonMediaListItemOtherVideo` (
    `AddonMediaListItemId` INT NOT NULL,
    `OtherVideoId` INT NOT NULL,
    CONSTRAINT `PK_AddonMediaListItemOtherVideo` PRIMARY KEY (`AddonMediaListItemId`),
    CONSTRAINT `FK_AddonMediaListItemOtherVideo_AddonMediaListItem_AddonMediaLi~`
        FOREIGN KEY (`AddonMediaListItemId`) REFERENCES `AddonMediaListItem` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_AddonMediaListItemOtherVideo_OtherVideo_OtherVideoId`
        FOREIGN KEY (`OtherVideoId`) REFERENCES `OtherVideo` (`Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE UNIQUE INDEX `IX_AddonMediaListItemOtherVideo_OtherVideoId`
    ON `AddonMediaListItemOtherVideo` (`OtherVideoId`);
