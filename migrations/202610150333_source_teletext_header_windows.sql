-- ErsatzRS-only bounded packet-0 observations (ADR 0121), not selectable pages.
-- Keep the applied parent ObservedPagesStatus CHECK=0 as a legacy sentinel.
-- A validated versioned child explicitly means HeadersObserved, never Ready.
CREATE TABLE "SourceTeletextHeaderWindow" (
    "SourceTeletextMetadataId" INTEGER NOT NULL CONSTRAINT "PK_SourceTeletextHeaderWindow" PRIMARY KEY,
    "SchemaVersion" INTEGER NOT NULL,
    "Payload" TEXT NOT NULL,
    CONSTRAINT "FK_SourceTeletextHeaderWindow_SourceTeletextMetadata_SourceTeletextMetadataId" FOREIGN KEY ("SourceTeletextMetadataId") REFERENCES "SourceTeletextMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "CK_SourceTeletextHeaderWindow_SchemaVersion" CHECK ("SchemaVersion" = 1),
    CONSTRAINT "CK_SourceTeletextHeaderWindow_Payload" CHECK (TYPEOF("Payload") = 'text' AND LENGTH(CAST("Payload" AS BLOB)) BETWEEN 1 AND 1048576)
);
