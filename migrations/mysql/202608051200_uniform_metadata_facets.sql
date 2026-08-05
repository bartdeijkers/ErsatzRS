-- ErsatzRS parity-plus extension: sparse multi-owner music-label and explicit
-- language facets. Existing media rows are populated by their next scan.

ALTER TABLE `MusicVideoMetadata` ADD COLUMN `ContentRating` LONGTEXT NULL;
ALTER TABLE `SongMetadata` ADD COLUMN `ContentRating` LONGTEXT NULL;
ALTER TABLE `ImageMetadata` ADD COLUMN `ContentRating` LONGTEXT NULL;
ALTER TABLE `EpisodeMetadata` ADD COLUMN `ContentRating` LONGTEXT NULL;

-- Existing local television folders may otherwise retain an unchanged ETag
-- and skip the episode upserts that populate the new facet relations.
UPDATE `LibraryFolder`
SET `Etag` = NULL
WHERE `LibraryPathId` IN (
    SELECT `lp`.`Id`
    FROM `LibraryPath` AS `lp`
    JOIN `Library` AS `l` ON `l`.`Id` = `lp`.`LibraryId`
    JOIN `LocalLibrary` AS `ll` ON `ll`.`Id` = `l`.`Id`
    WHERE `l`.`MediaKind` = 2
);

CREATE TABLE `MusicLabel` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `ArtistMetadataId` INT NULL,
    `EpisodeMetadataId` INT NULL,
    `ImageMetadataId` INT NULL,
    `MovieMetadataId` INT NULL,
    `MusicVideoMetadataId` INT NULL,
    `Name` LONGTEXT NULL,
    `OtherVideoMetadataId` INT NULL,
    `RemoteStreamMetadataId` INT NULL,
    `SeasonMetadataId` INT NULL,
    `ShowMetadataId` INT NULL,
    `SongMetadataId` INT NULL,
    CONSTRAINT `PK_MusicLabel` PRIMARY KEY (`Id`),
    CONSTRAINT `FK_MusicLabel_ArtistMetadata_ArtistMetadataId` FOREIGN KEY (`ArtistMetadataId`) REFERENCES `ArtistMetadata` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_MusicLabel_EpisodeMetadata_EpisodeMetadataId` FOREIGN KEY (`EpisodeMetadataId`) REFERENCES `EpisodeMetadata` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_MusicLabel_ImageMetadata_ImageMetadataId` FOREIGN KEY (`ImageMetadataId`) REFERENCES `ImageMetadata` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_MusicLabel_MovieMetadata_MovieMetadataId` FOREIGN KEY (`MovieMetadataId`) REFERENCES `MovieMetadata` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_MusicLabel_MusicVideoMetadata_MusicVideoMetadataId` FOREIGN KEY (`MusicVideoMetadataId`) REFERENCES `MusicVideoMetadata` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_MusicLabel_OtherVideoMetadata_OtherVideoMetadataId` FOREIGN KEY (`OtherVideoMetadataId`) REFERENCES `OtherVideoMetadata` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_MusicLabel_RemoteStreamMetadata_RemoteStreamMetadataId` FOREIGN KEY (`RemoteStreamMetadataId`) REFERENCES `RemoteStreamMetadata` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_MusicLabel_SeasonMetadata_SeasonMetadataId` FOREIGN KEY (`SeasonMetadataId`) REFERENCES `SeasonMetadata` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_MusicLabel_ShowMetadata_ShowMetadataId` FOREIGN KEY (`ShowMetadataId`) REFERENCES `ShowMetadata` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_MusicLabel_SongMetadata_SongMetadataId` FOREIGN KEY (`SongMetadataId`) REFERENCES `SongMetadata` (`Id`) ON DELETE CASCADE
);

CREATE TABLE `MetadataLanguage` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `ArtistMetadataId` INT NULL,
    `Code` LONGTEXT NULL,
    `EpisodeMetadataId` INT NULL,
    `ImageMetadataId` INT NULL,
    `MovieMetadataId` INT NULL,
    `MusicVideoMetadataId` INT NULL,
    `OtherVideoMetadataId` INT NULL,
    `RemoteStreamMetadataId` INT NULL,
    `SeasonMetadataId` INT NULL,
    `ShowMetadataId` INT NULL,
    `SongMetadataId` INT NULL,
    CONSTRAINT `PK_MetadataLanguage` PRIMARY KEY (`Id`),
    CONSTRAINT `FK_MetadataLanguage_ArtistMetadata_ArtistMetadataId` FOREIGN KEY (`ArtistMetadataId`) REFERENCES `ArtistMetadata` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_MetadataLanguage_EpisodeMetadata_EpisodeMetadataId` FOREIGN KEY (`EpisodeMetadataId`) REFERENCES `EpisodeMetadata` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_MetadataLanguage_ImageMetadata_ImageMetadataId` FOREIGN KEY (`ImageMetadataId`) REFERENCES `ImageMetadata` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_MetadataLanguage_MovieMetadata_MovieMetadataId` FOREIGN KEY (`MovieMetadataId`) REFERENCES `MovieMetadata` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_MetadataLanguage_MusicVideoMetadata_MusicVideoMetadataId` FOREIGN KEY (`MusicVideoMetadataId`) REFERENCES `MusicVideoMetadata` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_MetadataLanguage_OtherVideoMetadata_OtherVideoMetadataId` FOREIGN KEY (`OtherVideoMetadataId`) REFERENCES `OtherVideoMetadata` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_MetadataLanguage_RemoteStreamMetadata_RemoteStreamMetadataId` FOREIGN KEY (`RemoteStreamMetadataId`) REFERENCES `RemoteStreamMetadata` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_MetadataLanguage_SeasonMetadata_SeasonMetadataId` FOREIGN KEY (`SeasonMetadataId`) REFERENCES `SeasonMetadata` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_MetadataLanguage_ShowMetadata_ShowMetadataId` FOREIGN KEY (`ShowMetadataId`) REFERENCES `ShowMetadata` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_MetadataLanguage_SongMetadata_SongMetadataId` FOREIGN KEY (`SongMetadataId`) REFERENCES `SongMetadata` (`Id`) ON DELETE CASCADE
);

