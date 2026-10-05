-- Existing ID-only associations keep their startup/default variables.
-- Explicit programme-item overrides are copied into generated playout rows.
ALTER TABLE "ProgramScheduleItemGraphicsElement" ADD COLUMN "Variables" TEXT NULL;
ALTER TABLE "BlockItemGraphicsElement" ADD COLUMN "Variables" TEXT NULL;
