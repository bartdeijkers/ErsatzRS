-- Repair exact duplicate artwork rows owned by add-on-projected television.
--
-- This deliberately does not impose a uniqueness constraint: operators and
-- providers may retain distinct artwork variants for the same owner and kind.
-- Rows are repairable only when every persisted identity/payload field matches
-- (timestamps are provenance, not artwork identity). If any row in an exact
-- duplicate set is referenced by Actor.ArtworkId, leave the whole set intact
-- so the repair can never cascade-delete actor metadata.
CREATE TEMP TABLE "_ArtworkCardinalityRepair" (
    "ArtworkId" INTEGER NOT NULL PRIMARY KEY,
    "Repairable" INTEGER NOT NULL
);

INSERT INTO "_ArtworkCardinalityRepair" ("ArtworkId", "Repairable")
SELECT candidate."Id",
       CASE WHEN EXISTS (
           SELECT 1
           FROM "Artwork" AS member
           JOIN "Actor" AS actor ON actor."ArtworkId" = member."Id"
           WHERE member."ArtworkKind" = candidate."ArtworkKind"
             AND member."ArtistMetadataId" IS candidate."ArtistMetadataId"
             AND member."ChannelId" IS candidate."ChannelId"
             AND member."EpisodeMetadataId" IS candidate."EpisodeMetadataId"
             AND member."ImageMetadataId" IS candidate."ImageMetadataId"
             AND member."MovieMetadataId" IS candidate."MovieMetadataId"
             AND member."MusicVideoMetadataId" IS candidate."MusicVideoMetadataId"
             AND member."OtherVideoMetadataId" IS candidate."OtherVideoMetadataId"
             AND member."RemoteStreamMetadataId" IS candidate."RemoteStreamMetadataId"
             AND member."SeasonMetadataId" IS candidate."SeasonMetadataId"
             AND member."ShowMetadataId" IS candidate."ShowMetadataId"
             AND member."SongMetadataId" IS candidate."SongMetadataId"
             AND member."Path" IS candidate."Path"
             AND member."SourcePath" IS candidate."SourcePath"
             AND member."BlurHash43" IS candidate."BlurHash43"
             AND member."BlurHash54" IS candidate."BlurHash54"
             AND member."BlurHash64" IS candidate."BlurHash64"
             AND member."OriginalContentType" IS candidate."OriginalContentType"
       ) THEN 0 ELSE 1 END
FROM "Artwork" AS candidate
WHERE (
    (candidate."ArtistMetadataId" IS NOT NULL)
    + (candidate."ChannelId" IS NOT NULL)
    + (candidate."EpisodeMetadataId" IS NOT NULL)
    + (candidate."ImageMetadataId" IS NOT NULL)
    + (candidate."MovieMetadataId" IS NOT NULL)
    + (candidate."MusicVideoMetadataId" IS NOT NULL)
    + (candidate."OtherVideoMetadataId" IS NOT NULL)
    + (candidate."RemoteStreamMetadataId" IS NOT NULL)
    + (candidate."SeasonMetadataId" IS NOT NULL)
    + (candidate."ShowMetadataId" IS NOT NULL)
    + (candidate."SongMetadataId" IS NOT NULL)
  ) = 1
  AND (
      (
        candidate."ShowMetadataId" IS NOT NULL
        AND EXISTS (
            SELECT 1
            FROM "ShowMetadata" AS metadata
            JOIN "AddonMediaListTelevisionShow" AS projection
              ON projection."ShowId" = metadata."ShowId"
            WHERE metadata."Id" = candidate."ShowMetadataId"
        )
      )
   OR (
        candidate."SeasonMetadataId" IS NOT NULL
        AND EXISTS (
            SELECT 1
            FROM "SeasonMetadata" AS metadata
            JOIN "Season" AS season ON season."Id" = metadata."SeasonId"
            JOIN "AddonMediaListTelevisionShow" AS projection
              ON projection."ShowId" = season."ShowId"
            WHERE metadata."Id" = candidate."SeasonMetadataId"
        )
      )
   OR (
        candidate."EpisodeMetadataId" IS NOT NULL
        AND EXISTS (
            SELECT 1
            FROM "EpisodeMetadata" AS metadata
            JOIN "AddonMediaListItemEpisode" AS projection
              ON projection."EpisodeId" = metadata."EpisodeId"
            WHERE metadata."Id" = candidate."EpisodeMetadataId"
        )
      )
    )
  AND EXISTS (
      SELECT 1
      FROM "Artwork" AS survivor
      WHERE survivor."Id" < candidate."Id"
        AND survivor."ArtworkKind" = candidate."ArtworkKind"
        AND survivor."ArtistMetadataId" IS candidate."ArtistMetadataId"
        AND survivor."ChannelId" IS candidate."ChannelId"
        AND survivor."EpisodeMetadataId" IS candidate."EpisodeMetadataId"
        AND survivor."ImageMetadataId" IS candidate."ImageMetadataId"
        AND survivor."MovieMetadataId" IS candidate."MovieMetadataId"
        AND survivor."MusicVideoMetadataId" IS candidate."MusicVideoMetadataId"
        AND survivor."OtherVideoMetadataId" IS candidate."OtherVideoMetadataId"
        AND survivor."RemoteStreamMetadataId" IS candidate."RemoteStreamMetadataId"
        AND survivor."SeasonMetadataId" IS candidate."SeasonMetadataId"
        AND survivor."ShowMetadataId" IS candidate."ShowMetadataId"
        AND survivor."SongMetadataId" IS candidate."SongMetadataId"
        AND survivor."Path" IS candidate."Path"
        AND survivor."SourcePath" IS candidate."SourcePath"
        AND survivor."BlurHash43" IS candidate."BlurHash43"
        AND survivor."BlurHash54" IS candidate."BlurHash54"
        AND survivor."BlurHash64" IS candidate."BlurHash64"
        AND survivor."OriginalContentType" IS candidate."OriginalContentType"
  );

DELETE FROM "ConfigElement"
WHERE "Key" = 'migration.artwork_cardinality_repair';

INSERT INTO "ConfigElement" ("Key", "Value")
SELECT 'migration.artwork_cardinality_repair',
       '{"removedRows":'
       || CAST(COALESCE(SUM(CASE WHEN "Repairable" = 1 THEN 1 ELSE 0 END), 0) AS TEXT)
       || ',"skippedReferencedRows":'
       || CAST(COALESCE(SUM(CASE WHEN "Repairable" = 0 THEN 1 ELSE 0 END), 0) AS TEXT)
       || '}'
FROM "_ArtworkCardinalityRepair";

DELETE FROM "Artwork"
WHERE "Id" IN (
    SELECT "ArtworkId"
    FROM "_ArtworkCardinalityRepair"
    WHERE "Repairable" = 1
);

DROP TABLE "_ArtworkCardinalityRepair";
