-- Optional display context; old readiness results remain valid without it.
-- Add-on readiness is an ErsatzRS extension without an ErsatzTV counterpart.
ALTER TABLE "AddonInstallation" ADD COLUMN "LastCheckToolUpdateJson" TEXT NULL;
