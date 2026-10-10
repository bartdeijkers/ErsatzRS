-- ErsatzRS extension (ADR 0121): report-only source teletext metadata.
-- Existing stream/file identities and duplicate stream indexes are unchanged.
-- Binding is an adapter association, not proof of immutable source bytes.
-- Reported descriptors are separate from pages; no page observer exists yet.
CREATE UNIQUE INDEX "IX_MediaStream_Id_MediaVersionId" ON "MediaStream" ("Id", "MediaVersionId");
CREATE UNIQUE INDEX "IX_MediaFile_Id_MediaVersionId" ON "MediaFile" ("Id", "MediaVersionId");

CREATE TABLE "SourceTeletextMetadata" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_SourceTeletextMetadata" PRIMARY KEY AUTOINCREMENT,
    "MediaVersionId" INTEGER NOT NULL,
    "MediaStreamId" INTEGER NOT NULL,
    "MediaFileId" INTEGER NOT NULL,
    "StreamIndex" INTEGER NOT NULL,
    "BindingStatus" INTEGER NOT NULL,
    "MediaSourceId" INTEGER NULL,
    "Provider" TEXT NULL,
    "SourceRevision" TEXT NULL,
    "Provenance" INTEGER NOT NULL,
    "PidStatus" INTEGER NOT NULL,
    "Pid" INTEGER NULL,
    "ProgramStatus" INTEGER NOT NULL,
    "DescriptorStatus" INTEGER NOT NULL,
    "TrailingUnpairedPairs" INTEGER NULL,
    "ObservedPagesStatus" INTEGER NOT NULL,
    CONSTRAINT "FK_SourceTeletextMetadata_MediaStream_MediaStreamId_MediaVersionId"
        FOREIGN KEY ("MediaStreamId", "MediaVersionId") REFERENCES "MediaStream" ("Id", "MediaVersionId") ON DELETE CASCADE,
    CONSTRAINT "FK_SourceTeletextMetadata_MediaFile_MediaFileId_MediaVersionId"
        FOREIGN KEY ("MediaFileId", "MediaVersionId") REFERENCES "MediaFile" ("Id", "MediaVersionId") ON DELETE CASCADE,
    CONSTRAINT "FK_SourceTeletextMetadata_MediaSource_MediaSourceId"
        FOREIGN KEY ("MediaSourceId") REFERENCES "MediaSource" ("Id") ON DELETE CASCADE,
    CONSTRAINT "CK_SourceTeletextMetadata_ParentIdentity" CHECK (
        TYPEOF("MediaVersionId") = 'integer' AND "MediaVersionId" > 0
        AND TYPEOF("MediaStreamId") = 'integer' AND "MediaStreamId" > 0
        AND TYPEOF("MediaFileId") = 'integer' AND "MediaFileId" > 0),
    CONSTRAINT "CK_SourceTeletextMetadata_StreamIndex" CHECK (
        TYPEOF("StreamIndex") = 'integer' AND "StreamIndex" BETWEEN 0 AND 2147483647),
    CONSTRAINT "CK_SourceTeletextMetadata_BindingStatus" CHECK (
        TYPEOF("BindingStatus") = 'integer' AND "BindingStatus" IN (0, 1, 2)),
    CONSTRAINT "CK_SourceTeletextMetadata_Binding" CHECK (
        ("BindingStatus" IN (0, 1) AND "MediaSourceId" IS NULL AND "Provider" IS NULL AND "SourceRevision" IS NULL)
        OR ("BindingStatus" = 2 AND "MediaSourceId" IS NOT NULL
            AND TYPEOF("MediaSourceId") = 'integer' AND "MediaSourceId" > 0
            AND "Provider" IS NOT NULL AND TYPEOF("Provider") = 'text'
            AND "Provider" IN ('Local', 'Plex', 'Jellyfin', 'Emby')
            AND "SourceRevision" IS NOT NULL AND TYPEOF("SourceRevision") = 'text'
            AND LENGTH(CAST("SourceRevision" AS BLOB)) BETWEEN 1 AND 1024)),
    CONSTRAINT "CK_SourceTeletextMetadata_Provenance" CHECK (
        TYPEOF("Provenance") = 'integer' AND "Provenance" IN (0, 1)),
    CONSTRAINT "CK_SourceTeletextMetadata_PidStatus" CHECK (
        TYPEOF("PidStatus") = 'integer' AND "PidStatus" IN (0, 1, 2)),
    CONSTRAINT "CK_SourceTeletextMetadata_Pid" CHECK (
        ("PidStatus" IN (0, 1) AND "Pid" IS NULL)
        OR ("PidStatus" = 2 AND "Pid" IS NOT NULL AND TYPEOF("Pid") = 'integer' AND "Pid" BETWEEN 0 AND 8191)),
    CONSTRAINT "CK_SourceTeletextMetadata_ProgramStatus" CHECK (
        TYPEOF("ProgramStatus") = 'integer' AND "ProgramStatus" IN (0, 1, 2)),
    CONSTRAINT "CK_SourceTeletextMetadata_DescriptorStatus" CHECK (
        TYPEOF("DescriptorStatus") = 'integer' AND "DescriptorStatus" IN (0, 1, 2)),
    CONSTRAINT "CK_SourceTeletextMetadata_TrailingUnpairedPairs" CHECK (
        "TrailingUnpairedPairs" IS NULL OR ("DescriptorStatus" = 2
            AND TYPEOF("TrailingUnpairedPairs") = 'integer' AND "TrailingUnpairedPairs" BETWEEN 1 AND 4294967295)),
    CONSTRAINT "CK_SourceTeletextMetadata_ObservedPagesStatus" CHECK (
        TYPEOF("ObservedPagesStatus") = 'integer' AND "ObservedPagesStatus" = 0)
);
CREATE UNIQUE INDEX "IX_SourceTeletextMetadata_MediaStreamId_MediaFileId"
    ON "SourceTeletextMetadata" ("MediaStreamId", "MediaFileId");
