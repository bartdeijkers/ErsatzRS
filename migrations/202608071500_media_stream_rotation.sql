-- ErsatzRS source orientation (ADR 0003, milestone p8c-02).
--
-- Additive. NULL means "no display matrix", which is what every existing row
-- effectively had, so playback geometry is unchanged until a rescan populates
-- the column.
ALTER TABLE "MediaStream" ADD COLUMN "Rotation" INTEGER NULL;
