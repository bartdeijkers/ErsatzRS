-- ErsatzRS deviation from ErsatzTV: ADR 0049 replaces the ambiguous
-- NormalizeAudio identifier with TranscodeAudio. Preserve every existing value.
ALTER TABLE `FFmpegProfile` RENAME COLUMN `NormalizeAudio` TO `TranscodeAudio`;
