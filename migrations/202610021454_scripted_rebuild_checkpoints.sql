-- Scripted state at published item boundaries. Legacy timelines have no rows;
-- continuity-safe Refresh refuses them instead of silently resetting rotation.
CREATE TABLE "PlayoutScriptedCheckpoint" (
    "PlayoutId" INTEGER NOT NULL,
    "CheckpointDate" TEXT NOT NULL,
    "Context" TEXT NOT NULL,
    CONSTRAINT "PK_PlayoutScriptedCheckpoint" PRIMARY KEY ("PlayoutId", "CheckpointDate"),
    CONSTRAINT "FK_PlayoutScriptedCheckpoint_Playout_PlayoutId"
        FOREIGN KEY ("PlayoutId") REFERENCES "Playout" ("Id") ON DELETE CASCADE
);
