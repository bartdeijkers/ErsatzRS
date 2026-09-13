-- ADR 0093: preserve a copied draft replacement choice before a receiving-local
-- media binding exists. This host-owned intent has no ErsatzTV counterpart.
-- NULL means no unresolved choice; JSON retains the portable file reference or
-- explicit {"file":null}. Concrete local bindings remain in the existing table.
ALTER TABLE "AddonMediaListItem" ADD COLUMN "SavedReplacementJson" TEXT NULL;
