-- ADR 0121: retain static/lifecycle artifact roles 1..4 and admit explicit
-- source-qualified guide roles 5/6 without reinterpreting existing histories.
-- Saved designs, pending requests and both pointer roles retain exact identity;
-- fresh source/time qualification remains repository-owned.
-- SQLite cannot widen a CHECK in place. Rebuild only this retained table using
-- ADR 0124's deferred-FK pattern; preserve histories, pointers and sequence.
-- SQLx owns the transaction; foreign keys remain enabled throughout.
PRAGMA defer_foreign_keys = ON;
CREATE TEMP TABLE "TeletextAcceptedGenerationMigrationCopy" AS
    SELECT "Id", "ChannelId", "RequestId", "ArtifactVersion", "ArtifactDigest"
    FROM "TeletextAcceptedGeneration";
CREATE TEMP TABLE "TeletextAcceptedGenerationMigrationSequence" AS
    SELECT "seq" FROM sqlite_sequence WHERE "name" = 'TeletextAcceptedGeneration';
DROP TABLE "TeletextAcceptedGeneration";
CREATE TABLE "TeletextAcceptedGeneration" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_TeletextAcceptedGeneration" PRIMARY KEY AUTOINCREMENT,
    "ChannelId" INTEGER NOT NULL,
    "RequestId" INTEGER NOT NULL,
    "ArtifactVersion" INTEGER NOT NULL,
    "ArtifactDigest" TEXT NOT NULL,
    CONSTRAINT "AK_TeletextAcceptedGeneration_ChannelId_Id" UNIQUE ("ChannelId", "Id"),
    CONSTRAINT "AK_TeletextAcceptedGeneration_ChannelId_RequestId" UNIQUE ("ChannelId", "RequestId"),
    CONSTRAINT "FK_TeletextAcceptedGeneration_TeletextPublicationRequest_ChannelId_RequestId"
        FOREIGN KEY ("ChannelId", "RequestId")
        REFERENCES "TeletextPublicationRequest" ("ChannelId", "Id") ON DELETE RESTRICT,
    CONSTRAINT "CK_TeletextAcceptedGeneration_Id" CHECK (
        TYPEOF("Id") = 'integer' AND "Id" > 0),
    CONSTRAINT "CK_TeletextAcceptedGeneration_ChannelId" CHECK (
        TYPEOF("ChannelId") = 'integer' AND "ChannelId" > 0),
    CONSTRAINT "CK_TeletextAcceptedGeneration_RequestId" CHECK (
        TYPEOF("RequestId") = 'integer' AND "RequestId" > 0),
    CONSTRAINT "CK_TeletextAcceptedGeneration_ArtifactVersion" CHECK (
        TYPEOF("ArtifactVersion") = 'integer' AND "ArtifactVersion" IN (1, 2, 3, 4, 5, 6)),
    CONSTRAINT "CK_TeletextAcceptedGeneration_ArtifactDigest" CHECK (
        TYPEOF("ArtifactDigest") = 'text' AND LENGTH(CAST("ArtifactDigest" AS BLOB)) = 64
        AND "ArtifactDigest" NOT GLOB '*[^0-9a-f]*')
);
INSERT INTO "TeletextAcceptedGeneration" ("Id", "ChannelId", "RequestId", "ArtifactVersion", "ArtifactDigest")
    SELECT "Id", "ChannelId", "RequestId", "ArtifactVersion", "ArtifactDigest"
    FROM "TeletextAcceptedGenerationMigrationCopy";
DROP TABLE "TeletextAcceptedGenerationMigrationCopy";
-- Preserve a deliberately advanced original sequence, including an empty history.
UPDATE sqlite_sequence
    SET "seq" = (SELECT "seq" FROM "TeletextAcceptedGenerationMigrationSequence")
    WHERE "name" = 'TeletextAcceptedGeneration'
      AND EXISTS (SELECT 1 FROM "TeletextAcceptedGenerationMigrationSequence");
INSERT INTO sqlite_sequence ("name", "seq")
    SELECT 'TeletextAcceptedGeneration', "seq"
    FROM "TeletextAcceptedGenerationMigrationSequence"
    WHERE NOT EXISTS (SELECT 1 FROM sqlite_sequence WHERE "name" = 'TeletextAcceptedGeneration');
DROP TABLE "TeletextAcceptedGenerationMigrationSequence";
CREATE TRIGGER "TR_TeletextAcceptedGeneration_ImmutableUpdate"
BEFORE UPDATE ON "TeletextAcceptedGeneration"
BEGIN
    SELECT RAISE(ABORT, 'teletext accepted generations are immutable');
END;
CREATE TRIGGER "TR_TeletextAcceptedGeneration_ImmutableDelete"
BEFORE DELETE ON "TeletextAcceptedGeneration"
BEGIN
    SELECT RAISE(ABORT, 'teletext accepted generations are retained');
END;

CREATE TEMP TABLE "TeletextAcceptedGenerationMigrationIntegrity" (
    "Violations" INTEGER NOT NULL
        CONSTRAINT "CK_TeletextAcceptedGenerationMigrationIntegrity_Zero" CHECK ("Violations" = 0)
);
INSERT INTO "TeletextAcceptedGenerationMigrationIntegrity" ("Violations")
    SELECT COUNT(*) FROM pragma_foreign_key_check
    WHERE "table" = 'TeletextAcceptedGeneration' OR "parent" = 'TeletextAcceptedGeneration';
DROP TABLE "TeletextAcceptedGenerationMigrationIntegrity";
