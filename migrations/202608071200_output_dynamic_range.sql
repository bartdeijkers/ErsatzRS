-- ErsatzRS explicit SDR/HDR10 output contract (ADR 0064).
--
-- Additive only. Every pre-existing row defaults to 0 (`SdrBt709`), which is
-- exactly the behavior the pipeline already produced: transcoded HDR sources
-- were unconditionally tone-mapped to BT.709, including for the two seeded
-- profiles named `hdr`. Fresh ADR 0017 HDR seeds are set to 1
-- (`Hdr10Bt2020Pq`) by the seeding code, not by this migration, so upgrading
-- an existing database never changes what a channel already streams.
ALTER TABLE "FFmpegProfile"
    ADD COLUMN "OutputDynamicRange" INTEGER NOT NULL DEFAULT 0;
