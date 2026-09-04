-- Freeze the negotiated import transport for every durable generation. Package
-- upgrades may add a newer capability while an older job is still resumable.
ALTER TABLE "AddonMediaListImportJob"
    ADD COLUMN "ImportCapability" TEXT NOT NULL DEFAULT 'media-list.import.v1';

-- Final promotion spans existing list, item, library, and detail transactions.
-- Durable phases keep each replay idempotent while the job lease fences writers.
ALTER TABLE "AddonMediaListImportJob"
    ADD COLUMN "FinalizationPhase" INTEGER NOT NULL DEFAULT 0;

-- A list keeps one fully merged checkpoint while a running job may point at a
-- newer resumable page checkpoint. Immutable rows keep obsolete files
-- locatable until their best-effort cleanup succeeds.
DROP INDEX "IX_AddonMediaListImportCheckpoint_AddonMediaListId";

ALTER TABLE "AddonMediaListImportCheckpoint"
    ADD COLUMN "DataRoot" TEXT NULL;

ALTER TABLE "AddonMediaListImportCheckpoint"
    ADD COLUMN "Committed" INTEGER NOT NULL DEFAULT 1;

CREATE UNIQUE INDEX "IX_AddonMediaListImportCheckpoint_AddonMediaListId_Committed"
    ON "AddonMediaListImportCheckpoint" ("AddonMediaListId")
    WHERE "Committed" = 1;

CREATE INDEX "IX_AddonMediaListImportCheckpoint_Committed"
    ON "AddonMediaListImportCheckpoint" ("Committed");
