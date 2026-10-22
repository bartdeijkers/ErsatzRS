-- Source page transmissions/context (ADR 0121), not decoded playback authority.
-- Preserve the parent sentinel and applied header schema; old rows stay valid.
CREATE TABLE "SourceTeletextPageWindow" (
    "SourceTeletextMetadataId" INTEGER NOT NULL CONSTRAINT "PK_SourceTeletextPageWindow" PRIMARY KEY,
    "SchemaVersion" INTEGER NOT NULL,
    "Payload" TEXT NOT NULL,
    CONSTRAINT "FK_SourceTeletextPageWindow_SourceTeletextMetadata_SourceTeletextMetadataId" FOREIGN KEY ("SourceTeletextMetadataId") REFERENCES "SourceTeletextMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_SourceTeletextPageWindow_SourceTeletextHeaderWindow_SourceTeletextMetadataId" FOREIGN KEY ("SourceTeletextMetadataId") REFERENCES "SourceTeletextHeaderWindow" ("SourceTeletextMetadataId") ON DELETE CASCADE,
    CONSTRAINT "CK_SourceTeletextPageWindow_SchemaVersion" CHECK ("SchemaVersion" = 1),
    CONSTRAINT "CK_SourceTeletextPageWindow_Payload" CHECK (TYPEOF("Payload") = 'text' AND LENGTH(CAST("Payload" AS BLOB)) BETWEEN 1 AND 2097152)
);
