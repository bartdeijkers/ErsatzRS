-- Provider-neutral Remote Stream playback behavior. Values are
-- 0 = unknown, 1 = finite, and 2 = live.
ALTER TABLE "AddonMediaListItem" ADD COLUMN "ProviderLiveness" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "AddonMediaListItem" ADD COLUMN "LivenessOverride" INTEGER NULL;
ALTER TABLE "AddonMediaListItem" ADD COLUMN "EffectiveLiveness" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "AddonMediaListItem" ADD COLUMN "LivenessConflict" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "AddonMediaListItem" ADD COLUMN "PublishedLiveness" INTEGER NOT NULL DEFAULT 0;

-- A positive provider duration is trustworthy evidence of finite media. The
-- old publication path did not retain provider IsLive, so durationless rows
-- remain unknown instead of inheriting the old default-false behavior.
UPDATE "AddonMediaListItem"
SET "ProviderLiveness" = CASE
        WHEN COALESCE("DurationSeconds", 0) > 0 THEN 1
        ELSE 0
    END,
    "EffectiveLiveness" = CASE
        WHEN COALESCE("DurationSeconds", 0) > 0 THEN 1
        ELSE 0
    END,
    "PublishedLiveness" = CASE
        WHEN "Published" = 1 AND COALESCE("DurationSeconds", 0) > 0 THEN 1
        ELSE 0
    END;

ALTER TABLE "RemoteStream" ADD COLUMN "Liveness" INTEGER NOT NULL DEFAULT 0;

-- Persisted true is authoritative. A non-empty, non-zero configured duration
-- is the only finite fallback still recoverable from legacy rows.
UPDATE "RemoteStream"
SET "Liveness" = CASE
    WHEN "IsLive" = 1 THEN 2
    WHEN COALESCE(TRIM("Duration"), '') <> ''
         AND TRIM("Duration") NOT IN ('0', '00:00:00', '00:00:00.0', '00:00:00.000') THEN 1
    ELSE 0
END;

CREATE INDEX "IX_AddonMediaListItem_ListLiveness"
    ON "AddonMediaListItem" ("AddonMediaListId", "EffectiveLiveness", "Rank");
