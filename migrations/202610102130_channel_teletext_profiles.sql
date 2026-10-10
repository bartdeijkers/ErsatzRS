-- ADR 0122: ErsatzRS-only named teletext configuration, not publication state.
-- No installation FK: uninstall retains visibly unavailable configuration.
-- Application deletion is a tombstone. Binding->profile RESTRICT deliberately
-- departs from upstream cascade conventions so accidental physical deletion
-- cannot silently discard retained channel selections or diagnostics.
CREATE TABLE "TeletextProfile" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_TeletextProfile" PRIMARY KEY AUTOINCREMENT,
    "AddonId" TEXT NOT NULL,
    "Name" TEXT NOT NULL COLLATE NOCASE,
    "Revision" INTEGER NOT NULL DEFAULT 1,
    "IsDeleted" INTEGER NOT NULL DEFAULT 0,
    "InstallationStateRevision" TEXT NOT NULL,
    "InstallationVersion" TEXT NOT NULL,
    "InstallationPackageSha256" TEXT NOT NULL,
    "InstallationContentSha256" TEXT NOT NULL,
    "InstallationManifestSha256" TEXT NOT NULL,
    CONSTRAINT "CK_TeletextProfile_Name" CHECK (
        TYPEOF("Name") = 'text' AND LENGTH(CAST("Name" AS BLOB)) BETWEEN 1 AND 256),
    CONSTRAINT "CK_TeletextProfile_Revision" CHECK (TYPEOF("Revision") = 'integer' AND "Revision" > 0),
    CONSTRAINT "CK_TeletextProfile_IsDeleted" CHECK (TYPEOF("IsDeleted") = 'integer' AND "IsDeleted" IN (0, 1))
);
CREATE UNIQUE INDEX "IX_TeletextProfile_Id_AddonId" ON "TeletextProfile" ("Id", "AddonId");
CREATE UNIQUE INDEX "IX_TeletextProfile_AddonId_Name" ON "TeletextProfile" ("AddonId", "Name") WHERE "IsDeleted" = 0;

CREATE TABLE "TeletextProfileSetting" (
    "ProfileId" INTEGER NOT NULL,
    "Key" TEXT NOT NULL,
    "Kind" INTEGER NOT NULL,
    "Value" TEXT NULL,
    "Reference" TEXT NULL,
    CONSTRAINT "PK_TeletextProfileSetting" PRIMARY KEY ("ProfileId", "Key"),
    CONSTRAINT "FK_TeletextProfileSetting_TeletextProfile_ProfileId"
        FOREIGN KEY ("ProfileId") REFERENCES "TeletextProfile" ("Id") ON DELETE CASCADE,
    CONSTRAINT "CK_TeletextProfileSetting_Key" CHECK (
        TYPEOF("Key") = 'text' AND LENGTH(CAST("Key" AS BLOB)) BETWEEN 1 AND 128),
    CONSTRAINT "CK_TeletextProfileSetting_Kind" CHECK (TYPEOF("Kind") = 'integer' AND "Kind" IN (0, 1, 2)),
    CONSTRAINT "CK_TeletextProfileSetting_Value" CHECK (
        ("Kind" = 0 AND "Value" IS NOT NULL AND TYPEOF("Value") = 'text'
            AND LENGTH(CAST("Value" AS BLOB)) <= 16384 AND INSTR("Value", CHAR(0)) = 0 AND "Reference" IS NULL)
        OR ("Kind" IN (1, 2) AND "Value" IS NULL AND "Reference" IS NOT NULL
            AND TYPEOF("Reference") = 'text' AND LENGTH(CAST("Reference" AS BLOB)) BETWEEN 1 AND 16384
            AND INSTR("Reference", CHAR(0)) = 0))
);

CREATE TABLE "ChannelTeletextBinding" (
    "ChannelId" INTEGER NOT NULL,
    "AddonId" TEXT NOT NULL,
    "ProfileId" INTEGER NOT NULL,
    "Enabled" INTEGER NOT NULL DEFAULT 0,
    "Revision" INTEGER NOT NULL DEFAULT 1,
    CONSTRAINT "PK_ChannelTeletextBinding" PRIMARY KEY ("ChannelId", "AddonId"),
    CONSTRAINT "FK_ChannelTeletextBinding_Channel_ChannelId"
        FOREIGN KEY ("ChannelId") REFERENCES "Channel" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_ChannelTeletextBinding_TeletextProfile_ProfileId_AddonId"
        FOREIGN KEY ("ProfileId", "AddonId") REFERENCES "TeletextProfile" ("Id", "AddonId") ON DELETE RESTRICT,
    CONSTRAINT "CK_ChannelTeletextBinding_Enabled" CHECK (TYPEOF("Enabled") = 'integer' AND "Enabled" IN (0, 1)),
    CONSTRAINT "CK_ChannelTeletextBinding_Revision" CHECK (TYPEOF("Revision") = 'integer' AND "Revision" > 0)
);
CREATE INDEX "IX_ChannelTeletextBinding_ProfileId_AddonId" ON "ChannelTeletextBinding" ("ProfileId", "AddonId");

-- Aggregate host budgets also constrain direct row writes, before readers
-- allocate the settings bag. These are resource limits, not theme semantics.
CREATE TRIGGER "TR_TeletextProfileSetting_InsertBudget"
BEFORE INSERT ON "TeletextProfileSetting"
WHEN (SELECT COUNT(*) FROM "TeletextProfileSetting" WHERE "ProfileId" = NEW."ProfileId") >= 256
    OR (SELECT COALESCE(SUM(LENGTH(CAST("Key" AS BLOB))
            + COALESCE(LENGTH(CAST("Value" AS BLOB)), 0)
            + COALESCE(LENGTH(CAST("Reference" AS BLOB)), 0)), 0)
        FROM "TeletextProfileSetting" WHERE "ProfileId" = NEW."ProfileId")
        + LENGTH(CAST(NEW."Key" AS BLOB)) + COALESCE(LENGTH(CAST(NEW."Value" AS BLOB)), 0)
        + COALESCE(LENGTH(CAST(NEW."Reference" AS BLOB)), 0) > 65536
BEGIN
    SELECT RAISE(ABORT, 'teletext profile settings budget exceeded');
END;

CREATE TRIGGER "TR_TeletextProfileSetting_UpdateBudget"
BEFORE UPDATE ON "TeletextProfileSetting"
WHEN (SELECT COUNT(*) FROM "TeletextProfileSetting" WHERE "ProfileId" = NEW."ProfileId"
        AND NOT ("ProfileId" = OLD."ProfileId" AND "Key" = OLD."Key")) >= 256
    OR (SELECT COALESCE(SUM(LENGTH(CAST("Key" AS BLOB))
            + COALESCE(LENGTH(CAST("Value" AS BLOB)), 0)
            + COALESCE(LENGTH(CAST("Reference" AS BLOB)), 0)), 0)
        FROM "TeletextProfileSetting" WHERE "ProfileId" = NEW."ProfileId"
            AND NOT ("ProfileId" = OLD."ProfileId" AND "Key" = OLD."Key"))
        + LENGTH(CAST(NEW."Key" AS BLOB)) + COALESCE(LENGTH(CAST(NEW."Value" AS BLOB)), 0)
        + COALESCE(LENGTH(CAST(NEW."Reference" AS BLOB)), 0) > 65536
BEGIN
    SELECT RAISE(ABORT, 'teletext profile settings budget exceeded');
END;
