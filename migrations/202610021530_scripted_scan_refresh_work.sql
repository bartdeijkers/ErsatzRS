-- Scan completion and bounded source comparison use the existing refresh sweep.
-- Keep observed selections separate: Continue must not acknowledge a changed
-- source for its retained future, and repeated no-change scans must coalesce.
ALTER TABLE "PlayoutScriptedSource" ADD COLUMN "ObservedLibraries" TEXT NOT NULL DEFAULT '{}';
UPDATE "PlayoutScriptedSource"
SET "ObservedLibraries" = COALESCE(json_extract("Context", '$.dependency.library_fingerprints'), '{}');

CREATE TABLE "ScriptedLibraryRevision" (
    "LibraryId" INTEGER NOT NULL CONSTRAINT "PK_ScriptedLibraryRevision" PRIMARY KEY,
    "Revision" INTEGER NOT NULL,
    "UpdatedAt" TEXT NOT NULL,
    CONSTRAINT "FK_ScriptedLibraryRevision_Library_LibraryId"
        FOREIGN KEY ("LibraryId") REFERENCES "Library" ("Id") ON DELETE CASCADE
);
CREATE TABLE "ScriptedSourceRefreshWork" (
    "LibraryId" INTEGER NOT NULL,
    "PlayoutId" INTEGER NOT NULL,
    "Revision" INTEGER NOT NULL,
    "Attempts" INTEGER NOT NULL DEFAULT 0,
    "NextAttempt" TEXT NOT NULL,
    "Diagnostic" TEXT NULL,
    CONSTRAINT "PK_ScriptedSourceRefreshWork" PRIMARY KEY ("LibraryId", "PlayoutId"),
    CONSTRAINT "FK_ScriptedSourceRefreshWork_Library_LibraryId"
        FOREIGN KEY ("LibraryId") REFERENCES "Library" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_ScriptedSourceRefreshWork_Playout_PlayoutId"
        FOREIGN KEY ("PlayoutId") REFERENCES "Playout" ("Id") ON DELETE CASCADE
);
CREATE INDEX "IX_ScriptedSourceRefreshWork_NextAttempt" ON "ScriptedSourceRefreshWork" ("NextAttempt");
