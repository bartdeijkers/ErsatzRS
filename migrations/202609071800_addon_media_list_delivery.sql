-- AMM-M-01B / ADR 0087: provider-neutral delivery policy, with no upstream
-- equivalent. Identity evidence belongs to the list, not replaceable item rows.
ALTER TABLE "AddonMediaList" ADD COLUMN "DeliveryMode" INTEGER NOT NULL DEFAULT 0
    CONSTRAINT "CK_AddonMediaList_DeliveryMode" CHECK ("DeliveryMode" IN (0, 1, 2));
ALTER TABLE "AddonMediaList" ADD COLUMN "DeliveryCutoffAt" TEXT NULL;
ALTER TABLE "AddonMediaList" ADD COLUMN "DeliveryCutoffIdentityId" INTEGER NULL
    CONSTRAINT "CK_AddonMediaList_DeliveryCutoff" CHECK (
        ("DeliveryMode" = 2 AND "DeliveryCutoffAt" IS NOT NULL
            AND "DeliveryCutoffIdentityId" IS NOT NULL AND "DeliveryCutoffIdentityId" >= 0)
        OR ("DeliveryMode" != 2 AND "DeliveryCutoffAt" IS NULL
            AND "DeliveryCutoffIdentityId" IS NULL)
    );

CREATE TABLE "AddonMediaListKnownIdentity" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListKnownIdentity" PRIMARY KEY AUTOINCREMENT,
    "AddonMediaListId" INTEGER NOT NULL,
    "ProviderId" TEXT NOT NULL,
    CONSTRAINT "FK_AddonMediaListKnownIdentity_AddonMediaList_AddonMediaListId"
        FOREIGN KEY ("AddonMediaListId") REFERENCES "AddonMediaList" ("Id") ON DELETE CASCADE
);
CREATE UNIQUE INDEX "IX_AddonMediaListKnownIdentity_AddonMediaListId_ProviderId"
    ON "AddonMediaListKnownIdentity" ("AddonMediaListId", "ProviderId");
CREATE INDEX "IX_AddonMediaListKnownIdentity_AddonMediaListId_Id"
    ON "AddonMediaListKnownIdentity" ("AddonMediaListId", "Id");

-- Legacy current and restorable identities are already known. UNION produces
-- one logical identity even when multiple revisions retain the same item.
INSERT INTO "AddonMediaListKnownIdentity" ("AddonMediaListId", "ProviderId")
SELECT "AddonMediaListId", "ProviderId" FROM "AddonMediaListItem"
UNION
SELECT "revision"."AddonMediaListId", "item"."ProviderId"
FROM "AddonMediaListRevisionItem" AS "item"
JOIN "AddonMediaListRevision" AS "revision"
    ON "revision"."Id" = "item"."AddonMediaListRevisionId";

-- Import, accepted tombstone removal/reappearance, and revision restore all
-- insert item rows. Never delete the evidence when an individual item goes away.
CREATE TRIGGER "TR_AddonMediaListItem_KnownIdentity_Insert"
AFTER INSERT ON "AddonMediaListItem"
BEGIN
    INSERT INTO "AddonMediaListKnownIdentity" ("AddonMediaListId", "ProviderId")
    VALUES (NEW."AddonMediaListId", NEW."ProviderId")
    ON CONFLICT ("AddonMediaListId", "ProviderId") DO NOTHING;
END;

CREATE TRIGGER "TR_AddonMediaListItem_KnownIdentity_Update"
AFTER UPDATE OF "AddonMediaListId", "ProviderId" ON "AddonMediaListItem"
BEGIN
    INSERT INTO "AddonMediaListKnownIdentity" ("AddonMediaListId", "ProviderId")
    VALUES (NEW."AddonMediaListId", NEW."ProviderId")
    ON CONFLICT ("AddonMediaListId", "ProviderId") DO NOTHING;
END;
