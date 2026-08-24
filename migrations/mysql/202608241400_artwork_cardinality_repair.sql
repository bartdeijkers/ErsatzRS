-- Repair exact duplicate artwork rows owned by add-on-projected television.
-- Distinct variants remain valid; only exact persisted payload matches qualify.
-- Actor-referenced duplicate sets are retained to avoid ON DELETE CASCADE loss.
DROP TEMPORARY TABLE IF EXISTS `_ArtworkCardinalityRepair`;
CREATE TEMPORARY TABLE `_ArtworkCardinalityRepair` (
    `ArtworkId` INT NOT NULL,
    `Repairable` TINYINT NOT NULL,
    CONSTRAINT `PK_ArtworkCardinalityRepair` PRIMARY KEY (`ArtworkId`)
) ENGINE=InnoDB;

INSERT INTO `_ArtworkCardinalityRepair` (`ArtworkId`, `Repairable`)
SELECT candidate.`Id`,
       CASE WHEN EXISTS (
           SELECT 1
           FROM `Artwork` AS member
           JOIN `Actor` AS actor ON actor.`ArtworkId` = member.`Id`
           WHERE member.`ArtworkKind` = candidate.`ArtworkKind`
             AND member.`ArtistMetadataId` <=> candidate.`ArtistMetadataId`
             AND member.`ChannelId` <=> candidate.`ChannelId`
             AND member.`EpisodeMetadataId` <=> candidate.`EpisodeMetadataId`
             AND member.`ImageMetadataId` <=> candidate.`ImageMetadataId`
             AND member.`MovieMetadataId` <=> candidate.`MovieMetadataId`
             AND member.`MusicVideoMetadataId` <=> candidate.`MusicVideoMetadataId`
             AND member.`OtherVideoMetadataId` <=> candidate.`OtherVideoMetadataId`
             AND member.`RemoteStreamMetadataId` <=> candidate.`RemoteStreamMetadataId`
             AND member.`SeasonMetadataId` <=> candidate.`SeasonMetadataId`
             AND member.`ShowMetadataId` <=> candidate.`ShowMetadataId`
             AND member.`SongMetadataId` <=> candidate.`SongMetadataId`
             AND BINARY member.`Path` <=> BINARY candidate.`Path`
             AND BINARY member.`SourcePath` <=> BINARY candidate.`SourcePath`
             AND BINARY member.`BlurHash43` <=> BINARY candidate.`BlurHash43`
             AND BINARY member.`BlurHash54` <=> BINARY candidate.`BlurHash54`
             AND BINARY member.`BlurHash64` <=> BINARY candidate.`BlurHash64`
             AND BINARY member.`OriginalContentType` <=> BINARY candidate.`OriginalContentType`
       ) THEN 0 ELSE 1 END
FROM `Artwork` AS candidate
WHERE (
    (candidate.`ArtistMetadataId` IS NOT NULL)
    + (candidate.`ChannelId` IS NOT NULL)
    + (candidate.`EpisodeMetadataId` IS NOT NULL)
    + (candidate.`ImageMetadataId` IS NOT NULL)
    + (candidate.`MovieMetadataId` IS NOT NULL)
    + (candidate.`MusicVideoMetadataId` IS NOT NULL)
    + (candidate.`OtherVideoMetadataId` IS NOT NULL)
    + (candidate.`RemoteStreamMetadataId` IS NOT NULL)
    + (candidate.`SeasonMetadataId` IS NOT NULL)
    + (candidate.`ShowMetadataId` IS NOT NULL)
    + (candidate.`SongMetadataId` IS NOT NULL)
  ) = 1
  AND (
      (
        candidate.`ShowMetadataId` IS NOT NULL
        AND EXISTS (
            SELECT 1
            FROM `ShowMetadata` AS metadata
            JOIN `AddonMediaListTelevisionShow` AS projection
              ON projection.`ShowId` = metadata.`ShowId`
            WHERE metadata.`Id` = candidate.`ShowMetadataId`
        )
      )
   OR (
        candidate.`SeasonMetadataId` IS NOT NULL
        AND EXISTS (
            SELECT 1
            FROM `SeasonMetadata` AS metadata
            JOIN `Season` AS season ON season.`Id` = metadata.`SeasonId`
            JOIN `AddonMediaListTelevisionShow` AS projection
              ON projection.`ShowId` = season.`ShowId`
            WHERE metadata.`Id` = candidate.`SeasonMetadataId`
        )
      )
   OR (
        candidate.`EpisodeMetadataId` IS NOT NULL
        AND EXISTS (
            SELECT 1
            FROM `EpisodeMetadata` AS metadata
            JOIN `AddonMediaListItemEpisode` AS projection
              ON projection.`EpisodeId` = metadata.`EpisodeId`
            WHERE metadata.`Id` = candidate.`EpisodeMetadataId`
        )
      )
    )
  AND EXISTS (
      SELECT 1
      FROM `Artwork` AS survivor
      WHERE survivor.`Id` < candidate.`Id`
        AND survivor.`ArtworkKind` = candidate.`ArtworkKind`
        AND survivor.`ArtistMetadataId` <=> candidate.`ArtistMetadataId`
        AND survivor.`ChannelId` <=> candidate.`ChannelId`
        AND survivor.`EpisodeMetadataId` <=> candidate.`EpisodeMetadataId`
        AND survivor.`ImageMetadataId` <=> candidate.`ImageMetadataId`
        AND survivor.`MovieMetadataId` <=> candidate.`MovieMetadataId`
        AND survivor.`MusicVideoMetadataId` <=> candidate.`MusicVideoMetadataId`
        AND survivor.`OtherVideoMetadataId` <=> candidate.`OtherVideoMetadataId`
        AND survivor.`RemoteStreamMetadataId` <=> candidate.`RemoteStreamMetadataId`
        AND survivor.`SeasonMetadataId` <=> candidate.`SeasonMetadataId`
        AND survivor.`ShowMetadataId` <=> candidate.`ShowMetadataId`
        AND survivor.`SongMetadataId` <=> candidate.`SongMetadataId`
        AND BINARY survivor.`Path` <=> BINARY candidate.`Path`
        AND BINARY survivor.`SourcePath` <=> BINARY candidate.`SourcePath`
        AND BINARY survivor.`BlurHash43` <=> BINARY candidate.`BlurHash43`
        AND BINARY survivor.`BlurHash54` <=> BINARY candidate.`BlurHash54`
        AND BINARY survivor.`BlurHash64` <=> BINARY candidate.`BlurHash64`
        AND BINARY survivor.`OriginalContentType` <=> BINARY candidate.`OriginalContentType`
  );

DELETE FROM `ConfigElement`
WHERE `Key` = 'migration.artwork_cardinality_repair';

INSERT INTO `ConfigElement` (`Key`, `Value`)
SELECT 'migration.artwork_cardinality_repair',
       CONCAT(
           '{"removedRows":',
           COALESCE(SUM(CASE WHEN `Repairable` = 1 THEN 1 ELSE 0 END), 0),
           ',"skippedReferencedRows":',
           COALESCE(SUM(CASE WHEN `Repairable` = 0 THEN 1 ELSE 0 END), 0),
           '}'
       )
FROM `_ArtworkCardinalityRepair`;

DELETE artwork
FROM `Artwork` AS artwork
JOIN `_ArtworkCardinalityRepair` AS repair
  ON repair.`ArtworkId` = artwork.`Id`
WHERE repair.`Repairable` = 1;

DROP TEMPORARY TABLE `_ArtworkCardinalityRepair`;
