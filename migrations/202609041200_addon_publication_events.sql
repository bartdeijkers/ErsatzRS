CREATE TABLE "AddonMediaListPublicationEvent" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListPublicationEvent" PRIMARY KEY AUTOINCREMENT,
    "PublicationJobId" INTEGER NOT NULL,
    "Sequence" INTEGER NOT NULL,
    "DedupeKey" TEXT NOT NULL,
    "Kind" INTEGER NOT NULL,
    "EnqueueTrigger" INTEGER NULL,
    "Phase" INTEGER NULL,
    "Attempt" INTEGER NULL,
    "RelatedOperationId" INTEGER NULL,
    "FilesystemTransition" INTEGER NULL,
    "RetryClass" INTEGER NULL,
    "RecoveryAction" INTEGER NULL,
    "ErrorCategory" INTEGER NULL,
    "OsErrorCode" INTEGER NULL,
    "PublishedItemCount" INTEGER NULL,
    "PlaylistItemCount" INTEGER NULL,
    "ArtworkCount" INTEGER NULL,
    "Outcome" INTEGER NULL,
    "OccurredAt" TEXT NOT NULL,
    CONSTRAINT "FK_AddonMediaListPublicationEvent_AddonMediaListPublicationJob_PublicationJobId"
        FOREIGN KEY ("PublicationJobId") REFERENCES "AddonMediaListPublicationJob" ("Id") ON DELETE CASCADE,
    CONSTRAINT "CK_AddonMediaListPublicationEvent_Sequence"
        CHECK ("Sequence" >= 1 AND "Sequence" <= 256),
    CONSTRAINT "UQ_AddonMediaListPublicationEvent_JobSequence"
        UNIQUE ("PublicationJobId", "Sequence"),
    CONSTRAINT "UQ_AddonMediaListPublicationEvent_JobDedupeKey"
        UNIQUE ("PublicationJobId", "DedupeKey")
);

CREATE INDEX "IX_AddonMediaListPublicationEvent_JobSequence"
    ON "AddonMediaListPublicationEvent" ("PublicationJobId", "Sequence");
CREATE INDEX "IX_AddonMediaListPublicationEvent_OccurredAt"
    ON "AddonMediaListPublicationEvent" ("OccurredAt");

-- Earlier publication jobs cannot be assigned a history that was never
-- captured. Record one truthful adoption boundary so upgraded installations
-- do not present a synthesized event stream as original evidence.
INSERT INTO "AddonMediaListPublicationEvent" (
    "PublicationJobId", "Sequence", "DedupeKey", "Kind", "Phase",
    "Attempt", "Outcome", "OccurredAt"
)
SELECT "job"."Id", 1, 'existing_job_adopted', 0, "job"."Phase",
       "operation"."AttemptCount", "job"."Outcome", "job"."UpdatedAt"
FROM "AddonMediaListPublicationJob" AS "job"
JOIN "ManagedLibraryOperation" AS "operation"
  ON "operation"."Id" = "job"."OperationId";
