-- Trakt lists are now owned by repository-installed add-ons. This migration is
-- intentionally destructive: legacy Trakt rows are not imported or retained.

DELETE FROM `Playlist`
WHERE `IsSystem` = 1
  AND `Id` IN (
      SELECT `PlaylistId` FROM `TraktList` WHERE `PlaylistId` IS NOT NULL
  );

DELETE FROM `ConfigElement` WHERE `Key` = 'pages.trakt.lists_page_size';
DELETE FROM `ConfigElement` WHERE `Key` = 'pages.addon_media_lists.page_size';

DROP TABLE `TraktListItemGuid`;
DROP TABLE `TraktListItem`;
DROP TABLE `TraktList`;

CREATE TABLE `AddonMediaList` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `AddonId` VARCHAR(255) NOT NULL,
    `ProviderId` VARCHAR(384) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    `SourceUrl` LONGTEXT NOT NULL,
    `Name` VARCHAR(767) NOT NULL,
    `Description` LONGTEXT NULL,
    `ItemCount` INT NOT NULL DEFAULT 0,
    `AutoRefresh` INT NOT NULL DEFAULT 0,
    `GeneratePlaylist` INT NOT NULL DEFAULT 0,
    `PlaylistId` INT NULL,
    `LastUpdate` LONGTEXT NULL,
    `LastMatch` LONGTEXT NULL,
    `LastError` LONGTEXT NULL,
    CONSTRAINT `PK_AddonMediaList` PRIMARY KEY (`Id`),
    CONSTRAINT `FK_AddonMediaList_AddonInstallation_AddonId`
        FOREIGN KEY (`AddonId`) REFERENCES `AddonInstallation` (`AddonId`) ON DELETE RESTRICT,
    CONSTRAINT `FK_AddonMediaList_Playlist_PlaylistId`
        FOREIGN KEY (`PlaylistId`) REFERENCES `Playlist` (`Id`) ON DELETE SET NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE UNIQUE INDEX `IX_AddonMediaList_AddonId_ProviderId`
    ON `AddonMediaList` (`AddonId`, `ProviderId`);
CREATE INDEX `IX_AddonMediaList_Name` ON `AddonMediaList` (`Name`);
CREATE INDEX `IX_AddonMediaList_PlaylistId` ON `AddonMediaList` (`PlaylistId`);

CREATE TABLE `AddonMediaListItem` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `AddonMediaListId` INT NOT NULL,
    `ProviderId` VARCHAR(384) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    `DisplayTitle` LONGTEXT NOT NULL,
    `Kind` INT NOT NULL,
    `MediaItemId` INT NULL,
    `Rank` INT NOT NULL,
    `Title` LONGTEXT NOT NULL,
    `Year` INT NULL,
    `Season` INT NULL,
    `Episode` INT NULL,
    CONSTRAINT `PK_AddonMediaListItem` PRIMARY KEY (`Id`),
    CONSTRAINT `FK_AddonMediaListItem_AddonMediaList_AddonMediaListId`
        FOREIGN KEY (`AddonMediaListId`) REFERENCES `AddonMediaList` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_AddonMediaListItem_MediaItem_MediaItemId`
        FOREIGN KEY (`MediaItemId`) REFERENCES `MediaItem` (`Id`) ON DELETE SET NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE UNIQUE INDEX `IX_AddonMediaListItem_AddonMediaListId_ProviderId`
    ON `AddonMediaListItem` (`AddonMediaListId`, `ProviderId`);
CREATE INDEX `IX_AddonMediaListItem_MediaItemId` ON `AddonMediaListItem` (`MediaItemId`);
CREATE INDEX `IX_AddonMediaListItem_ListRank`
    ON `AddonMediaListItem` (`AddonMediaListId`, `Rank`);

CREATE TABLE `AddonMediaListItemGuid` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `AddonMediaListItemId` INT NOT NULL,
    `Guid` LONGTEXT NOT NULL,
    CONSTRAINT `PK_AddonMediaListItemGuid` PRIMARY KEY (`Id`),
    -- MySQL limits identifiers to 64 characters; retain the dependent column meaning.
    CONSTRAINT `FK_AddonMediaListItemGuid_AddonMediaListItem_ListItemId`
        FOREIGN KEY (`AddonMediaListItemId`) REFERENCES `AddonMediaListItem` (`Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE INDEX `IX_AddonMediaListItemGuid_AddonMediaListItemId`
    ON `AddonMediaListItemGuid` (`AddonMediaListItemId`);
CREATE INDEX `IX_AddonMediaListItemGuid_Guid`
    ON `AddonMediaListItemGuid` (`Guid`(255));
