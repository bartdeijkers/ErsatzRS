-- ADR 0093: a copied last publication can own items absent from the saved draft.
-- This host-owned distinction has no ErsatzTV counterpart. Existing rows remain
-- draft members; publication membership and local bindings are unchanged.
ALTER TABLE "AddonMediaListItem" ADD COLUMN "InDraft" INTEGER NOT NULL DEFAULT 1
    CONSTRAINT "CK_AddonMediaListItem_InDraft" CHECK ("InDraft" IN (0, 1));

-- Receiving publication-only ownership must not extend the saved delivery
-- cohort. Import/re-adoption records an identity when it actually enters draft.
-- Retain every existing ledger row, including identities whose items disappeared.
DROP TRIGGER "TR_AddonMediaListItem_KnownIdentity_Insert";
CREATE TRIGGER "TR_AddonMediaListItem_KnownIdentity_Insert"
AFTER INSERT ON "AddonMediaListItem"
WHEN NEW."InDraft" = 1
BEGIN
    INSERT INTO "AddonMediaListKnownIdentity" ("AddonMediaListId", "ProviderId")
    VALUES (NEW."AddonMediaListId", NEW."ProviderId")
    ON CONFLICT ("AddonMediaListId", "ProviderId") DO NOTHING;
END;

DROP TRIGGER "TR_AddonMediaListItem_KnownIdentity_Update";
CREATE TRIGGER "TR_AddonMediaListItem_KnownIdentity_Update"
AFTER UPDATE OF "AddonMediaListId", "ProviderId", "InDraft" ON "AddonMediaListItem"
WHEN NEW."InDraft" = 1
BEGIN
    INSERT INTO "AddonMediaListKnownIdentity" ("AddonMediaListId", "ProviderId")
    VALUES (NEW."AddonMediaListId", NEW."ProviderId")
    ON CONFLICT ("AddonMediaListId", "ProviderId") DO NOTHING;
END;
