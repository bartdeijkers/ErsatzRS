-- ADR 0085: bounded, list-owned scheduled-refresh publication assessment.
-- Existing/manual imports gain no automatic-publication authorization.
CREATE TABLE "AddonMediaListAutoPublication" (
    "AddonMediaListId" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListAutoPublication" PRIMARY KEY,
    "ImportJobId" INTEGER NULL,
    "DraftRevision" INTEGER NOT NULL,
    "PublishedRevision" INTEGER NOT NULL,
    "State" TEXT NOT NULL,
    "Reason" TEXT NULL,
    "InitialHoldReason" TEXT NULL,
    "RecordedAt" TEXT NOT NULL,
    "LastAssessedAt" TEXT NULL,
    "OperationId" INTEGER NULL,
    CONSTRAINT "FK_AddonMediaListAutoPublication_AddonMediaList_AddonMediaListId"
        FOREIGN KEY ("AddonMediaListId") REFERENCES "AddonMediaList" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_AddonMediaListAutoPublication_AddonMediaListImportJob_ImportJobId"
        FOREIGN KEY ("ImportJobId") REFERENCES "AddonMediaListImportJob" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_AddonMediaListAutoPublication_ManagedLibraryOperation_OperationId"
        FOREIGN KEY ("OperationId") REFERENCES "ManagedLibraryOperation" ("Id") ON DELETE SET NULL,
    CONSTRAINT "CK_AddonMediaListAutoPublication_State"
        CHECK ("State" IN ('importing', 'ready', 'preparing', 'held', 'queued', 'done'))
);
