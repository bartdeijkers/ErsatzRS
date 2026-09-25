-- A wrapped index of zero does not mean an enumerator has never advanced.
-- Existing states preserve their prior default until the next build advances them.
ALTER TABLE "ScheduleItemsEnumeratorState" ADD COLUMN "Started" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "FillGroupEnumeratorState" ADD COLUMN "Started" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "CollectionEnumeratorState" ADD COLUMN "Started" INTEGER NOT NULL DEFAULT 0;
