-- ErsatzRS source HDR static metadata (ADR 0064, milestone p8a-01).
--
-- Additive. NULL means the source carried no such metadata, which is what every
-- existing row effectively had, so HDR10 output keeps using the documented
-- profile defaults until a rescan populates these columns.
--
-- Mastering display is stored in FFmpeg's own units so it can be handed to
-- `-mastering_display` without re-deriving: chromaticity in 1/50000, luminance
-- in 1/10000 cd/m2. Content light is plain cd/m2.
ALTER TABLE `MediaStream` ADD COLUMN `MasteringDisplay` LONGTEXT NULL;
ALTER TABLE `MediaStream` ADD COLUMN `MaxCll` INT NULL;
ALTER TABLE `MediaStream` ADD COLUMN `MaxFall` INT NULL;
