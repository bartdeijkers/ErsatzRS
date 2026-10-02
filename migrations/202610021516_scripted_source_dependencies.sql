-- Successful Scripted builds publish the actual runtime-resolved sources.
-- Keep the last published selection and per-library observations together;
-- failed builds cannot replace dependency ownership for the existing timeline.
CREATE TABLE "PlayoutScriptedSource" (
    "PlayoutId" INTEGER NOT NULL,
    "SourceKey" TEXT NOT NULL,
    "Context" TEXT NOT NULL,
    CONSTRAINT "PK_PlayoutScriptedSource" PRIMARY KEY ("PlayoutId", "SourceKey"),
    CONSTRAINT "FK_PlayoutScriptedSource_Playout_PlayoutId"
        FOREIGN KEY ("PlayoutId") REFERENCES "Playout" ("Id") ON DELETE CASCADE
);