CREATE INDEX `IX_MusicLabel_ArtistMetadataId` ON `MusicLabel` (`ArtistMetadataId`);
CREATE INDEX `IX_MusicLabel_EpisodeMetadataId` ON `MusicLabel` (`EpisodeMetadataId`);
CREATE INDEX `IX_MusicLabel_ImageMetadataId` ON `MusicLabel` (`ImageMetadataId`);
CREATE INDEX `IX_MusicLabel_MovieMetadataId` ON `MusicLabel` (`MovieMetadataId`);
CREATE INDEX `IX_MusicLabel_MusicVideoMetadataId` ON `MusicLabel` (`MusicVideoMetadataId`);
CREATE INDEX `IX_MusicLabel_OtherVideoMetadataId` ON `MusicLabel` (`OtherVideoMetadataId`);
CREATE INDEX `IX_MusicLabel_RemoteStreamMetadataId` ON `MusicLabel` (`RemoteStreamMetadataId`);
CREATE INDEX `IX_MusicLabel_SeasonMetadataId` ON `MusicLabel` (`SeasonMetadataId`);
CREATE INDEX `IX_MusicLabel_ShowMetadataId` ON `MusicLabel` (`ShowMetadataId`);
CREATE INDEX `IX_MusicLabel_SongMetadataId` ON `MusicLabel` (`SongMetadataId`);
CREATE INDEX `IX_MetadataLanguage_ArtistMetadataId` ON `MetadataLanguage` (`ArtistMetadataId`);
CREATE INDEX `IX_MetadataLanguage_EpisodeMetadataId` ON `MetadataLanguage` (`EpisodeMetadataId`);
CREATE INDEX `IX_MetadataLanguage_ImageMetadataId` ON `MetadataLanguage` (`ImageMetadataId`);
CREATE INDEX `IX_MetadataLanguage_MovieMetadataId` ON `MetadataLanguage` (`MovieMetadataId`);
CREATE INDEX `IX_MetadataLanguage_MusicVideoMetadataId` ON `MetadataLanguage` (`MusicVideoMetadataId`);
CREATE INDEX `IX_MetadataLanguage_OtherVideoMetadataId` ON `MetadataLanguage` (`OtherVideoMetadataId`);
CREATE INDEX `IX_MetadataLanguage_RemoteStreamMetadataId` ON `MetadataLanguage` (`RemoteStreamMetadataId`);
CREATE INDEX `IX_MetadataLanguage_SeasonMetadataId` ON `MetadataLanguage` (`SeasonMetadataId`);
CREATE INDEX `IX_MetadataLanguage_ShowMetadataId` ON `MetadataLanguage` (`ShowMetadataId`);
CREATE INDEX `IX_MetadataLanguage_SongMetadataId` ON `MetadataLanguage` (`SongMetadataId`);
