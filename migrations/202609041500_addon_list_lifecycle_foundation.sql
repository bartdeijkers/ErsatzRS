-- AMM-K provider-neutral list lifecycle, option, revision, artifact, and tool state.
-- These additive tables and columns reserve durable ownership without enabling
-- scheduling, reset, reconciliation, artifact preparation, or tool updates.

-- Immutable schema snapshots deliberately outlive one installed package row so
-- retained revisions keep exact option provenance after Change add-on/uninstall.
-- Repository registration still validates the live add-on namespace.
CREATE TABLE "AddonMediaListOptionSchema" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListOptionSchema" PRIMARY KEY AUTOINCREMENT,
    "AddonId" TEXT NOT NULL,
    "CapabilityId" TEXT NOT NULL,
    "SchemaDigest" TEXT NOT NULL,
    "DefinitionJson" TEXT NOT NULL,
    "CreatedAt" TEXT NOT NULL
);

CREATE UNIQUE INDEX "IX_AddonMediaListOptionSchema_AddonId_CapabilityId"
    ON "AddonMediaListOptionSchema" ("AddonId", "CapabilityId");

ALTER TABLE "AddonMediaList" ADD COLUMN "RefreshCadence" INTEGER NOT NULL DEFAULT 4;
ALTER TABLE "AddonMediaList" ADD COLUMN "RefreshPublicationPolicy" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "AddonMediaList" ADD COLUMN "NextRefreshAt" TEXT NULL;
ALTER TABLE "AddonMediaList" ADD COLUMN "RefreshRetryAt" TEXT NULL;
ALTER TABLE "AddonMediaList" ADD COLUMN "LastRefreshSucceededAt" TEXT NULL;
ALTER TABLE "AddonMediaList" ADD COLUMN "DraftOptionSchemaId" INTEGER NULL
    REFERENCES "AddonMediaListOptionSchema" ("Id") ON DELETE SET NULL;
ALTER TABLE "AddonMediaList" ADD COLUMN "DraftOptionValuesJson" TEXT NOT NULL DEFAULT '{}';
ALTER TABLE "AddonMediaList" ADD COLUMN "ProcessedArtifactByteLimit" INTEGER NULL;

UPDATE "AddonMediaList"
SET "RefreshCadence" = CASE WHEN "AutoRefresh" = 1 THEN 0 ELSE 4 END,
    "LastRefreshSucceededAt" = "LastUpdate";

CREATE INDEX "IX_AddonMediaList_RefreshDue"
    ON "AddonMediaList" ("RefreshCadence", "NextRefreshAt", "RefreshRetryAt");
CREATE INDEX "IX_AddonMediaList_DraftOptionSchemaId"
    ON "AddonMediaList" ("DraftOptionSchemaId");

ALTER TABLE "AddonMediaListItem"
    ADD COLUMN "DraftOptionOverridesJson" TEXT NOT NULL DEFAULT '{}';

CREATE TABLE "AddonMediaListRevision" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListRevision" PRIMARY KEY AUTOINCREMENT,
    "AddonMediaListId" INTEGER NOT NULL,
    "Revision" INTEGER NOT NULL,
    "SourceDraftRevision" INTEGER NULL,
    "OptionSchemaId" INTEGER NULL,
    "OptionValuesJson" TEXT NOT NULL DEFAULT '{}',
    "CompletedAt" TEXT NULL,
    CONSTRAINT "FK_AddonMediaListRevision_AddonMediaList_AddonMediaListId"
        FOREIGN KEY ("AddonMediaListId") REFERENCES "AddonMediaList" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_AddonMediaListRevision_AddonMediaListOptionSchema_OptionSchemaId"
        FOREIGN KEY ("OptionSchemaId") REFERENCES "AddonMediaListOptionSchema" ("Id") ON DELETE SET NULL
);

CREATE UNIQUE INDEX "IX_AddonMediaListRevision_ListRevision"
    ON "AddonMediaListRevision" ("AddonMediaListId", "Revision");
CREATE INDEX "IX_AddonMediaListRevision_OptionSchemaId"
    ON "AddonMediaListRevision" ("OptionSchemaId");

CREATE TABLE "AddonMediaListRevisionItemOption" (
    "AddonMediaListRevisionId" INTEGER NOT NULL,
    "ProviderId" TEXT NOT NULL,
    "OptionOverridesJson" TEXT NOT NULL,
    CONSTRAINT "PK_AddonMediaListRevisionItemOption"
        PRIMARY KEY ("AddonMediaListRevisionId", "ProviderId"),
    CONSTRAINT "FK_AddonMediaListRevisionItemOption_AddonMediaListRevision_RevisionId"
        FOREIGN KEY ("AddonMediaListRevisionId") REFERENCES "AddonMediaListRevision" ("Id") ON DELETE CASCADE
);

-- Only the truthful current revision can be reconstructed from a legacy row.
-- Its originating draft and completion instant are intentionally left unknown.
INSERT INTO "AddonMediaListRevision" (
    "AddonMediaListId", "Revision", "OptionValuesJson"
)
SELECT "Id", "PublishedRevision", '{}'
FROM "AddonMediaList"
WHERE "PublishedRevision" > 0;

