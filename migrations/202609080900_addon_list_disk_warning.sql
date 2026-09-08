-- ErsatzRS-only advisory Health thresholds (ADR 0085, AMM-M-03).
-- Independent of processed-artifact admission; zero disables the warning.
ALTER TABLE "AddonMediaList"
    ADD COLUMN "DiskUsageWarningGb" INTEGER NOT NULL DEFAULT 25
    CONSTRAINT "CK_AddonMediaList_DiskUsageWarningGb"
        CHECK ("DiskUsageWarningGb" BETWEEN 0 AND 4294967295);
