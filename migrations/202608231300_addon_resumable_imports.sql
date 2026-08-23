-- AMM-G host-owned overview discovery and deferred detail state.
-- DetailImportPolicy: 0 = automatic, 1 = manual_only.
ALTER TABLE "AddonMediaList" ADD COLUMN "DetailImportPolicy" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "AddonMediaList" ADD COLUMN "DetailImportAuthorizedAt" TEXT NULL;
ALTER TABLE "AddonMediaList" ADD COLUMN "DetailProjectionPendingAt" TEXT NULL;

CREATE TABLE "AddonMediaListImportJob" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListImportJob" PRIMARY KEY AUTOINCREMENT,
    "AddonMediaListId" INTEGER NOT NULL,
    "Generation" INTEGER NOT NULL,
    "State" INTEGER NOT NULL,
    "RecordCapability" TEXT NOT NULL,
    "RequestedFolder" TEXT NULL,
    "Cursor" TEXT NULL,
    "RevisionToken" TEXT NULL,
    "VerificationPass" INTEGER NOT NULL DEFAULT 0,
    "ExpectedIdentityHash" TEXT NULL,
    "ListRecordJson" TEXT NULL,
    "TotalHint" INTEGER NULL,
    "DiscoveredCount" INTEGER NOT NULL DEFAULT 0,
    "OverviewComplete" INTEGER NOT NULL DEFAULT 0,
    "AttemptCount" INTEGER NOT NULL DEFAULT 0,
    "Diagnostic" TEXT NULL,
    "NextEligibleAt" TEXT NULL,
    "CancellationRequested" INTEGER NOT NULL DEFAULT 0,
    "LeaseOwner" TEXT NULL,
    "LeaseExpiresAt" TEXT NULL,
    "HeartbeatAt" TEXT NULL,
    "LastProgressAt" TEXT NULL,
    "CreatedAt" TEXT NOT NULL,
    "UpdatedAt" TEXT NOT NULL,
    "CompletedAt" TEXT NULL,
    CONSTRAINT "FK_AddonMediaListImportJob_AddonMediaList_AddonMediaListId"
        FOREIGN KEY ("AddonMediaListId") REFERENCES "AddonMediaList" ("Id") ON DELETE CASCADE
);

CREATE UNIQUE INDEX "IX_AddonMediaListImportJob_ListGeneration"
    ON "AddonMediaListImportJob" ("AddonMediaListId", "Generation");
CREATE INDEX "IX_AddonMediaListImportJob_StateLease"
    ON "AddonMediaListImportJob" ("State", "NextEligibleAt", "LeaseExpiresAt", "UpdatedAt");

CREATE TABLE "AddonMediaListImportItem" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListImportItem" PRIMARY KEY AUTOINCREMENT,
    "AddonMediaListImportJobId" INTEGER NOT NULL,
    "VerificationPass" INTEGER NOT NULL DEFAULT 0,
    "ProviderId" TEXT NOT NULL,
    "Rank" INTEGER NOT NULL,
    "RecordJson" TEXT NOT NULL,
    "OverviewFingerprint" TEXT NOT NULL,
    CONSTRAINT "FK_AddonMediaListImportItem_AddonMediaListImportJob_JobId"
        FOREIGN KEY ("AddonMediaListImportJobId") REFERENCES "AddonMediaListImportJob" ("Id") ON DELETE CASCADE
);

CREATE UNIQUE INDEX "IX_AddonMediaListImportItem_JobProviderId"
    ON "AddonMediaListImportItem" ("AddonMediaListImportJobId", "VerificationPass", "ProviderId");
CREATE UNIQUE INDEX "IX_AddonMediaListImportItem_JobRank"
    ON "AddonMediaListImportItem" ("AddonMediaListImportJobId", "VerificationPass", "Rank");

CREATE TABLE "AddonMediaListImportCursor" (
    "AddonMediaListImportJobId" INTEGER NOT NULL,
    "VerificationPass" INTEGER NOT NULL,
    "CursorHash" TEXT NOT NULL,
    CONSTRAINT "PK_AddonMediaListImportCursor"
        PRIMARY KEY ("AddonMediaListImportJobId", "VerificationPass", "CursorHash"),
    CONSTRAINT "FK_AddonMediaListImportCursor_AddonMediaListImportJob_JobId"
        FOREIGN KEY ("AddonMediaListImportJobId") REFERENCES "AddonMediaListImportJob" ("Id") ON DELETE CASCADE
);

-- State: 0 = not_requested, 1 = queued, 2 = waiting_for_slot,
-- 3 = running, 4 = waiting_to_retry, 5 = complete, 6 = skipped,
-- 7 = failed. ManualPriority is durable and never bypasses host pacing.
CREATE TABLE "AddonMediaListDetailJob" (
    "AddonMediaListItemId" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListDetailJob" PRIMARY KEY,
    "AddonMediaListId" INTEGER NOT NULL,
    "ProviderHost" TEXT NOT NULL,
    "State" INTEGER NOT NULL DEFAULT 0,
    "ManualPriority" INTEGER NOT NULL DEFAULT 0,
    "OverviewFingerprint" TEXT NOT NULL,
    "OverviewRecordJson" TEXT NOT NULL,
    "AttemptCount" INTEGER NOT NULL DEFAULT 0,
    "Outcome" TEXT NULL,
    "ResultRecordJson" TEXT NULL,
    "Diagnostic" TEXT NULL,
    "NextEligibleAt" TEXT NULL,
    "LeaseOwner" TEXT NULL,
    "LeaseExpiresAt" TEXT NULL,
    "RequestedAt" TEXT NULL,
    "StartedAt" TEXT NULL,
    "CompletedAt" TEXT NULL,
    "UpdatedAt" TEXT NOT NULL,
    CONSTRAINT "FK_AddonMediaListDetailJob_AddonMediaListItem_ItemId"
        FOREIGN KEY ("AddonMediaListItemId") REFERENCES "AddonMediaListItem" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_AddonMediaListDetailJob_AddonMediaList_ListId"
        FOREIGN KEY ("AddonMediaListId") REFERENCES "AddonMediaList" ("Id") ON DELETE CASCADE
);

CREATE INDEX "IX_AddonMediaListDetailJob_Claim"
    ON "AddonMediaListDetailJob" ("State", "ManualPriority", "NextEligibleAt", "UpdatedAt");
CREATE INDEX "IX_AddonMediaListDetailJob_ListState"
    ON "AddonMediaListDetailJob" ("AddonMediaListId", "State");
CREATE INDEX "IX_AddonMediaListDetailJob_HostState"
    ON "AddonMediaListDetailJob" ("ProviderHost", "State");

CREATE TABLE "AddonProviderHostCooldown" (
    "ProviderHost" TEXT NOT NULL CONSTRAINT "PK_AddonProviderHostCooldown" PRIMARY KEY,
    "NextEligibleAt" TEXT NOT NULL,
    "SampledDelaySeconds" INTEGER NOT NULL,
    "LastAttemptAt" TEXT NOT NULL,
    "LeaseOwner" TEXT NULL,
    "LeaseExpiresAt" TEXT NULL,
    "UpdatedAt" TEXT NOT NULL
);

CREATE INDEX "IX_AddonProviderHostCooldown_NextEligibleAt"
    ON "AddonProviderHostCooldown" ("NextEligibleAt");
