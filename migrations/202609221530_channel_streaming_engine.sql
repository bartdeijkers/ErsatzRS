-- Preserve existing channels on the native (upstream Legacy = 0) engine.
-- The explicit Next selection is 1; runtime integration is tracked by ADR 0095.
ALTER TABLE "Channel" ADD COLUMN "StreamingEngine" INTEGER NOT NULL DEFAULT 0;
