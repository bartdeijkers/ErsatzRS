-- Installation-local projection checkpoint; NULL leaves legacy/unadopted lists unchanged.
ALTER TABLE "AddonMediaList" ADD COLUMN "SavedDocumentDigest" TEXT NULL
    CONSTRAINT "CK_AddonMediaList_SavedDocumentDigest" CHECK (
        "SavedDocumentDigest" IS NULL OR (
            typeof("SavedDocumentDigest") = 'text'
            AND length("SavedDocumentDigest") = 64
            AND length(CAST("SavedDocumentDigest" AS BLOB)) = 64
            AND "SavedDocumentDigest" NOT GLOB '*[^0-9a-f]*'
        )
    );

-- Accepted saved publication content in the local projection/queue, not active playback proof.
-- NULL leaves legacy/unadopted publication state unchanged; no backfill is inferred.
ALTER TABLE "AddonMediaList" ADD COLUMN "SavedPublicationDigest" TEXT NULL
    CONSTRAINT "CK_AddonMediaList_SavedPublicationDigest" CHECK (
        "SavedPublicationDigest" IS NULL OR (
            typeof("SavedPublicationDigest") = 'text'
            AND length("SavedPublicationDigest") = 64
            AND length(CAST("SavedPublicationDigest" AS BLOB)) = 64
            AND "SavedPublicationDigest" NOT GLOB '*[^0-9a-f]*'
        )
    );
