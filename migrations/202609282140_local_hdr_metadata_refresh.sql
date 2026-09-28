-- UCU-09 row143: refresh unchanged local PQ/HLG files for HDR/Dolby Vision
-- metadata. Preserve media/stream/subtitle identities and scheduling state.
-- Reset both upstream scan gates; local TV uses the folder ETag fast path.
-- The migration runs once; subsequent successful scans republish fresh gates.
UPDATE "MediaVersion" SET "DateUpdated" = '1970-01-01 00:00:00'
WHERE "Id" IN (
    SELECT MF."MediaVersionId" FROM "MediaFile" MF
    INNER JOIN "LibraryFolder" LF ON LF."Id" = MF."LibraryFolderId"
    INNER JOIN "LibraryPath" LP ON LP."Id" = LF."LibraryPathId"
    INNER JOIN "LocalLibrary" LL ON LL."Id" = LP."LibraryId"
    INNER JOIN "MediaStream" MS ON MS."MediaVersionId" = MF."MediaVersionId"
    WHERE MS."MediaStreamKind" = 1
      AND LOWER(MS."ColorTransfer") IN ('smpte2084', 'arib-std-b67')
);

UPDATE "LibraryFolder" SET "Etag" = NULL
WHERE "Id" IN (
    SELECT MF."LibraryFolderId" FROM "MediaFile" MF
    INNER JOIN "LibraryFolder" LF ON LF."Id" = MF."LibraryFolderId"
    INNER JOIN "LibraryPath" LP ON LP."Id" = LF."LibraryPathId"
    INNER JOIN "LocalLibrary" LL ON LL."Id" = LP."LibraryId"
    INNER JOIN "MediaStream" MS ON MS."MediaVersionId" = MF."MediaVersionId"
    WHERE MS."MediaStreamKind" = 1
      AND LOWER(MS."ColorTransfer") IN ('smpte2084', 'arib-std-b67')
);
