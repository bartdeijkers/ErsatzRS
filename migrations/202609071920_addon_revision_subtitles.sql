-- Keep legacy snapshot serialization unchanged while freezing new caption ownership.
ALTER TABLE "AddonMediaListRevisionItem"
    ADD COLUMN "SubtitleArtifactIdsJson" TEXT NOT NULL DEFAULT '[]';
