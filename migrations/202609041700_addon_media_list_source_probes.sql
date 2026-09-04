-- AMM-K-04: durable, non-mutating first-page source probes.
-- Source URLs and provider records are deliberately not retained; the saved
-- fingerprint binds the job to its canonical inputs while the result stores
-- only the bounded list header and aggregate page facts needed by the UI.

CREATE TABLE "AddonMediaListSourceProbeJob" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListSourceProbeJob" PRIMARY KEY AUTOINCREMENT,
    "AddonMediaListId" INTEGER NOT NULL,
    "InputFingerprint" TEXT NOT NULL,
    "SavedDraftRevision" INTEGER NOT NULL,
    "RecordCapability" TEXT NULL,
    "CapabilitiesJson" TEXT NOT NULL,
    "State" INTEGER NOT NULL,
    "AttemptCount" INTEGER NOT NULL DEFAULT 0,
    "NextEligibleAt" TEXT NULL,
    "LeaseOwner" TEXT NULL,
    "LeaseExpiresAt" TEXT NULL,
    "HeartbeatAt" TEXT NULL,
    "AccessOutcome" INTEGER NULL,
    "ProviderId" TEXT NULL,
    "ListTitle" TEXT NULL,
    "CountHint" INTEGER NULL,
    "ReturnedItemCount" INTEGER NULL,
    "PageComplete" INTEGER NULL,
    "RevisionToken" TEXT NULL,
    "DurationMilliseconds" INTEGER NULL,
    "DiagnosticCode" TEXT NULL,
    "Diagnostic" TEXT NULL,
    "CreatedAt" TEXT NOT NULL,
    "UpdatedAt" TEXT NOT NULL,
    "StartedAt" TEXT NULL,
    "CompletedAt" TEXT NULL,
    CONSTRAINT "FK_AddonMediaListSourceProbeJob_AddonMediaList_ListId"
        FOREIGN KEY ("AddonMediaListId") REFERENCES "AddonMediaList" ("Id") ON DELETE CASCADE,
    CONSTRAINT "CK_AddonMediaListSourceProbeJob_AttemptCount"
        CHECK ("AttemptCount" >= 0),
    CONSTRAINT "CK_AddonMediaListSourceProbeJob_CountHint"
        CHECK ("CountHint" IS NULL OR "CountHint" >= 0),
    CONSTRAINT "CK_AddonMediaListSourceProbeJob_ReturnedItemCount"
        CHECK ("ReturnedItemCount" IS NULL OR "ReturnedItemCount" >= 0),
    CONSTRAINT "CK_AddonMediaListSourceProbeJob_DurationMilliseconds"
        CHECK ("DurationMilliseconds" IS NULL OR "DurationMilliseconds" >= 0)
);

CREATE UNIQUE INDEX "IX_AddonMediaListSourceProbeJob_ListActive"
    ON "AddonMediaListSourceProbeJob" ("AddonMediaListId")
    WHERE "State" IN (0, 1);

CREATE INDEX "IX_AddonMediaListSourceProbeJob_Claim"
    ON "AddonMediaListSourceProbeJob" ("State", "NextEligibleAt", "LeaseExpiresAt", "Id");

CREATE INDEX "IX_AddonMediaListSourceProbeJob_ListLatest"
    ON "AddonMediaListSourceProbeJob" ("AddonMediaListId", "Id" DESC);
