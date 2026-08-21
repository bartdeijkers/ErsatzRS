ALTER TABLE `AddonMediaList`
    ADD COLUMN `MetadataConflictJson` LONGTEXT NULL,
    ADD COLUMN `PublicationJournalJson` LONGTEXT NULL,
    ADD COLUMN `PublicationStartedAt` TEXT NULL;

CREATE TABLE `AddonMediaListArtworkUpload` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `AddonMediaListId` INT NOT NULL,
    `AddonMediaListItemId` INT NULL,
    `ScopeItemId` INT NOT NULL DEFAULT 0,
    `Role` INT NOT NULL,
    `ContentType` TEXT NOT NULL,
    `Extension` TEXT NOT NULL,
    `Bytes` LONGBLOB NOT NULL,
    `Sha256` TEXT NOT NULL,
    `CreatedAt` TEXT NOT NULL,
    `UpdatedAt` TEXT NOT NULL,
    CONSTRAINT `PK_AddonMediaListArtworkUpload` PRIMARY KEY (`Id`),
    CONSTRAINT `FK_AddonMediaListArtworkUpload_AddonMediaList_AddonMediaListId`
        FOREIGN KEY (`AddonMediaListId`) REFERENCES `AddonMediaList` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_AddonMediaListArtworkUpload_AddonMediaListItem_AddonMediaLis~`
        FOREIGN KEY (`AddonMediaListItemId`) REFERENCES `AddonMediaListItem` (`Id`) ON DELETE CASCADE
);

CREATE UNIQUE INDEX `IX_AddonMediaListArtworkUpload_ListScopeRole`
    ON `AddonMediaListArtworkUpload` (`AddonMediaListId`, `ScopeItemId`, `Role`);
CREATE INDEX `IX_AddonMediaListArtworkUpload_ItemId`
    ON `AddonMediaListArtworkUpload` (`AddonMediaListItemId`);

CREATE TABLE `AddonMediaListArtworkMaterialization` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `AddonMediaListId` INT NOT NULL,
    `AddonMediaListItemId` INT NULL,
    `ScopeItemId` INT NOT NULL DEFAULT 0,
    `Role` INT NOT NULL,
    `Source` TEXT NOT NULL,
    `FileName` TEXT NOT NULL,
    `Sha256` TEXT NOT NULL,
    `PublishedRevision` INT NOT NULL,
    CONSTRAINT `PK_AddonMediaListArtworkMaterialization` PRIMARY KEY (`Id`),
    CONSTRAINT `FK_AddonMediaListArtworkMaterialization_AddonMediaList_AddonMed~`
        FOREIGN KEY (`AddonMediaListId`) REFERENCES `AddonMediaList` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_AddonMediaListArtworkMaterialization_AddonMediaListItem_Addo~`
        FOREIGN KEY (`AddonMediaListItemId`) REFERENCES `AddonMediaListItem` (`Id`) ON DELETE CASCADE
);

CREATE UNIQUE INDEX `IX_AddonMediaListArtworkMaterialization_ListScopeRole`
    ON `AddonMediaListArtworkMaterialization` (`AddonMediaListId`, `ScopeItemId`, `Role`);
CREATE INDEX `IX_AddonMediaListArtworkMaterialization_ItemId`
    ON `AddonMediaListArtworkMaterialization` (`AddonMediaListItemId`);
