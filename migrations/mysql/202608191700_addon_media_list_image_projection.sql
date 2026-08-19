-- ErsatzRS extension: project an add-on list into the normal Image hierarchy
-- while retaining its Remote Stream source. Images have no parent container, so
-- this mirrors the Movie, Other Video, and Song projections.
--
-- The first foreign key is 66 characters spelled out, two over MySQL's
-- 64-character identifier limit, so it carries the same EF-style truncation the
-- rest of the MySQL schema uses. The SQLite spelling stays complete.

CREATE TABLE `AddonMediaListItemImage` (
    `AddonMediaListItemId` INT NOT NULL,
    `ImageId` INT NOT NULL,
    CONSTRAINT `PK_AddonMediaListItemImage` PRIMARY KEY (`AddonMediaListItemId`),
    CONSTRAINT `FK_AddonMediaListItemImage_AddonMediaListItem_AddonMediaListIte~`
        FOREIGN KEY (`AddonMediaListItemId`) REFERENCES `AddonMediaListItem` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_AddonMediaListItemImage_Image_ImageId`
        FOREIGN KEY (`ImageId`) REFERENCES `Image` (`Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE UNIQUE INDEX `IX_AddonMediaListItemImage_ImageId`
    ON `AddonMediaListItemImage` (`ImageId`);
