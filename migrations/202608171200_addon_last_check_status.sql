-- ADR 0066: persist the readiness status returned by `addon.check.v1` so the
-- health report can distinguish a ready add-on from one that last reported a
-- warning or unavailable result. Only the diagnostic code and message were
-- stored before, and the code vocabulary is package-defined, so it cannot be
-- used to infer readiness. This column has no ErsatzTV schema counterpart.

ALTER TABLE "AddonInstallation"
    ADD COLUMN "LastCheckStatus" TEXT NULL;
