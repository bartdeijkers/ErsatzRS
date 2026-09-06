-- ADR 0087: retain access and prepared-file jobs while adding interval metadata.
-- SQLite requires a lossless rebuild to extend the Kind constraint. No table
-- references this job table; all IDs, lease state, outcomes and indexes survive.
CREATE TEMP TABLE "AddonItemJobSequence" ("Seq" INTEGER NOT NULL);
INSERT INTO "AddonItemJobSequence" SELECT "seq" FROM "sqlite_sequence"
    WHERE "name" = 'AddonMediaListItemOperationJob';
CREATE TABLE "AddonMediaListItemOperationJob_Interval" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListItemOperationJob" PRIMARY KEY AUTOINCREMENT,
    "AddonMediaListId" INTEGER NOT NULL,
    "AddonMediaListItemId" INTEGER NOT NULL,
    "Kind" INTEGER NOT NULL,
    "Generation" INTEGER NOT NULL DEFAULT 0,
    "SavedDraftRevision" INTEGER NOT NULL,
    "InputJson" TEXT NOT NULL,
    "ContextFingerprint" TEXT NOT NULL,
    "State" INTEGER NOT NULL DEFAULT 0,
    "AttemptCount" INTEGER NOT NULL DEFAULT 0,
    "NextEligibleAt" TEXT NULL,
    "LeaseOwner" TEXT NULL,
    "LeaseExpiresAt" TEXT NULL,
    "HeartbeatAt" TEXT NULL,
    "ArtifactId" INTEGER NULL,
    "OutcomeJson" TEXT NULL,
    "ExpiresAt" TEXT NULL,
    "CreatedAt" TEXT NOT NULL,
    "UpdatedAt" TEXT NOT NULL,
    "CompletedAt" TEXT NULL,
    CONSTRAINT "FK_AddonMediaListItemOperationJob_AddonMediaList_AddonMediaListId" FOREIGN KEY ("AddonMediaListId") REFERENCES "AddonMediaList" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_AddonMediaListItemOperationJob_AddonMediaListItem_AddonMediaListItemId" FOREIGN KEY ("AddonMediaListItemId") REFERENCES "AddonMediaListItem" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_AddonMediaListItemOperationJob_AddonMediaListArtifact_ArtifactId" FOREIGN KEY ("ArtifactId") REFERENCES "AddonMediaListArtifact" ("Id") ON DELETE RESTRICT,
    CONSTRAINT "CK_AddonMediaListItemOperationJob_Kind" CHECK ("Kind" IN (0, 1, 2)),
    CONSTRAINT "CK_AddonMediaListItemOperationJob_State" CHECK ("State" BETWEEN 0 AND 5),
    CONSTRAINT "CK_AddonMediaListItemOperationJob_Attempts" CHECK ("AttemptCount" BETWEEN 0 AND 3 AND "Generation" >= 0),
    CONSTRAINT "CK_AddonMediaListItemOperationJob_Input" CHECK (length(CAST("InputJson" AS BLOB)) <= 524288)
);

INSERT INTO "AddonMediaListItemOperationJob_Interval"
SELECT * FROM "AddonMediaListItemOperationJob";
DROP TABLE "AddonMediaListItemOperationJob";
ALTER TABLE "AddonMediaListItemOperationJob_Interval" RENAME TO "AddonMediaListItemOperationJob";
INSERT INTO "sqlite_sequence" ("name", "seq")
SELECT 'AddonMediaListItemOperationJob', MAX("Seq") FROM "AddonItemJobSequence"
HAVING MAX("Seq") IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM "sqlite_sequence" WHERE "name" = 'AddonMediaListItemOperationJob');
UPDATE "sqlite_sequence" SET "seq" = MAX("seq", COALESCE((SELECT MAX("Seq") FROM "AddonItemJobSequence"), 0))
    WHERE "name" = 'AddonMediaListItemOperationJob';
DROP TABLE "AddonItemJobSequence";
CREATE UNIQUE INDEX "IX_AddonMediaListItemOperationJob_Active"
    ON "AddonMediaListItemOperationJob" ("AddonMediaListItemId", "Kind") WHERE "State" IN (0, 1);
CREATE INDEX "IX_AddonMediaListItemOperationJob_Claim"
    ON "AddonMediaListItemOperationJob" ("State", "NextEligibleAt", "LeaseExpiresAt", "Id");
CREATE INDEX "IX_AddonMediaListItemOperationJob_ItemKindHistory"
    ON "AddonMediaListItemOperationJob" ("AddonMediaListItemId", "Kind", "Id");
