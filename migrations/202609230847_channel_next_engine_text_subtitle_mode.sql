-- Upstream 1b5d6af7 (#2869): preserve existing subtitle burning by default.
-- NextEngineTextSubtitleMode matches Burn=0 and Convert=1.
ALTER TABLE "Channel" ADD COLUMN "NextEngineTextSubtitleMode" INTEGER NOT NULL DEFAULT 0;
