-- Automatic subtitle checks reuse an identical earlier outcome across saved
-- revisions until a manual list refresh requests a recheck at this instant.
ALTER TABLE "AddonMediaList" ADD COLUMN "SubtitleRecheckRequestedAt" TEXT NULL;
