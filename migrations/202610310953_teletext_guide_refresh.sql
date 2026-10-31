-- ADR 0121: explicit guide acceptance seeds a separate durable refresh lineage.
-- Manual originals, pending intents and accepted-generation pointers are retained.
-- Runtime snapshots never impersonate manual artifacts or accepted-generation IDs.
CREATE TABLE "TeletextGuideRefreshSeed" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_TeletextGuideRefreshSeed" PRIMARY KEY AUTOINCREMENT,
    "ChannelId" INTEGER NOT NULL,
    "Role" INTEGER NOT NULL,
    "GenerationId" INTEGER NOT NULL,
    "ExpectedDesignHead" INTEGER NOT NULL,
    "Source" BLOB NOT NULL,
    "ValidUntil" TEXT NOT NULL,
    CONSTRAINT "AK_TeletextGuideRefreshSeed_GenerationId" UNIQUE ("GenerationId"),
    CONSTRAINT "AK_TeletextGuideRefreshSeed_ChannelId_Role_Id" UNIQUE ("ChannelId", "Role", "Id"),
    CONSTRAINT "FK_TeletextGuideRefreshSeed_TeletextAcceptedGeneration_ChannelId_GenerationId"
        FOREIGN KEY ("ChannelId", "GenerationId") REFERENCES "TeletextAcceptedGeneration" ("ChannelId", "Id") ON DELETE RESTRICT,
    CONSTRAINT "CK_TeletextGuideRefreshSeed_Values" CHECK (
        TYPEOF("Id") = 'integer' AND "Id" > 0 AND TYPEOF("ChannelId") = 'integer' AND "ChannelId" > 0
        AND TYPEOF("Role") = 'integer' AND "Role" IN (1, 2)
        AND TYPEOF("GenerationId") = 'integer' AND "GenerationId" > 0
        AND TYPEOF("ExpectedDesignHead") = 'integer' AND "ExpectedDesignHead" > 0
        AND TYPEOF("Source") = 'blob' AND LENGTH("Source") > 0
        AND TYPEOF("ValidUntil") = 'text' AND LENGTH("ValidUntil") > 0)
);
CREATE TABLE "TeletextGuideRuntimeSnapshot" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_TeletextGuideRuntimeSnapshot" PRIMARY KEY AUTOINCREMENT,
    "SeedId" INTEGER NOT NULL,
    "Version" INTEGER NOT NULL,
    "DocumentVersion" INTEGER NOT NULL,
    "DocumentDigest" TEXT NOT NULL,
    "Document" BLOB NOT NULL,
    "Source" BLOB NOT NULL,
    "ValidUntil" TEXT NOT NULL,
    "ArtifactDigest" TEXT NOT NULL,
    CONSTRAINT "AK_TeletextGuideRuntimeSnapshot_SeedId_Id" UNIQUE ("SeedId", "Id"),
    CONSTRAINT "FK_TeletextGuideRuntimeSnapshot_TeletextGuideRefreshSeed_SeedId"
        FOREIGN KEY ("SeedId") REFERENCES "TeletextGuideRefreshSeed" ("Id") ON DELETE RESTRICT,
    CONSTRAINT "CK_TeletextGuideRuntimeSnapshot_Values" CHECK (
        TYPEOF("Id") = 'integer' AND "Id" > 0 AND TYPEOF("SeedId") = 'integer' AND "SeedId" > 0
        AND TYPEOF("Version") = 'integer' AND "Version" = 1
        AND TYPEOF("DocumentVersion") = 'integer' AND "DocumentVersion" = 1
        AND TYPEOF("Document") = 'blob' AND LENGTH("Document") > 0
        AND TYPEOF("Source") = 'blob' AND LENGTH("Source") > 0
        AND TYPEOF("ValidUntil") = 'text' AND LENGTH("ValidUntil") > 0),
    CONSTRAINT "CK_TeletextGuideRuntimeSnapshot_Digests" CHECK (
        TYPEOF("DocumentDigest") = 'text' AND LENGTH(CAST("DocumentDigest" AS BLOB)) = 64
        AND "DocumentDigest" NOT GLOB '*[^0-9a-f]*'
        AND TYPEOF("ArtifactDigest") = 'text' AND LENGTH(CAST("ArtifactDigest" AS BLOB)) = 64
        AND "ArtifactDigest" NOT GLOB '*[^0-9a-f]*')
);
CREATE TABLE "TeletextGuideRefreshCurrent" (
    "ChannelId" INTEGER NOT NULL,
    "Role" INTEGER NOT NULL,
    "SeedId" INTEGER NOT NULL,
    "RuntimeId" INTEGER NULL,
    "Revision" INTEGER NOT NULL,
    "NextDueAt" TEXT NOT NULL,
    "LastSuccessAt" TEXT NOT NULL,
    "ChangedAt" TEXT NULL,
    "FirstFailureAt" TEXT NULL,
    "Failure" INTEGER NULL,
    "Revoked" INTEGER NOT NULL DEFAULT 0,
    "LeaseOwner" TEXT NULL,
    "LeaseUntil" TEXT NULL,
    CONSTRAINT "PK_TeletextGuideRefreshCurrent" PRIMARY KEY ("ChannelId", "Role"),
    CONSTRAINT "AK_TeletextGuideRefreshCurrent_SeedId" UNIQUE ("SeedId"),
    CONSTRAINT "FK_TeletextGuideRefreshCurrent_TeletextGuideRefreshSeed_ChannelId_Role_SeedId"
        FOREIGN KEY ("ChannelId", "Role", "SeedId") REFERENCES "TeletextGuideRefreshSeed" ("ChannelId", "Role", "Id") ON DELETE RESTRICT,
    CONSTRAINT "FK_TeletextGuideRefreshCurrent_TeletextGuideRuntimeSnapshot_SeedId_RuntimeId"
        FOREIGN KEY ("SeedId", "RuntimeId") REFERENCES "TeletextGuideRuntimeSnapshot" ("SeedId", "Id") ON DELETE RESTRICT,
    CONSTRAINT "CK_TeletextGuideRefreshCurrent_Values" CHECK (
        TYPEOF("ChannelId") = 'integer' AND "ChannelId" > 0
        AND TYPEOF("Role") = 'integer' AND "Role" IN (1, 2)
        AND TYPEOF("SeedId") = 'integer' AND "SeedId" > 0
        AND TYPEOF("Revision") = 'integer' AND "Revision" > 0
        AND ("RuntimeId" IS NULL OR (TYPEOF("RuntimeId") = 'integer' AND "RuntimeId" > 0))
        AND TYPEOF("Revoked") = 'integer' AND "Revoked" IN (0, 1)
        AND ("Failure" IS NULL OR (TYPEOF("Failure") = 'integer' AND "Failure" IN (1, 2, 3, 4)))),
    CONSTRAINT "CK_TeletextGuideRefreshCurrent_Lease" CHECK (
        ("LeaseOwner" IS NULL AND "LeaseUntil" IS NULL)
        OR (TYPEOF("LeaseOwner") = 'text' AND LENGTH("LeaseOwner") = 36 AND TYPEOF("LeaseUntil") = 'text'))
);
CREATE INDEX "IX_TeletextGuideRefreshCurrent_Revoked_NextDueAt" ON "TeletextGuideRefreshCurrent" ("Revoked", "NextDueAt");
CREATE TRIGGER "TR_TeletextGuideRefreshSeed_ImmutableUpdate" BEFORE UPDATE ON "TeletextGuideRefreshSeed"
BEGIN SELECT RAISE(ABORT, 'teletext refresh seeds are immutable'); END;
CREATE TRIGGER "TR_TeletextGuideRefreshSeed_ImmutableDelete" BEFORE DELETE ON "TeletextGuideRefreshSeed"
BEGIN SELECT RAISE(ABORT, 'teletext refresh seeds are retained'); END;
CREATE TRIGGER "TR_TeletextGuideRuntimeSnapshot_ImmutableUpdate" BEFORE UPDATE ON "TeletextGuideRuntimeSnapshot"
BEGIN SELECT RAISE(ABORT, 'teletext runtime snapshots are immutable'); END;
CREATE TRIGGER "TR_TeletextGuideRuntimeSnapshot_ImmutableDelete" BEFORE DELETE ON "TeletextGuideRuntimeSnapshot"
WHEN EXISTS (SELECT 1 FROM "TeletextGuideRefreshCurrent"
    WHERE "SeedId" = OLD."SeedId" AND "RuntimeId" = OLD."Id")
BEGIN SELECT RAISE(ABORT, 'current teletext runtime snapshots are retained'); END;
