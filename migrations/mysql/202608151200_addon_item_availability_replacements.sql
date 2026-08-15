ALTER TABLE `AddonMediaListItem`
    ADD COLUMN `Availability` INT NOT NULL DEFAULT 0,
    ADD COLUMN `AvailabilityReason` TEXT NULL,
    ADD COLUMN `AvailabilityUpdated` VARCHAR(64) NULL,
    ADD COLUMN `ContentKind` INT NOT NULL DEFAULT 0;

ALTER TABLE `RemoteStreamMetadata`
    ADD COLUMN `Outline` LONGTEXT NULL,
    ADD COLUMN `Tagline` LONGTEXT NULL,
    ADD COLUMN `ShowTitle` LONGTEXT NULL,
    ADD COLUMN `SeasonNumber` INT NULL,
    ADD COLUMN `EpisodeNumber` INT NULL;

ALTER TABLE `AddonMediaList`
    ADD COLUMN `ContentKindOverride` INT NOT NULL DEFAULT 0;

CREATE TABLE `AddonMediaListItemReplacement` (
    `AddonMediaListItemId` INT NOT NULL,
    `MediaItemId` INT NOT NULL,
    `MediaVersionId` INT NOT NULL,
    `CreatedAt` VARCHAR(64) NOT NULL,
    `UpdatedAt` VARCHAR(64) NOT NULL,
    CONSTRAINT `PK_AddonMediaListItemReplacement` PRIMARY KEY (`AddonMediaListItemId`),
    CONSTRAINT `FK_AddonItemReplacement_AddonMediaListItem_ItemId`
        FOREIGN KEY (`AddonMediaListItemId`) REFERENCES `AddonMediaListItem` (`Id`) ON DELETE CASCADE
);

CREATE INDEX `IX_AddonMediaListItemReplacement_MediaItemId`
    ON `AddonMediaListItemReplacement` (`MediaItemId`);
CREATE INDEX `IX_AddonMediaListItemReplacement_MediaVersionId`
    ON `AddonMediaListItemReplacement` (`MediaVersionId`);

ALTER TABLE `RemoteStream`
    ADD COLUMN `Availability` INT NOT NULL DEFAULT 0,
    ADD COLUMN `AvailabilityReason` TEXT NULL,
    ADD COLUMN `ContentKind` INT NOT NULL DEFAULT 0,
    ADD COLUMN `ProviderId` VARCHAR(384) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NULL;

CREATE UNIQUE INDEX `IX_RemoteStream_AddonId_ProviderId`
    ON `RemoteStream` (`AddonId`, `ProviderId`);
