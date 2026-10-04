-- ADR 0110: opting into controlled External JSON publication retains an accepted
-- document. No row is created for legacy direct readers during upgrade.
CREATE TABLE "PlayoutExternalJsonSnapshot" (
    "PlayoutId" INTEGER NOT NULL,
    "Revision" INTEGER NOT NULL,
    "SchemaVersion" INTEGER NOT NULL,
    "ScheduleFile" TEXT NOT NULL,
    "Document" TEXT NOT NULL,
    "PublishedAt" TEXT NOT NULL,
    "EffectiveFrom" TEXT NULL,
    CONSTRAINT "PK_PlayoutExternalJsonSnapshot" PRIMARY KEY ("PlayoutId"),
    CONSTRAINT "FK_PlayoutExternalJsonSnapshot_Playout_PlayoutId"
        FOREIGN KEY ("PlayoutId") REFERENCES "Playout" ("Id") ON DELETE CASCADE,
    CONSTRAINT "CK_PlayoutExternalJsonSnapshot_Revision" CHECK ("Revision" > 0),
    CONSTRAINT "CK_PlayoutExternalJsonSnapshot_DocumentSize"
        CHECK (length(CAST("Document" AS BLOB)) <= 33554432)
);
