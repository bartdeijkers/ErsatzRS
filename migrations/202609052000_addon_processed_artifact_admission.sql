-- ADR 0085 processed-media metadata and attempt ownership; no ErsatzTV equivalent.
ALTER TABLE "AddonMediaListArtifact" ADD COLUMN "DurationMilliseconds" INTEGER NULL
    CHECK ("DurationMilliseconds" IS NULL OR "DurationMilliseconds" > 0);
ALTER TABLE "AddonMediaListArtifact" ADD COLUMN "SelectedFormat" TEXT NULL;
ALTER TABLE "AddonMediaListArtifact" ADD COLUMN "ToolVersion" TEXT NULL;
-- Provenance is intentionally not a foreign key: artifact and job retention
-- differ, and job output already references artifacts. Avoid a circular cascade.
ALTER TABLE "AddonMediaListArtifact" ADD COLUMN "PreparationJobId" INTEGER NULL;
ALTER TABLE "AddonMediaListArtifact" ADD COLUMN "PreparationGeneration" INTEGER NULL;
ALTER TABLE "AddonMediaListArtifact" ADD COLUMN "PreparationLeaseOwner" TEXT NULL;
CREATE INDEX "IX_AddonMediaListArtifact_PreparationJobId"
    ON "AddonMediaListArtifact" ("PreparationJobId");
