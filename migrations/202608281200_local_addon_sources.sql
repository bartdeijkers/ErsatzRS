-- ADR 0081 replaces remote add-on catalog acquisition with operator-managed
-- local checkouts and offline bundles. Existing catalog rows remain only when
-- an installed add-on still references them, preserving historical provenance.

CREATE TABLE "AddonSource" (
    "Id" TEXT NOT NULL CONSTRAINT "PK_AddonSource" PRIMARY KEY,
    "Name" TEXT NOT NULL COLLATE NOCASE,
    "Kind" TEXT NOT NULL,
    "RootPath" TEXT NOT NULL,
    "ManifestJson" TEXT NOT NULL,
    "ContentSha256" TEXT NOT NULL,
    "LastScan" TEXT NOT NULL,
    "LastError" TEXT NULL,
    "StateRevision" INTEGER NOT NULL DEFAULT 0
);

CREATE UNIQUE INDEX "IX_AddonSource_RootPath"
    ON "AddonSource" ("RootPath");

CREATE TABLE "AddonSourceOffer" (
    "SourceId" TEXT NOT NULL,
    "AddonId" TEXT NOT NULL,
    "Version" TEXT NOT NULL,
    "ManifestJson" TEXT NOT NULL,
    "RelativePath" TEXT NOT NULL,
    "ContentSha256" TEXT NOT NULL,
    CONSTRAINT "PK_AddonSourceOffer" PRIMARY KEY ("SourceId", "AddonId"),
    CONSTRAINT "FK_AddonSourceOffer_AddonSource_SourceId"
        FOREIGN KEY ("SourceId") REFERENCES "AddonSource" ("Id") ON DELETE CASCADE
);

CREATE INDEX "IX_AddonSourceOffer_AddonId"
    ON "AddonSourceOffer" ("AddonId");

ALTER TABLE "AddonInstallation" ADD COLUMN "SourceId" TEXT NULL
    CONSTRAINT "FK_AddonInstallation_AddonSource_SourceId"
    REFERENCES "AddonSource" ("Id") ON DELETE SET NULL;

CREATE INDEX "IX_AddonInstallation_SourceId"
    ON "AddonInstallation" ("SourceId");

DELETE FROM "AddonRepository"
WHERE NOT EXISTS (
    SELECT 1 FROM "AddonInstallation"
    WHERE "AddonInstallation"."RepositoryId" = "AddonRepository"."Id"
);

UPDATE "AddonRepository"
SET "IsEnabled" = 0,
    "CatalogJson" = NULL,
    "CatalogEtag" = NULL,
    "CatalogLastModified" = NULL,
    "LastError" = NULL;
