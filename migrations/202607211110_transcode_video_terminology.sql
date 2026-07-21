-- ErsatzRS deviation from ErsatzTV: ADR 0050 replaces the ambiguous
-- NormalizeVideo identifier with TranscodeVideo. Preserve every existing value.
ALTER TABLE "FFmpegProfile" RENAME COLUMN "NormalizeVideo" TO "TranscodeVideo";
