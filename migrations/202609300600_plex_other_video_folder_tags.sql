-- Revisit existing Plex Other Videos once after portable folder-tag projection.
-- Preserve all media, tags, subtitle/audio metadata and playout state. Only the
-- provider cache and matching library scan cursors become stale (upstream #3015).
UPDATE "PlexOtherVideo" SET "Etag" = NULL;
UPDATE "Library" SET "LastScan" = '1970-01-01 00:00:00'
WHERE "MediaKind" = 4 AND "Id" IN (SELECT "Id" FROM "PlexLibrary");
