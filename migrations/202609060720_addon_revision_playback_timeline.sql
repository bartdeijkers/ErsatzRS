-- ADR 0087: immutable interval streaming plans are metadata, not processed files.
-- NULL preserves pre-streaming snapshots and their existing logical digests.
ALTER TABLE "AddonMediaListRevisionItem" ADD COLUMN "PlaybackTimelineJson" TEXT NULL
    CONSTRAINT "CK_AddonMediaListRevisionItem_PlaybackTimelineJson"
    CHECK ("PlaybackTimelineJson" IS NULL
        OR length(CAST("PlaybackTimelineJson" AS BLOB)) <= 65536);
