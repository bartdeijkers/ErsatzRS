-- ADR 0123: ErsatzRS-only durable output selection, separate from pending intent.
-- Selection is not a sink/receiver acknowledgement. Channel/profile/package
-- removal retains history; runtime must revalidate before attaching output.
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
        TYPEOF("ArtifactVersion") = 'integer' AND "ArtifactVersion" = 1),
    CONSTRAINT "CK_TeletextAcceptedGeneration_ArtifactDigest" CHECK (
        TYPEOF("ArtifactDigest") = 'text' AND LENGTH(CAST("ArtifactDigest" AS BLOB)) = 64
        AND "ArtifactDigest" NOT GLOB '*[^0-9a-f]*')
);
CREATE TABLE "TeletextAcceptedGenerationCurrent" (
    "ChannelId" INTEGER NOT NULL CONSTRAINT "PK_TeletextAcceptedGenerationCurrent" PRIMARY KEY,
    "GenerationId" INTEGER NOT NULL,
    CONSTRAINT "FK_TeletextAcceptedGenerationCurrent_TeletextAcceptedGeneration_ChannelId_GenerationId"
        FOREIGN KEY ("ChannelId", "GenerationId")
        REFERENCES "TeletextAcceptedGeneration" ("ChannelId", "Id") ON DELETE RESTRICT,
    CONSTRAINT "CK_TeletextAcceptedGenerationCurrent_ChannelId" CHECK (
        TYPEOF("ChannelId") = 'integer' AND "ChannelId" > 0),
    CONSTRAINT "CK_TeletextAcceptedGenerationCurrent_GenerationId" CHECK (
        TYPEOF("GenerationId") = 'integer' AND "GenerationId" > 0)
);
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