CREATE TABLE "AddonMediaListImportCheckpoint" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListImportCheckpoint" PRIMARY KEY AUTOINCREMENT,
    "AddonMediaListId" INTEGER NOT NULL,
    "RelativePath" TEXT NOT NULL,
    "Generation" INTEGER NOT NULL,
    "Sha256" TEXT NOT NULL,
    "EntryCount" INTEGER NOT NULL,
    "PromotedAt" TEXT NOT NULL,
    CONSTRAINT "FK_AddonMediaListImportCheckpoint_AddonMediaList_ListId"
        FOREIGN KEY ("AddonMediaListId") REFERENCES "AddonMediaList" ("Id") ON DELETE CASCADE
);

CREATE UNIQUE INDEX "IX_AddonMediaListImportCheckpoint_AddonMediaListId"
    ON "AddonMediaListImportCheckpoint" ("AddonMediaListId");

ALTER TABLE "AddonMediaListImportJob" ADD COLUMN "Purpose" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "AddonMediaListImportJob" ADD COLUMN "Mode" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "AddonMediaListImportJob" ADD COLUMN "IncrementalStrategy" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "AddonMediaListImportJob" ADD COLUMN "ExaminedCount" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "AddonMediaListImportJob" ADD COLUMN "ArchivedSkippedCount" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "AddonMediaListImportJob" ADD COLUMN "EmittedCount" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "AddonMediaListImportJob" ADD COLUMN "CheckpointId" INTEGER NULL
    REFERENCES "AddonMediaListImportCheckpoint" ("Id") ON DELETE SET NULL;

CREATE INDEX "IX_AddonMediaListImportJob_CheckpointId"
    ON "AddonMediaListImportJob" ("CheckpointId");

CREATE TABLE "AddonMediaListLifecycleJob" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListLifecycleJob" PRIMARY KEY AUTOINCREMENT,
    "OperationId" INTEGER NOT NULL,
    "AddonMediaListId" INTEGER NOT NULL,
    "Generation" INTEGER NOT NULL,
    "Kind" INTEGER NOT NULL,
    "Phase" INTEGER NOT NULL,
    "SavedDraftRevision" INTEGER NOT NULL,
    "SavedPublishedRevision" INTEGER NOT NULL,
    "CandidateImportJobId" INTEGER NULL,
    "CreatedAt" TEXT NOT NULL,
    "UpdatedAt" TEXT NOT NULL,
    "CompletedAt" TEXT NULL,
    CONSTRAINT "FK_AddonMediaListLifecycleJob_ManagedLibraryOperation_OperationId"
        FOREIGN KEY ("OperationId") REFERENCES "ManagedLibraryOperation" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_AddonMediaListLifecycleJob_AddonMediaList_ListId"
        FOREIGN KEY ("AddonMediaListId") REFERENCES "AddonMediaList" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_AddonMediaListLifecycleJob_AddonMediaListImportJob_CandidateImportJobId"
        FOREIGN KEY ("CandidateImportJobId") REFERENCES "AddonMediaListImportJob" ("Id") ON DELETE SET NULL
);

CREATE UNIQUE INDEX "IX_AddonMediaListLifecycleJob_OperationId"
    ON "AddonMediaListLifecycleJob" ("OperationId");
CREATE UNIQUE INDEX "IX_AddonMediaListLifecycleJob_ListGeneration"
    ON "AddonMediaListLifecycleJob" ("AddonMediaListId", "Generation");
CREATE INDEX "IX_AddonMediaListLifecycleJob_ListPhase"
    ON "AddonMediaListLifecycleJob" ("AddonMediaListId", "Phase", "Id");

CREATE TABLE "AddonMediaListItemTombstone" (
    "AddonMediaListItemId" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListItemTombstone" PRIMARY KEY,
    "MissingSince" TEXT NOT NULL,
    "LastObservedAt" TEXT NOT NULL,
    "ObservationCount" INTEGER NOT NULL,
    "LastReconcileGeneration" INTEGER NOT NULL,
    "ConfirmedAt" TEXT NULL,
    CONSTRAINT "FK_AddonMediaListItemTombstone_AddonMediaListItem_ItemId"
        FOREIGN KEY ("AddonMediaListItemId") REFERENCES "AddonMediaListItem" ("Id") ON DELETE CASCADE,
    CONSTRAINT "CK_AddonMediaListItemTombstone_ObservationCount"
        CHECK ("ObservationCount" > 0)
);

CREATE INDEX "IX_AddonMediaListItemTombstone_MissingSince"
    ON "AddonMediaListItemTombstone" ("MissingSince");

