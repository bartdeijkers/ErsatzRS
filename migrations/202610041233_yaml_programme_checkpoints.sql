-- ADR 0110: YAML instruction/source state at published programme boundaries.
-- This additive ErsatzRS checkpoint contract has no upstream schema equivalent.
-- Legacy timelines remain readable; preserving Refresh requires a new build.
CREATE TABLE "PlayoutYamlCheckpoint" (
    "PlayoutId" INTEGER NOT NULL,
    "CheckpointDate" TEXT NOT NULL,
    "Context" TEXT NOT NULL,
    CONSTRAINT "PK_PlayoutYamlCheckpoint" PRIMARY KEY ("PlayoutId", "CheckpointDate"),
    CONSTRAINT "FK_PlayoutYamlCheckpoint_Playout_PlayoutId"
        FOREIGN KEY ("PlayoutId") REFERENCES "Playout" ("Id") ON DELETE CASCADE
);
