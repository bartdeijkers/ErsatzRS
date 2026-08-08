-- Operator-editable HDR10 static metadata defaults (ADR 0064).
--
-- ADR 0064 requires the mastering-display and content-light values ErsatzRS
-- signals to be documented *and* editable. Until now they were documented
-- constants with no profile field.
--
-- Additive and NULL by default, so every existing profile keeps emitting the
-- documented `BT2020_1000_NITS` / 1000,400 pair. NULL means "use the documented
-- default"; it is not the same as an empty override.
--
-- These never override metadata a source carried itself — ADR 0064 prefers a
-- real grade over a configured one — they replace the seeded default used when
-- an HDR10 encode has to synthesize metadata (HLG and SDR sources).
ALTER TABLE `FFmpegProfile`
    ADD COLUMN `HdrMasteringDisplay` VARCHAR(255) NULL;
ALTER TABLE `FFmpegProfile`
    ADD COLUMN `HdrMaxCll` INT NULL;
ALTER TABLE `FFmpegProfile`
    ADD COLUMN `HdrMaxFall` INT NULL;