CREATE TABLE "AddonMediaListArtifact" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListArtifact" PRIMARY KEY AUTOINCREMENT,
    "AddonMediaListId" INTEGER NOT NULL,
    "AddonMediaListItemId" INTEGER NULL,
    "CacheKey" TEXT NOT NULL,
    "Kind" INTEGER NOT NULL,
    "State" INTEGER NOT NULL,
    "RelativePath" TEXT NULL,
    "ReservedBytes" INTEGER NOT NULL DEFAULT 0,
    "ByteLength" INTEGER NULL,
    "Sha256" TEXT NULL,
    "DiagnosticCode" TEXT NULL,
    "CreatedAt" TEXT NOT NULL,
    "UpdatedAt" TEXT NOT NULL,
    "LastUsedAt" TEXT NULL,
    "CompletedAt" TEXT NULL,
    CONSTRAINT "FK_AddonMediaListArtifact_AddonMediaList_ListId"
        FOREIGN KEY ("AddonMediaListId") REFERENCES "AddonMediaList" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_AddonMediaListArtifact_AddonMediaListItem_ItemId"
        FOREIGN KEY ("AddonMediaListItemId") REFERENCES "AddonMediaListItem" ("Id") ON DELETE SET NULL,
    CONSTRAINT "CK_AddonMediaListArtifact_ReservedBytes"
        CHECK ("ReservedBytes" >= 0),
    CONSTRAINT "CK_AddonMediaListArtifact_ByteLength"
        CHECK ("ByteLength" IS NULL OR "ByteLength" >= 0)
);

CREATE UNIQUE INDEX "IX_AddonMediaListArtifact_ListCacheKey"
    ON "AddonMediaListArtifact" ("AddonMediaListId", "CacheKey");
CREATE INDEX "IX_AddonMediaListArtifact_ItemId"
    ON "AddonMediaListArtifact" ("AddonMediaListItemId");
CREATE INDEX "IX_AddonMediaListArtifact_StateLastUsed"
    ON "AddonMediaListArtifact" ("State", "LastUsedAt", "Id");

-- Retained revisions pin their artifacts; RESTRICT prevents silent loss until
-- the host explicitly unlinks the immutable revision reference.
CREATE TABLE "AddonMediaListRevisionArtifact" (
    "AddonMediaListRevisionId" INTEGER NOT NULL,
    "AddonMediaListArtifactId" INTEGER NOT NULL,
    CONSTRAINT "PK_AddonMediaListRevisionArtifact"
        PRIMARY KEY ("AddonMediaListRevisionId", "AddonMediaListArtifactId"),
    CONSTRAINT "FK_AddonMediaListRevisionArtifact_AddonMediaListRevision_RevisionId"
        FOREIGN KEY ("AddonMediaListRevisionId") REFERENCES "AddonMediaListRevision" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_AddonMediaListRevisionArtifact_AddonMediaListArtifact_ArtifactId"
        FOREIGN KEY ("AddonMediaListArtifactId") REFERENCES "AddonMediaListArtifact" ("Id") ON DELETE RESTRICT
);

CREATE INDEX "IX_AddonMediaListRevisionArtifact_ArtifactId"
    ON "AddonMediaListRevisionArtifact" ("AddonMediaListArtifactId");

CREATE TABLE "AddonToolMaintenanceJob" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_AddonToolMaintenanceJob" PRIMARY KEY AUTOINCREMENT,
    "AddonId" TEXT NOT NULL,
    "ToolKey" TEXT NOT NULL,
    "Trigger" INTEGER NOT NULL,
    "State" INTEGER NOT NULL,
    "AttemptCount" INTEGER NOT NULL DEFAULT 0,
    "NextEligibleAt" TEXT NULL,
    "LeaseOwner" TEXT NULL,
    "LeaseExpiresAt" TEXT NULL,
    "HeartbeatAt" TEXT NULL,
    "BeforeVersion" TEXT NULL,
    "BeforeSha256" TEXT NULL,
    "AfterVersion" TEXT NULL,
    "AfterSha256" TEXT NULL,
    "Validation" INTEGER NOT NULL DEFAULT 0,
    "Rollback" INTEGER NOT NULL DEFAULT 0,
    "Outcome" INTEGER NULL,
    "DiagnosticCode" TEXT NULL,
    "Diagnostic" TEXT NULL,
    "CreatedAt" TEXT NOT NULL,
    "UpdatedAt" TEXT NOT NULL,
    "CompletedAt" TEXT NULL,
    CONSTRAINT "FK_AddonToolMaintenanceJob_AddonInstallation_AddonId"
        FOREIGN KEY ("AddonId") REFERENCES "AddonInstallation" ("AddonId") ON DELETE CASCADE
);

CREATE UNIQUE INDEX "IX_AddonToolMaintenanceJob_ActiveTool"
    ON "AddonToolMaintenanceJob" ("AddonId", "ToolKey")
    WHERE "State" = 1;
CREATE INDEX "IX_AddonToolMaintenanceJob_Claim"
    ON "AddonToolMaintenanceJob" ("State", "NextEligibleAt", "LeaseExpiresAt", "CreatedAt", "Id");
CREATE INDEX "IX_AddonToolMaintenanceJob_History"
    ON "AddonToolMaintenanceJob" ("AddonId", "ToolKey", "CreatedAt" DESC, "Id" DESC);
