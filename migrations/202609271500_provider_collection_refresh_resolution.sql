-- Publication records the affected media and pre-publication dependencies in
-- its own transaction. The existing refresh sweep resolves the new dependency
-- graph and transfers this work to ScanPlayoutRefreshIntent atomically.
CREATE TABLE "ProviderCollectionRefreshResolution" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_ProviderCollectionRefreshResolution" PRIMARY KEY AUTOINCREMENT,
    "ChangedMediaIds" TEXT NOT NULL CHECK (json_valid("ChangedMediaIds") AND json_type("ChangedMediaIds") = 'array'),
    "PreviousPlayoutIds" TEXT NOT NULL CHECK (json_valid("PreviousPlayoutIds") AND json_type("PreviousPlayoutIds") = 'array'),
    "Reason" TEXT NOT NULL DEFAULT 'provider-collection-changed',
    "CreatedAt" TEXT NOT NULL
);
