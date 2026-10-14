-- ADR 0122: Scripted overrides are accepted with the actual playout timeline.
-- No profile/install/binding FK: removal leaves an active snapshot visibly
-- unavailable until its original expiry, rather than silently restoring base.
CREATE TABLE "PlayoutTeletextInterval" (
    "PlayoutId" INTEGER NOT NULL,
    "AddonId" TEXT NOT NULL,
    "Start" TEXT NOT NULL,
    "Finish" TEXT NOT NULL,
    "State" INTEGER NOT NULL,
    "CandidateSnapshot" TEXT NULL,
    CONSTRAINT "PK_PlayoutTeletextInterval" PRIMARY KEY ("PlayoutId", "AddonId", "Start"),
    CONSTRAINT "FK_PlayoutTeletextInterval_Playout_PlayoutId"
        FOREIGN KEY ("PlayoutId") REFERENCES "Playout" ("Id") ON DELETE CASCADE,
    CONSTRAINT "CK_PlayoutTeletextInterval_AddonId" CHECK (
        TYPEOF("AddonId") = 'text' AND LENGTH(CAST("AddonId" AS BLOB)) BETWEEN 1 AND 256),
    CONSTRAINT "CK_PlayoutTeletextInterval_Bounds" CHECK (
        TYPEOF("Start") = 'text' AND TYPEOF("Finish") = 'text' AND "Start" < "Finish"
        AND LENGTH(CAST("Start" AS BLOB)) BETWEEN 1 AND 64
        AND LENGTH(CAST("Finish" AS BLOB)) BETWEEN 1 AND 64),
    CONSTRAINT "CK_PlayoutTeletextInterval_State" CHECK (
        TYPEOF("State") = 'integer' AND
        (("State" = 0 AND "CandidateSnapshot" IS NULL) OR
         ("State" = 1 AND TYPEOF("CandidateSnapshot") = 'text' AND
          LENGTH(CAST("CandidateSnapshot" AS BLOB)) BETWEEN 1 AND 25231360)))
);
CREATE INDEX "IX_PlayoutTeletextInterval_PlayoutId_Start_Finish"
    ON "PlayoutTeletextInterval" ("PlayoutId", "Start", "Finish");
