-- ADR 0085 provider-neutral fixed/adaptive refresh scheduling state.
-- State rows are created lazily by the coordinator; this migration does not
-- fabricate attempts, validators, outcomes, or source-change observations.

-- The AMM-K foundation preserved the last successful legacy refresh but left
-- its first due instant uninitialized. Anchor migrated daily rows to that
-- success, or admit them once on upgrade when no success exists.
UPDATE "AddonMediaList"
SET "NextRefreshAt" = CASE
        WHEN "LastRefreshSucceededAt" IS NULL THEN CURRENT_TIMESTAMP
        ELSE datetime("LastRefreshSucceededAt", '+1 day')
    END
WHERE "RefreshCadence" = 0 AND "NextRefreshAt" IS NULL;

CREATE TABLE "AddonMediaListRefreshState" (
    "AddonMediaListId" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListRefreshState" PRIMARY KEY,
    "Generation" INTEGER NOT NULL DEFAULT 0,
    "State" INTEGER NOT NULL DEFAULT 0,
    "Validator" TEXT NULL,
    "MissCount" INTEGER NOT NULL DEFAULT 0,
    "AttemptCount" INTEGER NOT NULL DEFAULT 0,
    "LeaseOwner" TEXT NULL,
    "LeaseExpiresAt" TEXT NULL,
    "NextEligibleAt" TEXT NULL,
    "LastOutcome" INTEGER NULL,
    "DiagnosticCode" TEXT NULL,
    "UpdatedAt" TEXT NOT NULL,
    CONSTRAINT "FK_AddonMediaListRefreshState_AddonMediaList_AddonMediaListId"
        FOREIGN KEY ("AddonMediaListId") REFERENCES "AddonMediaList" ("Id") ON DELETE CASCADE,
    CONSTRAINT "CK_AddonMediaListRefreshState_Generation"
        CHECK ("Generation" >= 0),
    CONSTRAINT "CK_AddonMediaListRefreshState_State"
        CHECK ("State" BETWEEN 0 AND 4),
    CONSTRAINT "CK_AddonMediaListRefreshState_MissCount"
        CHECK ("MissCount" >= 0),
    CONSTRAINT "CK_AddonMediaListRefreshState_AttemptCount"
        CHECK ("AttemptCount" >= 0),
    CONSTRAINT "CK_AddonMediaListRefreshState_LastOutcome"
        CHECK ("LastOutcome" IS NULL OR "LastOutcome" BETWEEN 0 AND 3)
);

CREATE INDEX "IX_AddonMediaListRefreshState_Claim"
    ON "AddonMediaListRefreshState" (
        "State", "NextEligibleAt", "LeaseExpiresAt", "AddonMediaListId"
    );

CREATE TABLE "AddonMediaListSourceChangeObservation" (
    "AddonMediaListId" INTEGER NOT NULL,
    "SourceUpdatedAt" TEXT NOT NULL,
    CONSTRAINT "PK_AddonMediaListSourceChangeObservation"
        PRIMARY KEY ("AddonMediaListId", "SourceUpdatedAt"),
    CONSTRAINT "FK_AddonMediaListSourceChangeObservation_AddonMediaList_AddonMediaListId"
        FOREIGN KEY ("AddonMediaListId") REFERENCES "AddonMediaList" ("Id") ON DELETE CASCADE
);
