-- Existing inclusion choices are operator-owned; new provider items can be
-- excluded when later detail enrichment proves their source unavailable.
ALTER TABLE "AddonMediaListItem"
    ADD COLUMN "IncludedByUser" INTEGER NOT NULL DEFAULT 0;

UPDATE "AddonMediaListItem" SET "IncludedByUser" = 1;
