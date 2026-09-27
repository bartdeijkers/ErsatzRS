-- ErsatzTV #2990: retain unclaimed Plex servers and their synchronized content.
-- Existing sources remain available until a successful inventory marks them missing.
ALTER TABLE "PlexMediaSource" ADD COLUMN "MissingSince" TEXT NULL;
