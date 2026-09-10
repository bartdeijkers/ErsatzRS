-- ADR 0091: preserve every existing upload ID and byte while permitting
-- retained, immutable image handles for native task navigation.
DROP INDEX "IX_AddonMediaListArtworkUpload_ListScopeRole";
CREATE UNIQUE INDEX "IX_AddonMediaListArtworkUpload_ListScopeRoleHash"
    ON "AddonMediaListArtworkUpload" ("AddonMediaListId", "ScopeItemId", "Role", "Sha256");
