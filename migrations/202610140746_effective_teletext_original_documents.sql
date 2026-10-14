-- ADR 0124: retain exact legacy originals while permitting distinct version 2.
-- This compatible rebuild runs inside SQLx's migration transaction. Deferral is
-- transaction-local; foreign_keys remains ON and COMMIT validates every edge.
-- Reinsert under the original table name to settle deferred parent-removal
-- counters. Copy-then-rename leaves those counters unresolved on existing refs.
PRAGMA defer_foreign_keys = ON;

CREATE TEMP TABLE "TeletextDesignRevisionMigrationCopy" AS
    SELECT rowid AS "PreservedRowId", "DesignId", "Revision", "DocumentVersion", "Digest", "Document"
    FROM "TeletextDesignRevision";

DROP TABLE "TeletextDesignRevision";
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
        TYPEOF("DocumentVersion") = 'integer' AND "DocumentVersion" IN (1, 2)),
    CONSTRAINT "CK_TeletextDesignRevision_Digest" CHECK (
        TYPEOF("Digest") = 'text' AND LENGTH(CAST("Digest" AS BLOB)) = 64
        AND "Digest" NOT GLOB '*[^0-9a-f]*'),
    CONSTRAINT "CK_TeletextDesignRevision_Document" CHECK (
        TYPEOF("Document") = 'blob' AND LENGTH("Document") > 0)
);
INSERT INTO "TeletextDesignRevision" (rowid, "DesignId", "Revision", "DocumentVersion", "Digest", "Document")
    SELECT "PreservedRowId", "DesignId", "Revision", "DocumentVersion", "Digest", "Document"
    FROM "TeletextDesignRevisionMigrationCopy";
DROP TABLE "TeletextDesignRevisionMigrationCopy";

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

-- A SELECT/PRAGMA alone would not abort on a violation. This temporary guard
-- rejects any affected orphan before commit, without relaxing FK enforcement.
CREATE TEMP TABLE "TeletextDesignRevisionMigrationIntegrity" (
    "Violations" INTEGER NOT NULL
        CONSTRAINT "CK_TeletextDesignRevisionMigrationIntegrity_Zero" CHECK ("Violations" = 0)
);
INSERT INTO "TeletextDesignRevisionMigrationIntegrity" ("Violations")
    SELECT COUNT(*) FROM pragma_foreign_key_check
    WHERE "table" = 'TeletextDesignRevision' OR "parent" = 'TeletextDesignRevision';
DROP TABLE "TeletextDesignRevisionMigrationIntegrity";
