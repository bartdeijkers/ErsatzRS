-- ADR 0066: repository-backed add-ons are an ErsatzRS extension with no
-- ErsatzTV schema counterpart. The legacy RemoteStream.Script column remains
-- inert so this migration stays additive.

CREATE TABLE "AddonRepository" (
    "Id" TEXT NOT NULL CONSTRAINT "PK_AddonRepository" PRIMARY KEY,
    "Name" TEXT NOT NULL COLLATE NOCASE,
    "IndexUrl" TEXT NOT NULL,
    "PublicKey" TEXT NOT NULL,
    "IsOfficial" INTEGER NOT NULL DEFAULT 0,
    "IsEnabled" INTEGER NOT NULL DEFAULT 1,
    "LastSequence" INTEGER NULL,
    "LastRefresh" TEXT NULL,
    "LastError" TEXT NULL,
    "CatalogJson" TEXT NULL
);

CREATE UNIQUE INDEX "IX_AddonRepository_IndexUrl"
    ON "AddonRepository" ("IndexUrl");

CREATE TABLE "AddonInstallation" (
    "AddonId" TEXT NOT NULL CONSTRAINT "PK_AddonInstallation" PRIMARY KEY,
    "RepositoryId" TEXT NULL,
    "Version" TEXT NOT NULL,
    "Enabled" INTEGER NOT NULL DEFAULT 0,
    "PackageSha256" TEXT NOT NULL,
    "ContentSha256" TEXT NOT NULL,
    "ManifestJson" TEXT NOT NULL,
    "InstallPath" TEXT NOT NULL,
    "InstalledAt" TEXT NOT NULL,
    "UpdatedAt" TEXT NOT NULL,
    "LastCheckCode" TEXT NULL,
    "LastCheckMessage" TEXT NULL,
    CONSTRAINT "FK_AddonInstallation_AddonRepository_RepositoryId"
        FOREIGN KEY ("RepositoryId") REFERENCES "AddonRepository" ("Id") ON DELETE SET NULL
);

CREATE INDEX "IX_AddonInstallation_RepositoryId"
    ON "AddonInstallation" ("RepositoryId");

CREATE TABLE "AddonSetting" (
    "AddonId" TEXT NOT NULL,
    "Key" TEXT NOT NULL,
    "Value" TEXT NULL,
    "ReferenceKind" TEXT NULL,
    "Reference" TEXT NULL,
    CONSTRAINT "PK_AddonSetting" PRIMARY KEY ("AddonId", "Key"),
    CONSTRAINT "FK_AddonSetting_AddonInstallation_AddonId"
        FOREIGN KEY ("AddonId") REFERENCES "AddonInstallation" ("AddonId") ON DELETE CASCADE
);

ALTER TABLE "RemoteStream" ADD COLUMN "AddonId" TEXT NULL
    CONSTRAINT "FK_RemoteStream_AddonInstallation_AddonId"
    REFERENCES "AddonInstallation" ("AddonId") ON DELETE RESTRICT;

CREATE INDEX "IX_RemoteStream_AddonId" ON "RemoteStream" ("AddonId");

INSERT INTO "AddonRepository" (
    "Id", "Name", "IndexUrl", "PublicKey", "IsOfficial", "IsEnabled"
) VALUES (
    'org.ersatzrs.addons.official',
    'Official ErsatzRS add-ons',
    'https://bartdeijkers.github.io/ErsatzRS-addons/index-v1.json',
    '',
    1,
    1
);
