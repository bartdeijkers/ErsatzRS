-- ErsatzRS extension; Schatkamer chapter fragments have no direct ErsatzTV
-- provider-list equivalent. Keep identifiers below MySQL's 64-byte limit.
ALTER TABLE `AddonMediaList` ADD COLUMN `ChapterInput` LONGTEXT NULL;

ALTER TABLE `AddonMediaListItem` ADD COLUMN `DurationSeconds` BIGINT UNSIGNED NULL;
ALTER TABLE `AddonMediaListItem` ADD COLUMN `ParentProviderId` LONGTEXT NULL;
ALTER TABLE `AddonMediaListItem` ADD COLUMN `FragmentStartSeconds` BIGINT UNSIGNED NULL;
ALTER TABLE `AddonMediaListItem` ADD COLUMN `FragmentEndSeconds` BIGINT UNSIGNED NULL;
ALTER TABLE `AddonMediaListItem` ADD COLUMN `AdditionalImageUrls` LONGTEXT NULL;

CREATE TABLE `AddonMediaListChapter` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `AddonMediaListId` INT NOT NULL,
    `SourceItemId` INT NOT NULL,
    `FragmentItemId` INT NOT NULL,
    `ChapterIndex` INT NOT NULL,
    `Title` LONGTEXT NOT NULL,
    `StartSeconds` BIGINT UNSIGNED NOT NULL,
    `EndSeconds` BIGINT UNSIGNED NULL,
    CONSTRAINT `PK_AddonMediaListChapter` PRIMARY KEY (`Id`),
    CONSTRAINT `FK_AMLChapter_AML_ListId` FOREIGN KEY (`AddonMediaListId`) REFERENCES `AddonMediaList` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_AMLChapter_Item_SourceId` FOREIGN KEY (`SourceItemId`) REFERENCES `AddonMediaListItem` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_AMLChapter_Item_FragmentId` FOREIGN KEY (`FragmentItemId`) REFERENCES `AddonMediaListItem` (`Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE UNIQUE INDEX `IX_AMLChapter_FragmentItemId` ON `AddonMediaListChapter` (`FragmentItemId`);
CREATE UNIQUE INDEX `IX_AMLChapter_SourceItemId_Index` ON `AddonMediaListChapter` (`SourceItemId`, `ChapterIndex`);
