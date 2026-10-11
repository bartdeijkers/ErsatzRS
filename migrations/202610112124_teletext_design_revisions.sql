-- ADR 0123: ErsatzRS-only original design documents, never active output.
-- No channel/profile/package FK: removal of those models retains the design.
-- Revision->head RESTRICT and deletion guards deliberately avoid cascade:
-- application deletion is a retained tombstone, and revisions are immutable.
CREATE TABLE "TeletextDesign" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_TeletextDesign" PRIMARY KEY AUTOINCREMENT,
    "Name" TEXT NOT NULL,
    "Revision" INTEGER NOT NULL,
    "IsDeleted" INTEGER NOT NULL DEFAULT 0,
    CONSTRAINT "CK_TeletextDesign_Name" CHECK (
        TYPEOF("Name") = 'text' AND LENGTH(CAST("Name" AS BLOB)) BETWEEN 1 AND 256
        AND LENGTH(TRIM("Name")) > 0 AND INSTR("Name", CHAR(0)) = 0),
    CONSTRAINT "CK_TeletextDesign_Revision" CHECK (TYPEOF("Revision") = 'integer' AND "Revision" > 0),
    CONSTRAINT "CK_TeletextDesign_IsDeleted" CHECK (TYPEOF("IsDeleted") = 'integer' AND "IsDeleted" IN (0, 1))
);
CREATE TABLE "TeletextDesignRevision" (
    "DesignId" INTEGER NOT NULL,
    "Revision" INTEGER NOT NULL,
    "DocumentVersion" INTEGER NOT NULL,
    "Digest" TEXT NOT NULL,
    "Document" BLOB NOT NULL,
    CONSTRAINT "PK_TeletextDesignRevision" PRIMARY KEY ("DesignId", "Revision"),
    CONSTRAINT "FK_TeletextDesignRevision_TeletextDesign_DesignId"
        FOREIGN KEY ("DesignId") REFERENCES "TeletextDesign" ("Id") ON DELETE RESTRICT,
    CONSTRAINT "CK_TeletextDesignRevision_Revision" CHECK (TYPEOF("Revision") = 'integer' AND "Revision" > 0),
    CONSTRAINT "CK_TeletextDesignRevision_DocumentVersion" CHECK (
        TYPEOF("DocumentVersion") = 'integer' AND "DocumentVersion" = 1),
    CONSTRAINT "CK_TeletextDesignRevision_Digest" CHECK (
        TYPEOF("Digest") = 'text' AND LENGTH(CAST("Digest" AS BLOB)) = 64
        AND "Digest" NOT GLOB '*[^0-9a-f]*'),
    CONSTRAINT "CK_TeletextDesignRevision_Document" CHECK (
        TYPEOF("Document") = 'blob' AND LENGTH("Document") > 0)
);
CREATE TRIGGER "TR_TeletextDesign_RetainHead"
BEFORE DELETE ON "TeletextDesign"
BEGIN
    SELECT RAISE(ABORT, 'teletext design heads must be tombstoned');
END;
CREATE TRIGGER "TR_TeletextDesignRevision_ImmutableUpdate"
BEFORE UPDATE ON "TeletextDesignRevision"
BEGIN
    SELECT RAISE(ABORT, 'teletext design revisions are immutable');
END;
CREATE TRIGGER "TR_TeletextDesignRevision_ImmutableDelete"
BEFORE DELETE ON "TeletextDesignRevision"
BEGIN
    SELECT RAISE(ABORT, 'teletext design revisions are immutable');
END;
