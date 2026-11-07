-- Teletext elements: a library of user-supplied teletext pages (TTI, T42, EP1,
-- HTT uploads stored as canonical TTI) with field bindings, plus each channel's
-- links to elements. Links grant no publication or broadcast authority.
CREATE TABLE "TeletextElement" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_TeletextElement" PRIMARY KEY AUTOINCREMENT,
    "Name" TEXT NOT NULL,
    "SourceFormat" INTEGER NOT NULL,
    "SourceFileName" TEXT NOT NULL,
    "Document" BLOB NOT NULL,
    "PageNumber" INTEGER NOT NULL,
    "SubpageCount" INTEGER NOT NULL,
    "CycleSeconds" INTEGER NOT NULL,
    "Bindings" TEXT NOT NULL,
    "Revision" INTEGER NOT NULL,
    "UpdatedAt" TEXT NOT NULL,
    CONSTRAINT "CK_TeletextElement_Values" CHECK (
        LENGTH("Name") > 0
        AND "SourceFormat" IN (1, 2, 3, 4)
        AND TYPEOF("Document") = 'blob' AND LENGTH("Document") > 0
        AND "PageNumber" BETWEEN 256 AND 2303
        AND "SubpageCount" BETWEEN 1 AND 64
        AND "CycleSeconds" BETWEEN 1 AND 600
        AND "Revision" > 0)
);
CREATE UNIQUE INDEX "IX_TeletextElement_Name" ON "TeletextElement" ("Name");

CREATE TABLE "ChannelTeletextElement" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_ChannelTeletextElement" PRIMARY KEY AUTOINCREMENT,
    "ChannelId" INTEGER NOT NULL,
    "TeletextElementId" INTEGER NOT NULL,
    "PageNumber" INTEGER NULL,
    "IsEnabled" INTEGER NOT NULL DEFAULT 1,
    CONSTRAINT "FK_ChannelTeletextElement_Channel_ChannelId"
        FOREIGN KEY ("ChannelId") REFERENCES "Channel" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_ChannelTeletextElement_TeletextElement_TeletextElementId"
        FOREIGN KEY ("TeletextElementId") REFERENCES "TeletextElement" ("Id") ON DELETE CASCADE,
    CONSTRAINT "CK_ChannelTeletextElement_Values" CHECK (
        ("PageNumber" IS NULL OR "PageNumber" BETWEEN 256 AND 2303)
        AND "IsEnabled" IN (0, 1))
);
CREATE UNIQUE INDEX "IX_ChannelTeletextElement_ChannelId_TeletextElementId"
    ON "ChannelTeletextElement" ("ChannelId", "TeletextElementId");
CREATE INDEX "IX_ChannelTeletextElement_TeletextElementId"
    ON "ChannelTeletextElement" ("TeletextElementId");
