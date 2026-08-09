ALTER TABLE `MediaStream`
    ADD COLUMN `DolbyVisionMetadataState` INT NOT NULL DEFAULT 0,
    ADD COLUMN `DolbyVisionMetadataHint` TEXT NULL;

UPDATE `MediaStream`
SET `DolbyVisionMetadataState` = 1
WHERE `DolbyVisionProfile` IS NOT NULL;
