-- Dependency resolution state for membership-changing scans.
-- State: 0 = resolving, 1 = resolved, 2 = failed.
CREATE TABLE "ScanRefreshResolution" (
    "LibraryId" INTEGER NOT NULL CONSTRAINT "PK_ScanRefreshResolution" PRIMARY KEY,
    "Generation" INTEGER NOT NULL DEFAULT 1,
    "State" INTEGER NOT NULL DEFAULT 0,
    "Reason" TEXT NOT NULL,
    "Diagnostic" TEXT NULL,
    "StartedAt" TEXT NOT NULL,
    "UpdatedAt" TEXT NOT NULL,
    CONSTRAINT "FK_ScanRefreshResolution_Library_LibraryId"
        FOREIGN KEY ("LibraryId") REFERENCES "Library" ("Id") ON DELETE CASCADE
);

CREATE INDEX "IX_ScanRefreshResolution_StateUpdatedAt"
    ON "ScanRefreshResolution" ("State", "UpdatedAt");

-- Restart-safe Refresh intent retained until the scheduler successfully builds
-- the latest observed generation. State: 0 = pending, 1 = building, 2 = failed.
CREATE TABLE "ScanPlayoutRefreshIntent" (
    "PlayoutId" INTEGER NOT NULL CONSTRAINT "PK_ScanPlayoutRefreshIntent" PRIMARY KEY,
    "LibraryId" INTEGER NULL,
    "Generation" INTEGER NOT NULL DEFAULT 1,
    "State" INTEGER NOT NULL DEFAULT 0,
    "Reason" TEXT NOT NULL,
    "Diagnostic" TEXT NULL,
    "CreatedAt" TEXT NOT NULL,
    "UpdatedAt" TEXT NOT NULL,
    CONSTRAINT "FK_ScanPlayoutRefreshIntent_Playout_PlayoutId"
        FOREIGN KEY ("PlayoutId") REFERENCES "Playout" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_ScanPlayoutRefreshIntent_Library_LibraryId"
        FOREIGN KEY ("LibraryId") REFERENCES "Library" ("Id") ON DELETE SET NULL
);

CREATE INDEX "IX_ScanPlayoutRefreshIntent_StateUpdatedAt"
    ON "ScanPlayoutRefreshIntent" ("State", "UpdatedAt");
CREATE INDEX "IX_ScanPlayoutRefreshIntent_LibraryId"
    ON "ScanPlayoutRefreshIntent" ("LibraryId");
