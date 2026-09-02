-- AMM-J durable managed-library admission and add-on publication recovery.
ALTER TABLE "AddonMediaList" ADD COLUMN "DraftRevision" INTEGER NOT NULL DEFAULT 0;

UPDATE "AddonMediaList"
SET "DraftRevision" = "PublishedRevision";

CREATE TABLE "ManagedLibraryOperation" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_ManagedLibraryOperation" PRIMARY KEY AUTOINCREMENT,
    "LibraryId" INTEGER NOT NULL,
    "ParentOperationId" INTEGER NULL,
    "OwnerToken" TEXT NOT NULL,
    "Kind" INTEGER NOT NULL,
    "State" INTEGER NOT NULL,
    "Provider" INTEGER NULL,
    "ShowId" INTEGER NULL,
    "DeepScan" INTEGER NOT NULL DEFAULT 0,
    "AttemptCount" INTEGER NOT NULL DEFAULT 0,
    "NextEligibleAt" TEXT NULL,
    "LeaseOwner" TEXT NULL,
    "LeaseExpiresAt" TEXT NULL,
    "HeartbeatAt" TEXT NULL,
    "DiagnosticCode" TEXT NULL,
    "Diagnostic" TEXT NULL,
    "CreatedAt" TEXT NOT NULL,
    "UpdatedAt" TEXT NOT NULL,
    "CompletedAt" TEXT NULL,
    CONSTRAINT "FK_ManagedLibraryOperation_Library_LibraryId"
        FOREIGN KEY ("LibraryId") REFERENCES "Library" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_ManagedLibraryOperation_ManagedLibraryOperation_ParentOperationId"
        FOREIGN KEY ("ParentOperationId") REFERENCES "ManagedLibraryOperation" ("Id") ON DELETE CASCADE
);

CREATE INDEX "IX_ManagedLibraryOperation_Claim"
    ON "ManagedLibraryOperation" ("State", "NextEligibleAt", "LeaseExpiresAt", "CreatedAt", "Id");
CREATE INDEX "IX_ManagedLibraryOperation_LibraryState"
    ON "ManagedLibraryOperation" ("LibraryId", "State", "Id");
CREATE INDEX "IX_ManagedLibraryOperation_ParentOperationId"
    ON "ManagedLibraryOperation" ("ParentOperationId");

CREATE TABLE "ManagedLibraryLease" (
    "LibraryId" INTEGER NOT NULL CONSTRAINT "PK_ManagedLibraryLease" PRIMARY KEY,
    "OperationId" INTEGER NOT NULL,
    "OwnerToken" TEXT NOT NULL,
    "LeaseOwner" TEXT NOT NULL,
    "LeaseExpiresAt" TEXT NOT NULL,
    "HeartbeatAt" TEXT NOT NULL,
    "UpdatedAt" TEXT NOT NULL,
    CONSTRAINT "FK_ManagedLibraryLease_Library_LibraryId"
        FOREIGN KEY ("LibraryId") REFERENCES "Library" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_ManagedLibraryLease_ManagedLibraryOperation_OperationId"
        FOREIGN KEY ("OperationId") REFERENCES "ManagedLibraryOperation" ("Id") ON DELETE CASCADE
);

CREATE UNIQUE INDEX "IX_ManagedLibraryLease_OperationId"
    ON "ManagedLibraryLease" ("OperationId");
CREATE INDEX "IX_ManagedLibraryLease_Expiry"
    ON "ManagedLibraryLease" ("LeaseExpiresAt");

CREATE TABLE "AddonMediaListPublicationJob" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListPublicationJob" PRIMARY KEY AUTOINCREMENT,
    "OperationId" INTEGER NOT NULL,
    "AddonMediaListId" INTEGER NOT NULL,
    "Generation" INTEGER NOT NULL,
    "SavedRevision" INTEGER NOT NULL,
    "TargetPublishedRevision" INTEGER NOT NULL,
    "Phase" INTEGER NOT NULL,
    "ChildScanOperationId" INTEGER NULL,
    "OriginalFailureCode" TEXT NULL,
    "OriginalFailureDiagnostic" TEXT NULL,
    "RecoveryDiagnostic" TEXT NULL,
    "Outcome" INTEGER NULL,
    "CreatedAt" TEXT NOT NULL,
    "UpdatedAt" TEXT NOT NULL,
    "CompletedAt" TEXT NULL,
    CONSTRAINT "FK_AddonMediaListPublicationJob_ManagedLibraryOperation_OperationId"
        FOREIGN KEY ("OperationId") REFERENCES "ManagedLibraryOperation" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_AddonMediaListPublicationJob_AddonMediaList_AddonMediaListId"
        FOREIGN KEY ("AddonMediaListId") REFERENCES "AddonMediaList" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_AddonMediaListPublicationJob_ManagedLibraryOperation_ChildScanOperationId"
        FOREIGN KEY ("ChildScanOperationId") REFERENCES "ManagedLibraryOperation" ("Id") ON DELETE SET NULL
);

CREATE UNIQUE INDEX "IX_AddonMediaListPublicationJob_OperationId"
    ON "AddonMediaListPublicationJob" ("OperationId");
CREATE UNIQUE INDEX "IX_AddonMediaListPublicationJob_ListGeneration"
    ON "AddonMediaListPublicationJob" ("AddonMediaListId", "Generation");
CREATE INDEX "IX_AddonMediaListPublicationJob_ListPhase"
    ON "AddonMediaListPublicationJob" ("AddonMediaListId", "Phase", "Id");

-- Adopt journals stranded by the former request-owned publication path. The
-- durable worker resumes them in recovery instead of requiring an operator to
-- edit SQLite or managed files after upgrading.
INSERT INTO "ManagedLibraryOperation" (
    "LibraryId", "OwnerToken", "Kind", "State", "CreatedAt", "UpdatedAt"
)
SELECT "library"."LibraryId",
       'migration-publication-' || CAST("list"."Id" AS TEXT),
       6, 0,
       strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
       strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
FROM "AddonMediaList" AS "list"
JOIN "AddonMediaListLibrary" AS "library"
  ON "library"."AddonMediaListId" = "list"."Id"
WHERE "list"."ReviewState" = 3
  AND "list"."PublicationJournalJson" IS NOT NULL
  AND TRIM("list"."PublicationJournalJson") <> '';

INSERT INTO "AddonMediaListPublicationJob" (
    "OperationId", "AddonMediaListId", "Generation", "SavedRevision",
    "TargetPublishedRevision", "Phase", "OriginalFailureCode",
    "OriginalFailureDiagnostic", "CreatedAt", "UpdatedAt"
)
SELECT "operation"."Id", "list"."Id", 1, "list"."DraftRevision",
       "list"."PublishedRevision" + 1, 4, 'legacy_interrupted_publication',
       'publication was interrupted before durable coordination was available',
       "operation"."CreatedAt", "operation"."UpdatedAt"
FROM "AddonMediaList" AS "list"
JOIN "ManagedLibraryOperation" AS "operation"
  ON "operation"."OwnerToken" =
     'migration-publication-' || CAST("list"."Id" AS TEXT)
WHERE "list"."ReviewState" = 3
  AND "list"."PublicationJournalJson" IS NOT NULL
  AND TRIM("list"."PublicationJournalJson") <> '';
