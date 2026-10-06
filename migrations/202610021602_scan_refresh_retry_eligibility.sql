-- Transient source-publication races retain intent with bounded durable retry.
ALTER TABLE "ScanPlayoutRefreshIntent" ADD COLUMN "Attempts" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "ScanPlayoutRefreshIntent" ADD COLUMN "NextAttempt" TEXT NULL;
