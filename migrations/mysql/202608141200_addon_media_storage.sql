CREATE TABLE `AddonMediaSource` (
    `AddonId` VARCHAR(255) NOT NULL,
    `MediaSourceId` INT NOT NULL,
    `Name` VARCHAR(128) NOT NULL,
    `RootPath` LONGTEXT NOT NULL,
    CONSTRAINT `PK_AddonMediaSource` PRIMARY KEY (`AddonId`),
    CONSTRAINT `FK_AddonMediaSource_AddonInstallation_AddonId`
        FOREIGN KEY (`AddonId`) REFERENCES `AddonInstallation` (`AddonId`) ON DELETE CASCADE,
    CONSTRAINT `FK_AddonMediaSource_MediaSource_MediaSourceId`
        FOREIGN KEY (`MediaSourceId`) REFERENCES `MediaSource` (`Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE UNIQUE INDEX `IX_AddonMediaSource_MediaSourceId`
    ON `AddonMediaSource` (`MediaSourceId`);

CREATE TABLE `AddonMediaListLibrary` (
    `AddonMediaListId` INT NOT NULL,
    `LibraryId` INT NOT NULL,
    `FolderName` VARCHAR(767) NOT NULL,
    `ManifestPath` LONGTEXT NOT NULL,
    CONSTRAINT `PK_AddonMediaListLibrary` PRIMARY KEY (`AddonMediaListId`),
    CONSTRAINT `FK_AddonMediaListLibrary_AddonMediaList_ListId`
        FOREIGN KEY (`AddonMediaListId`) REFERENCES `AddonMediaList` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_AddonMediaListLibrary_Library_LibraryId`
        FOREIGN KEY (`LibraryId`) REFERENCES `Library` (`Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE UNIQUE INDEX `IX_AddonMediaListLibrary_LibraryId`
    ON `AddonMediaListLibrary` (`LibraryId`);

ALTER TABLE `AddonMediaListItem` ADD COLUMN `SourceUrl` LONGTEXT NULL;
