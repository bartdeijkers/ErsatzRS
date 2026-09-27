-- ErsatzTV SQLite 20260831123022_Add_PlayoutHistory_Seed.
-- Existing history predates cycle seeds and deliberately retains NULL.
ALTER TABLE "PlayoutHistory" ADD COLUMN "Seed" INTEGER NULL;
