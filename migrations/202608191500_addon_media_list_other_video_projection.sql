-- ErsatzRS extension: project an add-on list into the normal Other Video
-- hierarchy while retaining its Remote Stream playback source. Other Videos
-- have no parent container, so this mirrors the Movie projection exactly.

CREATE TABLE "AddonMediaListItemOtherVideo" (
    "AddonMediaListItemId" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListItemOtherVideo" PRIMARY KEY,
    "OtherVideoId" INTEGER NOT NULL,
    CONSTRAINT "FK_AddonMediaListItemOtherVideo_AddonMediaListItem_AddonMediaListItemId"
        FOREIGN KEY ("AddonMediaListItemId") REFERENCES "AddonMediaListItem" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_AddonMediaListItemOtherVideo_OtherVideo_OtherVideoId"
        FOREIGN KEY ("OtherVideoId") REFERENCES "OtherVideo" ("Id") ON DELETE CASCADE
);

CREATE UNIQUE INDEX "IX_AddonMediaListItemOtherVideo_OtherVideoId"
    ON "AddonMediaListItemOtherVideo" ("OtherVideoId");
