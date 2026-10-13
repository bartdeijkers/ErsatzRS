-- ADR 0123: opaque export receipts retain provenance, never editing authority.
-- Only revision ownership is persisted: uninstall/profile/channel removal does
-- not cascade receipts. Historical exports remain readable after tombstones.
CREATE TABLE "TeletextDesignExportReceipt" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_TeletextDesignExportReceipt" PRIMARY KEY AUTOINCREMENT,
    "DesignId" INTEGER NOT NULL,
    "Revision" INTEGER NOT NULL,
    "DesignDigest" TEXT NOT NULL,
    "ExpectedDesignHead" INTEGER NOT NULL,
    "ReceiptVersion" INTEGER NOT NULL,
    "Digest" TEXT NOT NULL,
    "Receipt" BLOB NOT NULL,
    CONSTRAINT "FK_TeletextDesignExportReceipt_TeletextDesignRevision_DesignId_Revision"
        FOREIGN KEY ("DesignId", "Revision")
        REFERENCES "TeletextDesignRevision" ("DesignId", "Revision") ON DELETE RESTRICT,
    CONSTRAINT "CK_TeletextDesignExportReceipt_DesignId" CHECK (
        TYPEOF("DesignId") = 'integer' AND "DesignId" > 0),
    CONSTRAINT "CK_TeletextDesignExportReceipt_Revision" CHECK (
        TYPEOF("Revision") = 'integer' AND "Revision" > 0),
    CONSTRAINT "CK_TeletextDesignExportReceipt_ExpectedDesignHead" CHECK (
        TYPEOF("ExpectedDesignHead") = 'integer' AND "ExpectedDesignHead" >= "Revision"),
    CONSTRAINT "CK_TeletextDesignExportReceipt_ReceiptVersion" CHECK (
        TYPEOF("ReceiptVersion") = 'integer' AND "ReceiptVersion" = 1),
    CONSTRAINT "CK_TeletextDesignExportReceipt_DesignDigest" CHECK (
        TYPEOF("DesignDigest") = 'text' AND LENGTH(CAST("DesignDigest" AS BLOB)) = 64
        AND "DesignDigest" NOT GLOB '*[^0-9a-f]*'),
    CONSTRAINT "CK_TeletextDesignExportReceipt_Digest" CHECK (
        TYPEOF("Digest") = 'text' AND LENGTH(CAST("Digest" AS BLOB)) = 64
        AND "Digest" NOT GLOB '*[^0-9a-f]*'),
    CONSTRAINT "CK_TeletextDesignExportReceipt_Receipt" CHECK (
        TYPEOF("Receipt") = 'blob' AND LENGTH("Receipt") > 0)
);
CREATE INDEX "IX_TeletextDesignExportReceipt_DesignId_Revision"
    ON "TeletextDesignExportReceipt" ("DesignId", "Revision");
CREATE TRIGGER "TR_TeletextDesignExportReceipt_ImmutableUpdate"
BEFORE UPDATE ON "TeletextDesignExportReceipt"
BEGIN
    SELECT RAISE(ABORT, 'teletext export receipts are immutable');
END;
CREATE TRIGGER "TR_TeletextDesignExportReceipt_ImmutableDelete"
BEFORE DELETE ON "TeletextDesignExportReceipt"
BEGIN
    SELECT RAISE(ABORT, 'teletext export receipts are immutable');
END;
