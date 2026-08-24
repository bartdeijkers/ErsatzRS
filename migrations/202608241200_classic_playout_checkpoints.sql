-- ErsatzRS-owned complete Classic Refresh state. Dated program-schedule
-- anchors retain the ErsatzTV-compatible per-source state; this row proves
-- that the corresponding scheduler-wide checkpoint is complete before a
-- destructive Refresh can replace the materialized window.
CREATE TABLE "PlayoutClassicCheckpoint" (
    "PlayoutId" INTEGER NOT NULL,
    "CheckpointDate" TEXT NOT NULL,
    "Context" TEXT NOT NULL,
    CONSTRAINT "PK_PlayoutClassicCheckpoint" PRIMARY KEY ("PlayoutId", "CheckpointDate"),
    CONSTRAINT "FK_PlayoutClassicCheckpoint_Playout_PlayoutId"
        FOREIGN KEY ("PlayoutId") REFERENCES "Playout" ("Id") ON DELETE CASCADE
);
