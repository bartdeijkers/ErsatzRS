-- ErsatzRS-specific drawvg overlay configuration columns for GraphicsElement.
-- ErsatzTV stores only Id, Path, Kind, Name. These columns are required to drive
-- the FFmpeg 8.1 drawvg filter (ADR 0005) from persisted metadata.
-- ALTERs are safe against existing ErsatzTV databases: new rows default to 0/NULL.

ALTER TABLE `GraphicsElement` ADD COLUMN `X` INT NOT NULL DEFAULT 0;
ALTER TABLE `GraphicsElement` ADD COLUMN `Y` INT NOT NULL DEFAULT 0;
ALTER TABLE `GraphicsElement` ADD COLUMN `Width` INT NOT NULL DEFAULT 0;
ALTER TABLE `GraphicsElement` ADD COLUMN `Height` INT NOT NULL DEFAULT 0;
ALTER TABLE `GraphicsElement` ADD COLUMN `ZIndex` INT NOT NULL DEFAULT 0;
ALTER TABLE `GraphicsElement` ADD COLUMN `EnableExpression` LONGTEXT NULL;
ALTER TABLE `GraphicsElement` ADD COLUMN `Text` LONGTEXT NULL;
