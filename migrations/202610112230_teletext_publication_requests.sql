-- ADR 0123: immutable pending publication intent, never active output.
-- Channel/profile/package removal retains request history and its current pointer.
CREATE TABLE "TeletextPublicationRequest" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_TeletextPublicationRequest" PRIMARY KEY AUTOINCREMENT,
    "ChannelId" INTEGER NOT NULL,
    "DesignId" INTEGER NOT NULL,
    "Revision" INTEGER NOT NULL,
    "Digest" TEXT NOT NULL,
    CONSTRAINT "AK_TeletextPublicationRequest_ChannelId_Id" UNIQUE ("ChannelId", "Id"),
    CONSTRAINT "FK_TeletextPublicationRequest_TeletextDesignRevision_DesignId_Revision"
        FOREIGN KEY ("DesignId", "Revision")
        REFERENCES "TeletextDesignRevision" ("DesignId", "Revision") ON DELETE RESTRICT,
    CONSTRAINT "CK_TeletextPublicationRequest_ChannelId" CHECK (
        TYPEOF("ChannelId") = 'integer' AND "ChannelId" > 0),
    CONSTRAINT "CK_TeletextPublicationRequest_DesignId" CHECK (
        TYPEOF("DesignId") = 'integer' AND "DesignId" > 0),
    CONSTRAINT "CK_TeletextPublicationRequest_Revision" CHECK (
        TYPEOF("Revision") = 'integer' AND "Revision" > 0),
    CONSTRAINT "CK_TeletextPublicationRequest_Digest" CHECK (
        TYPEOF("Digest") = 'text' AND LENGTH(CAST("Digest" AS BLOB)) = 64
        AND "Digest" NOT GLOB '*[^0-9a-f]*')
);
CREATE TABLE "TeletextPublicationRequestCurrent" (
    "ChannelId" INTEGER NOT NULL CONSTRAINT "PK_TeletextPublicationRequestCurrent" PRIMARY KEY,
    "RequestId" INTEGER NOT NULL,
    CONSTRAINT "FK_TeletextPublicationRequestCurrent_TeletextPublicationRequest_ChannelId_RequestId"
        FOREIGN KEY ("ChannelId", "RequestId")
        REFERENCES "TeletextPublicationRequest" ("ChannelId", "Id") ON DELETE RESTRICT,
    CONSTRAINT "CK_TeletextPublicationRequestCurrent_ChannelId" CHECK (
        TYPEOF("ChannelId") = 'integer' AND "ChannelId" > 0),
    CONSTRAINT "CK_TeletextPublicationRequestCurrent_RequestId" CHECK (
        TYPEOF("RequestId") = 'integer' AND "RequestId" > 0)
);
CREATE TRIGGER "TR_TeletextPublicationRequest_ImmutableUpdate"
BEFORE UPDATE ON "TeletextPublicationRequest"
BEGIN
    SELECT RAISE(ABORT, 'teletext publication requests are immutable');
END;
CREATE TRIGGER "TR_TeletextPublicationRequest_ImmutableDelete"
BEFORE DELETE ON "TeletextPublicationRequest"
BEGIN
    SELECT RAISE(ABORT, 'teletext publication requests are retained');
END;
