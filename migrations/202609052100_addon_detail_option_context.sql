-- ADR 0085: bind the existing detail queue to saved or published options.
-- Legacy checkpoints remain readable only while no option schema is bound.
ALTER TABLE "AddonMediaListDetailJob" ADD COLUMN "OptionPurpose" INTEGER NOT NULL DEFAULT 0
    CHECK ("OptionPurpose" IN (0, 1));
ALTER TABLE "AddonMediaListDetailJob" ADD COLUMN "OptionContextJson" TEXT NULL;
