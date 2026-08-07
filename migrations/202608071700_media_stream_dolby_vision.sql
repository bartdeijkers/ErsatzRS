-- ErsatzRS Dolby Vision source configuration (ADR 0064, milestone p8a-02).
--
-- Additive. NULL means the stream is not Dolby Vision, which is what every
-- existing row effectively had, so nothing changes until a rescan.
--
-- Persisted so playback can apply the profile policy without re-probing and so
-- troubleshooting can explain why an item was rejected.
ALTER TABLE "MediaStream" ADD COLUMN "DolbyVisionProfile" INTEGER NULL;
ALTER TABLE "MediaStream" ADD COLUMN "DolbyVisionBlCompatibilityId" INTEGER NULL;
ALTER TABLE "MediaStream" ADD COLUMN "DolbyVisionElPresent" INTEGER NULL;
ALTER TABLE "MediaStream" ADD COLUMN "DolbyVisionRpuPresent" INTEGER NULL;
