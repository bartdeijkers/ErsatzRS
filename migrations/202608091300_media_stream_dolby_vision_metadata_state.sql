ALTER TABLE "MediaStream" ADD COLUMN "DolbyVisionMetadataState" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "MediaStream" ADD COLUMN "DolbyVisionMetadataHint" TEXT NULL;

UPDATE "MediaStream"
SET "DolbyVisionMetadataState" = 1
WHERE "DolbyVisionProfile" IS NOT NULL;
