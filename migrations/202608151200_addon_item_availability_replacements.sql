ALTER TABLE "AddonMediaListItem" ADD COLUMN "Availability" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "AddonMediaListItem" ADD COLUMN "AvailabilityReason" TEXT NULL;
ALTER TABLE "AddonMediaListItem" ADD COLUMN "AvailabilityUpdated" TEXT NULL;
ALTER TABLE "AddonMediaListItem" ADD COLUMN "ContentKind" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "AddonMediaList" ADD COLUMN "ContentKindOverride" INTEGER NOT NULL DEFAULT 0;

CREATE TABLE "AddonMediaListItemReplacement" (
    "AddonMediaListItemId" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListItemReplacement" PRIMARY KEY,
    "MediaItemId" INTEGER NOT NULL,
    "MediaVersionId" INTEGER NOT NULL,
    "CreatedAt" TEXT NOT NULL,
    "UpdatedAt" TEXT NOT NULL,
    CONSTRAINT "FK_AddonMediaListItemReplacement_AddonMediaListItem_AddonMediaListItemId"
        FOREIGN KEY ("AddonMediaListItemId") REFERENCES "AddonMediaListItem" ("Id") ON DELETE CASCADE
);

CREATE INDEX "IX_AddonMediaListItemReplacement_MediaItemId"
    ON "AddonMediaListItemReplacement" ("MediaItemId");
CREATE INDEX "IX_AddonMediaListItemReplacement_MediaVersionId"
    ON "AddonMediaListItemReplacement" ("MediaVersionId");

ALTER TABLE "RemoteStream" ADD COLUMN "Availability" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "RemoteStream" ADD COLUMN "AvailabilityReason" TEXT NULL;
ALTER TABLE "RemoteStream" ADD COLUMN "ContentKind" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "RemoteStream" ADD COLUMN "ProviderId" TEXT NULL;
CREATE UNIQUE INDEX "IX_RemoteStream_AddonId_ProviderId"
    ON "RemoteStream" ("AddonId", "ProviderId")
    WHERE "AddonId" IS NOT NULL AND "ProviderId" IS NOT NULL;
ALTER TABLE "RemoteStreamMetadata" ADD COLUMN "Outline" TEXT NULL;
ALTER TABLE "RemoteStreamMetadata" ADD COLUMN "Tagline" TEXT NULL;
ALTER TABLE "RemoteStreamMetadata" ADD COLUMN "ShowTitle" TEXT NULL;
ALTER TABLE "RemoteStreamMetadata" ADD COLUMN "SeasonNumber" INTEGER NULL;
ALTER TABLE "RemoteStreamMetadata" ADD COLUMN "EpisodeNumber" INTEGER NULL;