CREATE INDEX "IX_SourceTeletextMetadata_MediaFileId_MediaVersionId"
    ON "SourceTeletextMetadata" ("MediaFileId", "MediaVersionId");
CREATE INDEX "IX_SourceTeletextMetadata_MediaVersionId"
    ON "SourceTeletextMetadata" ("MediaVersionId");
CREATE INDEX "IX_SourceTeletextMetadata_MediaSourceId"
    ON "SourceTeletextMetadata" ("MediaSourceId");

CREATE TABLE "SourceTeletextDescriptor" (
    "SourceTeletextMetadataId" INTEGER NOT NULL,
    "ProjectionOrdinal" INTEGER NOT NULL,
    "Language" TEXT NOT NULL,
    "DescriptorType" INTEGER NOT NULL,
    "WireMagazine" INTEGER NOT NULL,
    "WirePage" INTEGER NOT NULL,
    CONSTRAINT "PK_SourceTeletextDescriptor" PRIMARY KEY ("SourceTeletextMetadataId", "ProjectionOrdinal"),
    CONSTRAINT "FK_SourceTeletextDescriptor_SourceTeletextMetadata_SourceTeletextMetadataId"
        FOREIGN KEY ("SourceTeletextMetadataId") REFERENCES "SourceTeletextMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "CK_SourceTeletextDescriptor_ProjectionOrdinal" CHECK (
        TYPEOF("ProjectionOrdinal") = 'integer' AND "ProjectionOrdinal" BETWEEN 0 AND 4294967295),
    CONSTRAINT "CK_SourceTeletextDescriptor_Language" CHECK (
        TYPEOF("Language") = 'text' AND LENGTH("Language") = 3
        AND LENGTH(CAST("Language" AS BLOB)) = 3 AND "Language" NOT GLOB '*[^a-z]*'),
    CONSTRAINT "CK_SourceTeletextDescriptor_DescriptorType" CHECK (
        TYPEOF("DescriptorType") = 'integer' AND "DescriptorType" BETWEEN 0 AND 31),
    CONSTRAINT "CK_SourceTeletextDescriptor_WireMagazine" CHECK (
        TYPEOF("WireMagazine") = 'integer' AND "WireMagazine" BETWEEN 0 AND 7),
    CONSTRAINT "CK_SourceTeletextDescriptor_WirePage" CHECK (
        TYPEOF("WirePage") = 'integer' AND "WirePage" BETWEEN 0 AND 255)
);

