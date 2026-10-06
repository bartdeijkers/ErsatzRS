-- Additive upgrade: legacy list settings and explicit inclusion decisions survive.
CREATE TABLE "AddonMediaListSelection" (
    "AddonMediaListId" INTEGER NOT NULL,
    "PolicyJson" TEXT,
    "RefreshPaused" INTEGER NOT NULL DEFAULT 0,
    "ReviewRequired" INTEGER NOT NULL DEFAULT 0 CHECK ("ReviewRequired" IN (0, 1)),
    "EvaluatedAt" TEXT,
    "CutoffAt" TEXT,
    CONSTRAINT "PK_AddonMediaListSelection" PRIMARY KEY ("AddonMediaListId"),
    CONSTRAINT "FK_AddonMediaListSelection_AddonMediaList_AddonMediaListId"
        FOREIGN KEY ("AddonMediaListId") REFERENCES "AddonMediaList" ("Id") ON DELETE CASCADE,
    CONSTRAINT "CK_AddonMediaListSelection_RefreshPaused" CHECK ("RefreshPaused" IN (0, 1))
);
ALTER TABLE "AddonMediaListItem" ADD COLUMN "SelectionOverride" INTEGER
    CHECK ("SelectionOverride" IS NULL OR "SelectionOverride" IN (0, 1));
ALTER TABLE "AddonMediaListItem" ADD COLUMN "SelectionReason" TEXT;
UPDATE "AddonMediaListItem" SET "SelectionOverride" = "Included" WHERE "IncludedByUser" = 1;
