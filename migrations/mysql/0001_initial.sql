-- ErsatzRS collapsed MySQL initial schema derived from the final ErsatzTV model.
-- Foreign keys are added after all tables and indexes so MySQL can apply
-- this migration from an empty schema without forward-reference failures.

SET NAMES utf8mb4 COLLATE utf8mb4_general_ci;
SET FOREIGN_KEY_CHECKS = 0;

CREATE TABLE `Actor` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `ArtistMetadataId` INT NULL,
    `ArtworkId` INT NULL,
    `EpisodeMetadataId` INT NULL,
    `ImageMetadataId` INT NULL,
    `MovieMetadataId` INT NULL,
    `MusicVideoMetadataId` INT NULL,
    `Name` LONGTEXT NULL,
    `Order` INT NULL,
    `OtherVideoMetadataId` INT NULL,
    `RemoteStreamMetadataId` INT NULL,
    `Role` LONGTEXT NULL,
    `SeasonMetadataId` INT NULL,
    `ShowMetadataId` INT NULL,
    `SongMetadataId` INT NULL,
    CONSTRAINT `PK_Actor` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `Artist` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    CONSTRAINT `PK_Artist` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `ArtistMetadata` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `ArtistId` INT NOT NULL,
    `Biography` LONGTEXT NULL,
    `DateAdded` LONGTEXT NOT NULL,
    `DateUpdated` LONGTEXT NOT NULL,
    `Disambiguation` LONGTEXT NULL,
    `Formed` LONGTEXT NULL,
    `MetadataKind` INT NOT NULL,
    `OriginalTitle` LONGTEXT NULL,
    `ReleaseDate` LONGTEXT NULL,
    `SortTitle` LONGTEXT NULL,
    `Title` varchar(255) COLLATE utf8mb4_general_ci NULL,
    `Year` INT NULL,
    CONSTRAINT `PK_ArtistMetadata` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `Artwork` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `ArtistMetadataId` INT NULL,
    `ArtworkKind` INT NOT NULL,
    `BlurHash43` LONGTEXT NULL,
    `BlurHash54` LONGTEXT NULL,
    `BlurHash64` LONGTEXT NULL,
    `ChannelId` INT NULL,
    `DateAdded` LONGTEXT NOT NULL,
    `DateUpdated` LONGTEXT NOT NULL,
    `EpisodeMetadataId` INT NULL,
    `ImageMetadataId` INT NULL,
    `MovieMetadataId` INT NULL,
    `MusicVideoMetadataId` INT NULL,
    `OriginalContentType` LONGTEXT NULL,
    `OtherVideoMetadataId` INT NULL,
    `Path` LONGTEXT NULL,
    `RemoteStreamMetadataId` INT NULL,
    `SeasonMetadataId` INT NULL,
    `ShowMetadataId` INT NULL,
    `SongMetadataId` INT NULL,
    `SourcePath` LONGTEXT NULL,
    `IsMetadataOrphan` INT GENERATED ALWAYS AS (CASE WHEN COALESCE(ArtistMetadataId, ChannelId, EpisodeMetadataId, MovieMetadataId, MusicVideoMetadataId, OtherVideoMetadataId, SeasonMetadataId, ShowMetadataId, SongMetadataId, ImageMetadataId, RemoteStreamMetadataId) IS NULL THEN 1 ELSE NULL END) VIRTUAL,
    CONSTRAINT `PK_Artwork` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `Block` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `BlockGroupId` INT NOT NULL,
    `DateUpdated` LONGTEXT NOT NULL,
    `Minutes` INT NOT NULL,
    `Name` varchar(50) COLLATE utf8mb4_general_ci NULL,
    `StopScheduling` INT NOT NULL,
    CONSTRAINT `PK_Block` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `BlockGroup` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Name` varchar(50) COLLATE utf8mb4_general_ci NULL,
    CONSTRAINT `PK_BlockGroup` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `BlockItem` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Index` INT NOT NULL,
    `BlockId` INT NOT NULL,
    `CollectionType` INT NOT NULL,
    `CollectionId` INT NULL,
    `MediaItemId` INT NULL,
    `MultiCollectionId` INT NULL,
    `SmartCollectionId` INT NULL,
    `PlaybackOrder` INT NOT NULL,
    `IncludeInProgramGuide` INT NOT NULL DEFAULT 0,
    `DisableWatermarks` INT NOT NULL DEFAULT 0,
    `SearchQuery` LONGTEXT NULL,
    `SearchTitle` LONGTEXT NULL,
    CONSTRAINT `PK_BlockItem` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `BlockItemGraphicsElement` (
    `BlockItemId` INT NOT NULL,
    `GraphicsElementId` INT NOT NULL,
    CONSTRAINT `PK_BlockItemGraphicsElement` PRIMARY KEY (`BlockItemId`, `GraphicsElementId`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `BlockItemWatermark` (
    `BlockItemId` INT NOT NULL,
    `WatermarkId` INT NOT NULL,
    CONSTRAINT `PK_BlockItemWatermark` PRIMARY KEY (`BlockItemId`, `WatermarkId`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `Channel` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Categories` LONGTEXT NULL,
    `FFmpegProfileId` INT NOT NULL,
    `FallbackFillerId` INT NULL,
    `Group` varchar(255) NOT NULL DEFAULT 'ErsatzTV',
    `IdleBehavior` INT NOT NULL,
    `IsEnabled` INT NOT NULL DEFAULT 1,
    `MirrorSourceChannelId` INT NULL,
    `MusicVideoCreditsMode` INT NOT NULL,
    `MusicVideoCreditsTemplate` LONGTEXT NULL,
    `Name` varchar(50) COLLATE utf8mb4_general_ci NULL,
    `Number` varchar(255) NULL,
    `PlayoutMode` INT NOT NULL,
    `PlayoutOffset` LONGTEXT NULL,
    `PlayoutSource` INT NOT NULL,
    `PreferredAudioLanguageCode` LONGTEXT NULL,
    `PreferredAudioTitle` LONGTEXT NULL,
    `PreferredSubtitleLanguageCode` LONGTEXT NULL,
    `ShowInEpg` INT NOT NULL DEFAULT 1,
    `SongVideoMode` INT NOT NULL,
    `SortNumber` DOUBLE NOT NULL,
    `StreamSelector` LONGTEXT NULL,
    `StreamSelectorMode` INT NOT NULL,
    `StreamingMode` INT NOT NULL,
    `SubtitleMode` INT NOT NULL,
    `TranscodeMode` INT NOT NULL,
    `UniqueId` char(36) NOT NULL,
    `WatermarkId` INT NULL,
    `SlugSeconds` DOUBLE NULL,
    CONSTRAINT `PK_Channel` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `ChannelWatermark` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `DurationSeconds` INT NOT NULL,
    `FrequencyMinutes` INT NOT NULL,
    `HorizontalMarginPercent` DOUBLE NOT NULL,
    `Image` LONGTEXT NULL,
    `ImageSource` INT NOT NULL,
    `Location` INT NOT NULL,
    `Mode` INT NOT NULL,
    `Name` varchar(50) COLLATE utf8mb4_general_ci NULL,
    `Opacity` INT NOT NULL,
    `OpacityExpression` LONGTEXT NULL,
    `OriginalContentType` LONGTEXT NULL,
    `PlaceWithinSourceContent` INT NOT NULL,
    `Size` INT NOT NULL,
    `VerticalMarginPercent` DOUBLE NOT NULL,
    `WidthPercent` DOUBLE NOT NULL,
    `ZIndex` INT NOT NULL,
    CONSTRAINT `PK_ChannelWatermark` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `Collection` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Name` varchar(50) COLLATE utf8mb4_general_ci NULL,
    `UseCustomPlaybackOrder` INT NOT NULL,
    CONSTRAINT `PK_Collection` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `CollectionEnumeratorState` (
    `PlayoutProgramScheduleAnchorId` INT NOT NULL,
    `Seed` INT NOT NULL,
    `Index` INT NOT NULL,
    CONSTRAINT `PK_CollectionEnumeratorState` PRIMARY KEY (`PlayoutProgramScheduleAnchorId`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `CollectionItem` (
    `CollectionId` INT NOT NULL,
    `MediaItemId` INT NOT NULL,
    `CustomIndex` INT NULL,
    CONSTRAINT `PK_CollectionItem` PRIMARY KEY (`CollectionId`, `MediaItemId`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `ConfigElement` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Key` varchar(255) NULL,
    `Value` LONGTEXT NULL,
    CONSTRAINT `PK_ConfigElement` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `Deco` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `BreakContentMode` INT NOT NULL,
    `DeadAirFallbackCollectionId` INT NULL,
    `DeadAirFallbackCollectionType` INT NOT NULL,
    `DeadAirFallbackMediaItemId` INT NULL,
    `DeadAirFallbackMode` INT NOT NULL,
    `DeadAirFallbackMultiCollectionId` INT NULL,
    `DeadAirFallbackSmartCollectionId` INT NULL,
    `DecoGroupId` INT NOT NULL,
    `DefaultFillerCollectionId` INT NULL,
    `DefaultFillerCollectionType` INT NOT NULL,
    `DefaultFillerMediaItemId` INT NULL,
    `DefaultFillerMode` INT NOT NULL,
    `DefaultFillerMultiCollectionId` INT NULL,
    `DefaultFillerSmartCollectionId` INT NULL,
    `DefaultFillerTrimToFit` INT NOT NULL,
    `GraphicsElementsMode` INT NOT NULL,
    `Name` varchar(50) COLLATE utf8mb4_general_ci NULL,
    `UseGraphicsElementsDuringFiller` INT NOT NULL,
    `UseWatermarkDuringFiller` INT NOT NULL,
    `WatermarkMode` INT NOT NULL,
    CONSTRAINT `PK_Deco` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `DecoBreakContent` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `DecoId` INT NOT NULL,
    `CollectionType` INT NOT NULL,
    `CollectionId` INT NULL,
    `MediaItemId` INT NULL,
    `MultiCollectionId` INT NULL,
    `SmartCollectionId` INT NULL,
    `PlaylistId` INT NULL,
    `Placement` INT NOT NULL,
    CONSTRAINT `PK_DecoBreakContent` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `DecoGraphicsElement` (
    `DecoId` INT NOT NULL,
    `GraphicsElementId` INT NOT NULL,
    CONSTRAINT `PK_DecoGraphicsElement` PRIMARY KEY (`DecoId`, `GraphicsElementId`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `DecoGroup` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Name` varchar(50) COLLATE utf8mb4_general_ci NULL,
    CONSTRAINT `PK_DecoGroup` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `DecoTemplate` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `DateUpdated` LONGTEXT NOT NULL,
    `DecoTemplateGroupId` INT NOT NULL,
    `Name` varchar(50) COLLATE utf8mb4_general_ci NULL,
    CONSTRAINT `PK_DecoTemplate` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `DecoTemplateGroup` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Name` varchar(50) COLLATE utf8mb4_general_ci NULL,
    CONSTRAINT `PK_DecoTemplateGroup` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `DecoTemplateItem` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `DecoTemplateId` INT NOT NULL,
    `DecoId` INT NOT NULL,
    `StartTime` varchar(32) NOT NULL,
    `EndTime` varchar(32) NOT NULL DEFAULT '00:00:00',
    CONSTRAINT `PK_DecoTemplateItem` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `DecoWatermark` (
    `DecoId` INT NOT NULL,
    `WatermarkId` INT NOT NULL,
    CONSTRAINT `PK_DecoWatermark` PRIMARY KEY (`DecoId`, `WatermarkId`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `Director` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `EpisodeMetadataId` INT NULL,
    `MovieMetadataId` INT NULL,
    `MusicVideoMetadataId` INT NULL,
    `Name` LONGTEXT NULL,
    `OtherVideoMetadataId` INT NULL,
    CONSTRAINT `PK_Director` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `EmbyCollection` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `ItemId` varchar(255) NULL,
    `Etag` LONGTEXT NULL,
    `Name` LONGTEXT NULL,
    CONSTRAINT `PK_EmbyCollection` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `EmbyConnection` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Address` LONGTEXT NULL,
    `EmbyMediaSourceId` INT NOT NULL,
    CONSTRAINT `PK_EmbyConnection` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `EmbyEpisode` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `ItemId` LONGTEXT NULL,
    `Etag` LONGTEXT NULL,
    CONSTRAINT `PK_EmbyEpisode` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `EmbyLibrary` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `ItemId` varchar(255) NULL,
    `ShouldSyncItems` INT NOT NULL,
    CONSTRAINT `PK_EmbyLibrary` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `EmbyMediaSource` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `ServerName` LONGTEXT NULL,
    `OperatingSystem` LONGTEXT NULL,
    `LastCollectionsScan` LONGTEXT NULL,
    CONSTRAINT `PK_EmbyMediaSource` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `EmbyMovie` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `ItemId` LONGTEXT NULL,
    `Etag` LONGTEXT NULL,
    CONSTRAINT `PK_EmbyMovie` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `EmbyPathInfo` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Path` LONGTEXT NULL,
    `NetworkPath` LONGTEXT NULL,
    `EmbyLibraryId` INT NULL,
    CONSTRAINT `PK_EmbyPathInfo` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `EmbyPathReplacement` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `EmbyPath` LONGTEXT NULL,
    `LocalPath` LONGTEXT NULL,
    `EmbyMediaSourceId` INT NOT NULL,
    CONSTRAINT `PK_EmbyPathReplacement` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `EmbySeason` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `ItemId` LONGTEXT NULL,
    `Etag` LONGTEXT NULL,
    CONSTRAINT `PK_EmbySeason` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `EmbyShow` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `ItemId` LONGTEXT NULL,
    `Etag` LONGTEXT NULL,
    CONSTRAINT `PK_EmbyShow` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `Episode` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `SeasonId` INT NOT NULL,
    CONSTRAINT `PK_Episode` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `EpisodeMetadata` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Outline` LONGTEXT NULL,
    `Plot` LONGTEXT NULL,
    `Tagline` LONGTEXT NULL,
    `EpisodeId` INT NOT NULL,
    `MetadataKind` INT NOT NULL,
    `Title` LONGTEXT NULL,
    `OriginalTitle` LONGTEXT NULL,
    `SortTitle` LONGTEXT NULL,
    `ReleaseDate` LONGTEXT NULL,
    `DateAdded` LONGTEXT NOT NULL,
    `DateUpdated` LONGTEXT NOT NULL,
    `Year` INT NULL,
    `EpisodeNumber` INT NOT NULL DEFAULT 0,
    CONSTRAINT `PK_EpisodeMetadata` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `FFmpegProfile` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `AudioBitrate` INT NOT NULL,
    `AudioBufferSize` INT NOT NULL,
    `AudioChannels` INT NOT NULL,
    `AudioFormat` INT NOT NULL,
    `AudioSampleRate` INT NOT NULL,
    `BitDepth` INT NOT NULL,
    `DeinterlaceVideo` INT NULL DEFAULT 1,
    `HardwareAcceleration` INT NOT NULL,
    `Name` LONGTEXT NULL,
    `NormalizeFramerate` INT NOT NULL DEFAULT 0,
    `NormalizeLoudnessMode` INT NOT NULL,
    `QsvExtraHardwareFrames` INT NULL,
    `ResolutionId` INT NOT NULL,
    `ScalingBehavior` INT NOT NULL,
    `ThreadCount` INT NOT NULL,
    `VaapiDevice` LONGTEXT NULL,
    `VaapiDriver` INT NOT NULL,
    `VideoBitrate` INT NOT NULL,
    `VideoBufferSize` INT NOT NULL,
    `VideoFormat` INT NOT NULL,
    `VideoProfile` LONGTEXT NULL,
    `VideoPreset` LONGTEXT NULL,
    `AllowBFrames` INT NOT NULL DEFAULT 0,
    `VaapiDisplay` LONGTEXT NULL,
    `TonemapAlgorithm` INT NOT NULL DEFAULT 0,
    `TargetLoudness` DOUBLE NULL,
    `PadMode` INT NOT NULL DEFAULT 0,
    `NormalizeAudio` INT NOT NULL DEFAULT 1,
    `NormalizeColors` INT NOT NULL DEFAULT 1,
    `NormalizeVideo` INT NOT NULL DEFAULT 1,
    CONSTRAINT `PK_FFmpegProfile` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `FillGroupEnumeratorState` (
    `PlayoutScheduleItemFillGroupIndexId` INT NOT NULL,
    `Seed` INT NOT NULL,
    `Index` INT NOT NULL,
    CONSTRAINT `PK_FillGroupEnumeratorState` PRIMARY KEY (`PlayoutScheduleItemFillGroupIndexId`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `FillerPreset` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `AllowWatermarks` INT NOT NULL,
    `CollectionId` INT NULL,
    `CollectionType` INT NOT NULL,
    `Count` INT NULL,
    `Duration` LONGTEXT NULL,
    `Expression` LONGTEXT NULL,
    `FillerKind` INT NOT NULL,
    `FillerMode` INT NOT NULL,
    `MediaItemId` INT NULL,
    `MultiCollectionId` INT NULL,
    `Name` varchar(50) COLLATE utf8mb4_general_ci NULL,
    `PadToNearestMinute` INT NULL,
    `PlaylistId` INT NULL,
    `SmartCollectionId` INT NULL,
    `UseChaptersAsMediaItems` INT NOT NULL,
    CONSTRAINT `PK_FillerPreset` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `Genre` (
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
    CONSTRAINT `PK_Genre` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `GraphicsElement` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Path` LONGTEXT NULL,
    `Kind` INT NOT NULL,
    `Name` LONGTEXT NULL,
    CONSTRAINT `PK_GraphicsElement` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `Image` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    CONSTRAINT `PK_Image` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `ImageFolderDuration` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `DurationSeconds` DOUBLE NOT NULL,
    `LibraryFolderId` INT NOT NULL,
    CONSTRAINT `PK_ImageFolderDuration` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `ImageMetadata` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `DateAdded` LONGTEXT NOT NULL,
    `DateUpdated` LONGTEXT NOT NULL,
    `DurationSeconds` DOUBLE NULL,
    `ImageId` INT NOT NULL,
    `MetadataKind` INT NOT NULL,
    `OriginalTitle` LONGTEXT NULL,
    `ReleaseDate` LONGTEXT NULL,
    `SortTitle` LONGTEXT NULL,
    `Title` LONGTEXT NULL,
    `Year` INT NULL,
    CONSTRAINT `PK_ImageMetadata` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `JellyfinCollection` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `ItemId` varchar(255) NULL,
    `Etag` LONGTEXT NULL,
    `Name` LONGTEXT NULL,
    CONSTRAINT `PK_JellyfinCollection` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `JellyfinConnection` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Address` LONGTEXT NULL,
    `JellyfinMediaSourceId` INT NOT NULL,
    CONSTRAINT `PK_JellyfinConnection` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `JellyfinEpisode` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `ItemId` varchar(255) NULL,
    `Etag` LONGTEXT NULL,
    CONSTRAINT `PK_JellyfinEpisode` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `JellyfinLibrary` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `ItemId` varchar(255) NULL,
    `ShouldSyncItems` INT NOT NULL,
    CONSTRAINT `PK_JellyfinLibrary` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `JellyfinMediaSource` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `ServerName` LONGTEXT NULL,
    `OperatingSystem` LONGTEXT NULL,
    `LastCollectionsScan` LONGTEXT NULL,
    CONSTRAINT `PK_JellyfinMediaSource` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `JellyfinMovie` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `ItemId` varchar(255) NULL,
    `Etag` LONGTEXT NULL,
    CONSTRAINT `PK_JellyfinMovie` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `JellyfinPathInfo` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Path` LONGTEXT NULL,
    `NetworkPath` LONGTEXT NULL,
    `JellyfinLibraryId` INT NULL,
    CONSTRAINT `PK_JellyfinPathInfo` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `JellyfinPathReplacement` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `JellyfinPath` LONGTEXT NULL,
    `LocalPath` LONGTEXT NULL,
    `JellyfinMediaSourceId` INT NOT NULL,
    CONSTRAINT `PK_JellyfinPathReplacement` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `JellyfinSeason` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `ItemId` varchar(255) NULL,
    `Etag` LONGTEXT NULL,
    CONSTRAINT `PK_JellyfinSeason` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `JellyfinShow` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `ItemId` varchar(255) NULL,
    `Etag` LONGTEXT NULL,
    CONSTRAINT `PK_JellyfinShow` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `LanguageCode` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `ThreeCode1` LONGTEXT NULL,
    `ThreeCode2` LONGTEXT NULL,
    `TwoCode` LONGTEXT NULL,
    `EnglishName` LONGTEXT NULL,
    `FrenchName` LONGTEXT NULL,
    CONSTRAINT `PK_LanguageCode` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `Library` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `LastScan` LONGTEXT NULL,
    `MediaKind` INT NOT NULL,
    `MediaSourceId` INT NOT NULL,
    `Name` LONGTEXT NULL,
    CONSTRAINT `PK_Library` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `LibraryFolder` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Etag` LONGTEXT NULL,
    `LibraryPathId` INT NOT NULL,
    `ParentId` INT NULL,
    `Path` LONGTEXT NULL,
    CONSTRAINT `PK_LibraryFolder` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `LibraryPath` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Path` LONGTEXT NULL,
    `LibraryId` INT NOT NULL,
    `LastScan` LONGTEXT NULL,
    CONSTRAINT `PK_LibraryPath` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `LocalLibrary` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    CONSTRAINT `PK_LocalLibrary` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `LocalMediaSource` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    CONSTRAINT `PK_LocalMediaSource` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `MediaChapter` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `MediaVersionId` INT NOT NULL,
    `ChapterId` INT NOT NULL,
    `StartTime` varchar(32) NOT NULL,
    `EndTime` varchar(32) NOT NULL,
    `Title` LONGTEXT NULL,
    CONSTRAINT `PK_MediaChapter` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `MediaFile` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `LibraryFolderId` INT NULL,
    `MediaVersionId` INT NOT NULL,
    `Path` LONGTEXT NULL,
    `PathHash` varchar(255) NOT NULL,
    CONSTRAINT `PK_MediaFile` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `MediaItem` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `LibraryPathId` INT NOT NULL,
    `State` INT NOT NULL DEFAULT 0,
    CONSTRAINT `PK_MediaItem` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `MediaSource` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    CONSTRAINT `PK_MediaSource` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `MediaStream` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Index` INT NOT NULL,
    `Codec` LONGTEXT NULL,
    `Profile` LONGTEXT NULL,
    `MediaStreamKind` INT NOT NULL,
    `Language` LONGTEXT NULL,
    `Channels` INT NOT NULL,
    `Title` LONGTEXT NULL,
    `Default` INT NOT NULL,
    `Forced` INT NOT NULL,
    `MediaVersionId` INT NOT NULL,
    `BitsPerRawSample` INT NOT NULL DEFAULT 0,
    `PixelFormat` LONGTEXT NULL,
    `AttachedPic` INT NOT NULL DEFAULT 0,
    `FileName` LONGTEXT NULL,
    `MimeType` LONGTEXT NULL,
    `ColorPrimaries` LONGTEXT NULL,
    `ColorRange` LONGTEXT NULL,
    `ColorSpace` LONGTEXT NULL,
    `ColorTransfer` LONGTEXT NULL,
    CONSTRAINT `PK_MediaStream` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `MediaVersion` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `DateAdded` LONGTEXT NOT NULL,
    `DateUpdated` LONGTEXT NOT NULL,
    `DisplayAspectRatio` LONGTEXT NULL,
    `Duration` LONGTEXT NOT NULL,
    `EpisodeId` INT NULL,
    `Height` INT NOT NULL,
    `ImageId` INT NULL,
    `MovieId` INT NULL,
    `MusicVideoId` INT NULL,
    `Name` LONGTEXT NULL,
    `OtherVideoId` INT NULL,
    `RFrameRate` LONGTEXT NULL,
    `RemoteStreamId` INT NULL,
    `SampleAspectRatio` LONGTEXT NULL,
    `SongId` INT NULL,
    `VideoScanKind` INT NOT NULL,
    `Width` INT NOT NULL,
    `InterlacedRatio` DOUBLE NULL,
    CONSTRAINT `PK_MediaVersion` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `MetadataGuid` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `ArtistMetadataId` INT NULL,
    `EpisodeMetadataId` INT NULL,
    `Guid` varchar(128) COLLATE utf8mb4_general_ci NULL,
    `ImageMetadataId` INT NULL,
    `MovieMetadataId` INT NULL,
    `MusicVideoMetadataId` INT NULL,
    `OtherVideoMetadataId` INT NULL,
    `RemoteStreamMetadataId` INT NULL,
    `SeasonMetadataId` INT NULL,
    `ShowMetadataId` INT NULL,
    `SongMetadataId` INT NULL,
    CONSTRAINT `PK_MetadataGuid` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `Mood` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Name` LONGTEXT NULL,
    `ArtistMetadataId` INT NULL,
    CONSTRAINT `PK_Mood` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `Movie` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    CONSTRAINT `PK_Movie` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `MovieMetadata` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `ContentRating` LONGTEXT NULL,
    `DateAdded` LONGTEXT NOT NULL,
    `DateUpdated` LONGTEXT NOT NULL,
    `MetadataKind` INT NOT NULL,
    `MovieId` INT NOT NULL,
    `OriginalTitle` LONGTEXT NULL,
    `Outline` LONGTEXT NULL,
    `Plot` LONGTEXT NULL,
    `ReleaseDate` LONGTEXT NULL,
    `SortTitle` LONGTEXT NULL,
    `Tagline` LONGTEXT NULL,
    `Title` varchar(255) COLLATE utf8mb4_general_ci NULL,
    `Year` INT NULL,
    CONSTRAINT `PK_MovieMetadata` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `MultiCollection` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Name` varchar(50) COLLATE utf8mb4_general_ci NULL,
    CONSTRAINT `PK_MultiCollection` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `MultiCollectionItem` (
    `MultiCollectionId` INT NOT NULL,
    `CollectionId` INT NOT NULL,
    `ScheduleAsGroup` INT NOT NULL,
    `PlaybackOrder` INT NOT NULL,
    CONSTRAINT `PK_MultiCollectionItem` PRIMARY KEY (`MultiCollectionId`, `CollectionId`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `MultiCollectionSmartItem` (
    `MultiCollectionId` INT NOT NULL,
    `SmartCollectionId` INT NOT NULL,
    `ScheduleAsGroup` INT NOT NULL,
    `PlaybackOrder` INT NOT NULL,
    CONSTRAINT `PK_MultiCollectionSmartItem` PRIMARY KEY (`MultiCollectionId`, `SmartCollectionId`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `MusicVideo` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `ArtistId` INT NOT NULL,
    CONSTRAINT `PK_MusicVideo` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `MusicVideoArtist` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Name` LONGTEXT NULL,
    `MusicVideoMetadataId` INT NULL,
    CONSTRAINT `PK_MusicVideoArtist` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `MusicVideoMetadata` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Album` LONGTEXT NULL,
    `DateAdded` LONGTEXT NOT NULL,
    `DateUpdated` LONGTEXT NOT NULL,
    `MetadataKind` INT NOT NULL,
    `MusicVideoId` INT NOT NULL,
    `OriginalTitle` LONGTEXT NULL,
    `Plot` LONGTEXT NULL,
    `ReleaseDate` LONGTEXT NULL,
    `SortTitle` LONGTEXT NULL,
    `Title` LONGTEXT NULL,
    `Year` INT NULL,
    `Track` INT NULL,
    CONSTRAINT `PK_MusicVideoMetadata` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `OtherVideo` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    CONSTRAINT `PK_OtherVideo` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `OtherVideoMetadata` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `OtherVideoId` INT NOT NULL,
    `MetadataKind` INT NOT NULL,
    `Title` LONGTEXT NULL,
    `OriginalTitle` LONGTEXT NULL,
    `SortTitle` LONGTEXT NULL,
    `Year` INT NULL,
    `ReleaseDate` LONGTEXT NULL,
    `DateAdded` LONGTEXT NOT NULL,
    `DateUpdated` LONGTEXT NOT NULL,
    `ContentRating` LONGTEXT NULL,
    `Outline` LONGTEXT NULL,
    `Plot` LONGTEXT NULL,
    `Tagline` LONGTEXT NULL,
    CONSTRAINT `PK_OtherVideoMetadata` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `Playlist` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `IsSystem` INT NOT NULL,
    `Name` varchar(50) COLLATE utf8mb4_general_ci NULL,
    `PlaylistGroupId` INT NOT NULL,
    CONSTRAINT `PK_Playlist` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `PlaylistGroup` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Name` varchar(50) NULL,
    `IsSystem` INT NOT NULL DEFAULT 0,
    CONSTRAINT `PK_PlaylistGroup` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `PlaylistItem` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Index` INT NOT NULL,
    `PlaylistId` INT NOT NULL,
    `CollectionType` INT NOT NULL,
    `CollectionId` INT NULL,
    `MediaItemId` INT NULL,
    `MultiCollectionId` INT NULL,
    `SmartCollectionId` INT NULL,
    `IncludeInProgramGuide` INT NOT NULL DEFAULT 0,
    `PlaybackOrder` INT NOT NULL DEFAULT 0,
    `PlayAll` INT NOT NULL DEFAULT 0,
    `Count` INT NULL,
    CONSTRAINT `PK_PlaylistItem` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `Playout` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `ChannelId` INT NOT NULL,
    `DailyRebuildTime` LONGTEXT NULL,
    `DecoId` INT NULL,
    `OnDemandCheckpoint` LONGTEXT NULL,
    `ProgramScheduleId` INT NULL,
    `ScheduleFile` LONGTEXT NULL,
    `ScheduleKind` INT NOT NULL,
    `Seed` INT NOT NULL,
    CONSTRAINT `PK_Playout` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `PlayoutAnchor` (
    `PlayoutId` INT NOT NULL,
    `DurationFinish` LONGTEXT NULL,
    `InDurationFiller` INT NOT NULL,
    `InFlood` INT NOT NULL,
    `MultipleRemaining` INT NULL,
    `NextGuideGroup` INT NOT NULL,
    `NextStart` varchar(32) NOT NULL,
    `NextInstructionIndex` INT NOT NULL DEFAULT 0,
    `Context` LONGTEXT NULL,
    CONSTRAINT `PK_PlayoutAnchor` PRIMARY KEY (`PlayoutId`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `PlayoutBuildStatus` (
    `PlayoutId` INT NOT NULL,
    `LastBuild` LONGTEXT NOT NULL,
    `Success` INT NOT NULL,
    `Message` LONGTEXT NULL,
    CONSTRAINT `PK_PlayoutBuildStatus` PRIMARY KEY (`PlayoutId`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `PlayoutGap` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `PlayoutId` INT NOT NULL,
    `Start` varchar(255) NOT NULL,
    `Finish` varchar(255) NOT NULL,
    CONSTRAINT `PK_PlayoutGap` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `PlayoutHistory` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `BlockId` INT NULL,
    `Details` LONGTEXT NULL,
    `Index` INT NOT NULL,
    `Key` LONGTEXT NULL,
    `PlaybackOrder` INT NOT NULL,
    `PlayoutId` INT NOT NULL,
    `When` varchar(32) NOT NULL,
    `Finish` varchar(32) NOT NULL DEFAULT '0001-01-01 00:00:00',
    `ChildKey` LONGTEXT NULL,
    `IsCurrentChild` INT NOT NULL DEFAULT 0,
    CONSTRAINT `PK_PlayoutHistory` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `PlayoutItem` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `BlockKey` LONGTEXT NULL,
    `ChapterTitle` LONGTEXT NULL,
    `CollectionEtag` LONGTEXT NULL,
    `CollectionKey` LONGTEXT NULL,
    `CustomTitle` LONGTEXT NULL,
    `DisableWatermarks` INT NOT NULL,
    `FillerKind` INT NOT NULL,
    `Finish` varchar(255) NOT NULL,
    `GuideFinish` LONGTEXT NULL,
    `GuideGroup` INT NOT NULL,
    `GuideStart` LONGTEXT NULL,
    `InPoint` LONGTEXT NOT NULL,
    `MediaItemId` INT NOT NULL,
    `OutPoint` LONGTEXT NOT NULL,
    `PlayoutId` INT NOT NULL,
    `PreferredAudioLanguageCode` LONGTEXT NULL,
    `PreferredAudioTitle` LONGTEXT NULL,
    `PreferredSubtitleLanguageCode` LONGTEXT NULL,
    `Start` varchar(255) NOT NULL,
    `SubtitleMode` INT NULL,
    `SchedulingContext` LONGTEXT NULL,
    CONSTRAINT `PK_PlayoutItem` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `PlayoutItemGraphicsElement` (
    `PlayoutItemId` INT NOT NULL,
    `GraphicsElementId` INT NOT NULL,
    `Variables` LONGTEXT NULL,
    CONSTRAINT `PK_PlayoutItemGraphicsElement` PRIMARY KEY (`PlayoutItemId`, `GraphicsElementId`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `PlayoutItemWatermark` (
    `PlayoutItemId` INT NOT NULL,
    `WatermarkId` INT NOT NULL,
    CONSTRAINT `PK_PlayoutItemWatermark` PRIMARY KEY (`PlayoutItemId`, `WatermarkId`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `PlayoutProgramScheduleAnchor` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `AnchorDate` LONGTEXT NULL,
    `CollectionId` INT NULL,
    `CollectionType` INT NOT NULL,
    `FakeCollectionKey` LONGTEXT NULL,
    `MediaItemId` INT NULL,
    `MultiCollectionId` INT NULL,
    `PlaylistId` INT NULL,
    `PlayoutId` INT NOT NULL,
    `RerunCollectionId` INT NULL,
    `SmartCollectionId` INT NULL,
    `SearchQuery` LONGTEXT NULL,
    CONSTRAINT `PK_PlayoutProgramScheduleAnchor` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `PlayoutScheduleItemFillGroupIndex` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `PlayoutId` INT NOT NULL,
    `ProgramScheduleItemId` INT NOT NULL,
    CONSTRAINT `PK_PlayoutScheduleItemFillGroupIndex` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `PlayoutTemplate` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `DateUpdated` LONGTEXT NOT NULL,
    `DaysOfMonth` LONGTEXT NULL,
    `DaysOfWeek` LONGTEXT NULL,
    `DecoTemplateId` INT NULL,
    `EndDay` INT NOT NULL,
    `EndMonth` INT NOT NULL,
    `Index` INT NOT NULL,
    `LimitToDateRange` INT NOT NULL,
    `MonthsOfYear` LONGTEXT NULL,
    `PlayoutId` INT NOT NULL,
    `StartDay` INT NOT NULL,
    `StartMonth` INT NOT NULL,
    `TemplateId` INT NOT NULL,
    `EndYear` INT NULL,
    `StartYear` INT NULL,
    CONSTRAINT `PK_PlayoutTemplate` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `PlexCollection` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Key` varchar(255) NULL,
    `Etag` LONGTEXT NULL,
    `Name` LONGTEXT NULL,
    CONSTRAINT `PK_PlexCollection` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `PlexConnection` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `IsActive` INT NOT NULL,
    `Uri` LONGTEXT NULL,
    `PlexMediaSourceId` INT NOT NULL,
    CONSTRAINT `PK_PlexConnection` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `PlexEpisode` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Key` LONGTEXT NULL,
    `Etag` LONGTEXT NULL,
    CONSTRAINT `PK_PlexEpisode` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `PlexLibrary` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Key` varchar(255) NULL,
    `ShouldSyncItems` INT NOT NULL,
    `LastNetworksScan` LONGTEXT NULL,
    CONSTRAINT `PK_PlexLibrary` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `PlexMediaFile` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `PlexId` INT NOT NULL,
    `Key` LONGTEXT NULL,
    CONSTRAINT `PK_PlexMediaFile` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `PlexMediaSource` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `ClientIdentifier` LONGTEXT NULL,
    `ProductVersion` LONGTEXT NULL,
    `ServerName` LONGTEXT NULL,
    `Platform` LONGTEXT NULL,
    `PlatformVersion` LONGTEXT NULL,
    `LastCollectionsScan` LONGTEXT NULL,
    CONSTRAINT `PK_PlexMediaSource` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `PlexMovie` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Key` LONGTEXT NULL,
    `Etag` LONGTEXT NULL,
    CONSTRAINT `PK_PlexMovie` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `PlexOtherVideo` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Key` LONGTEXT NULL,
    `Etag` LONGTEXT NULL,
    CONSTRAINT `PK_PlexOtherVideo` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `PlexPathReplacement` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `PlexPath` LONGTEXT NULL,
    `LocalPath` LONGTEXT NULL,
    `PlexMediaSourceId` INT NOT NULL,
    CONSTRAINT `PK_PlexPathReplacement` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `PlexSeason` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Key` LONGTEXT NULL,
    `Etag` LONGTEXT NULL,
    CONSTRAINT `PK_PlexSeason` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `PlexShow` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Key` LONGTEXT NULL,
    `Etag` LONGTEXT NULL,
    CONSTRAINT `PK_PlexShow` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `ProgramSchedule` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `FixedStartTimeBehavior` INT NOT NULL,
    `KeepMultiPartEpisodesTogether` INT NOT NULL,
    `Name` varchar(50) COLLATE utf8mb4_general_ci NULL,
    `RandomStartPoint` INT NOT NULL,
    `ShuffleScheduleItems` INT NOT NULL,
    `TreatCollectionsAsShows` INT NOT NULL,
    CONSTRAINT `PK_ProgramSchedule` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `ProgramScheduleAlternate` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `PlayoutId` INT NOT NULL,
    `ProgramScheduleId` INT NOT NULL,
    `Index` INT NOT NULL,
    `DaysOfWeek` LONGTEXT NULL,
    `DaysOfMonth` LONGTEXT NULL,
    `MonthsOfYear` LONGTEXT NULL,
    `EndDay` INT NOT NULL DEFAULT 0,
    `EndMonth` INT NOT NULL DEFAULT 0,
    `EndYear` INT NULL,
    `LimitToDateRange` INT NOT NULL DEFAULT 0,
    `StartDay` INT NOT NULL DEFAULT 0,
    `StartMonth` INT NOT NULL DEFAULT 0,
    `StartYear` INT NULL,
    CONSTRAINT `PK_ProgramScheduleAlternate` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `ProgramScheduleDurationItem` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `PlayoutDuration` LONGTEXT NOT NULL,
    `TailMode` INT NOT NULL,
    `DiscardToFillAttempts` INT NOT NULL DEFAULT 0,
    CONSTRAINT `PK_ProgramScheduleDurationItem` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `ProgramScheduleFloodItem` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    CONSTRAINT `PK_ProgramScheduleFloodItem` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `ProgramScheduleItem` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `CollectionId` INT NULL,
    `CollectionType` INT NOT NULL,
    `CustomTitle` LONGTEXT NULL,
    `FakeCollectionKey` LONGTEXT NULL,
    `FallbackFillerId` INT NULL,
    `FillWithGroupMode` INT NOT NULL,
    `FixedStartTimeBehavior` INT NULL,
    `GuideMode` INT NOT NULL,
    `Index` INT NOT NULL,
    `MarathonBatchSize` INT NULL,
    `MarathonGroupBy` INT NOT NULL,
    `MarathonShuffleGroups` INT NOT NULL,
    `MarathonShuffleItems` INT NOT NULL,
    `MediaItemId` INT NULL,
    `MidRollFillerId` INT NULL,
    `MultiCollectionId` INT NULL,
    `PlaybackOrder` INT NOT NULL,
    `PlaylistId` INT NULL,
    `PostRollFillerId` INT NULL,
    `PreRollFillerId` INT NULL,
    `PreferredAudioLanguageCode` LONGTEXT NULL,
    `PreferredAudioTitle` LONGTEXT NULL,
    `PreferredSubtitleLanguageCode` LONGTEXT NULL,
    `ProgramScheduleId` INT NOT NULL,
    `RerunCollectionId` INT NULL,
    `SmartCollectionId` INT NULL,
    `StartTime` varchar(32) NULL,
    `SubtitleMode` INT NULL,
    `TailFillerId` INT NULL,
    `SearchQuery` LONGTEXT NULL,
    `SearchTitle` LONGTEXT NULL,
    CONSTRAINT `PK_ProgramScheduleItem` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `ProgramScheduleItemGraphicsElement` (
    `ProgramScheduleItemId` INT NOT NULL,
    `GraphicsElementId` INT NOT NULL,
    CONSTRAINT `PK_ProgramScheduleItemGraphicsElement` PRIMARY KEY (`ProgramScheduleItemId`, `GraphicsElementId`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `ProgramScheduleItemWatermark` (
    `ProgramScheduleItemId` INT NOT NULL,
    `WatermarkId` INT NOT NULL,
    CONSTRAINT `PK_ProgramScheduleItemWatermark` PRIMARY KEY (`ProgramScheduleItemId`, `WatermarkId`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `ProgramScheduleMultipleItem` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Count` LONGTEXT NULL,
    `MultipleMode` INT NOT NULL,
    CONSTRAINT `PK_ProgramScheduleMultipleItem` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `ProgramScheduleOneItem` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    CONSTRAINT `PK_ProgramScheduleOneItem` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `RemoteStream` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Duration` LONGTEXT NULL,
    `FallbackQuery` LONGTEXT NULL,
    `Script` LONGTEXT NULL,
    `Url` LONGTEXT NULL,
    `IsLive` INT NOT NULL DEFAULT 0,
    CONSTRAINT `PK_RemoteStream` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `RemoteStreamMetadata` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `RemoteStreamId` INT NOT NULL,
    `MetadataKind` INT NOT NULL,
    `Title` LONGTEXT NULL,
    `OriginalTitle` LONGTEXT NULL,
    `SortTitle` LONGTEXT NULL,
    `Year` INT NULL,
    `ReleaseDate` LONGTEXT NULL,
    `DateAdded` LONGTEXT NOT NULL,
    `DateUpdated` LONGTEXT NOT NULL,
    `ContentRating` LONGTEXT NULL,
    `Plot` LONGTEXT NULL,
    CONSTRAINT `PK_RemoteStreamMetadata` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `RerunCollection` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `CollectionId` INT NULL,
    `CollectionType` INT NOT NULL,
    `FirstRunPlaybackOrder` INT NOT NULL,
    `MediaItemId` INT NULL,
    `MultiCollectionId` INT NULL,
    `Name` varchar(50) COLLATE utf8mb4_general_ci NULL,
    `RerunPlaybackOrder` INT NOT NULL,
    `SmartCollectionId` INT NULL,
    CONSTRAINT `PK_RerunCollection` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `RerunHistory` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `PlayoutId` INT NOT NULL,
    `RerunCollectionId` INT NOT NULL,
    `MediaItemId` INT NOT NULL,
    `When` varchar(32) NOT NULL,
    CONSTRAINT `PK_RerunHistory` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `Resolution` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Height` INT NOT NULL,
    `Name` LONGTEXT NULL,
    `Width` INT NOT NULL,
    `IsCustom` INT NOT NULL DEFAULT 0,
    CONSTRAINT `PK_Resolution` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `ScheduleItemsEnumeratorState` (
    `PlayoutAnchorPlayoutId` INT NOT NULL,
    `Seed` INT NOT NULL,
    `Index` INT NOT NULL,
    CONSTRAINT `PK_ScheduleItemsEnumeratorState` PRIMARY KEY (`PlayoutAnchorPlayoutId`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `Season` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `ShowId` INT NOT NULL,
    `SeasonNumber` INT NOT NULL DEFAULT 0,
    CONSTRAINT `PK_Season` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `SeasonMetadata` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Outline` LONGTEXT NULL,
    `SeasonId` INT NOT NULL,
    `MetadataKind` INT NOT NULL,
    `Title` LONGTEXT NULL,
    `OriginalTitle` LONGTEXT NULL,
    `SortTitle` LONGTEXT NULL,
    `ReleaseDate` LONGTEXT NULL,
    `DateAdded` LONGTEXT NOT NULL,
    `DateUpdated` LONGTEXT NOT NULL,
    `Year` INT NULL,
    CONSTRAINT `PK_SeasonMetadata` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `Show` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    CONSTRAINT `PK_Show` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `ShowMetadata` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `ContentRating` LONGTEXT NULL,
    `DateAdded` LONGTEXT NOT NULL,
    `DateUpdated` LONGTEXT NOT NULL,
    `MetadataKind` INT NOT NULL,
    `OriginalTitle` LONGTEXT NULL,
    `Outline` LONGTEXT NULL,
    `Plot` LONGTEXT NULL,
    `ReleaseDate` LONGTEXT NULL,
    `ShowId` INT NOT NULL,
    `SortTitle` LONGTEXT NULL,
    `Tagline` LONGTEXT NULL,
    `Title` varchar(255) COLLATE utf8mb4_general_ci NULL,
    `Year` INT NULL,
    CONSTRAINT `PK_ShowMetadata` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `SmartCollection` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Name` varchar(50) COLLATE utf8mb4_general_ci NULL,
    `Query` LONGTEXT NULL,
    CONSTRAINT `PK_SmartCollection` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `Song` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    CONSTRAINT `PK_Song` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `SongMetadata` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `SongId` INT NOT NULL,
    `MetadataKind` INT NOT NULL,
    `Title` LONGTEXT NULL,
    `OriginalTitle` LONGTEXT NULL,
    `SortTitle` LONGTEXT NULL,
    `Year` INT NULL,
    `ReleaseDate` LONGTEXT NULL,
    `DateAdded` LONGTEXT NOT NULL,
    `DateUpdated` LONGTEXT NOT NULL,
    `Album` LONGTEXT NULL,
    `Artists` LONGTEXT NULL,
    `Comment` LONGTEXT NULL,
    `Track` LONGTEXT NULL,
    `AlbumArtists` LONGTEXT NULL,
    CONSTRAINT `PK_SongMetadata` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `Studio` (
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
    CONSTRAINT `PK_Studio` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `Style` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Name` LONGTEXT NULL,
    `ArtistMetadataId` INT NULL,
    CONSTRAINT `PK_Style` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `Subtitle` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `ArtistMetadataId` INT NULL,
    `Codec` LONGTEXT NULL,
    `DateAdded` LONGTEXT NOT NULL,
    `DateUpdated` LONGTEXT NOT NULL,
    `Default` INT NOT NULL,
    `EpisodeMetadataId` INT NULL,
    `Forced` INT NOT NULL,
    `ImageMetadataId` INT NULL,
    `IsExtracted` INT NOT NULL DEFAULT 0,
    `Language` LONGTEXT NULL,
    `MovieMetadataId` INT NULL,
    `MusicVideoMetadataId` INT NULL,
    `OtherVideoMetadataId` INT NULL,
    `Path` LONGTEXT NULL,
    `RemoteStreamMetadataId` INT NULL,
    `SDH` INT NOT NULL DEFAULT 0,
    `SeasonMetadataId` INT NULL,
    `ShowMetadataId` INT NULL,
    `SongMetadataId` INT NULL,
    `StreamIndex` INT NOT NULL,
    `SubtitleKind` INT NOT NULL,
    `Title` LONGTEXT NULL,
    CONSTRAINT `PK_Subtitle` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `Tag` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `ArtistMetadataId` INT NULL,
    `EpisodeMetadataId` INT NULL,
    `ExternalCollectionId` LONGTEXT NULL,
    `ExternalTypeId` LONGTEXT NULL,
    `ImageMetadataId` INT NULL,
    `MovieMetadataId` INT NULL,
    `MusicVideoMetadataId` INT NULL,
    `Name` LONGTEXT NULL,
    `OtherVideoMetadataId` INT NULL,
    `RemoteStreamMetadataId` INT NULL,
    `SeasonMetadataId` INT NULL,
    `ShowMetadataId` INT NULL,
    `SongMetadataId` INT NULL,
    CONSTRAINT `PK_Tag` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `Template` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `DateUpdated` LONGTEXT NOT NULL,
    `Name` varchar(50) COLLATE utf8mb4_general_ci NULL,
    `TemplateGroupId` INT NOT NULL,
    CONSTRAINT `PK_Template` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `TemplateGroup` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Name` varchar(50) COLLATE utf8mb4_general_ci NULL,
    CONSTRAINT `PK_TemplateGroup` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `TemplateItem` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `TemplateId` INT NOT NULL,
    `BlockId` INT NOT NULL,
    `StartTime` varchar(32) NOT NULL,
    CONSTRAINT `PK_TemplateItem` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `TraktList` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `AutoRefresh` INT NOT NULL,
    `Description` LONGTEXT NULL,
    `GeneratePlaylist` INT NOT NULL,
    `ItemCount` INT NOT NULL,
    `LastMatch` LONGTEXT NULL,
    `LastUpdate` LONGTEXT NULL,
    `List` LONGTEXT NULL,
    `Name` varchar(255) COLLATE utf8mb4_general_ci NULL,
    `PlaylistId` INT NULL,
    `TraktId` INT NOT NULL,
    `User` LONGTEXT NULL,
    CONSTRAINT `PK_TraktList` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `TraktListItem` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Episode` INT NULL,
    `Kind` INT NOT NULL,
    `MediaItemId` INT NULL,
    `Rank` INT NOT NULL,
    `Season` INT NULL,
    `Title` LONGTEXT NULL,
    `TraktId` INT NOT NULL,
    `TraktListId` INT NOT NULL,
    `Year` INT NULL,
    CONSTRAINT `PK_TraktListItem` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `TraktListItemGuid` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `Guid` LONGTEXT NULL,
    `TraktListItemId` INT NOT NULL,
    CONSTRAINT `PK_TraktListItemGuid` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `Writer` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `EpisodeMetadataId` INT NULL,
    `MovieMetadataId` INT NULL,
    `Name` LONGTEXT NULL,
    `OtherVideoMetadataId` INT NULL,
    CONSTRAINT `PK_Writer` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `__EFMigrationsHistory` (
    `MigrationId` varchar(150) NOT NULL,
    `ProductVersion` LONGTEXT NOT NULL,
    CONSTRAINT `PK___EFMigrationsHistory` PRIMARY KEY (`MigrationId`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `__EFMigrationsLock` (
    `Id` INT NOT NULL,
    `Timestamp` LONGTEXT NOT NULL,
    CONSTRAINT `PK___EFMigrationsLock` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE INDEX `IX_Actor_ArtistMetadataId` ON `Actor` (`ArtistMetadataId`);
CREATE UNIQUE INDEX `IX_Actor_ArtworkId` ON `Actor` (`ArtworkId`);
CREATE INDEX `IX_Actor_EpisodeMetadataId` ON `Actor` (`EpisodeMetadataId`);
CREATE INDEX `IX_Actor_ImageMetadataId` ON `Actor` (`ImageMetadataId`);
CREATE INDEX `IX_Actor_MovieMetadataId` ON `Actor` (`MovieMetadataId`);
CREATE INDEX `IX_Actor_MusicVideoMetadataId` ON `Actor` (`MusicVideoMetadataId`);
CREATE INDEX `IX_Actor_OtherVideoMetadataId` ON `Actor` (`OtherVideoMetadataId`);
CREATE INDEX `IX_Actor_RemoteStreamMetadataId` ON `Actor` (`RemoteStreamMetadataId`);
CREATE INDEX `IX_Actor_SeasonMetadataId` ON `Actor` (`SeasonMetadataId`);
CREATE INDEX `IX_Actor_ShowMetadataId` ON `Actor` (`ShowMetadataId`);
CREATE INDEX `IX_Actor_SongMetadataId` ON `Actor` (`SongMetadataId`);
CREATE INDEX `IX_ArtistMetadata_ArtistId` ON `ArtistMetadata` (`ArtistId`);
CREATE INDEX `IX_ArtistMetadata_Title` ON `ArtistMetadata` (`Title`);
CREATE INDEX `IX_Artwork_ArtistMetadataId` ON `Artwork` (`ArtistMetadataId`);
CREATE INDEX `IX_Artwork_ChannelId` ON `Artwork` (`ChannelId`);
CREATE INDEX `IX_Artwork_EpisodeMetadataId` ON `Artwork` (`EpisodeMetadataId`);
CREATE INDEX `IX_Artwork_ImageMetadataId` ON `Artwork` (`ImageMetadataId`);
CREATE INDEX `IX_Artwork_IsMetadataOrphan` ON `Artwork` (`IsMetadataOrphan`);
CREATE INDEX `IX_Artwork_MovieMetadataId` ON `Artwork` (`MovieMetadataId`);
CREATE INDEX `IX_Artwork_MusicVideoMetadataId` ON `Artwork` (`MusicVideoMetadataId`);
CREATE INDEX `IX_Artwork_OtherVideoMetadataId` ON `Artwork` (`OtherVideoMetadataId`);
CREATE INDEX `IX_Artwork_RemoteStreamMetadataId` ON `Artwork` (`RemoteStreamMetadataId`);
CREATE INDEX `IX_Artwork_SeasonMetadataId` ON `Artwork` (`SeasonMetadataId`);
CREATE INDEX `IX_Artwork_ShowMetadataId` ON `Artwork` (`ShowMetadataId`);
CREATE INDEX `IX_Artwork_SongMetadataId` ON `Artwork` (`SongMetadataId`);
CREATE UNIQUE INDEX `IX_BlockGroup_Name` ON `BlockGroup` (`Name`);
CREATE INDEX `IX_BlockItemGraphicsElement_GraphicsElementId` ON `BlockItemGraphicsElement` (`GraphicsElementId`);
CREATE INDEX `IX_BlockItemWatermark_WatermarkId` ON `BlockItemWatermark` (`WatermarkId`);
CREATE INDEX `IX_BlockItem_BlockId` ON `BlockItem` (`BlockId`);
CREATE INDEX `IX_BlockItem_CollectionId` ON `BlockItem` (`CollectionId`);
CREATE INDEX `IX_BlockItem_MediaItemId` ON `BlockItem` (`MediaItemId`);
CREATE INDEX `IX_BlockItem_MultiCollectionId` ON `BlockItem` (`MultiCollectionId`);
CREATE INDEX `IX_BlockItem_SmartCollectionId` ON `BlockItem` (`SmartCollectionId`);
CREATE UNIQUE INDEX `IX_Block_BlockGroupId_Name` ON `Block` (`BlockGroupId`, `Name`);
CREATE UNIQUE INDEX `IX_ChannelWatermark_Name` ON `ChannelWatermark` (`Name`);
CREATE INDEX `IX_Channel_FFmpegProfileId` ON `Channel` (`FFmpegProfileId`);
CREATE INDEX `IX_Channel_FallbackFillerId` ON `Channel` (`FallbackFillerId`);
CREATE INDEX `IX_Channel_MirrorSourceChannelId` ON `Channel` (`MirrorSourceChannelId`);
CREATE UNIQUE INDEX `IX_Channel_Number` ON `Channel` (`Number`);
CREATE INDEX `IX_Channel_WatermarkId` ON `Channel` (`WatermarkId`);
CREATE INDEX `IX_CollectionItem_MediaItemId` ON `CollectionItem` (`MediaItemId`);
CREATE UNIQUE INDEX `IX_Collection_Name` ON `Collection` (`Name`);
CREATE UNIQUE INDEX `IX_ConfigElement_Key` ON `ConfigElement` (`Key`);
CREATE INDEX `IX_DecoBreakContent_CollectionId` ON `DecoBreakContent` (`CollectionId`);
CREATE INDEX `IX_DecoBreakContent_DecoId` ON `DecoBreakContent` (`DecoId`);
CREATE INDEX `IX_DecoBreakContent_MediaItemId` ON `DecoBreakContent` (`MediaItemId`);
CREATE INDEX `IX_DecoBreakContent_MultiCollectionId` ON `DecoBreakContent` (`MultiCollectionId`);
CREATE INDEX `IX_DecoBreakContent_PlaylistId` ON `DecoBreakContent` (`PlaylistId`);
CREATE INDEX `IX_DecoBreakContent_SmartCollectionId` ON `DecoBreakContent` (`SmartCollectionId`);
CREATE INDEX `IX_DecoGraphicsElement_GraphicsElementId` ON `DecoGraphicsElement` (`GraphicsElementId`);
CREATE UNIQUE INDEX `IX_DecoGroup_Name` ON `DecoGroup` (`Name`);
CREATE UNIQUE INDEX `IX_DecoTemplateGroup_Name` ON `DecoTemplateGroup` (`Name`);
CREATE INDEX `IX_DecoTemplateItem_DecoId` ON `DecoTemplateItem` (`DecoId`);
CREATE INDEX `IX_DecoTemplateItem_DecoTemplateId` ON `DecoTemplateItem` (`DecoTemplateId`);
CREATE UNIQUE INDEX `IX_DecoTemplate_DecoTemplateGroupId_Name` ON `DecoTemplate` (`DecoTemplateGroupId`, `Name`);
CREATE INDEX `IX_DecoWatermark_WatermarkId` ON `DecoWatermark` (`WatermarkId`);
CREATE INDEX `IX_Deco_DeadAirFallbackCollectionId` ON `Deco` (`DeadAirFallbackCollectionId`);
CREATE INDEX `IX_Deco_DeadAirFallbackMediaItemId` ON `Deco` (`DeadAirFallbackMediaItemId`);
CREATE INDEX `IX_Deco_DeadAirFallbackMultiCollectionId` ON `Deco` (`DeadAirFallbackMultiCollectionId`);
CREATE INDEX `IX_Deco_DeadAirFallbackSmartCollectionId` ON `Deco` (`DeadAirFallbackSmartCollectionId`);
CREATE UNIQUE INDEX `IX_Deco_DecoGroupId_Name` ON `Deco` (`DecoGroupId`, `Name`);
CREATE INDEX `IX_Deco_DefaultFillerCollectionId` ON `Deco` (`DefaultFillerCollectionId`);
CREATE INDEX `IX_Deco_DefaultFillerMediaItemId` ON `Deco` (`DefaultFillerMediaItemId`);
CREATE INDEX `IX_Deco_DefaultFillerMultiCollectionId` ON `Deco` (`DefaultFillerMultiCollectionId`);
CREATE INDEX `IX_Deco_DefaultFillerSmartCollectionId` ON `Deco` (`DefaultFillerSmartCollectionId`);
CREATE INDEX `IX_Director_EpisodeMetadataId` ON `Director` (`EpisodeMetadataId`);
CREATE INDEX `IX_Director_MovieMetadataId` ON `Director` (`MovieMetadataId`);
CREATE INDEX `IX_Director_MusicVideoMetadataId` ON `Director` (`MusicVideoMetadataId`);
CREATE INDEX `IX_Director_OtherVideoMetadataId` ON `Director` (`OtherVideoMetadataId`);
CREATE INDEX `IX_EmbyConnection_EmbyMediaSourceId` ON `EmbyConnection` (`EmbyMediaSourceId`);
CREATE INDEX `IX_EmbyPathInfo_EmbyLibraryId` ON `EmbyPathInfo` (`EmbyLibraryId`);
CREATE INDEX `IX_EmbyPathReplacement_EmbyMediaSourceId` ON `EmbyPathReplacement` (`EmbyMediaSourceId`);
CREATE INDEX `IX_EpisodeMetadata_EpisodeId` ON `EpisodeMetadata` (`EpisodeId`);
CREATE INDEX `IX_Episode_SeasonId` ON `Episode` (`SeasonId`);
CREATE INDEX `IX_FFmpegProfile_ResolutionId` ON `FFmpegProfile` (`ResolutionId`);
CREATE INDEX `IX_FillerPreset_CollectionId` ON `FillerPreset` (`CollectionId`);
CREATE INDEX `IX_FillerPreset_MediaItemId` ON `FillerPreset` (`MediaItemId`);
CREATE INDEX `IX_FillerPreset_MultiCollectionId` ON `FillerPreset` (`MultiCollectionId`);
CREATE UNIQUE INDEX `IX_FillerPreset_Name` ON `FillerPreset` (`Name`);
CREATE INDEX `IX_FillerPreset_PlaylistId` ON `FillerPreset` (`PlaylistId`);
CREATE INDEX `IX_FillerPreset_SmartCollectionId` ON `FillerPreset` (`SmartCollectionId`);
CREATE INDEX `IX_Genre_ArtistMetadataId` ON `Genre` (`ArtistMetadataId`);
CREATE INDEX `IX_Genre_EpisodeMetadataId` ON `Genre` (`EpisodeMetadataId`);
CREATE INDEX `IX_Genre_ImageMetadataId` ON `Genre` (`ImageMetadataId`);
CREATE INDEX `IX_Genre_MovieMetadataId` ON `Genre` (`MovieMetadataId`);
CREATE INDEX `IX_Genre_MusicVideoMetadataId` ON `Genre` (`MusicVideoMetadataId`);
CREATE INDEX `IX_Genre_OtherVideoMetadataId` ON `Genre` (`OtherVideoMetadataId`);
CREATE INDEX `IX_Genre_RemoteStreamMetadataId` ON `Genre` (`RemoteStreamMetadataId`);
CREATE INDEX `IX_Genre_SeasonMetadataId` ON `Genre` (`SeasonMetadataId`);
CREATE INDEX `IX_Genre_ShowMetadataId` ON `Genre` (`ShowMetadataId`);
CREATE INDEX `IX_Genre_SongMetadataId` ON `Genre` (`SongMetadataId`);
CREATE UNIQUE INDEX `IX_ImageFolderDuration_LibraryFolderId` ON `ImageFolderDuration` (`LibraryFolderId`);
CREATE INDEX `IX_ImageMetadata_ImageId` ON `ImageMetadata` (`ImageId`);
CREATE INDEX `IX_JellyfinCollection_ItemId` ON `JellyfinCollection` (`ItemId`);
CREATE INDEX `IX_JellyfinConnection_JellyfinMediaSourceId` ON `JellyfinConnection` (`JellyfinMediaSourceId`);
CREATE INDEX `IX_JellyfinEpisode_ItemId` ON `JellyfinEpisode` (`ItemId`);
CREATE INDEX `IX_JellyfinLibrary_ItemId` ON `JellyfinLibrary` (`ItemId`);
CREATE INDEX `IX_JellyfinMovie_ItemId` ON `JellyfinMovie` (`ItemId`);
CREATE INDEX `IX_JellyfinPathInfo_JellyfinLibraryId` ON `JellyfinPathInfo` (`JellyfinLibraryId`);
CREATE INDEX `IX_JellyfinPathReplacement_JellyfinMediaSourceId` ON `JellyfinPathReplacement` (`JellyfinMediaSourceId`);
CREATE INDEX `IX_JellyfinSeason_ItemId` ON `JellyfinSeason` (`ItemId`);
CREATE INDEX `IX_JellyfinShow_ItemId` ON `JellyfinShow` (`ItemId`);
CREATE INDEX `IX_LibraryFolder_LibraryPathId` ON `LibraryFolder` (`LibraryPathId`);
CREATE INDEX `IX_LibraryFolder_ParentId` ON `LibraryFolder` (`ParentId`);
CREATE INDEX `IX_LibraryPath_LibraryId` ON `LibraryPath` (`LibraryId`);
CREATE INDEX `IX_Library_MediaSourceId` ON `Library` (`MediaSourceId`);
CREATE INDEX `IX_MediaChapter_MediaVersionId` ON `MediaChapter` (`MediaVersionId`);
CREATE INDEX `IX_MediaFile_LibraryFolderId` ON `MediaFile` (`LibraryFolderId`);
CREATE INDEX `IX_MediaFile_MediaVersionId` ON `MediaFile` (`MediaVersionId`);
CREATE UNIQUE INDEX `IX_MediaFile_PathHash` ON `MediaFile` (`PathHash`);
CREATE INDEX `IX_MediaItem_LibraryPathId` ON `MediaItem` (`LibraryPathId`);
CREATE INDEX `IX_MediaStream_MediaVersionId` ON `MediaStream` (`MediaVersionId`);
CREATE INDEX `IX_MediaVersion_EpisodeId` ON `MediaVersion` (`EpisodeId`);
CREATE INDEX `IX_MediaVersion_ImageId` ON `MediaVersion` (`ImageId`);
CREATE INDEX `IX_MediaVersion_MovieId` ON `MediaVersion` (`MovieId`);
CREATE INDEX `IX_MediaVersion_MusicVideoId` ON `MediaVersion` (`MusicVideoId`);
CREATE INDEX `IX_MediaVersion_OtherVideoId` ON `MediaVersion` (`OtherVideoId`);
CREATE INDEX `IX_MediaVersion_RemoteStreamId` ON `MediaVersion` (`RemoteStreamId`);
CREATE INDEX `IX_MediaVersion_SongId` ON `MediaVersion` (`SongId`);
CREATE INDEX `IX_MetadataGuid_ArtistMetadataId` ON `MetadataGuid` (`ArtistMetadataId`);
CREATE INDEX `IX_MetadataGuid_EpisodeMetadataId` ON `MetadataGuid` (`EpisodeMetadataId`);
CREATE INDEX `IX_MetadataGuid_Guid` ON `MetadataGuid` (`Guid`);
CREATE INDEX `IX_MetadataGuid_ImageMetadataId` ON `MetadataGuid` (`ImageMetadataId`);
CREATE INDEX `IX_MetadataGuid_MovieMetadataId` ON `MetadataGuid` (`MovieMetadataId`);
CREATE INDEX `IX_MetadataGuid_MusicVideoMetadataId` ON `MetadataGuid` (`MusicVideoMetadataId`);
CREATE INDEX `IX_MetadataGuid_OtherVideoMetadataId` ON `MetadataGuid` (`OtherVideoMetadataId`);
CREATE INDEX `IX_MetadataGuid_RemoteStreamMetadataId` ON `MetadataGuid` (`RemoteStreamMetadataId`);
CREATE INDEX `IX_MetadataGuid_SeasonMetadataId` ON `MetadataGuid` (`SeasonMetadataId`);
CREATE INDEX `IX_MetadataGuid_ShowMetadataId` ON `MetadataGuid` (`ShowMetadataId`);
CREATE INDEX `IX_MetadataGuid_SongMetadataId` ON `MetadataGuid` (`SongMetadataId`);
CREATE INDEX `IX_Mood_ArtistMetadataId` ON `Mood` (`ArtistMetadataId`);
CREATE INDEX `IX_MovieMetadata_MovieId` ON `MovieMetadata` (`MovieId`);
CREATE INDEX `IX_MovieMetadata_Title` ON `MovieMetadata` (`Title`);
CREATE INDEX `IX_MultiCollectionItem_CollectionId` ON `MultiCollectionItem` (`CollectionId`);
CREATE INDEX `IX_MultiCollectionSmartItem_SmartCollectionId` ON `MultiCollectionSmartItem` (`SmartCollectionId`);
CREATE UNIQUE INDEX `IX_MultiCollection_Name` ON `MultiCollection` (`Name`);
CREATE INDEX `IX_MusicVideoArtist_MusicVideoMetadataId` ON `MusicVideoArtist` (`MusicVideoMetadataId`);
CREATE INDEX `IX_MusicVideoMetadata_MusicVideoId` ON `MusicVideoMetadata` (`MusicVideoId`);
CREATE INDEX `IX_MusicVideo_ArtistId` ON `MusicVideo` (`ArtistId`);
CREATE INDEX `IX_OtherVideoMetadata_OtherVideoId` ON `OtherVideoMetadata` (`OtherVideoId`);
CREATE UNIQUE INDEX `IX_PlaylistGroup_Name` ON `PlaylistGroup` (`Name`);
CREATE INDEX `IX_PlaylistItem_CollectionId` ON `PlaylistItem` (`CollectionId`);
CREATE INDEX `IX_PlaylistItem_MediaItemId` ON `PlaylistItem` (`MediaItemId`);
CREATE INDEX `IX_PlaylistItem_MultiCollectionId` ON `PlaylistItem` (`MultiCollectionId`);
CREATE INDEX `IX_PlaylistItem_PlaylistId` ON `PlaylistItem` (`PlaylistId`);
CREATE INDEX `IX_PlaylistItem_SmartCollectionId` ON `PlaylistItem` (`SmartCollectionId`);
CREATE UNIQUE INDEX `IX_Playlist_PlaylistGroupId_Name` ON `Playlist` (`PlaylistGroupId`, `Name`);
CREATE INDEX `IX_PlayoutGap_PlayoutId` ON `PlayoutGap` (`PlayoutId`);
CREATE INDEX `IX_PlayoutGap_Start_Finish` ON `PlayoutGap` (`Start`, `Finish`);
CREATE INDEX `IX_PlayoutHistory_BlockId` ON `PlayoutHistory` (`BlockId`);
CREATE INDEX `IX_PlayoutHistory_PlayoutId` ON `PlayoutHistory` (`PlayoutId`);
CREATE INDEX `IX_PlayoutItemGraphicsElement_GraphicsElementId` ON `PlayoutItemGraphicsElement` (`GraphicsElementId`);
CREATE INDEX `IX_PlayoutItemWatermark_WatermarkId` ON `PlayoutItemWatermark` (`WatermarkId`);
CREATE INDEX `IX_PlayoutItem_MediaItemId` ON `PlayoutItem` (`MediaItemId`);
CREATE INDEX `IX_PlayoutItem_PlayoutId` ON `PlayoutItem` (`PlayoutId`);
CREATE INDEX `IX_PlayoutItem_Start_Finish` ON `PlayoutItem` (`Start`, `Finish`);
CREATE INDEX `IX_PlayoutProgramScheduleAnchor_CollectionId` ON `PlayoutProgramScheduleAnchor` (`CollectionId`);
CREATE INDEX `IX_PlayoutProgramScheduleAnchor_MediaItemId` ON `PlayoutProgramScheduleAnchor` (`MediaItemId`);
CREATE INDEX `IX_PlayoutProgramScheduleAnchor_MultiCollectionId` ON `PlayoutProgramScheduleAnchor` (`MultiCollectionId`);
CREATE INDEX `IX_PlayoutProgramScheduleAnchor_PlaylistId` ON `PlayoutProgramScheduleAnchor` (`PlaylistId`);
CREATE INDEX `IX_PlayoutProgramScheduleAnchor_PlayoutId` ON `PlayoutProgramScheduleAnchor` (`PlayoutId`);
CREATE INDEX `IX_PlayoutProgramScheduleAnchor_RerunCollectionId` ON `PlayoutProgramScheduleAnchor` (`RerunCollectionId`);
CREATE INDEX `IX_PlayoutProgramScheduleAnchor_SmartCollectionId` ON `PlayoutProgramScheduleAnchor` (`SmartCollectionId`);
CREATE INDEX `IX_PlayoutScheduleItemFillGroupIndex_PlayoutId` ON `PlayoutScheduleItemFillGroupIndex` (`PlayoutId`);
CREATE INDEX `IX_PlayoutScheduleItemFillGroupIndex_ProgramScheduleItemId` ON `PlayoutScheduleItemFillGroupIndex` (`ProgramScheduleItemId`);
CREATE INDEX `IX_PlayoutTemplate_DecoTemplateId` ON `PlayoutTemplate` (`DecoTemplateId`);
CREATE INDEX `IX_PlayoutTemplate_PlayoutId` ON `PlayoutTemplate` (`PlayoutId`);
CREATE INDEX `IX_PlayoutTemplate_TemplateId` ON `PlayoutTemplate` (`TemplateId`);
CREATE INDEX `IX_Playout_ChannelId` ON `Playout` (`ChannelId`);
CREATE INDEX `IX_Playout_DecoId` ON `Playout` (`DecoId`);
CREATE INDEX `IX_Playout_ProgramScheduleId` ON `Playout` (`ProgramScheduleId`);
CREATE INDEX `IX_PlexConnection_PlexMediaSourceId` ON `PlexConnection` (`PlexMediaSourceId`);
CREATE INDEX `IX_PlexPathReplacement_PlexMediaSourceId` ON `PlexPathReplacement` (`PlexMediaSourceId`);
CREATE INDEX `IX_ProgramScheduleAlternate_PlayoutId` ON `ProgramScheduleAlternate` (`PlayoutId`);
CREATE INDEX `IX_ProgramScheduleAlternate_ProgramScheduleId` ON `ProgramScheduleAlternate` (`ProgramScheduleId`);
CREATE INDEX `IX_ProgramScheduleItemGraphicsElement_GraphicsElementId` ON `ProgramScheduleItemGraphicsElement` (`GraphicsElementId`);
CREATE INDEX `IX_ProgramScheduleItemWatermark_WatermarkId` ON `ProgramScheduleItemWatermark` (`WatermarkId`);
CREATE INDEX `IX_ProgramScheduleItem_CollectionId` ON `ProgramScheduleItem` (`CollectionId`);
CREATE INDEX `IX_ProgramScheduleItem_FallbackFillerId` ON `ProgramScheduleItem` (`FallbackFillerId`);
CREATE INDEX `IX_ProgramScheduleItem_MediaItemId` ON `ProgramScheduleItem` (`MediaItemId`);
CREATE INDEX `IX_ProgramScheduleItem_MidRollFillerId` ON `ProgramScheduleItem` (`MidRollFillerId`);
CREATE INDEX `IX_ProgramScheduleItem_MultiCollectionId` ON `ProgramScheduleItem` (`MultiCollectionId`);
CREATE INDEX `IX_ProgramScheduleItem_PlaylistId` ON `ProgramScheduleItem` (`PlaylistId`);
CREATE INDEX `IX_ProgramScheduleItem_PostRollFillerId` ON `ProgramScheduleItem` (`PostRollFillerId`);
CREATE INDEX `IX_ProgramScheduleItem_PreRollFillerId` ON `ProgramScheduleItem` (`PreRollFillerId`);
CREATE INDEX `IX_ProgramScheduleItem_ProgramScheduleId` ON `ProgramScheduleItem` (`ProgramScheduleId`);
CREATE INDEX `IX_ProgramScheduleItem_RerunCollectionId` ON `ProgramScheduleItem` (`RerunCollectionId`);
CREATE INDEX `IX_ProgramScheduleItem_SmartCollectionId` ON `ProgramScheduleItem` (`SmartCollectionId`);
CREATE INDEX `IX_ProgramScheduleItem_TailFillerId` ON `ProgramScheduleItem` (`TailFillerId`);
CREATE UNIQUE INDEX `IX_ProgramSchedule_Name` ON `ProgramSchedule` (`Name`);
CREATE INDEX `IX_RemoteStreamMetadata_RemoteStreamId` ON `RemoteStreamMetadata` (`RemoteStreamId`);
CREATE INDEX `IX_RerunCollection_CollectionId` ON `RerunCollection` (`CollectionId`);
CREATE INDEX `IX_RerunCollection_MediaItemId` ON `RerunCollection` (`MediaItemId`);
CREATE INDEX `IX_RerunCollection_MultiCollectionId` ON `RerunCollection` (`MultiCollectionId`);
CREATE UNIQUE INDEX `IX_RerunCollection_Name` ON `RerunCollection` (`Name`);
CREATE INDEX `IX_RerunCollection_SmartCollectionId` ON `RerunCollection` (`SmartCollectionId`);
CREATE INDEX `IX_RerunHistory_MediaItemId` ON `RerunHistory` (`MediaItemId`);
CREATE INDEX `IX_RerunHistory_PlayoutId` ON `RerunHistory` (`PlayoutId`);
CREATE INDEX `IX_RerunHistory_RerunCollectionId` ON `RerunHistory` (`RerunCollectionId`);
CREATE INDEX `IX_SeasonMetadata_SeasonId` ON `SeasonMetadata` (`SeasonId`);
CREATE INDEX `IX_Season_ShowId` ON `Season` (`ShowId`);
CREATE INDEX `IX_ShowMetadata_ShowId` ON `ShowMetadata` (`ShowId`);
CREATE INDEX `IX_ShowMetadata_Title` ON `ShowMetadata` (`Title`);
CREATE UNIQUE INDEX `IX_SmartCollection_Name` ON `SmartCollection` (`Name`);
CREATE INDEX `IX_SongMetadata_SongId` ON `SongMetadata` (`SongId`);
CREATE INDEX `IX_Studio_ArtistMetadataId` ON `Studio` (`ArtistMetadataId`);
CREATE INDEX `IX_Studio_EpisodeMetadataId` ON `Studio` (`EpisodeMetadataId`);
CREATE INDEX `IX_Studio_ImageMetadataId` ON `Studio` (`ImageMetadataId`);
CREATE INDEX `IX_Studio_MovieMetadataId` ON `Studio` (`MovieMetadataId`);
CREATE INDEX `IX_Studio_MusicVideoMetadataId` ON `Studio` (`MusicVideoMetadataId`);
CREATE INDEX `IX_Studio_OtherVideoMetadataId` ON `Studio` (`OtherVideoMetadataId`);
CREATE INDEX `IX_Studio_RemoteStreamMetadataId` ON `Studio` (`RemoteStreamMetadataId`);
CREATE INDEX `IX_Studio_SeasonMetadataId` ON `Studio` (`SeasonMetadataId`);
CREATE INDEX `IX_Studio_ShowMetadataId` ON `Studio` (`ShowMetadataId`);
CREATE INDEX `IX_Studio_SongMetadataId` ON `Studio` (`SongMetadataId`);
CREATE INDEX `IX_Style_ArtistMetadataId` ON `Style` (`ArtistMetadataId`);
CREATE INDEX `IX_Subtitle_ArtistMetadataId` ON `Subtitle` (`ArtistMetadataId`);
CREATE INDEX `IX_Subtitle_EpisodeMetadataId` ON `Subtitle` (`EpisodeMetadataId`);
CREATE INDEX `IX_Subtitle_ImageMetadataId` ON `Subtitle` (`ImageMetadataId`);
CREATE INDEX `IX_Subtitle_MovieMetadataId` ON `Subtitle` (`MovieMetadataId`);
CREATE INDEX `IX_Subtitle_MusicVideoMetadataId` ON `Subtitle` (`MusicVideoMetadataId`);
CREATE INDEX `IX_Subtitle_OtherVideoMetadataId` ON `Subtitle` (`OtherVideoMetadataId`);
CREATE INDEX `IX_Subtitle_RemoteStreamMetadataId` ON `Subtitle` (`RemoteStreamMetadataId`);
CREATE INDEX `IX_Subtitle_SeasonMetadataId` ON `Subtitle` (`SeasonMetadataId`);
CREATE INDEX `IX_Subtitle_ShowMetadataId` ON `Subtitle` (`ShowMetadataId`);
CREATE INDEX `IX_Subtitle_SongMetadataId` ON `Subtitle` (`SongMetadataId`);
CREATE INDEX `IX_Tag_ArtistMetadataId` ON `Tag` (`ArtistMetadataId`);
CREATE INDEX `IX_Tag_EpisodeMetadataId` ON `Tag` (`EpisodeMetadataId`);
CREATE INDEX `IX_Tag_ImageMetadataId` ON `Tag` (`ImageMetadataId`);
CREATE INDEX `IX_Tag_MovieMetadataId` ON `Tag` (`MovieMetadataId`);
CREATE INDEX `IX_Tag_MusicVideoMetadataId` ON `Tag` (`MusicVideoMetadataId`);
CREATE INDEX `IX_Tag_OtherVideoMetadataId` ON `Tag` (`OtherVideoMetadataId`);
CREATE INDEX `IX_Tag_RemoteStreamMetadataId` ON `Tag` (`RemoteStreamMetadataId`);
CREATE INDEX `IX_Tag_SeasonMetadataId` ON `Tag` (`SeasonMetadataId`);
CREATE INDEX `IX_Tag_ShowMetadataId` ON `Tag` (`ShowMetadataId`);
CREATE INDEX `IX_Tag_SongMetadataId` ON `Tag` (`SongMetadataId`);
CREATE UNIQUE INDEX `IX_TemplateGroup_Name` ON `TemplateGroup` (`Name`);
CREATE INDEX `IX_TemplateItem_BlockId` ON `TemplateItem` (`BlockId`);
CREATE INDEX `IX_TemplateItem_TemplateId` ON `TemplateItem` (`TemplateId`);
CREATE UNIQUE INDEX `IX_Template_TemplateGroupId_Name` ON `Template` (`TemplateGroupId`, `Name`);
CREATE INDEX `IX_TraktListItemGuid_TraktListItemId` ON `TraktListItemGuid` (`TraktListItemId`);
CREATE INDEX `IX_TraktListItem_MediaItemId` ON `TraktListItem` (`MediaItemId`);
CREATE INDEX `IX_TraktListItem_TraktListId` ON `TraktListItem` (`TraktListId`);
CREATE INDEX `IX_TraktList_Name` ON `TraktList` (`Name`);
CREATE INDEX `IX_TraktList_PlaylistId` ON `TraktList` (`PlaylistId`);
CREATE INDEX `IX_Writer_EpisodeMetadataId` ON `Writer` (`EpisodeMetadataId`);
CREATE INDEX `IX_Writer_MovieMetadataId` ON `Writer` (`MovieMetadataId`);
CREATE INDEX `IX_Writer_OtherVideoMetadataId` ON `Writer` (`OtherVideoMetadataId`);

ALTER TABLE `Actor` ADD CONSTRAINT `FK_Actor_ArtistMetadata_ArtistMetadataId` FOREIGN KEY (`ArtistMetadataId`) REFERENCES `ArtistMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Actor` ADD CONSTRAINT `FK_Actor_Artwork_ArtworkId` FOREIGN KEY (`ArtworkId`) REFERENCES `Artwork` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Actor` ADD CONSTRAINT `FK_Actor_EpisodeMetadata_EpisodeMetadataId` FOREIGN KEY (`EpisodeMetadataId`) REFERENCES `EpisodeMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Actor` ADD CONSTRAINT `FK_Actor_ImageMetadata_ImageMetadataId` FOREIGN KEY (`ImageMetadataId`) REFERENCES `ImageMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Actor` ADD CONSTRAINT `FK_Actor_MovieMetadata_MovieMetadataId` FOREIGN KEY (`MovieMetadataId`) REFERENCES `MovieMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Actor` ADD CONSTRAINT `FK_Actor_MusicVideoMetadata_MusicVideoMetadataId` FOREIGN KEY (`MusicVideoMetadataId`) REFERENCES `MusicVideoMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Actor` ADD CONSTRAINT `FK_Actor_OtherVideoMetadata_OtherVideoMetadataId` FOREIGN KEY (`OtherVideoMetadataId`) REFERENCES `OtherVideoMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Actor` ADD CONSTRAINT `FK_Actor_RemoteStreamMetadata_RemoteStreamMetadataId` FOREIGN KEY (`RemoteStreamMetadataId`) REFERENCES `RemoteStreamMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Actor` ADD CONSTRAINT `FK_Actor_SeasonMetadata_SeasonMetadataId` FOREIGN KEY (`SeasonMetadataId`) REFERENCES `SeasonMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Actor` ADD CONSTRAINT `FK_Actor_ShowMetadata_ShowMetadataId` FOREIGN KEY (`ShowMetadataId`) REFERENCES `ShowMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Actor` ADD CONSTRAINT `FK_Actor_SongMetadata_SongMetadataId` FOREIGN KEY (`SongMetadataId`) REFERENCES `SongMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Artist` ADD CONSTRAINT `FK_Artist_MediaItem_Id` FOREIGN KEY (`Id`) REFERENCES `MediaItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `ArtistMetadata` ADD CONSTRAINT `FK_ArtistMetadata_Artist_ArtistId` FOREIGN KEY (`ArtistId`) REFERENCES `Artist` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Artwork` ADD CONSTRAINT `FK_Artwork_ArtistMetadata_ArtistMetadataId` FOREIGN KEY (`ArtistMetadataId`) REFERENCES `ArtistMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Artwork` ADD CONSTRAINT `FK_Artwork_Channel_ChannelId` FOREIGN KEY (`ChannelId`) REFERENCES `Channel` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Artwork` ADD CONSTRAINT `FK_Artwork_EpisodeMetadata_EpisodeMetadataId` FOREIGN KEY (`EpisodeMetadataId`) REFERENCES `EpisodeMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Artwork` ADD CONSTRAINT `FK_Artwork_ImageMetadata_ImageMetadataId` FOREIGN KEY (`ImageMetadataId`) REFERENCES `ImageMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Artwork` ADD CONSTRAINT `FK_Artwork_MovieMetadata_MovieMetadataId` FOREIGN KEY (`MovieMetadataId`) REFERENCES `MovieMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Artwork` ADD CONSTRAINT `FK_Artwork_MusicVideoMetadata_MusicVideoMetadataId` FOREIGN KEY (`MusicVideoMetadataId`) REFERENCES `MusicVideoMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Artwork` ADD CONSTRAINT `FK_Artwork_OtherVideoMetadata_OtherVideoMetadataId` FOREIGN KEY (`OtherVideoMetadataId`) REFERENCES `OtherVideoMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Artwork` ADD CONSTRAINT `FK_Artwork_RemoteStreamMetadata_RemoteStreamMetadataId` FOREIGN KEY (`RemoteStreamMetadataId`) REFERENCES `RemoteStreamMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Artwork` ADD CONSTRAINT `FK_Artwork_SeasonMetadata_SeasonMetadataId` FOREIGN KEY (`SeasonMetadataId`) REFERENCES `SeasonMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Artwork` ADD CONSTRAINT `FK_Artwork_ShowMetadata_ShowMetadataId` FOREIGN KEY (`ShowMetadataId`) REFERENCES `ShowMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Artwork` ADD CONSTRAINT `FK_Artwork_SongMetadata_SongMetadataId` FOREIGN KEY (`SongMetadataId`) REFERENCES `SongMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Block` ADD CONSTRAINT `FK_Block_BlockGroup_BlockGroupId` FOREIGN KEY (`BlockGroupId`) REFERENCES `BlockGroup` (`Id`) ON DELETE CASCADE;
ALTER TABLE `BlockItem` ADD CONSTRAINT `FK_BlockItem_Block_BlockId` FOREIGN KEY (`BlockId`) REFERENCES `Block` (`Id`) ON DELETE CASCADE;
ALTER TABLE `BlockItem` ADD CONSTRAINT `FK_BlockItem_Collection_CollectionId` FOREIGN KEY (`CollectionId`) REFERENCES `Collection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `BlockItem` ADD CONSTRAINT `FK_BlockItem_MediaItem_MediaItemId` FOREIGN KEY (`MediaItemId`) REFERENCES `MediaItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `BlockItem` ADD CONSTRAINT `FK_BlockItem_MultiCollection_MultiCollectionId` FOREIGN KEY (`MultiCollectionId`) REFERENCES `MultiCollection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `BlockItem` ADD CONSTRAINT `FK_BlockItem_SmartCollection_SmartCollectionId` FOREIGN KEY (`SmartCollectionId`) REFERENCES `SmartCollection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `BlockItemGraphicsElement` ADD CONSTRAINT `FK_BlockItemGraphicsElement_BlockItem_BlockItemId` FOREIGN KEY (`BlockItemId`) REFERENCES `BlockItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `BlockItemGraphicsElement` ADD CONSTRAINT `FK_BlockItemGraphicsElement_GraphicsElement_GraphicsElementId` FOREIGN KEY (`GraphicsElementId`) REFERENCES `GraphicsElement` (`Id`) ON DELETE CASCADE;
ALTER TABLE `BlockItemWatermark` ADD CONSTRAINT `FK_BlockItemWatermark_BlockItem_BlockItemId` FOREIGN KEY (`BlockItemId`) REFERENCES `BlockItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `BlockItemWatermark` ADD CONSTRAINT `FK_BlockItemWatermark_ChannelWatermark_WatermarkId` FOREIGN KEY (`WatermarkId`) REFERENCES `ChannelWatermark` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Channel` ADD CONSTRAINT `FK_Channel_ChannelWatermark_WatermarkId` FOREIGN KEY (`WatermarkId`) REFERENCES `ChannelWatermark` (`Id`) ON DELETE SET NULL;
ALTER TABLE `Channel` ADD CONSTRAINT `FK_Channel_Channel_MirrorSourceChannelId` FOREIGN KEY (`MirrorSourceChannelId`) REFERENCES `Channel` (`Id`) ON DELETE SET NULL;
ALTER TABLE `Channel` ADD CONSTRAINT `FK_Channel_FFmpegProfile_FFmpegProfileId` FOREIGN KEY (`FFmpegProfileId`) REFERENCES `FFmpegProfile` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Channel` ADD CONSTRAINT `FK_Channel_FillerPreset_FallbackFillerId` FOREIGN KEY (`FallbackFillerId`) REFERENCES `FillerPreset` (`Id`) ON DELETE SET NULL;
ALTER TABLE `CollectionEnumeratorState` ADD CONSTRAINT `FK_CollectionEnumeratorState_PlayoutProgramScheduleAnchor_Playo~` FOREIGN KEY (`PlayoutProgramScheduleAnchorId`) REFERENCES `PlayoutProgramScheduleAnchor` (`Id`) ON DELETE CASCADE;
ALTER TABLE `CollectionItem` ADD CONSTRAINT `FK_CollectionItem_Collection_CollectionId` FOREIGN KEY (`CollectionId`) REFERENCES `Collection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `CollectionItem` ADD CONSTRAINT `FK_CollectionItem_MediaItem_MediaItemId` FOREIGN KEY (`MediaItemId`) REFERENCES `MediaItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Deco` ADD CONSTRAINT `FK_Deco_Collection_DeadAirFallbackCollectionId` FOREIGN KEY (`DeadAirFallbackCollectionId`) REFERENCES `Collection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Deco` ADD CONSTRAINT `FK_Deco_Collection_DefaultFillerCollectionId` FOREIGN KEY (`DefaultFillerCollectionId`) REFERENCES `Collection` (`Id`);
ALTER TABLE `Deco` ADD CONSTRAINT `FK_Deco_DecoGroup_DecoGroupId` FOREIGN KEY (`DecoGroupId`) REFERENCES `DecoGroup` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Deco` ADD CONSTRAINT `FK_Deco_MediaItem_DeadAirFallbackMediaItemId` FOREIGN KEY (`DeadAirFallbackMediaItemId`) REFERENCES `MediaItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Deco` ADD CONSTRAINT `FK_Deco_MediaItem_DefaultFillerMediaItemId` FOREIGN KEY (`DefaultFillerMediaItemId`) REFERENCES `MediaItem` (`Id`);
ALTER TABLE `Deco` ADD CONSTRAINT `FK_Deco_MultiCollection_DeadAirFallbackMultiCollectionId` FOREIGN KEY (`DeadAirFallbackMultiCollectionId`) REFERENCES `MultiCollection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Deco` ADD CONSTRAINT `FK_Deco_MultiCollection_DefaultFillerMultiCollectionId` FOREIGN KEY (`DefaultFillerMultiCollectionId`) REFERENCES `MultiCollection` (`Id`);
ALTER TABLE `Deco` ADD CONSTRAINT `FK_Deco_SmartCollection_DeadAirFallbackSmartCollectionId` FOREIGN KEY (`DeadAirFallbackSmartCollectionId`) REFERENCES `SmartCollection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Deco` ADD CONSTRAINT `FK_Deco_SmartCollection_DefaultFillerSmartCollectionId` FOREIGN KEY (`DefaultFillerSmartCollectionId`) REFERENCES `SmartCollection` (`Id`);
ALTER TABLE `DecoBreakContent` ADD CONSTRAINT `FK_DecoBreakContent_Collection_CollectionId` FOREIGN KEY (`CollectionId`) REFERENCES `Collection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `DecoBreakContent` ADD CONSTRAINT `FK_DecoBreakContent_Deco_DecoId` FOREIGN KEY (`DecoId`) REFERENCES `Deco` (`Id`) ON DELETE CASCADE;
ALTER TABLE `DecoBreakContent` ADD CONSTRAINT `FK_DecoBreakContent_MediaItem_MediaItemId` FOREIGN KEY (`MediaItemId`) REFERENCES `MediaItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `DecoBreakContent` ADD CONSTRAINT `FK_DecoBreakContent_MultiCollection_MultiCollectionId` FOREIGN KEY (`MultiCollectionId`) REFERENCES `MultiCollection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `DecoBreakContent` ADD CONSTRAINT `FK_DecoBreakContent_Playlist_PlaylistId` FOREIGN KEY (`PlaylistId`) REFERENCES `Playlist` (`Id`) ON DELETE CASCADE;
ALTER TABLE `DecoBreakContent` ADD CONSTRAINT `FK_DecoBreakContent_SmartCollection_SmartCollectionId` FOREIGN KEY (`SmartCollectionId`) REFERENCES `SmartCollection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `DecoGraphicsElement` ADD CONSTRAINT `FK_DecoGraphicsElement_Deco_DecoId` FOREIGN KEY (`DecoId`) REFERENCES `Deco` (`Id`) ON DELETE CASCADE;
ALTER TABLE `DecoGraphicsElement` ADD CONSTRAINT `FK_DecoGraphicsElement_GraphicsElement_GraphicsElementId` FOREIGN KEY (`GraphicsElementId`) REFERENCES `GraphicsElement` (`Id`) ON DELETE CASCADE;
ALTER TABLE `DecoTemplate` ADD CONSTRAINT `FK_DecoTemplate_DecoTemplateGroup_DecoTemplateGroupId` FOREIGN KEY (`DecoTemplateGroupId`) REFERENCES `DecoTemplateGroup` (`Id`) ON DELETE CASCADE;
ALTER TABLE `DecoTemplateItem` ADD CONSTRAINT `FK_DecoTemplateItem_DecoTemplate_DecoTemplateId` FOREIGN KEY (`DecoTemplateId`) REFERENCES `DecoTemplate` (`Id`) ON DELETE CASCADE;
ALTER TABLE `DecoTemplateItem` ADD CONSTRAINT `FK_DecoTemplateItem_Deco_DecoId` FOREIGN KEY (`DecoId`) REFERENCES `Deco` (`Id`) ON DELETE CASCADE;
ALTER TABLE `DecoWatermark` ADD CONSTRAINT `FK_DecoWatermark_ChannelWatermark_WatermarkId` FOREIGN KEY (`WatermarkId`) REFERENCES `ChannelWatermark` (`Id`) ON DELETE CASCADE;
ALTER TABLE `DecoWatermark` ADD CONSTRAINT `FK_DecoWatermark_Deco_DecoId` FOREIGN KEY (`DecoId`) REFERENCES `Deco` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Director` ADD CONSTRAINT `FK_Director_EpisodeMetadata_EpisodeMetadataId` FOREIGN KEY (`EpisodeMetadataId`) REFERENCES `EpisodeMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Director` ADD CONSTRAINT `FK_Director_MovieMetadata_MovieMetadataId` FOREIGN KEY (`MovieMetadataId`) REFERENCES `MovieMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Director` ADD CONSTRAINT `FK_Director_MusicVideoMetadata_MusicVideoMetadataId` FOREIGN KEY (`MusicVideoMetadataId`) REFERENCES `MusicVideoMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Director` ADD CONSTRAINT `FK_Director_OtherVideoMetadata_OtherVideoMetadataId` FOREIGN KEY (`OtherVideoMetadataId`) REFERENCES `OtherVideoMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `EmbyConnection` ADD CONSTRAINT `FK_EmbyConnection_EmbyMediaSource_EmbyMediaSourceId` FOREIGN KEY (`EmbyMediaSourceId`) REFERENCES `EmbyMediaSource` (`Id`) ON DELETE CASCADE;
ALTER TABLE `EmbyEpisode` ADD CONSTRAINT `FK_EmbyEpisode_Episode_Id` FOREIGN KEY (`Id`) REFERENCES `Episode` (`Id`) ON DELETE CASCADE;
ALTER TABLE `EmbyLibrary` ADD CONSTRAINT `FK_EmbyLibrary_Library_Id` FOREIGN KEY (`Id`) REFERENCES `Library` (`Id`) ON DELETE CASCADE;
ALTER TABLE `EmbyMediaSource` ADD CONSTRAINT `FK_EmbyMediaSource_MediaSource_Id` FOREIGN KEY (`Id`) REFERENCES `MediaSource` (`Id`) ON DELETE CASCADE;
ALTER TABLE `EmbyMovie` ADD CONSTRAINT `FK_EmbyMovie_Movie_Id` FOREIGN KEY (`Id`) REFERENCES `Movie` (`Id`) ON DELETE CASCADE;
ALTER TABLE `EmbyPathInfo` ADD CONSTRAINT `FK_EmbyPathInfo_EmbyLibrary_EmbyLibraryId` FOREIGN KEY (`EmbyLibraryId`) REFERENCES `EmbyLibrary` (`Id`) ON DELETE CASCADE;
ALTER TABLE `EmbyPathReplacement` ADD CONSTRAINT `FK_EmbyPathReplacement_EmbyMediaSource_EmbyMediaSourceId` FOREIGN KEY (`EmbyMediaSourceId`) REFERENCES `EmbyMediaSource` (`Id`) ON DELETE CASCADE;
ALTER TABLE `EmbySeason` ADD CONSTRAINT `FK_EmbySeason_Season_Id` FOREIGN KEY (`Id`) REFERENCES `Season` (`Id`) ON DELETE CASCADE;
ALTER TABLE `EmbyShow` ADD CONSTRAINT `FK_EmbyShow_Show_Id` FOREIGN KEY (`Id`) REFERENCES `Show` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Episode` ADD CONSTRAINT `FK_Episode_MediaItem_Id` FOREIGN KEY (`Id`) REFERENCES `MediaItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Episode` ADD CONSTRAINT `FK_Episode_Season_SeasonId` FOREIGN KEY (`SeasonId`) REFERENCES `Season` (`Id`) ON DELETE CASCADE;
ALTER TABLE `EpisodeMetadata` ADD CONSTRAINT `FK_EpisodeMetadata_Episode_EpisodeId` FOREIGN KEY (`EpisodeId`) REFERENCES `Episode` (`Id`) ON DELETE CASCADE;
ALTER TABLE `FFmpegProfile` ADD CONSTRAINT `FK_FFmpegProfile_Resolution_ResolutionId` FOREIGN KEY (`ResolutionId`) REFERENCES `Resolution` (`Id`) ON DELETE CASCADE;
ALTER TABLE `FillGroupEnumeratorState` ADD CONSTRAINT `FK_FillGroupEnumeratorState_PlayoutScheduleItemFillGroupIndex_P~` FOREIGN KEY (`PlayoutScheduleItemFillGroupIndexId`) REFERENCES `PlayoutScheduleItemFillGroupIndex` (`Id`) ON DELETE CASCADE;
ALTER TABLE `FillerPreset` ADD CONSTRAINT `FK_FillerPreset_Collection_CollectionId` FOREIGN KEY (`CollectionId`) REFERENCES `Collection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `FillerPreset` ADD CONSTRAINT `FK_FillerPreset_MediaItem_MediaItemId` FOREIGN KEY (`MediaItemId`) REFERENCES `MediaItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `FillerPreset` ADD CONSTRAINT `FK_FillerPreset_MultiCollection_MultiCollectionId` FOREIGN KEY (`MultiCollectionId`) REFERENCES `MultiCollection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `FillerPreset` ADD CONSTRAINT `FK_FillerPreset_Playlist_PlaylistId` FOREIGN KEY (`PlaylistId`) REFERENCES `Playlist` (`Id`) ON DELETE CASCADE;
ALTER TABLE `FillerPreset` ADD CONSTRAINT `FK_FillerPreset_SmartCollection_SmartCollectionId` FOREIGN KEY (`SmartCollectionId`) REFERENCES `SmartCollection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Genre` ADD CONSTRAINT `FK_Genre_ArtistMetadata_ArtistMetadataId` FOREIGN KEY (`ArtistMetadataId`) REFERENCES `ArtistMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Genre` ADD CONSTRAINT `FK_Genre_EpisodeMetadata_EpisodeMetadataId` FOREIGN KEY (`EpisodeMetadataId`) REFERENCES `EpisodeMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Genre` ADD CONSTRAINT `FK_Genre_ImageMetadata_ImageMetadataId` FOREIGN KEY (`ImageMetadataId`) REFERENCES `ImageMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Genre` ADD CONSTRAINT `FK_Genre_MovieMetadata_MovieMetadataId` FOREIGN KEY (`MovieMetadataId`) REFERENCES `MovieMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Genre` ADD CONSTRAINT `FK_Genre_MusicVideoMetadata_MusicVideoMetadataId` FOREIGN KEY (`MusicVideoMetadataId`) REFERENCES `MusicVideoMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Genre` ADD CONSTRAINT `FK_Genre_OtherVideoMetadata_OtherVideoMetadataId` FOREIGN KEY (`OtherVideoMetadataId`) REFERENCES `OtherVideoMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Genre` ADD CONSTRAINT `FK_Genre_RemoteStreamMetadata_RemoteStreamMetadataId` FOREIGN KEY (`RemoteStreamMetadataId`) REFERENCES `RemoteStreamMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Genre` ADD CONSTRAINT `FK_Genre_SeasonMetadata_SeasonMetadataId` FOREIGN KEY (`SeasonMetadataId`) REFERENCES `SeasonMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Genre` ADD CONSTRAINT `FK_Genre_ShowMetadata_ShowMetadataId` FOREIGN KEY (`ShowMetadataId`) REFERENCES `ShowMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Genre` ADD CONSTRAINT `FK_Genre_SongMetadata_SongMetadataId` FOREIGN KEY (`SongMetadataId`) REFERENCES `SongMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Image` ADD CONSTRAINT `FK_Image_MediaItem_Id` FOREIGN KEY (`Id`) REFERENCES `MediaItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `ImageFolderDuration` ADD CONSTRAINT `FK_ImageFolderDuration_LibraryFolder_LibraryFolderId` FOREIGN KEY (`LibraryFolderId`) REFERENCES `LibraryFolder` (`Id`) ON DELETE CASCADE;
ALTER TABLE `ImageMetadata` ADD CONSTRAINT `FK_ImageMetadata_Image_ImageId` FOREIGN KEY (`ImageId`) REFERENCES `Image` (`Id`) ON DELETE CASCADE;
ALTER TABLE `JellyfinConnection` ADD CONSTRAINT `FK_JellyfinConnection_JellyfinMediaSource_JellyfinMediaSourceId` FOREIGN KEY (`JellyfinMediaSourceId`) REFERENCES `JellyfinMediaSource` (`Id`) ON DELETE CASCADE;
ALTER TABLE `JellyfinEpisode` ADD CONSTRAINT `FK_JellyfinEpisode_Episode_Id` FOREIGN KEY (`Id`) REFERENCES `Episode` (`Id`) ON DELETE CASCADE;
ALTER TABLE `JellyfinLibrary` ADD CONSTRAINT `FK_JellyfinLibrary_Library_Id` FOREIGN KEY (`Id`) REFERENCES `Library` (`Id`) ON DELETE CASCADE;
ALTER TABLE `JellyfinMediaSource` ADD CONSTRAINT `FK_JellyfinMediaSource_MediaSource_Id` FOREIGN KEY (`Id`) REFERENCES `MediaSource` (`Id`) ON DELETE CASCADE;
ALTER TABLE `JellyfinMovie` ADD CONSTRAINT `FK_JellyfinMovie_Movie_Id` FOREIGN KEY (`Id`) REFERENCES `Movie` (`Id`) ON DELETE CASCADE;
ALTER TABLE `JellyfinPathInfo` ADD CONSTRAINT `FK_JellyfinPathInfo_JellyfinLibrary_JellyfinLibraryId` FOREIGN KEY (`JellyfinLibraryId`) REFERENCES `JellyfinLibrary` (`Id`) ON DELETE CASCADE;
ALTER TABLE `JellyfinPathReplacement` ADD CONSTRAINT `FK_JellyfinPathReplacement_JellyfinMediaSource_JellyfinMediaSou~` FOREIGN KEY (`JellyfinMediaSourceId`) REFERENCES `JellyfinMediaSource` (`Id`) ON DELETE CASCADE;
ALTER TABLE `JellyfinSeason` ADD CONSTRAINT `FK_JellyfinSeason_Season_Id` FOREIGN KEY (`Id`) REFERENCES `Season` (`Id`) ON DELETE CASCADE;
ALTER TABLE `JellyfinShow` ADD CONSTRAINT `FK_JellyfinShow_Show_Id` FOREIGN KEY (`Id`) REFERENCES `Show` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Library` ADD CONSTRAINT `FK_Library_MediaSource_MediaSourceId` FOREIGN KEY (`MediaSourceId`) REFERENCES `MediaSource` (`Id`) ON DELETE CASCADE;
ALTER TABLE `LibraryFolder` ADD CONSTRAINT `FK_LibraryFolder_LibraryFolder_ParentId` FOREIGN KEY (`ParentId`) REFERENCES `LibraryFolder` (`Id`) ON DELETE RESTRICT;
ALTER TABLE `LibraryFolder` ADD CONSTRAINT `FK_LibraryFolder_LibraryPath_LibraryPathId` FOREIGN KEY (`LibraryPathId`) REFERENCES `LibraryPath` (`Id`) ON DELETE CASCADE;
ALTER TABLE `LibraryPath` ADD CONSTRAINT `FK_LibraryPath_Library_LibraryId` FOREIGN KEY (`LibraryId`) REFERENCES `Library` (`Id`) ON DELETE CASCADE;
ALTER TABLE `LocalLibrary` ADD CONSTRAINT `FK_LocalLibrary_Library_Id` FOREIGN KEY (`Id`) REFERENCES `Library` (`Id`) ON DELETE CASCADE;
ALTER TABLE `LocalMediaSource` ADD CONSTRAINT `FK_LocalMediaSource_MediaSource_Id` FOREIGN KEY (`Id`) REFERENCES `MediaSource` (`Id`) ON DELETE CASCADE;
ALTER TABLE `MediaChapter` ADD CONSTRAINT `FK_MediaChapter_MediaVersion_MediaVersionId` FOREIGN KEY (`MediaVersionId`) REFERENCES `MediaVersion` (`Id`) ON DELETE CASCADE;
ALTER TABLE `MediaFile` ADD CONSTRAINT `FK_MediaFile_LibraryFolder_LibraryFolderId` FOREIGN KEY (`LibraryFolderId`) REFERENCES `LibraryFolder` (`Id`) ON DELETE RESTRICT;
ALTER TABLE `MediaFile` ADD CONSTRAINT `FK_MediaFile_MediaVersion_MediaVersionId` FOREIGN KEY (`MediaVersionId`) REFERENCES `MediaVersion` (`Id`) ON DELETE CASCADE;
ALTER TABLE `MediaItem` ADD CONSTRAINT `FK_MediaItem_LibraryPath_LibraryPathId` FOREIGN KEY (`LibraryPathId`) REFERENCES `LibraryPath` (`Id`) ON DELETE CASCADE;
ALTER TABLE `MediaStream` ADD CONSTRAINT `FK_MediaStream_MediaVersion_MediaVersionId` FOREIGN KEY (`MediaVersionId`) REFERENCES `MediaVersion` (`Id`) ON DELETE CASCADE;
ALTER TABLE `MediaVersion` ADD CONSTRAINT `FK_MediaVersion_Episode_EpisodeId` FOREIGN KEY (`EpisodeId`) REFERENCES `Episode` (`Id`) ON DELETE CASCADE;
ALTER TABLE `MediaVersion` ADD CONSTRAINT `FK_MediaVersion_Image_ImageId` FOREIGN KEY (`ImageId`) REFERENCES `Image` (`Id`) ON DELETE CASCADE;
ALTER TABLE `MediaVersion` ADD CONSTRAINT `FK_MediaVersion_Movie_MovieId` FOREIGN KEY (`MovieId`) REFERENCES `Movie` (`Id`) ON DELETE CASCADE;
ALTER TABLE `MediaVersion` ADD CONSTRAINT `FK_MediaVersion_MusicVideo_MusicVideoId` FOREIGN KEY (`MusicVideoId`) REFERENCES `MusicVideo` (`Id`) ON DELETE CASCADE;
ALTER TABLE `MediaVersion` ADD CONSTRAINT `FK_MediaVersion_OtherVideo_OtherVideoId` FOREIGN KEY (`OtherVideoId`) REFERENCES `OtherVideo` (`Id`) ON DELETE CASCADE;
ALTER TABLE `MediaVersion` ADD CONSTRAINT `FK_MediaVersion_RemoteStream_RemoteStreamId` FOREIGN KEY (`RemoteStreamId`) REFERENCES `RemoteStream` (`Id`) ON DELETE CASCADE;
ALTER TABLE `MediaVersion` ADD CONSTRAINT `FK_MediaVersion_Song_SongId` FOREIGN KEY (`SongId`) REFERENCES `Song` (`Id`) ON DELETE CASCADE;
ALTER TABLE `MetadataGuid` ADD CONSTRAINT `FK_MetadataGuid_ArtistMetadata_ArtistMetadataId` FOREIGN KEY (`ArtistMetadataId`) REFERENCES `ArtistMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `MetadataGuid` ADD CONSTRAINT `FK_MetadataGuid_EpisodeMetadata_EpisodeMetadataId` FOREIGN KEY (`EpisodeMetadataId`) REFERENCES `EpisodeMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `MetadataGuid` ADD CONSTRAINT `FK_MetadataGuid_ImageMetadata_ImageMetadataId` FOREIGN KEY (`ImageMetadataId`) REFERENCES `ImageMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `MetadataGuid` ADD CONSTRAINT `FK_MetadataGuid_MovieMetadata_MovieMetadataId` FOREIGN KEY (`MovieMetadataId`) REFERENCES `MovieMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `MetadataGuid` ADD CONSTRAINT `FK_MetadataGuid_MusicVideoMetadata_MusicVideoMetadataId` FOREIGN KEY (`MusicVideoMetadataId`) REFERENCES `MusicVideoMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `MetadataGuid` ADD CONSTRAINT `FK_MetadataGuid_OtherVideoMetadata_OtherVideoMetadataId` FOREIGN KEY (`OtherVideoMetadataId`) REFERENCES `OtherVideoMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `MetadataGuid` ADD CONSTRAINT `FK_MetadataGuid_RemoteStreamMetadata_RemoteStreamMetadataId` FOREIGN KEY (`RemoteStreamMetadataId`) REFERENCES `RemoteStreamMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `MetadataGuid` ADD CONSTRAINT `FK_MetadataGuid_SeasonMetadata_SeasonMetadataId` FOREIGN KEY (`SeasonMetadataId`) REFERENCES `SeasonMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `MetadataGuid` ADD CONSTRAINT `FK_MetadataGuid_ShowMetadata_ShowMetadataId` FOREIGN KEY (`ShowMetadataId`) REFERENCES `ShowMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `MetadataGuid` ADD CONSTRAINT `FK_MetadataGuid_SongMetadata_SongMetadataId` FOREIGN KEY (`SongMetadataId`) REFERENCES `SongMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Mood` ADD CONSTRAINT `FK_Mood_ArtistMetadata_ArtistMetadataId` FOREIGN KEY (`ArtistMetadataId`) REFERENCES `ArtistMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Movie` ADD CONSTRAINT `FK_Movie_MediaItem_Id` FOREIGN KEY (`Id`) REFERENCES `MediaItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `MovieMetadata` ADD CONSTRAINT `FK_MovieMetadata_Movie_MovieId` FOREIGN KEY (`MovieId`) REFERENCES `Movie` (`Id`) ON DELETE CASCADE;
ALTER TABLE `MultiCollectionItem` ADD CONSTRAINT `FK_MultiCollectionItem_Collection_CollectionId` FOREIGN KEY (`CollectionId`) REFERENCES `Collection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `MultiCollectionItem` ADD CONSTRAINT `FK_MultiCollectionItem_MultiCollection_MultiCollectionId` FOREIGN KEY (`MultiCollectionId`) REFERENCES `MultiCollection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `MultiCollectionSmartItem` ADD CONSTRAINT `FK_MultiCollectionSmartItem_MultiCollection_MultiCollectionId` FOREIGN KEY (`MultiCollectionId`) REFERENCES `MultiCollection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `MultiCollectionSmartItem` ADD CONSTRAINT `FK_MultiCollectionSmartItem_SmartCollection_SmartCollectionId` FOREIGN KEY (`SmartCollectionId`) REFERENCES `SmartCollection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `MusicVideo` ADD CONSTRAINT `FK_MusicVideo_Artist_ArtistId` FOREIGN KEY (`ArtistId`) REFERENCES `Artist` (`Id`) ON DELETE CASCADE;
ALTER TABLE `MusicVideo` ADD CONSTRAINT `FK_MusicVideo_MediaItem_Id` FOREIGN KEY (`Id`) REFERENCES `MediaItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `MusicVideoArtist` ADD CONSTRAINT `FK_MusicVideoArtist_MusicVideoMetadata_MusicVideoMetadataId` FOREIGN KEY (`MusicVideoMetadataId`) REFERENCES `MusicVideoMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `MusicVideoMetadata` ADD CONSTRAINT `FK_MusicVideoMetadata_MusicVideo_MusicVideoId` FOREIGN KEY (`MusicVideoId`) REFERENCES `MusicVideo` (`Id`) ON DELETE CASCADE;
ALTER TABLE `OtherVideo` ADD CONSTRAINT `FK_OtherVideo_MediaItem_Id` FOREIGN KEY (`Id`) REFERENCES `MediaItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `OtherVideoMetadata` ADD CONSTRAINT `FK_OtherVideoMetadata_OtherVideo_OtherVideoId` FOREIGN KEY (`OtherVideoId`) REFERENCES `OtherVideo` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Playlist` ADD CONSTRAINT `FK_Playlist_PlaylistGroup_PlaylistGroupId` FOREIGN KEY (`PlaylistGroupId`) REFERENCES `PlaylistGroup` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlaylistItem` ADD CONSTRAINT `FK_PlaylistItem_Collection_CollectionId` FOREIGN KEY (`CollectionId`) REFERENCES `Collection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlaylistItem` ADD CONSTRAINT `FK_PlaylistItem_MediaItem_MediaItemId` FOREIGN KEY (`MediaItemId`) REFERENCES `MediaItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlaylistItem` ADD CONSTRAINT `FK_PlaylistItem_MultiCollection_MultiCollectionId` FOREIGN KEY (`MultiCollectionId`) REFERENCES `MultiCollection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlaylistItem` ADD CONSTRAINT `FK_PlaylistItem_Playlist_PlaylistId` FOREIGN KEY (`PlaylistId`) REFERENCES `Playlist` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlaylistItem` ADD CONSTRAINT `FK_PlaylistItem_SmartCollection_SmartCollectionId` FOREIGN KEY (`SmartCollectionId`) REFERENCES `SmartCollection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Playout` ADD CONSTRAINT `FK_Playout_Channel_ChannelId` FOREIGN KEY (`ChannelId`) REFERENCES `Channel` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Playout` ADD CONSTRAINT `FK_Playout_Deco_DecoId` FOREIGN KEY (`DecoId`) REFERENCES `Deco` (`Id`) ON DELETE SET NULL;
ALTER TABLE `Playout` ADD CONSTRAINT `FK_Playout_ProgramSchedule_ProgramScheduleId` FOREIGN KEY (`ProgramScheduleId`) REFERENCES `ProgramSchedule` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlayoutAnchor` ADD CONSTRAINT `FK_PlayoutAnchor_Playout_PlayoutId` FOREIGN KEY (`PlayoutId`) REFERENCES `Playout` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlayoutBuildStatus` ADD CONSTRAINT `FK_PlayoutBuildStatus_Playout_PlayoutId` FOREIGN KEY (`PlayoutId`) REFERENCES `Playout` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlayoutGap` ADD CONSTRAINT `FK_PlayoutGap_Playout_PlayoutId` FOREIGN KEY (`PlayoutId`) REFERENCES `Playout` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlayoutHistory` ADD CONSTRAINT `FK_PlayoutHistory_Block_BlockId` FOREIGN KEY (`BlockId`) REFERENCES `Block` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlayoutHistory` ADD CONSTRAINT `FK_PlayoutHistory_Playout_PlayoutId` FOREIGN KEY (`PlayoutId`) REFERENCES `Playout` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlayoutItem` ADD CONSTRAINT `FK_PlayoutItem_MediaItem_MediaItemId` FOREIGN KEY (`MediaItemId`) REFERENCES `MediaItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlayoutItem` ADD CONSTRAINT `FK_PlayoutItem_Playout_PlayoutId` FOREIGN KEY (`PlayoutId`) REFERENCES `Playout` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlayoutItemGraphicsElement` ADD CONSTRAINT `FK_PlayoutItemGraphicsElement_GraphicsElement_GraphicsElementId` FOREIGN KEY (`GraphicsElementId`) REFERENCES `GraphicsElement` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlayoutItemGraphicsElement` ADD CONSTRAINT `FK_PlayoutItemGraphicsElement_PlayoutItem_PlayoutItemId` FOREIGN KEY (`PlayoutItemId`) REFERENCES `PlayoutItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlayoutItemWatermark` ADD CONSTRAINT `FK_PlayoutItemWatermark_ChannelWatermark_WatermarkId` FOREIGN KEY (`WatermarkId`) REFERENCES `ChannelWatermark` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlayoutItemWatermark` ADD CONSTRAINT `FK_PlayoutItemWatermark_PlayoutItem_PlayoutItemId` FOREIGN KEY (`PlayoutItemId`) REFERENCES `PlayoutItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlayoutProgramScheduleAnchor` ADD CONSTRAINT `FK_PlayoutProgramScheduleAnchor_Collection_CollectionId` FOREIGN KEY (`CollectionId`) REFERENCES `Collection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlayoutProgramScheduleAnchor` ADD CONSTRAINT `FK_PlayoutProgramScheduleAnchor_MediaItem_MediaItemId` FOREIGN KEY (`MediaItemId`) REFERENCES `MediaItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlayoutProgramScheduleAnchor` ADD CONSTRAINT `FK_PlayoutProgramScheduleAnchor_MultiCollection_MultiCollection~` FOREIGN KEY (`MultiCollectionId`) REFERENCES `MultiCollection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlayoutProgramScheduleAnchor` ADD CONSTRAINT `FK_PlayoutProgramScheduleAnchor_Playlist_PlaylistId` FOREIGN KEY (`PlaylistId`) REFERENCES `Playlist` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlayoutProgramScheduleAnchor` ADD CONSTRAINT `FK_PlayoutProgramScheduleAnchor_Playout_PlayoutId` FOREIGN KEY (`PlayoutId`) REFERENCES `Playout` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlayoutProgramScheduleAnchor` ADD CONSTRAINT `FK_PlayoutProgramScheduleAnchor_RerunCollection_RerunCollection~` FOREIGN KEY (`RerunCollectionId`) REFERENCES `RerunCollection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlayoutProgramScheduleAnchor` ADD CONSTRAINT `FK_PlayoutProgramScheduleAnchor_SmartCollection_SmartCollection~` FOREIGN KEY (`SmartCollectionId`) REFERENCES `SmartCollection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlayoutScheduleItemFillGroupIndex` ADD CONSTRAINT `FK_PlayoutScheduleItemFillGroupIndex_Playout_PlayoutId` FOREIGN KEY (`PlayoutId`) REFERENCES `Playout` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlayoutScheduleItemFillGroupIndex` ADD CONSTRAINT `FK_PlayoutScheduleItemFillGroupIndex_ProgramScheduleItem_Progra~` FOREIGN KEY (`ProgramScheduleItemId`) REFERENCES `ProgramScheduleItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlayoutTemplate` ADD CONSTRAINT `FK_PlayoutTemplate_DecoTemplate_DecoTemplateId` FOREIGN KEY (`DecoTemplateId`) REFERENCES `DecoTemplate` (`Id`) ON DELETE SET NULL;
ALTER TABLE `PlayoutTemplate` ADD CONSTRAINT `FK_PlayoutTemplate_Playout_PlayoutId` FOREIGN KEY (`PlayoutId`) REFERENCES `Playout` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlayoutTemplate` ADD CONSTRAINT `FK_PlayoutTemplate_Template_TemplateId` FOREIGN KEY (`TemplateId`) REFERENCES `Template` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlexConnection` ADD CONSTRAINT `FK_PlexConnection_PlexMediaSource_PlexMediaSourceId` FOREIGN KEY (`PlexMediaSourceId`) REFERENCES `PlexMediaSource` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlexEpisode` ADD CONSTRAINT `FK_PlexEpisode_Episode_Id` FOREIGN KEY (`Id`) REFERENCES `Episode` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlexLibrary` ADD CONSTRAINT `FK_PlexLibrary_Library_Id` FOREIGN KEY (`Id`) REFERENCES `Library` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlexMediaFile` ADD CONSTRAINT `FK_PlexMediaFile_MediaFile_Id` FOREIGN KEY (`Id`) REFERENCES `MediaFile` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlexMediaSource` ADD CONSTRAINT `FK_PlexMediaSource_MediaSource_Id` FOREIGN KEY (`Id`) REFERENCES `MediaSource` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlexMovie` ADD CONSTRAINT `FK_PlexMovie_Movie_Id` FOREIGN KEY (`Id`) REFERENCES `Movie` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlexOtherVideo` ADD CONSTRAINT `FK_PlexOtherVideo_OtherVideo_Id` FOREIGN KEY (`Id`) REFERENCES `OtherVideo` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlexPathReplacement` ADD CONSTRAINT `FK_PlexPathReplacement_PlexMediaSource_PlexMediaSourceId` FOREIGN KEY (`PlexMediaSourceId`) REFERENCES `PlexMediaSource` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlexSeason` ADD CONSTRAINT `FK_PlexSeason_Season_Id` FOREIGN KEY (`Id`) REFERENCES `Season` (`Id`) ON DELETE CASCADE;
ALTER TABLE `PlexShow` ADD CONSTRAINT `FK_PlexShow_Show_Id` FOREIGN KEY (`Id`) REFERENCES `Show` (`Id`) ON DELETE CASCADE;
ALTER TABLE `ProgramScheduleAlternate` ADD CONSTRAINT `FK_ProgramScheduleAlternate_Playout_PlayoutId` FOREIGN KEY (`PlayoutId`) REFERENCES `Playout` (`Id`) ON DELETE CASCADE;
ALTER TABLE `ProgramScheduleAlternate` ADD CONSTRAINT `FK_ProgramScheduleAlternate_ProgramSchedule_ProgramScheduleId` FOREIGN KEY (`ProgramScheduleId`) REFERENCES `ProgramSchedule` (`Id`) ON DELETE CASCADE;
ALTER TABLE `ProgramScheduleDurationItem` ADD CONSTRAINT `FK_ProgramScheduleDurationItem_ProgramScheduleItem_Id` FOREIGN KEY (`Id`) REFERENCES `ProgramScheduleItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `ProgramScheduleFloodItem` ADD CONSTRAINT `FK_ProgramScheduleFloodItem_ProgramScheduleItem_Id` FOREIGN KEY (`Id`) REFERENCES `ProgramScheduleItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `ProgramScheduleItem` ADD CONSTRAINT `FK_ProgramScheduleItem_Collection_CollectionId` FOREIGN KEY (`CollectionId`) REFERENCES `Collection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `ProgramScheduleItem` ADD CONSTRAINT `FK_ProgramScheduleItem_FillerPreset_FallbackFillerId` FOREIGN KEY (`FallbackFillerId`) REFERENCES `FillerPreset` (`Id`) ON DELETE SET NULL;
ALTER TABLE `ProgramScheduleItem` ADD CONSTRAINT `FK_ProgramScheduleItem_FillerPreset_MidRollFillerId` FOREIGN KEY (`MidRollFillerId`) REFERENCES `FillerPreset` (`Id`) ON DELETE SET NULL;
ALTER TABLE `ProgramScheduleItem` ADD CONSTRAINT `FK_ProgramScheduleItem_FillerPreset_PostRollFillerId` FOREIGN KEY (`PostRollFillerId`) REFERENCES `FillerPreset` (`Id`) ON DELETE SET NULL;
ALTER TABLE `ProgramScheduleItem` ADD CONSTRAINT `FK_ProgramScheduleItem_FillerPreset_PreRollFillerId` FOREIGN KEY (`PreRollFillerId`) REFERENCES `FillerPreset` (`Id`) ON DELETE SET NULL;
ALTER TABLE `ProgramScheduleItem` ADD CONSTRAINT `FK_ProgramScheduleItem_FillerPreset_TailFillerId` FOREIGN KEY (`TailFillerId`) REFERENCES `FillerPreset` (`Id`) ON DELETE SET NULL;
ALTER TABLE `ProgramScheduleItem` ADD CONSTRAINT `FK_ProgramScheduleItem_MediaItem_MediaItemId` FOREIGN KEY (`MediaItemId`) REFERENCES `MediaItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `ProgramScheduleItem` ADD CONSTRAINT `FK_ProgramScheduleItem_MultiCollection_MultiCollectionId` FOREIGN KEY (`MultiCollectionId`) REFERENCES `MultiCollection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `ProgramScheduleItem` ADD CONSTRAINT `FK_ProgramScheduleItem_Playlist_PlaylistId` FOREIGN KEY (`PlaylistId`) REFERENCES `Playlist` (`Id`) ON DELETE CASCADE;
ALTER TABLE `ProgramScheduleItem` ADD CONSTRAINT `FK_ProgramScheduleItem_ProgramSchedule_ProgramScheduleId` FOREIGN KEY (`ProgramScheduleId`) REFERENCES `ProgramSchedule` (`Id`) ON DELETE CASCADE;
ALTER TABLE `ProgramScheduleItem` ADD CONSTRAINT `FK_ProgramScheduleItem_RerunCollection_RerunCollectionId` FOREIGN KEY (`RerunCollectionId`) REFERENCES `RerunCollection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `ProgramScheduleItem` ADD CONSTRAINT `FK_ProgramScheduleItem_SmartCollection_SmartCollectionId` FOREIGN KEY (`SmartCollectionId`) REFERENCES `SmartCollection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `ProgramScheduleItemGraphicsElement` ADD CONSTRAINT `FK_ProgramScheduleItemGraphicsElement_GraphicsElement_GraphicsE~` FOREIGN KEY (`GraphicsElementId`) REFERENCES `GraphicsElement` (`Id`) ON DELETE CASCADE;
ALTER TABLE `ProgramScheduleItemGraphicsElement` ADD CONSTRAINT `FK_ProgramScheduleItemGraphicsElement_ProgramScheduleItem_Progr~` FOREIGN KEY (`ProgramScheduleItemId`) REFERENCES `ProgramScheduleItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `ProgramScheduleItemWatermark` ADD CONSTRAINT `FK_ProgramScheduleItemWatermark_ChannelWatermark_WatermarkId` FOREIGN KEY (`WatermarkId`) REFERENCES `ChannelWatermark` (`Id`) ON DELETE CASCADE;
ALTER TABLE `ProgramScheduleItemWatermark` ADD CONSTRAINT `FK_ProgramScheduleItemWatermark_ProgramScheduleItem_ProgramSche~` FOREIGN KEY (`ProgramScheduleItemId`) REFERENCES `ProgramScheduleItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `ProgramScheduleMultipleItem` ADD CONSTRAINT `FK_ProgramScheduleMultipleItem_ProgramScheduleItem_Id` FOREIGN KEY (`Id`) REFERENCES `ProgramScheduleItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `ProgramScheduleOneItem` ADD CONSTRAINT `FK_ProgramScheduleOneItem_ProgramScheduleItem_Id` FOREIGN KEY (`Id`) REFERENCES `ProgramScheduleItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `RemoteStream` ADD CONSTRAINT `FK_RemoteStream_MediaItem_Id` FOREIGN KEY (`Id`) REFERENCES `MediaItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `RemoteStreamMetadata` ADD CONSTRAINT `FK_RemoteStreamMetadata_RemoteStream_RemoteStreamId` FOREIGN KEY (`RemoteStreamId`) REFERENCES `RemoteStream` (`Id`) ON DELETE CASCADE;
ALTER TABLE `RerunCollection` ADD CONSTRAINT `FK_RerunCollection_Collection_CollectionId` FOREIGN KEY (`CollectionId`) REFERENCES `Collection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `RerunCollection` ADD CONSTRAINT `FK_RerunCollection_MediaItem_MediaItemId` FOREIGN KEY (`MediaItemId`) REFERENCES `MediaItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `RerunCollection` ADD CONSTRAINT `FK_RerunCollection_MultiCollection_MultiCollectionId` FOREIGN KEY (`MultiCollectionId`) REFERENCES `MultiCollection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `RerunCollection` ADD CONSTRAINT `FK_RerunCollection_SmartCollection_SmartCollectionId` FOREIGN KEY (`SmartCollectionId`) REFERENCES `SmartCollection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `RerunHistory` ADD CONSTRAINT `FK_RerunHistory_MediaItem_MediaItemId` FOREIGN KEY (`MediaItemId`) REFERENCES `MediaItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `RerunHistory` ADD CONSTRAINT `FK_RerunHistory_Playout_PlayoutId` FOREIGN KEY (`PlayoutId`) REFERENCES `Playout` (`Id`) ON DELETE CASCADE;
ALTER TABLE `RerunHistory` ADD CONSTRAINT `FK_RerunHistory_RerunCollection_RerunCollectionId` FOREIGN KEY (`RerunCollectionId`) REFERENCES `RerunCollection` (`Id`) ON DELETE CASCADE;
ALTER TABLE `ScheduleItemsEnumeratorState` ADD CONSTRAINT `FK_ScheduleItemsEnumeratorState_PlayoutAnchor_PlayoutAnchorPlay~` FOREIGN KEY (`PlayoutAnchorPlayoutId`) REFERENCES `PlayoutAnchor` (`PlayoutId`) ON DELETE CASCADE;
ALTER TABLE `Season` ADD CONSTRAINT `FK_Season_MediaItem_Id` FOREIGN KEY (`Id`) REFERENCES `MediaItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Season` ADD CONSTRAINT `FK_Season_Show_ShowId` FOREIGN KEY (`ShowId`) REFERENCES `Show` (`Id`) ON DELETE CASCADE;
ALTER TABLE `SeasonMetadata` ADD CONSTRAINT `FK_SeasonMetadata_Season_SeasonId` FOREIGN KEY (`SeasonId`) REFERENCES `Season` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Show` ADD CONSTRAINT `FK_Show_MediaItem_Id` FOREIGN KEY (`Id`) REFERENCES `MediaItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `ShowMetadata` ADD CONSTRAINT `FK_ShowMetadata_Show_ShowId` FOREIGN KEY (`ShowId`) REFERENCES `Show` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Song` ADD CONSTRAINT `FK_Song_MediaItem_Id` FOREIGN KEY (`Id`) REFERENCES `MediaItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `SongMetadata` ADD CONSTRAINT `FK_SongMetadata_Song_SongId` FOREIGN KEY (`SongId`) REFERENCES `Song` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Studio` ADD CONSTRAINT `FK_Studio_ArtistMetadata_ArtistMetadataId` FOREIGN KEY (`ArtistMetadataId`) REFERENCES `ArtistMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Studio` ADD CONSTRAINT `FK_Studio_EpisodeMetadata_EpisodeMetadataId` FOREIGN KEY (`EpisodeMetadataId`) REFERENCES `EpisodeMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Studio` ADD CONSTRAINT `FK_Studio_ImageMetadata_ImageMetadataId` FOREIGN KEY (`ImageMetadataId`) REFERENCES `ImageMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Studio` ADD CONSTRAINT `FK_Studio_MovieMetadata_MovieMetadataId` FOREIGN KEY (`MovieMetadataId`) REFERENCES `MovieMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Studio` ADD CONSTRAINT `FK_Studio_MusicVideoMetadata_MusicVideoMetadataId` FOREIGN KEY (`MusicVideoMetadataId`) REFERENCES `MusicVideoMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Studio` ADD CONSTRAINT `FK_Studio_OtherVideoMetadata_OtherVideoMetadataId` FOREIGN KEY (`OtherVideoMetadataId`) REFERENCES `OtherVideoMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Studio` ADD CONSTRAINT `FK_Studio_RemoteStreamMetadata_RemoteStreamMetadataId` FOREIGN KEY (`RemoteStreamMetadataId`) REFERENCES `RemoteStreamMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Studio` ADD CONSTRAINT `FK_Studio_SeasonMetadata_SeasonMetadataId` FOREIGN KEY (`SeasonMetadataId`) REFERENCES `SeasonMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Studio` ADD CONSTRAINT `FK_Studio_ShowMetadata_ShowMetadataId` FOREIGN KEY (`ShowMetadataId`) REFERENCES `ShowMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Studio` ADD CONSTRAINT `FK_Studio_SongMetadata_SongMetadataId` FOREIGN KEY (`SongMetadataId`) REFERENCES `SongMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Style` ADD CONSTRAINT `FK_Style_ArtistMetadata_ArtistMetadataId` FOREIGN KEY (`ArtistMetadataId`) REFERENCES `ArtistMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Subtitle` ADD CONSTRAINT `FK_Subtitle_ArtistMetadata_ArtistMetadataId` FOREIGN KEY (`ArtistMetadataId`) REFERENCES `ArtistMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Subtitle` ADD CONSTRAINT `FK_Subtitle_EpisodeMetadata_EpisodeMetadataId` FOREIGN KEY (`EpisodeMetadataId`) REFERENCES `EpisodeMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Subtitle` ADD CONSTRAINT `FK_Subtitle_ImageMetadata_ImageMetadataId` FOREIGN KEY (`ImageMetadataId`) REFERENCES `ImageMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Subtitle` ADD CONSTRAINT `FK_Subtitle_MovieMetadata_MovieMetadataId` FOREIGN KEY (`MovieMetadataId`) REFERENCES `MovieMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Subtitle` ADD CONSTRAINT `FK_Subtitle_MusicVideoMetadata_MusicVideoMetadataId` FOREIGN KEY (`MusicVideoMetadataId`) REFERENCES `MusicVideoMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Subtitle` ADD CONSTRAINT `FK_Subtitle_OtherVideoMetadata_OtherVideoMetadataId` FOREIGN KEY (`OtherVideoMetadataId`) REFERENCES `OtherVideoMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Subtitle` ADD CONSTRAINT `FK_Subtitle_RemoteStreamMetadata_RemoteStreamMetadataId` FOREIGN KEY (`RemoteStreamMetadataId`) REFERENCES `RemoteStreamMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Subtitle` ADD CONSTRAINT `FK_Subtitle_SeasonMetadata_SeasonMetadataId` FOREIGN KEY (`SeasonMetadataId`) REFERENCES `SeasonMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Subtitle` ADD CONSTRAINT `FK_Subtitle_ShowMetadata_ShowMetadataId` FOREIGN KEY (`ShowMetadataId`) REFERENCES `ShowMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Subtitle` ADD CONSTRAINT `FK_Subtitle_SongMetadata_SongMetadataId` FOREIGN KEY (`SongMetadataId`) REFERENCES `SongMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Tag` ADD CONSTRAINT `FK_Tag_ArtistMetadata_ArtistMetadataId` FOREIGN KEY (`ArtistMetadataId`) REFERENCES `ArtistMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Tag` ADD CONSTRAINT `FK_Tag_EpisodeMetadata_EpisodeMetadataId` FOREIGN KEY (`EpisodeMetadataId`) REFERENCES `EpisodeMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Tag` ADD CONSTRAINT `FK_Tag_ImageMetadata_ImageMetadataId` FOREIGN KEY (`ImageMetadataId`) REFERENCES `ImageMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Tag` ADD CONSTRAINT `FK_Tag_MovieMetadata_MovieMetadataId` FOREIGN KEY (`MovieMetadataId`) REFERENCES `MovieMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Tag` ADD CONSTRAINT `FK_Tag_MusicVideoMetadata_MusicVideoMetadataId` FOREIGN KEY (`MusicVideoMetadataId`) REFERENCES `MusicVideoMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Tag` ADD CONSTRAINT `FK_Tag_OtherVideoMetadata_OtherVideoMetadataId` FOREIGN KEY (`OtherVideoMetadataId`) REFERENCES `OtherVideoMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Tag` ADD CONSTRAINT `FK_Tag_RemoteStreamMetadata_RemoteStreamMetadataId` FOREIGN KEY (`RemoteStreamMetadataId`) REFERENCES `RemoteStreamMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Tag` ADD CONSTRAINT `FK_Tag_SeasonMetadata_SeasonMetadataId` FOREIGN KEY (`SeasonMetadataId`) REFERENCES `SeasonMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Tag` ADD CONSTRAINT `FK_Tag_ShowMetadata_ShowMetadataId` FOREIGN KEY (`ShowMetadataId`) REFERENCES `ShowMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Tag` ADD CONSTRAINT `FK_Tag_SongMetadata_SongMetadataId` FOREIGN KEY (`SongMetadataId`) REFERENCES `SongMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Template` ADD CONSTRAINT `FK_Template_TemplateGroup_TemplateGroupId` FOREIGN KEY (`TemplateGroupId`) REFERENCES `TemplateGroup` (`Id`) ON DELETE CASCADE;
ALTER TABLE `TemplateItem` ADD CONSTRAINT `FK_TemplateItem_Block_BlockId` FOREIGN KEY (`BlockId`) REFERENCES `Block` (`Id`) ON DELETE CASCADE;
ALTER TABLE `TemplateItem` ADD CONSTRAINT `FK_TemplateItem_Template_TemplateId` FOREIGN KEY (`TemplateId`) REFERENCES `Template` (`Id`) ON DELETE CASCADE;
ALTER TABLE `TraktList` ADD CONSTRAINT `FK_TraktList_Playlist_PlaylistId` FOREIGN KEY (`PlaylistId`) REFERENCES `Playlist` (`Id`) ON DELETE SET NULL;
ALTER TABLE `TraktListItem` ADD CONSTRAINT `FK_TraktListItem_MediaItem_MediaItemId` FOREIGN KEY (`MediaItemId`) REFERENCES `MediaItem` (`Id`) ON DELETE SET NULL;
ALTER TABLE `TraktListItem` ADD CONSTRAINT `FK_TraktListItem_TraktList_TraktListId` FOREIGN KEY (`TraktListId`) REFERENCES `TraktList` (`Id`) ON DELETE CASCADE;
ALTER TABLE `TraktListItemGuid` ADD CONSTRAINT `FK_TraktListItemGuid_TraktListItem_TraktListItemId` FOREIGN KEY (`TraktListItemId`) REFERENCES `TraktListItem` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Writer` ADD CONSTRAINT `FK_Writer_EpisodeMetadata_EpisodeMetadataId` FOREIGN KEY (`EpisodeMetadataId`) REFERENCES `EpisodeMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Writer` ADD CONSTRAINT `FK_Writer_MovieMetadata_MovieMetadataId` FOREIGN KEY (`MovieMetadataId`) REFERENCES `MovieMetadata` (`Id`) ON DELETE CASCADE;
ALTER TABLE `Writer` ADD CONSTRAINT `FK_Writer_OtherVideoMetadata_OtherVideoMetadataId` FOREIGN KEY (`OtherVideoMetadataId`) REFERENCES `OtherVideoMetadata` (`Id`) ON DELETE CASCADE;

SET FOREIGN_KEY_CHECKS = 1;