CREATE TABLE "SourceTeletextProgram" (
    "SourceTeletextMetadataId" INTEGER NOT NULL,
    "MembershipOrdinal" INTEGER NOT NULL,
    "ProgramId" INTEGER NOT NULL,
    "ProgramNumber" INTEGER NOT NULL,
    "PmtPid" INTEGER NOT NULL,
    "PcrPid" INTEGER NOT NULL,
    CONSTRAINT "PK_SourceTeletextProgram" PRIMARY KEY ("SourceTeletextMetadataId", "MembershipOrdinal"),
    CONSTRAINT "FK_SourceTeletextProgram_SourceTeletextMetadata_SourceTeletextMetadataId"
        FOREIGN KEY ("SourceTeletextMetadataId") REFERENCES "SourceTeletextMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "CK_SourceTeletextProgram_MembershipOrdinal" CHECK (
        TYPEOF("MembershipOrdinal") = 'integer' AND "MembershipOrdinal" BETWEEN 0 AND 4294967295),
    CONSTRAINT "CK_SourceTeletextProgram_ProgramId" CHECK (
        TYPEOF("ProgramId") = 'integer' AND "ProgramId" BETWEEN 0 AND 4294967295),
    CONSTRAINT "CK_SourceTeletextProgram_ProgramNumber" CHECK (
        TYPEOF("ProgramNumber") = 'integer' AND "ProgramNumber" BETWEEN 0 AND 4294967295),
    CONSTRAINT "CK_SourceTeletextProgram_PmtPid" CHECK (
        TYPEOF("PmtPid") = 'integer' AND "PmtPid" BETWEEN 0 AND 8191),
    CONSTRAINT "CK_SourceTeletextProgram_PcrPid" CHECK (
        TYPEOF("PcrPid") = 'integer' AND "PcrPid" BETWEEN 0 AND 8191)
);

-- Provider PathHash-conflict upserts can reassign an existing file to a version;
-- local rescans can change its path/hash without deleting the row. Invalidate
-- dependent observations before the update so composite FKs do not block these
-- existing writes. No-op updates preserve metadata; neither path is a revision.
CREATE TRIGGER "TR_MediaFile_InvalidateSourceTeletextMetadata"
BEFORE UPDATE OF "MediaVersionId", "Path", "PathHash" ON "MediaFile"
FOR EACH ROW
WHEN OLD."MediaVersionId" IS NOT NEW."MediaVersionId"
    OR OLD."Path" IS NOT NEW."Path"
    OR OLD."PathHash" IS NOT NEW."PathHash"
BEGIN
    DELETE FROM "SourceTeletextMetadata" WHERE "MediaFileId" = OLD."Id";
END;

-- An in-place stream identity/projection change also makes its prior report
-- stale. Other ordinary stream settings do not invalidate source metadata.
CREATE TRIGGER "TR_MediaStream_InvalidateSourceTeletextMetadata"
BEFORE UPDATE OF "MediaVersionId", "Index", "Codec", "MediaStreamKind", "Language" ON "MediaStream"
FOR EACH ROW
WHEN OLD."MediaVersionId" IS NOT NEW."MediaVersionId"
    OR OLD."Index" IS NOT NEW."Index"
    OR OLD."Codec" IS NOT NEW."Codec"
    OR OLD."MediaStreamKind" IS NOT NEW."MediaStreamKind"
    OR OLD."Language" IS NOT NEW."Language"
BEGIN
    DELETE FROM "SourceTeletextMetadata" WHERE "MediaStreamId" = OLD."Id";
END;
