-- ErsatzRS extension: persist an exact media-version and stream combination
-- for reusable playlist, schedule, and materialized playout items.
-- These columns intentionally have no foreign keys so a rescan can leave a
-- stale selection visible to validation and health reporting.

ALTER TABLE `PlaylistItem` ADD COLUMN `MediaVersionId` INT NULL;
ALTER TABLE `PlaylistItem` ADD COLUMN `VideoStreamIndex` INT NULL;
ALTER TABLE `PlaylistItem` ADD COLUMN `AudioStreamIndex` INT NULL;
ALTER TABLE `PlaylistItem` ADD COLUMN `SubtitleStreamIndex` INT NULL;

ALTER TABLE `ProgramScheduleItem` ADD COLUMN `MediaVersionId` INT NULL;
ALTER TABLE `ProgramScheduleItem` ADD COLUMN `VideoStreamIndex` INT NULL;
ALTER TABLE `ProgramScheduleItem` ADD COLUMN `AudioStreamIndex` INT NULL;
ALTER TABLE `ProgramScheduleItem` ADD COLUMN `SubtitleStreamIndex` INT NULL;

ALTER TABLE `PlayoutItem` ADD COLUMN `MediaVersionId` INT NULL;
ALTER TABLE `PlayoutItem` ADD COLUMN `VideoStreamIndex` INT NULL;
ALTER TABLE `PlayoutItem` ADD COLUMN `AudioStreamIndex` INT NULL;
ALTER TABLE `PlayoutItem` ADD COLUMN `SubtitleStreamIndex` INT NULL;
