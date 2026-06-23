-- Add the ADR 0018 content-variant foundation.
--
-- These columns have no ErsatzTV equivalent: ErsatzTV discards local extras at
-- scan time and has no parent/alternate linkage. Defaults preserve the
-- ErsatzTV-compatible primary-only behavior until later scanner/UI slices opt in.

ALTER TABLE `MediaItem` ADD COLUMN `ContentKind` INT NOT NULL DEFAULT 0;
ALTER TABLE `MediaItem` ADD COLUMN `SubKind` LONGTEXT NULL;
ALTER TABLE `MediaItem` ADD COLUMN `ParentMediaItemId` INT NULL;

ALTER TABLE `MediaItem`
    ADD CONSTRAINT `FK_MediaItem_MediaItem_ParentMediaItemId`
    FOREIGN KEY (`ParentMediaItemId`) REFERENCES `MediaItem` (`Id`) ON DELETE SET NULL;

CREATE INDEX `IX_MediaItem_ContentKind` ON `MediaItem` (`ContentKind`);
CREATE INDEX `IX_MediaItem_ParentMediaItemId` ON `MediaItem` (`ParentMediaItemId`);
