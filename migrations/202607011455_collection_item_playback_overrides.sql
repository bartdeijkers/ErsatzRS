-- ErsatzRS extension; no direct ErsatzTV migration exists.
-- ADR 0048 adds per collection/playlist item playback preferences so admins
-- can schedule the same media item with alternate audio/subtitle selections.
ALTER TABLE "CollectionItem" ADD COLUMN "PreferredAudioLanguageCode" TEXT NULL;
ALTER TABLE "CollectionItem" ADD COLUMN "PreferredAudioTitle" TEXT NULL;
ALTER TABLE "CollectionItem" ADD COLUMN "PreferredSubtitleLanguageCode" TEXT NULL;
ALTER TABLE "CollectionItem" ADD COLUMN "SubtitleMode" INTEGER NULL;

ALTER TABLE "PlaylistItem" ADD COLUMN "PreferredAudioLanguageCode" TEXT NULL;
ALTER TABLE "PlaylistItem" ADD COLUMN "PreferredAudioTitle" TEXT NULL;
ALTER TABLE "PlaylistItem" ADD COLUMN "PreferredSubtitleLanguageCode" TEXT NULL;
ALTER TABLE "PlaylistItem" ADD COLUMN "SubtitleMode" INTEGER NULL;
