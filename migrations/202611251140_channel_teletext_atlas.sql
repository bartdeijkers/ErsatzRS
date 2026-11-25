-- Existing links retain slot zero and continue to inherit their element.
ALTER TABLE "ChannelTeletextElement" ADD COLUMN "InstanceSlot" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "ChannelTeletextElement" ADD COLUMN "InstanceSettings" TEXT;
DROP INDEX "IX_ChannelTeletextElement_ChannelId_TeletextElementId";
CREATE UNIQUE INDEX "IX_ChannelTeletextElement_ChannelId_TeletextElementId_InstanceSlot"
    ON "ChannelTeletextElement" ("ChannelId", "TeletextElementId", "InstanceSlot");
CREATE TABLE "ChannelTeletextAtlas" (
    "ChannelId" INTEGER NOT NULL CONSTRAINT "PK_ChannelTeletextAtlas" PRIMARY KEY,
    "Revision" INTEGER NOT NULL,
    "GuidePages" TEXT NOT NULL,
    "PublicationKey" TEXT,
    "Generation" INTEGER,
    CONSTRAINT "FK_ChannelTeletextAtlas_Channel_ChannelId" FOREIGN KEY ("ChannelId")
        REFERENCES "Channel" ("Id") ON DELETE CASCADE,
    CONSTRAINT "CK_ChannelTeletextAtlas_Values" CHECK ("Revision" > 0 AND LENGTH("GuidePages") <= 16384)
);
CREATE TABLE "TeletextAtlasCandidate" (
    "DesignId" INTEGER NOT NULL,
    "DesignRevision" INTEGER NOT NULL,
    "ChannelId" INTEGER NOT NULL,
    "AtlasRevision" INTEGER NOT NULL,
    "PublicationKey" TEXT NOT NULL,
    CONSTRAINT "PK_TeletextAtlasCandidate" PRIMARY KEY ("DesignId", "DesignRevision"),
    CONSTRAINT "FK_TeletextAtlasCandidate_TeletextDesignRevision" FOREIGN KEY ("DesignId", "DesignRevision")
        REFERENCES "TeletextDesignRevision" ("DesignId", "Revision") ON DELETE CASCADE,
    CONSTRAINT "FK_TeletextAtlasCandidate_Channel" FOREIGN KEY ("ChannelId") REFERENCES "Channel" ("Id") ON DELETE CASCADE
);

-- Link or element changes through the existing library invalidate a rendered
-- Atlas form too. Keep the user's guide pages when only library content changes.
CREATE TRIGGER "TR_ChannelTeletextElement_AtlasInsert" AFTER INSERT ON "ChannelTeletextElement"
BEGIN
    INSERT INTO "ChannelTeletextAtlas" ("ChannelId","Revision","GuidePages") VALUES (NEW."ChannelId",1,'')
    ON CONFLICT ("ChannelId") DO UPDATE SET "Revision"="Revision"+1, "PublicationKey"=NULL;
END;
CREATE TRIGGER "TR_ChannelTeletextElement_AtlasUpdate" AFTER UPDATE ON "ChannelTeletextElement"
BEGIN
    UPDATE "ChannelTeletextAtlas" SET "Revision"="Revision"+1, "PublicationKey"=NULL WHERE "ChannelId"=NEW."ChannelId";
END;
CREATE TRIGGER "TR_ChannelTeletextElement_AtlasDelete" AFTER DELETE ON "ChannelTeletextElement"
BEGIN
    UPDATE "ChannelTeletextAtlas" SET "Revision"="Revision"+1, "PublicationKey"=NULL WHERE "ChannelId"=OLD."ChannelId";
END;
INSERT OR IGNORE INTO "ChannelTeletextAtlas" ("ChannelId","Revision","GuidePages")
SELECT DISTINCT "ChannelId",1,'' FROM "ChannelTeletextElement";

CREATE TRIGGER "TR_TeletextElement_AtlasRevision" AFTER UPDATE OF "Revision" ON "TeletextElement"
BEGIN
    UPDATE "ChannelTeletextAtlas" SET "Revision"="Revision"+1,"PublicationKey"=NULL
    WHERE "ChannelId" IN (SELECT "ChannelId" FROM "ChannelTeletextElement"
        WHERE "TeletextElementId" = NEW."Id" AND "IsEnabled" = 1);
END;

-- The authoring snapshot may include a prospective binding. Output recovery
-- continues reading ChannelTeletextBinding until a generation is selected.
CREATE TABLE "TeletextAtlasBindingDraft" (
    "ChannelId" INTEGER NOT NULL CONSTRAINT "PK_TeletextAtlasBindingDraft" PRIMARY KEY,
    "AddonId" TEXT NOT NULL,
    "ProfileId" INTEGER NOT NULL,
    "Enabled" INTEGER NOT NULL,
    "Revision" INTEGER NOT NULL,
    "ExpectedRevision" INTEGER NOT NULL,
    CONSTRAINT "FK_TeletextAtlasBindingDraft_Channel" FOREIGN KEY ("ChannelId") REFERENCES "Channel" ("Id") ON DELETE CASCADE
);

-- Profile or binding edits through another admin workflow supersede the staged switch.
CREATE TRIGGER "TR_ChannelTeletextBinding_AtlasUpdate" AFTER UPDATE ON "ChannelTeletextBinding"
BEGIN
    DELETE FROM "TeletextAtlasBindingDraft" WHERE "ChannelId"=NEW."ChannelId" AND "AddonId"=NEW."AddonId";
END;
CREATE TRIGGER "TR_ChannelTeletextBinding_AtlasInsert" AFTER INSERT ON "ChannelTeletextBinding"
BEGIN
    DELETE FROM "TeletextAtlasBindingDraft" WHERE "ChannelId"=NEW."ChannelId" AND "AddonId"=NEW."AddonId";
END;
