-- ErsatzTV row247: 20261022121255_Reset_MediaServerHdrStatistics.cs.
-- ErsatzRS already stores optional HDR side data, not the old false-by-default
-- HasHdr10Metadata column. Missing persisted side data stays unknown on the wire.
-- Refresh possible provider DV content through ordinary scans; keep all source,
-- track, subtitle-cache and disabled-library state until successful replacement.
-- Also revisit legacy DV records with an absent/nonpositive profile: old zero
-- sentinels could be persisted as positive DV evidence by these adapters.

UPDATE "PlexMovie" SET "Etag" = NULL
WHERE "Id" IN (
    SELECT COALESCE(V."MovieId", V."EpisodeId", V."OtherVideoId")
    FROM "MediaVersion" V
    INNER JOIN "MediaStream" S ON S."MediaVersionId" = V."Id"
    WHERE S."MediaStreamKind" = 1
      AND ((LOWER(S."Codec") IN ('hevc', 'av1')
            AND (S."ColorTransfer" IS NULL
                 OR LOWER(S."ColorTransfer") IN ('', 'smpte2084', 'arib-std-b67')))
           OR (S."DolbyVisionMetadataState" <> 0
               AND COALESCE(S."DolbyVisionProfile", 0) <= 0))
);

UPDATE "PlexEpisode" SET "Etag" = NULL
WHERE "Id" IN (
    SELECT COALESCE(V."MovieId", V."EpisodeId", V."OtherVideoId")
    FROM "MediaVersion" V
    INNER JOIN "MediaStream" S ON S."MediaVersionId" = V."Id"
    WHERE S."MediaStreamKind" = 1
      AND ((LOWER(S."Codec") IN ('hevc', 'av1')
            AND (S."ColorTransfer" IS NULL
                 OR LOWER(S."ColorTransfer") IN ('', 'smpte2084', 'arib-std-b67')))
           OR (S."DolbyVisionMetadataState" <> 0
               AND COALESCE(S."DolbyVisionProfile", 0) <= 0))
);

UPDATE "PlexOtherVideo" SET "Etag" = NULL
WHERE "Id" IN (
    SELECT COALESCE(V."MovieId", V."EpisodeId", V."OtherVideoId")
    FROM "MediaVersion" V
    INNER JOIN "MediaStream" S ON S."MediaVersionId" = V."Id"
    WHERE S."MediaStreamKind" = 1
      AND ((LOWER(S."Codec") IN ('hevc', 'av1')
            AND (S."ColorTransfer" IS NULL
                 OR LOWER(S."ColorTransfer") IN ('', 'smpte2084', 'arib-std-b67')))
           OR (S."DolbyVisionMetadataState" <> 0
               AND COALESCE(S."DolbyVisionProfile", 0) <= 0))
);

UPDATE "EmbyMovie" SET "Etag" = NULL
WHERE "Id" IN (
    SELECT COALESCE(V."MovieId", V."EpisodeId", V."OtherVideoId")
    FROM "MediaVersion" V
    INNER JOIN "MediaStream" S ON S."MediaVersionId" = V."Id"
    WHERE S."MediaStreamKind" = 1
      AND ((LOWER(S."Codec") IN ('hevc', 'av1')
            AND (S."ColorTransfer" IS NULL
                 OR LOWER(S."ColorTransfer") IN ('', 'smpte2084', 'arib-std-b67')))
           OR (S."DolbyVisionMetadataState" <> 0
               AND COALESCE(S."DolbyVisionProfile", 0) <= 0))
);

UPDATE "EmbyEpisode" SET "Etag" = NULL
WHERE "Id" IN (
    SELECT COALESCE(V."MovieId", V."EpisodeId", V."OtherVideoId")
    FROM "MediaVersion" V
    INNER JOIN "MediaStream" S ON S."MediaVersionId" = V."Id"
    WHERE S."MediaStreamKind" = 1
      AND ((LOWER(S."Codec") IN ('hevc', 'av1')
            AND (S."ColorTransfer" IS NULL
                 OR LOWER(S."ColorTransfer") IN ('', 'smpte2084', 'arib-std-b67')))
           OR (S."DolbyVisionMetadataState" <> 0
               AND COALESCE(S."DolbyVisionProfile", 0) <= 0))
);

UPDATE "JellyfinMovie" SET "Etag" = NULL
WHERE "Id" IN (
    SELECT COALESCE(V."MovieId", V."EpisodeId", V."OtherVideoId")
    FROM "MediaVersion" V
    INNER JOIN "MediaStream" S ON S."MediaVersionId" = V."Id"
    WHERE S."MediaStreamKind" = 1
      AND ((LOWER(S."Codec") IN ('hevc', 'av1')
            AND (S."ColorTransfer" IS NULL
                 OR LOWER(S."ColorTransfer") IN ('', 'smpte2084', 'arib-std-b67')))
           OR (S."DolbyVisionMetadataState" <> 0
               AND COALESCE(S."DolbyVisionProfile", 0) <= 0))
);

UPDATE "JellyfinEpisode" SET "Etag" = NULL
WHERE "Id" IN (
    SELECT COALESCE(V."MovieId", V."EpisodeId", V."OtherVideoId")
    FROM "MediaVersion" V
    INNER JOIN "MediaStream" S ON S."MediaVersionId" = V."Id"
    WHERE S."MediaStreamKind" = 1
      AND ((LOWER(S."Codec") IN ('hevc', 'av1')
            AND (S."ColorTransfer" IS NULL
                 OR LOWER(S."ColorTransfer") IN ('', 'smpte2084', 'arib-std-b67')))
           OR (S."DolbyVisionMetadataState" <> 0
               AND COALESCE(S."DolbyVisionProfile", 0) <= 0))
);
