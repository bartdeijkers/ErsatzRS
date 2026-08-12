-- ADR 0066: persist add-on provenance and health-check timing for the mature
-- management UI. These columns have no ErsatzTV schema counterpart.

ALTER TABLE `AddonInstallation`
    ADD COLUMN `TrustTier` VARCHAR(32) NOT NULL DEFAULT 'verified';

ALTER TABLE `AddonInstallation`
    ADD COLUMN `LastCheckAt` LONGTEXT NULL;
