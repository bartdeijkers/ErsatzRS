-- M-02: sidecars keep a language-specific cache key while sharing the durable
-- operation context. Legacy artifacts remain valid with NULL metadata.
ALTER TABLE "AddonMediaListArtifact" ADD COLUMN "OperationContextFingerprint" TEXT NULL;
ALTER TABLE "AddonMediaListArtifact" ADD COLUMN "SubtitleLanguage" TEXT NULL;
ALTER TABLE "AddonMediaListArtifact" ADD COLUMN "PreparedSubtitleIdsJson" TEXT NOT NULL DEFAULT '[]';
ALTER TABLE "AddonMediaListArtifact" ADD COLUMN "SubtitleTimeline" INTEGER NULL
    CONSTRAINT "CK_AddonMediaListArtifact_SubtitleTimeline" CHECK ("SubtitleTimeline" IN (0, 1));
