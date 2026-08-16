-- ErsatzRS extension; Schatkamer chapter fragments have no direct ErsatzTV
-- provider-list equivalent. Reuse the existing MediaChapter semantics while
-- retaining add-on item identity for independently selectable Remote Streams.
ALTER TABLE "AddonMediaList" ADD COLUMN "ChapterInput" TEXT NULL;

ALTER TABLE "AddonMediaListItem" ADD COLUMN "DurationSeconds" INTEGER NULL;
ALTER TABLE "AddonMediaListItem" ADD COLUMN "ParentProviderId" TEXT NULL;
ALTER TABLE "AddonMediaListItem" ADD COLUMN "FragmentStartSeconds" INTEGER NULL;
ALTER TABLE "AddonMediaListItem" ADD COLUMN "FragmentEndSeconds" INTEGER NULL;
ALTER TABLE "AddonMediaListItem" ADD COLUMN "AdditionalImageUrls" TEXT NULL;

CREATE TABLE "AddonMediaListChapter" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListChapter" PRIMARY KEY AUTOINCREMENT,
    "AddonMediaListId" INTEGER NOT NULL,
    "SourceItemId" INTEGER NOT NULL,
    "FragmentItemId" INTEGER NOT NULL,
    "ChapterIndex" INTEGER NOT NULL,
    "Title" TEXT NOT NULL,
    "StartSeconds" INTEGER NOT NULL,
    "EndSeconds" INTEGER NULL,
    CONSTRAINT "FK_AMLChapter_AML_AddonMediaListId" FOREIGN KEY ("AddonMediaListId") REFERENCES "AddonMediaList" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_AMLChapter_Item_SourceItemId" FOREIGN KEY ("SourceItemId") REFERENCES "AddonMediaListItem" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_AMLChapter_Item_FragmentItemId" FOREIGN KEY ("FragmentItemId") REFERENCES "AddonMediaListItem" ("Id") ON DELETE CASCADE
);

CREATE UNIQUE INDEX "IX_AddonMediaListChapter_FragmentItemId" ON "AddonMediaListChapter" ("FragmentItemId");
CREATE UNIQUE INDEX "IX_AddonMediaListChapter_SourceItemId_ChapterIndex" ON "AddonMediaListChapter" ("SourceItemId", "ChapterIndex");
