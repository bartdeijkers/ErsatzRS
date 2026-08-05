-- ErsatzRS FFmpeg 9 profile contract (ADR 0063).
ALTER TABLE "FFmpegProfile" ADD COLUMN "VideoTune" TEXT NULL;

UPDATE "FFmpegProfile"
SET "VideoTune" = CASE
        WHEN LOWER(TRIM("VideoPreset")) IN ('llhp', 'llhq') THEN 'll'
        ELSE "VideoTune"
    END,
    "VideoPreset" = CASE LOWER(TRIM("VideoPreset"))
        WHEN 'llhp' THEN 'p1'
        WHEN 'llhq' THEN 'p7'
        ELSE "VideoPreset"
    END
WHERE LOWER(TRIM("VideoPreset")) IN ('llhp', 'llhq');
