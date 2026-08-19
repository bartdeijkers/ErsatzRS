-- ErsatzRS extension: project an add-on list into the normal Movie hierarchy
-- while retaining its Remote Stream playback source, mirroring the television
-- projection from ADR 0071. A movie list has no parent container, so only the
-- per-item link exists.
--
-- The first foreign key is 66 characters spelled out, two over MySQL's
-- 64-character identifier limit, so it carries the same EF-style truncation the
-- rest of the MySQL schema uses. The SQLite spelling stays complete.

CREATE TABLE `AddonMediaListItemMovie` (
    `AddonMediaListItemId` INT NOT NULL,
    `MovieId` INT NOT NULL,
    CONSTRAINT `PK_AddonMediaListItemMovie` PRIMARY KEY (`AddonMediaListItemId`),
    CONSTRAINT `FK_AddonMediaListItemMovie_AddonMediaListItem_AddonMediaListIte~`
        FOREIGN KEY (`AddonMediaListItemId`) REFERENCES `AddonMediaListItem` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_AddonMediaListItemMovie_Movie_MovieId`
        FOREIGN KEY (`MovieId`) REFERENCES `Movie` (`Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE UNIQUE INDEX `IX_AddonMediaListItemMovie_MovieId`
    ON `AddonMediaListItemMovie` (`MovieId`);
