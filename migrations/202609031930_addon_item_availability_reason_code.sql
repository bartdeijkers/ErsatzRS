-- AMM-J-03 adds a provider-neutral terminal reason alongside the retained
-- bounded legacy availability text. Existing add-ons remain compatible.
ALTER TABLE "AddonMediaListItem" ADD COLUMN "AvailabilityReasonCode" INTEGER NULL;
ALTER TABLE "AddonMediaListItem" ADD COLUMN "PublishedAvailability" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "AddonMediaListItem" ADD COLUMN "PublishedAvailabilityReason" TEXT NULL;
ALTER TABLE "AddonMediaListItem" ADD COLUMN "PublishedAvailabilityReasonCode" INTEGER NULL;

UPDATE "AddonMediaListItem"
SET "PublishedAvailability" = "Availability",
    "PublishedAvailabilityReason" = "AvailabilityReason",
    "PublishedAvailabilityReasonCode" = "AvailabilityReasonCode"
WHERE "Published" = 1;
