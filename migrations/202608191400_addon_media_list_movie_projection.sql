-- ErsatzRS extension: project an add-on list into the normal Movie hierarchy
-- while retaining its Remote Stream playback source, mirroring the television
-- projection from ADR 0071. A movie list has no parent container, so only the
-- per-item link exists.

CREATE TABLE "AddonMediaListItemMovie" (
    "AddonMediaListItemId" INTEGER NOT NULL CONSTRAINT "PK_AddonMediaListItemMovie" PRIMARY KEY,
    "MovieId" INTEGER NOT NULL,
    CONSTRAINT "FK_AddonMediaListItemMovie_AddonMediaListItem_AddonMediaListItemId"
        FOREIGN KEY ("AddonMediaListItemId") REFERENCES "AddonMediaListItem" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_AddonMediaListItemMovie_Movie_MovieId"
        FOREIGN KEY ("MovieId") REFERENCES "Movie" ("Id") ON DELETE CASCADE
);

CREATE UNIQUE INDEX "IX_AddonMediaListItemMovie_MovieId"
    ON "AddonMediaListItemMovie" ("MovieId");
