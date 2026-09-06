-- ADR 0085: retain the selected file by revision/item, not by mutable draft owner.
-- RESTRICT matches revision artifact pins: deleting a file must not silently
-- alter an immutable snapshot or turn processed playback into raw playback.
ALTER TABLE "AddonMediaListRevisionItem" ADD COLUMN "PreparedArtifactId" INTEGER NULL
    CONSTRAINT "FK_AddonMediaListRevisionItem_AddonMediaListArtifact_PreparedArtifactId"
    REFERENCES "AddonMediaListArtifact" ("Id") ON DELETE RESTRICT;
CREATE INDEX "IX_AddonMediaListRevisionItem_PreparedArtifactId"
    ON "AddonMediaListRevisionItem" ("PreparedArtifactId");
