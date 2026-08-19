-- ErsatzRS extension: project an add-on list into the normal Image hierarchy
-- while retaining its Remote Stream source. Images have no parent container, so
-- this mirrors the Movie, Other Video, and Song projections.

CREATE TABLE "AddonMediaListItemImage" (
    "AddonMediaListItemId" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListItemImage" PRIMARY KEY,
    "ImageId" INTEGER NOT NULL,
    CONSTRAINT "FK_AddonMediaListItemImage_AddonMediaListItem_AddonMediaListItemId"
        FOREIGN KEY ("AddonMediaListItemId") REFERENCES "AddonMediaListItem" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_AddonMediaListItemImage_Image_ImageId"
        FOREIGN KEY ("ImageId") REFERENCES "Image" ("Id") ON DELETE CASCADE
);

CREATE UNIQUE INDEX "IX_AddonMediaListItemImage_ImageId"
    ON "AddonMediaListItemImage" ("ImageId");
