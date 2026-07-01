-- ErsatzRS extension; no direct ErsatzTV migration exists.
-- ADR 0048 adds per collection/playlist item playback preferences so admins
-- can schedule the same media item with alternate audio/subtitle selections.
ALTER TABLE `CollectionItem` ADD COLUMN `PreferredAudioLanguageCode` LONGTEXT NULL;
ALTER TABLE `CollectionItem` ADD COLUMN `PreferredAudioTitle` LONGTEXT NULL;
ALTER TABLE `CollectionItem` ADD COLUMN `PreferredSubtitleLanguageCode` LONGTEXT NULL;
ALTER TABLE `CollectionItem` ADD COLUMN `SubtitleMode` INT NULL;

ALTER TABLE `PlaylistItem` ADD COLUMN `PreferredAudioLanguageCode` LONGTEXT NULL;
ALTER TABLE `PlaylistItem` ADD COLUMN `PreferredAudioTitle` LONGTEXT NULL;
ALTER TABLE `PlaylistItem` ADD COLUMN `PreferredSubtitleLanguageCode` LONGTEXT NULL;
ALTER TABLE `PlaylistItem` ADD COLUMN `SubtitleMode` INT NULL;
