-- ErsatzRS extension: persist an exact media-version and stream combination
-- for reusable playlist, schedule, and materialized playout items.
-- These columns intentionally have no foreign keys so a rescan can leave a
-- stale selection visible to validation and health reporting.

ALTER TABLE "PlaylistItem" ADD COLUMN "MediaVersionId" INTEGER NULL;
ALTER TABLE "PlaylistItem" ADD COLUMN "VideoStreamIndex" INTEGER NULL;
ALTER TABLE "PlaylistItem" ADD COLUMN "AudioStreamIndex" INTEGER NULL;
ALTER TABLE "PlaylistItem" ADD COLUMN "SubtitleStreamIndex" INTEGER NULL;

ALTER TABLE "ProgramScheduleItem" ADD COLUMN "MediaVersionId" INTEGER NULL;
ALTER TABLE "ProgramScheduleItem" ADD COLUMN "VideoStreamIndex" INTEGER NULL;
ALTER TABLE "ProgramScheduleItem" ADD COLUMN "AudioStreamIndex" INTEGER NULL;
ALTER TABLE "ProgramScheduleItem" ADD COLUMN "SubtitleStreamIndex" INTEGER NULL;

ALTER TABLE "PlayoutItem" ADD COLUMN "MediaVersionId" INTEGER NULL;
ALTER TABLE "PlayoutItem" ADD COLUMN "VideoStreamIndex" INTEGER NULL;
ALTER TABLE "PlayoutItem" ADD COLUMN "AudioStreamIndex" INTEGER NULL;
ALTER TABLE "PlayoutItem" ADD COLUMN "SubtitleStreamIndex" INTEGER NULL;
