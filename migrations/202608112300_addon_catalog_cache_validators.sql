-- ADR 0066: persist HTTP validators only for the signature-verified catalog
-- bytes. A 304 response may reuse CatalogJson without re-downloading its
-- detached signature because repository trust changes clear all three fields.

ALTER TABLE "AddonRepository" ADD COLUMN "CatalogEtag" TEXT NULL;
ALTER TABLE "AddonRepository" ADD COLUMN "CatalogLastModified" TEXT NULL;
ALTER TABLE "AddonRepository" ADD COLUMN "StateRevision" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "AddonRepository" ADD COLUMN "CatalogRevision" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "AddonInstallation" ADD COLUMN "StateRevision" TEXT NOT NULL DEFAULT 'initial';
ALTER TABLE "AddonInstallation" ADD COLUMN "DataNamespace" TEXT NOT NULL DEFAULT 'legacy';

CREATE TABLE "AddonLifecycleLock" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_AddonLifecycleLock" PRIMARY KEY,
    "Revision" INTEGER NOT NULL DEFAULT 0,
    CONSTRAINT "CK_AddonLifecycleLock_Id" CHECK ("Id" = 1)
);

INSERT INTO "AddonLifecycleLock" ("Id", "Revision") VALUES (1, 0);
