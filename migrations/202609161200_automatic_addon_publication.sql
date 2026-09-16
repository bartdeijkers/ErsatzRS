-- ADR 0094: upgrade saved drafts without changing inclusion or stored media.
ALTER TABLE "AddonMediaList" ADD COLUMN "DerivedMetadataLimited" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "AddonMediaListAutoPublication" ADD COLUMN "FirstPendingAt" TEXT NULL;
ALTER TABLE "AddonMediaListAutoPublication" ADD COLUMN "NextEligibleAt" TEXT NULL;
ALTER TABLE "AddonMediaListAutoPublication" ADD COLUMN "AttemptCount" INTEGER NOT NULL DEFAULT 0;
UPDATE "AddonMediaList" SET "RefreshPublicationPolicy" = 2,
    "DetailImportPolicy" = 0,
    "DetailImportAuthorizedAt" = COALESCE("DetailImportAuthorizedAt", STRFTIME('%Y-%m-%d %H:%M:%f', 'now')),
    "DraftRevision" = "DraftRevision" + CASE WHEN "ReviewState"=3
        OR ("PublicationJournalJson" IS NOT NULL AND TRIM("PublicationJournalJson")<>'')
        THEN 0 ELSE 1 END;
UPDATE "AddonMediaListDetailJob" SET "State" = 1,
    "RequestedAt" = COALESCE("RequestedAt", STRFTIME('%Y-%m-%d %H:%M:%f', 'now'))
    WHERE "State" = 0;
-- Active publication journals keep their owner; admission resumes after recovery.
