-- Preserve the complete Dolby Vision configuration needed by the profile 7
-- base-layer separation policy (ADR 0064).
--
-- Additive. Existing Dolby Vision rows remain NULL and are therefore rejected
-- until a media scan records the actual ffprobe flag.
ALTER TABLE `MediaStream` ADD COLUMN `DolbyVisionBlPresent` INT NULL;
