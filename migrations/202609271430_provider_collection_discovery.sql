-- ErsatzRS extension (ADR 0104): absence is authoritative only for a complete
-- successful discovery root. Legacy collections have no invented root claims.
-- Deleting these links never deletes the referenced collection or its schedules.
CREATE TABLE "ProviderCollectionDiscovery" (
    "Provider" TEXT NOT NULL,
    "MediaSourceId" INTEGER NOT NULL,
    "RootId" TEXT NOT NULL,
    "CollectionId" INTEGER NOT NULL,
    CONSTRAINT "PK_ProviderCollectionDiscovery" PRIMARY KEY ("Provider", "MediaSourceId", "RootId", "CollectionId"),
    CONSTRAINT "FK_ProviderCollectionDiscovery_MediaSource_MediaSourceId" FOREIGN KEY ("MediaSourceId") REFERENCES "MediaSource" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_ProviderCollectionDiscovery_Collection_CollectionId" FOREIGN KEY ("CollectionId") REFERENCES "Collection" ("Id") ON DELETE CASCADE
);
CREATE INDEX "IX_ProviderCollectionDiscovery_CollectionId"
    ON "ProviderCollectionDiscovery" ("CollectionId");
