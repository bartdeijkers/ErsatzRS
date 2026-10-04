-- Keep the last proven success after a failed attempt. Older failures cannot
-- reconstruct their preceding success, so leave that timestamp unknown.
ALTER TABLE "PlayoutBuildStatus" ADD COLUMN "LastSuccessfulBuild" TEXT NULL;
UPDATE "PlayoutBuildStatus" SET "LastSuccessfulBuild" = "LastBuild" WHERE "Success" = 1;

-- Per-playout observed selection changes, not a count of library scans. This
-- survives refresh-intent acknowledgement and starts at the upgrade baseline.
ALTER TABLE "Playout" ADD COLUMN "ScriptedSourceRevision" INTEGER NOT NULL DEFAULT 0;
CREATE INDEX "IX_ScriptedSourceRefreshWork_PlayoutId" ON "ScriptedSourceRefreshWork" ("PlayoutId");
