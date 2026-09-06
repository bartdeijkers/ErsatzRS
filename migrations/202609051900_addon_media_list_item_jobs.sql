-- ADR 0085 extension (no ErsatzTV equivalent): item work is independent of
-- the managed-library publication lease.
CREATE TABLE "AddonMediaListItemOperationJob" (
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
    CONSTRAINT "CK_AddonMediaListItemOperationJob_Kind" CHECK ("Kind" IN (0, 1)),
    CONSTRAINT "CK_AddonMediaListItemOperationJob_State" CHECK ("State" BETWEEN 0 AND 5),
    CONSTRAINT "CK_AddonMediaListItemOperationJob_Attempts" CHECK ("AttemptCount" BETWEEN 0 AND 3 AND "Generation" >= 0),
    CONSTRAINT "CK_AddonMediaListItemOperationJob_Input" CHECK (length(CAST("InputJson" AS BLOB)) <= 524288)
);
CREATE UNIQUE INDEX "IX_AddonMediaListItemOperationJob_Active"
    ON "AddonMediaListItemOperationJob" ("AddonMediaListItemId", "Kind") WHERE "State" IN (0, 1);
CREATE INDEX "IX_AddonMediaListItemOperationJob_Claim"
    ON "AddonMediaListItemOperationJob" ("State", "NextEligibleAt", "LeaseExpiresAt", "Id");
CREATE INDEX "IX_AddonMediaListItemOperationJob_ItemKindHistory"
    ON "AddonMediaListItemOperationJob" ("AddonMediaListItemId", "Kind", "Id");
