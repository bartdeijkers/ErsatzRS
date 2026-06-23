CREATE TABLE "Actor" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_Actor" PRIMARY KEY AUTOINCREMENT,
    "ArtistMetadataId" INTEGER NULL,
    "ArtworkId" INTEGER NULL,
    "EpisodeMetadataId" INTEGER NULL,
    "ImageMetadataId" INTEGER NULL,
    "MovieMetadataId" INTEGER NULL,
    "MusicVideoMetadataId" INTEGER NULL,
    "Name" TEXT NULL,
    "Order" INTEGER NULL,
    "OtherVideoMetadataId" INTEGER NULL,
    "RemoteStreamMetadataId" INTEGER NULL,
    "Role" TEXT NULL,
    "SeasonMetadataId" INTEGER NULL,
    "ShowMetadataId" INTEGER NULL,
    "SongMetadataId" INTEGER NULL,
    CONSTRAINT "FK_Actor_ArtistMetadata_ArtistMetadataId" FOREIGN KEY ("ArtistMetadataId") REFERENCES "ArtistMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Actor_Artwork_ArtworkId" FOREIGN KEY ("ArtworkId") REFERENCES "Artwork" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Actor_EpisodeMetadata_EpisodeMetadataId" FOREIGN KEY ("EpisodeMetadataId") REFERENCES "EpisodeMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Actor_ImageMetadata_ImageMetadataId" FOREIGN KEY ("ImageMetadataId") REFERENCES "ImageMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Actor_MovieMetadata_MovieMetadataId" FOREIGN KEY ("MovieMetadataId") REFERENCES "MovieMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Actor_MusicVideoMetadata_MusicVideoMetadataId" FOREIGN KEY ("MusicVideoMetadataId") REFERENCES "MusicVideoMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Actor_OtherVideoMetadata_OtherVideoMetadataId" FOREIGN KEY ("OtherVideoMetadataId") REFERENCES "OtherVideoMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Actor_RemoteStreamMetadata_RemoteStreamMetadataId" FOREIGN KEY ("RemoteStreamMetadataId") REFERENCES "RemoteStreamMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Actor_SeasonMetadata_SeasonMetadataId" FOREIGN KEY ("SeasonMetadataId") REFERENCES "SeasonMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Actor_ShowMetadata_ShowMetadataId" FOREIGN KEY ("ShowMetadataId") REFERENCES "ShowMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Actor_SongMetadata_SongMetadataId" FOREIGN KEY ("SongMetadataId") REFERENCES "SongMetadata" ("Id") ON DELETE CASCADE
);
CREATE TABLE "Artist" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_Artist" PRIMARY KEY AUTOINCREMENT,
    CONSTRAINT "FK_Artist_MediaItem_Id" FOREIGN KEY ("Id") REFERENCES "MediaItem" ("Id") ON DELETE CASCADE
);
CREATE TABLE "ArtistMetadata" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_ArtistMetadata" PRIMARY KEY AUTOINCREMENT,
    "ArtistId" INTEGER NOT NULL,
    "Biography" TEXT NULL,
    "DateAdded" TEXT NOT NULL,
    "DateUpdated" TEXT NOT NULL,
    "Disambiguation" TEXT NULL,
    "Formed" TEXT NULL,
    "MetadataKind" INTEGER NOT NULL,
    "OriginalTitle" TEXT NULL,
    "ReleaseDate" TEXT NULL,
    "SortTitle" TEXT NULL,
    "Title" TEXT COLLATE NOCASE NULL,
    "Year" INTEGER NULL,
    CONSTRAINT "FK_ArtistMetadata_Artist_ArtistId" FOREIGN KEY ("ArtistId") REFERENCES "Artist" ("Id") ON DELETE CASCADE
);
CREATE TABLE "Artwork" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_Artwork" PRIMARY KEY AUTOINCREMENT,
    "ArtistMetadataId" INTEGER NULL,
    "ArtworkKind" INTEGER NOT NULL,
    "BlurHash43" TEXT NULL,
    "BlurHash54" TEXT NULL,
    "BlurHash64" TEXT NULL,
    "ChannelId" INTEGER NULL,
    "DateAdded" TEXT NOT NULL,
    "DateUpdated" TEXT NOT NULL,
    "EpisodeMetadataId" INTEGER NULL,
    "ImageMetadataId" INTEGER NULL,
    "MovieMetadataId" INTEGER NULL,
    "MusicVideoMetadataId" INTEGER NULL,
    "OriginalContentType" TEXT NULL,
    "OtherVideoMetadataId" INTEGER NULL,
    "Path" TEXT NULL,
    "RemoteStreamMetadataId" INTEGER NULL,
    "SeasonMetadataId" INTEGER NULL,
    "ShowMetadataId" INTEGER NULL,
    "SongMetadataId" INTEGER NULL,
    "SourcePath" TEXT NULL, "IsMetadataOrphan" AS (CASE WHEN COALESCE(ArtistMetadataId, ChannelId, EpisodeMetadataId, MovieMetadataId, MusicVideoMetadataId, OtherVideoMetadataId, SeasonMetadataId, ShowMetadataId, SongMetadataId, ImageMetadataId, RemoteStreamMetadataId) IS NULL THEN 1 ELSE NULL END),
    CONSTRAINT "FK_Artwork_ArtistMetadata_ArtistMetadataId" FOREIGN KEY ("ArtistMetadataId") REFERENCES "ArtistMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Artwork_Channel_ChannelId" FOREIGN KEY ("ChannelId") REFERENCES "Channel" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Artwork_EpisodeMetadata_EpisodeMetadataId" FOREIGN KEY ("EpisodeMetadataId") REFERENCES "EpisodeMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Artwork_ImageMetadata_ImageMetadataId" FOREIGN KEY ("ImageMetadataId") REFERENCES "ImageMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Artwork_MovieMetadata_MovieMetadataId" FOREIGN KEY ("MovieMetadataId") REFERENCES "MovieMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Artwork_MusicVideoMetadata_MusicVideoMetadataId" FOREIGN KEY ("MusicVideoMetadataId") REFERENCES "MusicVideoMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Artwork_OtherVideoMetadata_OtherVideoMetadataId" FOREIGN KEY ("OtherVideoMetadataId") REFERENCES "OtherVideoMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Artwork_RemoteStreamMetadata_RemoteStreamMetadataId" FOREIGN KEY ("RemoteStreamMetadataId") REFERENCES "RemoteStreamMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Artwork_SeasonMetadata_SeasonMetadataId" FOREIGN KEY ("SeasonMetadataId") REFERENCES "SeasonMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Artwork_ShowMetadata_ShowMetadataId" FOREIGN KEY ("ShowMetadataId") REFERENCES "ShowMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Artwork_SongMetadata_SongMetadataId" FOREIGN KEY ("SongMetadataId") REFERENCES "SongMetadata" ("Id") ON DELETE CASCADE
);
CREATE TABLE "Block" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_Block" PRIMARY KEY AUTOINCREMENT,
    "BlockGroupId" INTEGER NOT NULL,
    "DateUpdated" TEXT NOT NULL,
    "Minutes" INTEGER NOT NULL,
    "Name" varchar(50) COLLATE NOCASE NULL,
    "StopScheduling" INTEGER NOT NULL,
    CONSTRAINT "FK_Block_BlockGroup_BlockGroupId" FOREIGN KEY ("BlockGroupId") REFERENCES "BlockGroup" ("Id") ON DELETE CASCADE
);
CREATE TABLE "BlockGroup" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_BlockGroup" PRIMARY KEY AUTOINCREMENT,
    "Name" varchar(50) COLLATE NOCASE NULL
);
CREATE TABLE "BlockItem" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_BlockItem" PRIMARY KEY AUTOINCREMENT,
    "Index" INTEGER NOT NULL,
    "BlockId" INTEGER NOT NULL,
    "CollectionType" INTEGER NOT NULL,
    "CollectionId" INTEGER NULL,
    "MediaItemId" INTEGER NULL,
    "MultiCollectionId" INTEGER NULL,
    "SmartCollectionId" INTEGER NULL,
    "PlaybackOrder" INTEGER NOT NULL, "IncludeInProgramGuide" INTEGER NOT NULL DEFAULT 0, "DisableWatermarks" INTEGER NOT NULL DEFAULT 0, "SearchQuery" TEXT NULL, "SearchTitle" TEXT NULL,
    CONSTRAINT "FK_BlockItem_Block_BlockId" FOREIGN KEY ("BlockId") REFERENCES "Block" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_BlockItem_Collection_CollectionId" FOREIGN KEY ("CollectionId") REFERENCES "Collection" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_BlockItem_MediaItem_MediaItemId" FOREIGN KEY ("MediaItemId") REFERENCES "MediaItem" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_BlockItem_MultiCollection_MultiCollectionId" FOREIGN KEY ("MultiCollectionId") REFERENCES "MultiCollection" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_BlockItem_SmartCollection_SmartCollectionId" FOREIGN KEY ("SmartCollectionId") REFERENCES "SmartCollection" ("Id") ON DELETE CASCADE
);
CREATE TABLE "BlockItemGraphicsElement" (
    "BlockItemId" INTEGER NOT NULL,
    "GraphicsElementId" INTEGER NOT NULL,
    CONSTRAINT "PK_BlockItemGraphicsElement" PRIMARY KEY ("BlockItemId", "GraphicsElementId"),
    CONSTRAINT "FK_BlockItemGraphicsElement_BlockItem_BlockItemId" FOREIGN KEY ("BlockItemId") REFERENCES "BlockItem" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_BlockItemGraphicsElement_GraphicsElement_GraphicsElementId" FOREIGN KEY ("GraphicsElementId") REFERENCES "GraphicsElement" ("Id") ON DELETE CASCADE
);
CREATE TABLE "BlockItemWatermark" (
    "BlockItemId" INTEGER NOT NULL,
    "WatermarkId" INTEGER NOT NULL,
    CONSTRAINT "PK_BlockItemWatermark" PRIMARY KEY ("BlockItemId", "WatermarkId"),
    CONSTRAINT "FK_BlockItemWatermark_BlockItem_BlockItemId" FOREIGN KEY ("BlockItemId") REFERENCES "BlockItem" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_BlockItemWatermark_ChannelWatermark_WatermarkId" FOREIGN KEY ("WatermarkId") REFERENCES "ChannelWatermark" ("Id") ON DELETE CASCADE
);
CREATE TABLE "Channel" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_Channel" PRIMARY KEY AUTOINCREMENT,
    "Categories" TEXT NULL,
    "FFmpegProfileId" INTEGER NOT NULL,
    "FallbackFillerId" INTEGER NULL,
    "Group" TEXT NOT NULL DEFAULT 'ErsatzRS',
    "IdleBehavior" INTEGER NOT NULL,
    "IsEnabled" INTEGER NOT NULL DEFAULT 1,
    "MirrorSourceChannelId" INTEGER NULL,
    "MusicVideoCreditsMode" INTEGER NOT NULL,
    "MusicVideoCreditsTemplate" TEXT NULL,
    "Name" varchar(50) COLLATE NOCASE NULL,
    "Number" TEXT NULL,
    "PlayoutMode" INTEGER NOT NULL,
    "PlayoutOffset" TEXT NULL,
    "PlayoutSource" INTEGER NOT NULL,
    "PreferredAudioLanguageCode" TEXT NULL,
    "PreferredAudioTitle" TEXT NULL,
    "PreferredSubtitleLanguageCode" TEXT NULL,
    "ShowInEpg" INTEGER NOT NULL DEFAULT 1,
    "SongVideoMode" INTEGER NOT NULL,
    "SortNumber" REAL NOT NULL,
    "StreamSelector" TEXT NULL,
    "StreamSelectorMode" INTEGER NOT NULL,
    "StreamingMode" INTEGER NOT NULL,
    "SubtitleMode" INTEGER NOT NULL,
    "TranscodeMode" INTEGER NOT NULL,
    "UniqueId" TEXT NOT NULL,
    "WatermarkId" INTEGER NULL, "SlugSeconds" REAL NULL,
    CONSTRAINT "FK_Channel_ChannelWatermark_WatermarkId" FOREIGN KEY ("WatermarkId") REFERENCES "ChannelWatermark" ("Id") ON DELETE SET NULL,
    CONSTRAINT "FK_Channel_Channel_MirrorSourceChannelId" FOREIGN KEY ("MirrorSourceChannelId") REFERENCES "Channel" ("Id") ON DELETE SET NULL,
    CONSTRAINT "FK_Channel_FFmpegProfile_FFmpegProfileId" FOREIGN KEY ("FFmpegProfileId") REFERENCES "FFmpegProfile" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Channel_FillerPreset_FallbackFillerId" FOREIGN KEY ("FallbackFillerId") REFERENCES "FillerPreset" ("Id") ON DELETE SET NULL
);
CREATE TABLE "ChannelWatermark" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_ChannelWatermark" PRIMARY KEY AUTOINCREMENT,
    "DurationSeconds" INTEGER NOT NULL,
    "FrequencyMinutes" INTEGER NOT NULL,
    "HorizontalMarginPercent" REAL NOT NULL,
    "Image" TEXT NULL,
    "ImageSource" INTEGER NOT NULL,
    "Location" INTEGER NOT NULL,
    "Mode" INTEGER NOT NULL,
    "Name" varchar(50) COLLATE NOCASE NULL,
    "Opacity" INTEGER NOT NULL,
    "OpacityExpression" TEXT NULL,
    "OriginalContentType" TEXT NULL,
    "PlaceWithinSourceContent" INTEGER NOT NULL,
    "Size" INTEGER NOT NULL,
    "VerticalMarginPercent" REAL NOT NULL,
    "WidthPercent" REAL NOT NULL,
    "ZIndex" INTEGER NOT NULL
);
CREATE TABLE "Collection" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_Collection" PRIMARY KEY AUTOINCREMENT,
    "Name" varchar(50) COLLATE NOCASE NULL,
    "UseCustomPlaybackOrder" INTEGER NOT NULL
);
CREATE TABLE "CollectionEnumeratorState" (
    "PlayoutProgramScheduleAnchorId" INTEGER NOT NULL CONSTRAINT "PK_CollectionEnumeratorState" PRIMARY KEY,
    "Seed" INTEGER NOT NULL,
    "Index" INTEGER NOT NULL,
    CONSTRAINT "FK_CollectionEnumeratorState_PlayoutProgramScheduleAnchor_PlayoutProgramScheduleAnchorId" FOREIGN KEY ("PlayoutProgramScheduleAnchorId") REFERENCES "PlayoutProgramScheduleAnchor" ("Id") ON DELETE CASCADE
);
CREATE TABLE "CollectionItem" (
    "CollectionId" INTEGER NOT NULL,
    "MediaItemId" INTEGER NOT NULL, "CustomIndex" INTEGER NULL,
    CONSTRAINT "PK_CollectionItem" PRIMARY KEY ("CollectionId", "MediaItemId"),
    CONSTRAINT "FK_CollectionItem_Collection_CollectionId" FOREIGN KEY ("CollectionId") REFERENCES "Collection" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_CollectionItem_MediaItem_MediaItemId" FOREIGN KEY ("MediaItemId") REFERENCES "MediaItem" ("Id") ON DELETE CASCADE
);
CREATE TABLE "ConfigElement" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_ConfigElement" PRIMARY KEY AUTOINCREMENT,
    "Key" TEXT NULL,
    "Value" TEXT NULL
);
CREATE TABLE "Deco" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_Deco" PRIMARY KEY AUTOINCREMENT,
    "BreakContentMode" INTEGER NOT NULL,
    "DeadAirFallbackCollectionId" INTEGER NULL,
    "DeadAirFallbackCollectionType" INTEGER NOT NULL,
    "DeadAirFallbackMediaItemId" INTEGER NULL,
    "DeadAirFallbackMode" INTEGER NOT NULL,
    "DeadAirFallbackMultiCollectionId" INTEGER NULL,
    "DeadAirFallbackSmartCollectionId" INTEGER NULL,
    "DecoGroupId" INTEGER NOT NULL,
    "DefaultFillerCollectionId" INTEGER NULL,
    "DefaultFillerCollectionType" INTEGER NOT NULL,
    "DefaultFillerMediaItemId" INTEGER NULL,
    "DefaultFillerMode" INTEGER NOT NULL,
    "DefaultFillerMultiCollectionId" INTEGER NULL,
    "DefaultFillerSmartCollectionId" INTEGER NULL,
    "DefaultFillerTrimToFit" INTEGER NOT NULL,
    "GraphicsElementsMode" INTEGER NOT NULL,
    "Name" varchar(50) COLLATE NOCASE NULL,
    "UseGraphicsElementsDuringFiller" INTEGER NOT NULL,
    "UseWatermarkDuringFiller" INTEGER NOT NULL,
    "WatermarkMode" INTEGER NOT NULL,
    CONSTRAINT "FK_Deco_Collection_DeadAirFallbackCollectionId" FOREIGN KEY ("DeadAirFallbackCollectionId") REFERENCES "Collection" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Deco_Collection_DefaultFillerCollectionId" FOREIGN KEY ("DefaultFillerCollectionId") REFERENCES "Collection" ("Id"),
    CONSTRAINT "FK_Deco_DecoGroup_DecoGroupId" FOREIGN KEY ("DecoGroupId") REFERENCES "DecoGroup" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Deco_MediaItem_DeadAirFallbackMediaItemId" FOREIGN KEY ("DeadAirFallbackMediaItemId") REFERENCES "MediaItem" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Deco_MediaItem_DefaultFillerMediaItemId" FOREIGN KEY ("DefaultFillerMediaItemId") REFERENCES "MediaItem" ("Id"),
    CONSTRAINT "FK_Deco_MultiCollection_DeadAirFallbackMultiCollectionId" FOREIGN KEY ("DeadAirFallbackMultiCollectionId") REFERENCES "MultiCollection" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Deco_MultiCollection_DefaultFillerMultiCollectionId" FOREIGN KEY ("DefaultFillerMultiCollectionId") REFERENCES "MultiCollection" ("Id"),
    CONSTRAINT "FK_Deco_SmartCollection_DeadAirFallbackSmartCollectionId" FOREIGN KEY ("DeadAirFallbackSmartCollectionId") REFERENCES "SmartCollection" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Deco_SmartCollection_DefaultFillerSmartCollectionId" FOREIGN KEY ("DefaultFillerSmartCollectionId") REFERENCES "SmartCollection" ("Id")
);
CREATE TABLE "DecoBreakContent" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_DecoBreakContent" PRIMARY KEY AUTOINCREMENT,
    "DecoId" INTEGER NOT NULL,
    "CollectionType" INTEGER NOT NULL,
    "CollectionId" INTEGER NULL,
    "MediaItemId" INTEGER NULL,
    "MultiCollectionId" INTEGER NULL,
    "SmartCollectionId" INTEGER NULL,
    "PlaylistId" INTEGER NULL,
    "Placement" INTEGER NOT NULL,
    CONSTRAINT "FK_DecoBreakContent_Collection_CollectionId" FOREIGN KEY ("CollectionId") REFERENCES "Collection" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_DecoBreakContent_Deco_DecoId" FOREIGN KEY ("DecoId") REFERENCES "Deco" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_DecoBreakContent_MediaItem_MediaItemId" FOREIGN KEY ("MediaItemId") REFERENCES "MediaItem" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_DecoBreakContent_MultiCollection_MultiCollectionId" FOREIGN KEY ("MultiCollectionId") REFERENCES "MultiCollection" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_DecoBreakContent_Playlist_PlaylistId" FOREIGN KEY ("PlaylistId") REFERENCES "Playlist" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_DecoBreakContent_SmartCollection_SmartCollectionId" FOREIGN KEY ("SmartCollectionId") REFERENCES "SmartCollection" ("Id") ON DELETE CASCADE
);
CREATE TABLE "DecoGraphicsElement" (
    "DecoId" INTEGER NOT NULL,
    "GraphicsElementId" INTEGER NOT NULL,
    CONSTRAINT "PK_DecoGraphicsElement" PRIMARY KEY ("DecoId", "GraphicsElementId"),
    CONSTRAINT "FK_DecoGraphicsElement_Deco_DecoId" FOREIGN KEY ("DecoId") REFERENCES "Deco" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_DecoGraphicsElement_GraphicsElement_GraphicsElementId" FOREIGN KEY ("GraphicsElementId") REFERENCES "GraphicsElement" ("Id") ON DELETE CASCADE
);
CREATE TABLE "DecoGroup" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_DecoGroup" PRIMARY KEY AUTOINCREMENT,
    "Name" varchar(50) COLLATE NOCASE NULL
);
CREATE TABLE "DecoTemplate" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_DecoTemplate" PRIMARY KEY AUTOINCREMENT,
    "DateUpdated" TEXT NOT NULL,
    "DecoTemplateGroupId" INTEGER NOT NULL,
    "Name" varchar(50) COLLATE NOCASE NULL,
    CONSTRAINT "FK_DecoTemplate_DecoTemplateGroup_DecoTemplateGroupId" FOREIGN KEY ("DecoTemplateGroupId") REFERENCES "DecoTemplateGroup" ("Id") ON DELETE CASCADE
);
CREATE TABLE "DecoTemplateGroup" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_DecoTemplateGroup" PRIMARY KEY AUTOINCREMENT,
    "Name" varchar(50) COLLATE NOCASE NULL
);
CREATE TABLE "DecoTemplateItem" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_DecoTemplateItem" PRIMARY KEY AUTOINCREMENT,
    "DecoTemplateId" INTEGER NOT NULL,
    "DecoId" INTEGER NOT NULL,
    "StartTime" TEXT NOT NULL, "EndTime" TEXT NOT NULL DEFAULT '00:00:00',
    CONSTRAINT "FK_DecoTemplateItem_DecoTemplate_DecoTemplateId" FOREIGN KEY ("DecoTemplateId") REFERENCES "DecoTemplate" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_DecoTemplateItem_Deco_DecoId" FOREIGN KEY ("DecoId") REFERENCES "Deco" ("Id") ON DELETE CASCADE
);
CREATE TABLE "DecoWatermark" (
    "DecoId" INTEGER NOT NULL,
    "WatermarkId" INTEGER NOT NULL,
    CONSTRAINT "PK_DecoWatermark" PRIMARY KEY ("DecoId", "WatermarkId"),
    CONSTRAINT "FK_DecoWatermark_ChannelWatermark_WatermarkId" FOREIGN KEY ("WatermarkId") REFERENCES "ChannelWatermark" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_DecoWatermark_Deco_DecoId" FOREIGN KEY ("DecoId") REFERENCES "Deco" ("Id") ON DELETE CASCADE
);
CREATE TABLE "Director" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_Director" PRIMARY KEY AUTOINCREMENT,
    "EpisodeMetadataId" INTEGER NULL,
    "MovieMetadataId" INTEGER NULL,
    "MusicVideoMetadataId" INTEGER NULL,
    "Name" TEXT NULL,
    "OtherVideoMetadataId" INTEGER NULL,
    CONSTRAINT "FK_Director_EpisodeMetadata_EpisodeMetadataId" FOREIGN KEY ("EpisodeMetadataId") REFERENCES "EpisodeMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Director_MovieMetadata_MovieMetadataId" FOREIGN KEY ("MovieMetadataId") REFERENCES "MovieMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Director_MusicVideoMetadata_MusicVideoMetadataId" FOREIGN KEY ("MusicVideoMetadataId") REFERENCES "MusicVideoMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Director_OtherVideoMetadata_OtherVideoMetadataId" FOREIGN KEY ("OtherVideoMetadataId") REFERENCES "OtherVideoMetadata" ("Id") ON DELETE CASCADE
);
CREATE TABLE "EmbyCollection" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_EmbyCollection" PRIMARY KEY AUTOINCREMENT,
    "ItemId" TEXT NULL,
    "Etag" TEXT NULL,
    "Name" TEXT NULL
);
CREATE TABLE "EmbyConnection" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_EmbyConnection" PRIMARY KEY AUTOINCREMENT,
    "Address" TEXT NULL,
    "EmbyMediaSourceId" INTEGER NOT NULL,
    CONSTRAINT "FK_EmbyConnection_EmbyMediaSource_EmbyMediaSourceId" FOREIGN KEY ("EmbyMediaSourceId") REFERENCES "EmbyMediaSource" ("Id") ON DELETE CASCADE
);
CREATE TABLE "EmbyEpisode" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_EmbyEpisode" PRIMARY KEY AUTOINCREMENT,
    "ItemId" TEXT NULL,
    "Etag" TEXT NULL,
    CONSTRAINT "FK_EmbyEpisode_Episode_Id" FOREIGN KEY ("Id") REFERENCES "Episode" ("Id") ON DELETE CASCADE
);
CREATE TABLE "EmbyLibrary" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_EmbyLibrary" PRIMARY KEY AUTOINCREMENT,
    "ItemId" TEXT NULL,
    "ShouldSyncItems" INTEGER NOT NULL,
    CONSTRAINT "FK_EmbyLibrary_Library_Id" FOREIGN KEY ("Id") REFERENCES "Library" ("Id") ON DELETE CASCADE
);
CREATE TABLE "EmbyMediaSource" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_EmbyMediaSource" PRIMARY KEY AUTOINCREMENT,
    "ServerName" TEXT NULL,
    "OperatingSystem" TEXT NULL, "LastCollectionsScan" TEXT NULL,
    CONSTRAINT "FK_EmbyMediaSource_MediaSource_Id" FOREIGN KEY ("Id") REFERENCES "MediaSource" ("Id") ON DELETE CASCADE
);
CREATE TABLE "EmbyMovie" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_EmbyMovie" PRIMARY KEY AUTOINCREMENT,
    "ItemId" TEXT NULL,
    "Etag" TEXT NULL,
    CONSTRAINT "FK_EmbyMovie_Movie_Id" FOREIGN KEY ("Id") REFERENCES "Movie" ("Id") ON DELETE CASCADE
);
CREATE TABLE "EmbyPathInfo" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_EmbyPathInfo" PRIMARY KEY AUTOINCREMENT,
    "Path" TEXT NULL,
    "NetworkPath" TEXT NULL,
    "EmbyLibraryId" INTEGER NULL,
    CONSTRAINT "FK_EmbyPathInfo_EmbyLibrary_EmbyLibraryId" FOREIGN KEY ("EmbyLibraryId") REFERENCES "EmbyLibrary" ("Id") ON DELETE CASCADE
);
CREATE TABLE "EmbyPathReplacement" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_EmbyPathReplacement" PRIMARY KEY AUTOINCREMENT,
    "EmbyPath" TEXT NULL,
    "LocalPath" TEXT NULL,
    "EmbyMediaSourceId" INTEGER NOT NULL,
    CONSTRAINT "FK_EmbyPathReplacement_EmbyMediaSource_EmbyMediaSourceId" FOREIGN KEY ("EmbyMediaSourceId") REFERENCES "EmbyMediaSource" ("Id") ON DELETE CASCADE
);
CREATE TABLE "EmbySeason" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_EmbySeason" PRIMARY KEY AUTOINCREMENT,
    "ItemId" TEXT NULL,
    "Etag" TEXT NULL,
    CONSTRAINT "FK_EmbySeason_Season_Id" FOREIGN KEY ("Id") REFERENCES "Season" ("Id") ON DELETE CASCADE
);
CREATE TABLE "EmbyShow" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_EmbyShow" PRIMARY KEY AUTOINCREMENT,
    "ItemId" TEXT NULL,
    "Etag" TEXT NULL,
    CONSTRAINT "FK_EmbyShow_Show_Id" FOREIGN KEY ("Id") REFERENCES "Show" ("Id") ON DELETE CASCADE
);
CREATE TABLE "Episode" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_Episode" PRIMARY KEY AUTOINCREMENT,
    "SeasonId" INTEGER NOT NULL,
    CONSTRAINT "FK_Episode_MediaItem_Id" FOREIGN KEY ("Id") REFERENCES "MediaItem" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Episode_Season_SeasonId" FOREIGN KEY ("SeasonId") REFERENCES "Season" ("Id") ON DELETE CASCADE
);
CREATE TABLE "EpisodeMetadata" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_EpisodeMetadata" PRIMARY KEY AUTOINCREMENT,
    "Outline" TEXT NULL,
    "Plot" TEXT NULL,
    "Tagline" TEXT NULL,
    "EpisodeId" INTEGER NOT NULL,
    "MetadataKind" INTEGER NOT NULL,
    "Title" TEXT NULL,
    "OriginalTitle" TEXT NULL,
    "SortTitle" TEXT NULL,
    "ReleaseDate" TEXT NULL,
    "DateAdded" TEXT NOT NULL,
    "DateUpdated" TEXT NOT NULL, "Year" INTEGER NULL, "EpisodeNumber" INTEGER NOT NULL DEFAULT 0,
    CONSTRAINT "FK_EpisodeMetadata_Episode_EpisodeId" FOREIGN KEY ("EpisodeId") REFERENCES "Episode" ("Id") ON DELETE CASCADE
);
CREATE TABLE "FFmpegProfile" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_FFmpegProfile" PRIMARY KEY AUTOINCREMENT,
    "AudioBitrate" INTEGER NOT NULL,
    "AudioBufferSize" INTEGER NOT NULL,
    "AudioChannels" INTEGER NOT NULL,
    "AudioFormat" INTEGER NOT NULL,
    "AudioSampleRate" INTEGER NOT NULL,
    "BitDepth" INTEGER NOT NULL,
    "DeinterlaceVideo" INTEGER NULL DEFAULT 1,
    "HardwareAcceleration" INTEGER NOT NULL,
    "Name" TEXT NULL,
    "NormalizeFramerate" INTEGER NOT NULL DEFAULT 0,
    "NormalizeLoudnessMode" INTEGER NOT NULL,
    "QsvExtraHardwareFrames" INTEGER NULL,
    "ResolutionId" INTEGER NOT NULL,
    "ScalingBehavior" INTEGER NOT NULL,
    "ThreadCount" INTEGER NOT NULL,
    "VaapiDevice" TEXT NULL,
    "VaapiDriver" INTEGER NOT NULL,
    "VideoBitrate" INTEGER NOT NULL,
    "VideoBufferSize" INTEGER NOT NULL,
    "VideoFormat" INTEGER NOT NULL, "VideoProfile" TEXT NULL, "VideoPreset" TEXT NULL, "AllowBFrames" INTEGER NOT NULL DEFAULT 0, "VaapiDisplay" TEXT NULL, "TonemapAlgorithm" INTEGER NOT NULL DEFAULT 0, "TargetLoudness" REAL NULL, "PadMode" INTEGER NOT NULL DEFAULT 0, "NormalizeAudio" INTEGER NOT NULL DEFAULT 1, "NormalizeColors" INTEGER NOT NULL DEFAULT 1, "NormalizeVideo" INTEGER NOT NULL DEFAULT 1,
    CONSTRAINT "FK_FFmpegProfile_Resolution_ResolutionId" FOREIGN KEY ("ResolutionId") REFERENCES "Resolution" ("Id") ON DELETE CASCADE
);
CREATE TABLE "FillGroupEnumeratorState" (
    "PlayoutScheduleItemFillGroupIndexId" INTEGER NOT NULL CONSTRAINT "PK_FillGroupEnumeratorState" PRIMARY KEY,
    "Seed" INTEGER NOT NULL,
    "Index" INTEGER NOT NULL,
    CONSTRAINT "FK_FillGroupEnumeratorState_PlayoutScheduleItemFillGroupIndex_PlayoutScheduleItemFillGroupIndexId" FOREIGN KEY ("PlayoutScheduleItemFillGroupIndexId") REFERENCES "PlayoutScheduleItemFillGroupIndex" ("Id") ON DELETE CASCADE
);
CREATE TABLE "FillerPreset" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_FillerPreset" PRIMARY KEY AUTOINCREMENT,
    "AllowWatermarks" INTEGER NOT NULL,
    "CollectionId" INTEGER NULL,
    "CollectionType" INTEGER NOT NULL,
    "Count" INTEGER NULL,
    "Duration" TEXT NULL,
    "Expression" TEXT NULL,
    "FillerKind" INTEGER NOT NULL,
    "FillerMode" INTEGER NOT NULL,
    "MediaItemId" INTEGER NULL,
    "MultiCollectionId" INTEGER NULL,
    "Name" varchar(50) COLLATE NOCASE NULL,
    "PadToNearestMinute" INTEGER NULL,
    "PlaylistId" INTEGER NULL,
    "SmartCollectionId" INTEGER NULL,
    "UseChaptersAsMediaItems" INTEGER NOT NULL,
    CONSTRAINT "FK_FillerPreset_Collection_CollectionId" FOREIGN KEY ("CollectionId") REFERENCES "Collection" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_FillerPreset_MediaItem_MediaItemId" FOREIGN KEY ("MediaItemId") REFERENCES "MediaItem" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_FillerPreset_MultiCollection_MultiCollectionId" FOREIGN KEY ("MultiCollectionId") REFERENCES "MultiCollection" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_FillerPreset_Playlist_PlaylistId" FOREIGN KEY ("PlaylistId") REFERENCES "Playlist" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_FillerPreset_SmartCollection_SmartCollectionId" FOREIGN KEY ("SmartCollectionId") REFERENCES "SmartCollection" ("Id") ON DELETE CASCADE
);
CREATE TABLE "Genre" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_Genre" PRIMARY KEY AUTOINCREMENT,
    "ArtistMetadataId" INTEGER NULL,
    "EpisodeMetadataId" INTEGER NULL,
    "ImageMetadataId" INTEGER NULL,
    "MovieMetadataId" INTEGER NULL,
    "MusicVideoMetadataId" INTEGER NULL,
    "Name" TEXT NULL,
    "OtherVideoMetadataId" INTEGER NULL,
    "RemoteStreamMetadataId" INTEGER NULL,
    "SeasonMetadataId" INTEGER NULL,
    "ShowMetadataId" INTEGER NULL,
    "SongMetadataId" INTEGER NULL,
    CONSTRAINT "FK_Genre_ArtistMetadata_ArtistMetadataId" FOREIGN KEY ("ArtistMetadataId") REFERENCES "ArtistMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Genre_EpisodeMetadata_EpisodeMetadataId" FOREIGN KEY ("EpisodeMetadataId") REFERENCES "EpisodeMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Genre_ImageMetadata_ImageMetadataId" FOREIGN KEY ("ImageMetadataId") REFERENCES "ImageMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Genre_MovieMetadata_MovieMetadataId" FOREIGN KEY ("MovieMetadataId") REFERENCES "MovieMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Genre_MusicVideoMetadata_MusicVideoMetadataId" FOREIGN KEY ("MusicVideoMetadataId") REFERENCES "MusicVideoMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Genre_OtherVideoMetadata_OtherVideoMetadataId" FOREIGN KEY ("OtherVideoMetadataId") REFERENCES "OtherVideoMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Genre_RemoteStreamMetadata_RemoteStreamMetadataId" FOREIGN KEY ("RemoteStreamMetadataId") REFERENCES "RemoteStreamMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Genre_SeasonMetadata_SeasonMetadataId" FOREIGN KEY ("SeasonMetadataId") REFERENCES "SeasonMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Genre_ShowMetadata_ShowMetadataId" FOREIGN KEY ("ShowMetadataId") REFERENCES "ShowMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Genre_SongMetadata_SongMetadataId" FOREIGN KEY ("SongMetadataId") REFERENCES "SongMetadata" ("Id") ON DELETE CASCADE
);
CREATE TABLE "GraphicsElement" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_GraphicsElement" PRIMARY KEY AUTOINCREMENT,
    "Path" TEXT NULL,
    "Kind" INTEGER NOT NULL
, "Name" TEXT NULL);
CREATE TABLE "Image" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_Image" PRIMARY KEY AUTOINCREMENT,
    CONSTRAINT "FK_Image_MediaItem_Id" FOREIGN KEY ("Id") REFERENCES "MediaItem" ("Id") ON DELETE CASCADE
);
CREATE TABLE "ImageFolderDuration" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_ImageFolderDuration" PRIMARY KEY AUTOINCREMENT,
    "DurationSeconds" REAL NOT NULL,
    "LibraryFolderId" INTEGER NOT NULL,
    CONSTRAINT "FK_ImageFolderDuration_LibraryFolder_LibraryFolderId" FOREIGN KEY ("LibraryFolderId") REFERENCES "LibraryFolder" ("Id") ON DELETE CASCADE
);
CREATE TABLE "ImageMetadata" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_ImageMetadata" PRIMARY KEY AUTOINCREMENT,
    "DateAdded" TEXT NOT NULL,
    "DateUpdated" TEXT NOT NULL,
    "DurationSeconds" REAL NULL,
    "ImageId" INTEGER NOT NULL,
    "MetadataKind" INTEGER NOT NULL,
    "OriginalTitle" TEXT NULL,
    "ReleaseDate" TEXT NULL,
    "SortTitle" TEXT NULL,
    "Title" TEXT NULL,
    "Year" INTEGER NULL,
    CONSTRAINT "FK_ImageMetadata_Image_ImageId" FOREIGN KEY ("ImageId") REFERENCES "Image" ("Id") ON DELETE CASCADE
);
CREATE TABLE "JellyfinCollection" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_JellyfinCollection" PRIMARY KEY AUTOINCREMENT,
    "ItemId" TEXT NULL,
    "Etag" TEXT NULL,
    "Name" TEXT NULL
);
CREATE TABLE "JellyfinConnection" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_JellyfinConnection" PRIMARY KEY AUTOINCREMENT,
    "Address" TEXT NULL,
    "JellyfinMediaSourceId" INTEGER NOT NULL,
    CONSTRAINT "FK_JellyfinConnection_JellyfinMediaSource_JellyfinMediaSourceId" FOREIGN KEY ("JellyfinMediaSourceId") REFERENCES "JellyfinMediaSource" ("Id") ON DELETE CASCADE
);
CREATE TABLE "JellyfinEpisode" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_JellyfinEpisode" PRIMARY KEY AUTOINCREMENT,
    "ItemId" TEXT NULL,
    "Etag" TEXT NULL,
    CONSTRAINT "FK_JellyfinEpisode_Episode_Id" FOREIGN KEY ("Id") REFERENCES "Episode" ("Id") ON DELETE CASCADE
);
CREATE TABLE "JellyfinLibrary" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_JellyfinLibrary" PRIMARY KEY AUTOINCREMENT,
    "ItemId" TEXT NULL,
    "ShouldSyncItems" INTEGER NOT NULL,
    CONSTRAINT "FK_JellyfinLibrary_Library_Id" FOREIGN KEY ("Id") REFERENCES "Library" ("Id") ON DELETE CASCADE
);
CREATE TABLE "JellyfinMediaSource" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_JellyfinMediaSource" PRIMARY KEY AUTOINCREMENT,
    "ServerName" TEXT NULL, "OperatingSystem" TEXT NULL, "LastCollectionsScan" TEXT NULL,
    CONSTRAINT "FK_JellyfinMediaSource_MediaSource_Id" FOREIGN KEY ("Id") REFERENCES "MediaSource" ("Id") ON DELETE CASCADE
);
CREATE TABLE "JellyfinMovie" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_JellyfinMovie" PRIMARY KEY AUTOINCREMENT,
    "ItemId" TEXT NULL,
    "Etag" TEXT NULL,
    CONSTRAINT "FK_JellyfinMovie_Movie_Id" FOREIGN KEY ("Id") REFERENCES "Movie" ("Id") ON DELETE CASCADE
);
CREATE TABLE "JellyfinPathInfo" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_JellyfinPathInfo" PRIMARY KEY AUTOINCREMENT,
    "Path" TEXT NULL,
    "NetworkPath" TEXT NULL,
    "JellyfinLibraryId" INTEGER NULL,
    CONSTRAINT "FK_JellyfinPathInfo_JellyfinLibrary_JellyfinLibraryId" FOREIGN KEY ("JellyfinLibraryId") REFERENCES "JellyfinLibrary" ("Id") ON DELETE CASCADE
);
CREATE TABLE "JellyfinPathReplacement" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_JellyfinPathReplacement" PRIMARY KEY AUTOINCREMENT,
    "JellyfinPath" TEXT NULL,
    "LocalPath" TEXT NULL,
    "JellyfinMediaSourceId" INTEGER NOT NULL,
    CONSTRAINT "FK_JellyfinPathReplacement_JellyfinMediaSource_JellyfinMediaSourceId" FOREIGN KEY ("JellyfinMediaSourceId") REFERENCES "JellyfinMediaSource" ("Id") ON DELETE CASCADE
);
CREATE TABLE "JellyfinSeason" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_JellyfinSeason" PRIMARY KEY AUTOINCREMENT,
    "ItemId" TEXT NULL,
    "Etag" TEXT NULL,
    CONSTRAINT "FK_JellyfinSeason_Season_Id" FOREIGN KEY ("Id") REFERENCES "Season" ("Id") ON DELETE CASCADE
);
CREATE TABLE "JellyfinShow" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_JellyfinShow" PRIMARY KEY AUTOINCREMENT,
    "ItemId" TEXT NULL,
    "Etag" TEXT NULL,
    CONSTRAINT "FK_JellyfinShow_Show_Id" FOREIGN KEY ("Id") REFERENCES "Show" ("Id") ON DELETE CASCADE
);
CREATE TABLE "LanguageCode" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_LanguageCode" PRIMARY KEY AUTOINCREMENT,
    "ThreeCode1" TEXT NULL,
    "ThreeCode2" TEXT NULL,
    "TwoCode" TEXT NULL,
    "EnglishName" TEXT NULL,
    "FrenchName" TEXT NULL
);
CREATE TABLE "Library" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_Library" PRIMARY KEY AUTOINCREMENT,
    "LastScan" TEXT NULL,
    "MediaKind" INTEGER NOT NULL,
    "MediaSourceId" INTEGER NOT NULL,
    "Name" TEXT NULL,
    CONSTRAINT "FK_Library_MediaSource_MediaSourceId" FOREIGN KEY ("MediaSourceId") REFERENCES "MediaSource" ("Id") ON DELETE CASCADE
);
CREATE TABLE "LibraryFolder" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_LibraryFolder" PRIMARY KEY AUTOINCREMENT,
    "Etag" TEXT NULL,
    "LibraryPathId" INTEGER NOT NULL,
    "ParentId" INTEGER NULL,
    "Path" TEXT NULL,
    CONSTRAINT "FK_LibraryFolder_LibraryFolder_ParentId" FOREIGN KEY ("ParentId") REFERENCES "LibraryFolder" ("Id") ON DELETE RESTRICT,
    CONSTRAINT "FK_LibraryFolder_LibraryPath_LibraryPathId" FOREIGN KEY ("LibraryPathId") REFERENCES "LibraryPath" ("Id") ON DELETE CASCADE
);
CREATE TABLE "LibraryPath" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_LibraryPath" PRIMARY KEY AUTOINCREMENT,
    "Path" TEXT NULL,
    "LibraryId" INTEGER NOT NULL, "LastScan" TEXT NULL,
    CONSTRAINT "FK_LibraryPath_Library_LibraryId" FOREIGN KEY ("LibraryId") REFERENCES "Library" ("Id") ON DELETE CASCADE
);
CREATE TABLE "LocalLibrary" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_LocalLibrary" PRIMARY KEY AUTOINCREMENT,
    CONSTRAINT "FK_LocalLibrary_Library_Id" FOREIGN KEY ("Id") REFERENCES "Library" ("Id") ON DELETE CASCADE
);
CREATE TABLE "LocalMediaSource" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_LocalMediaSource" PRIMARY KEY AUTOINCREMENT,
    CONSTRAINT "FK_LocalMediaSource_MediaSource_Id" FOREIGN KEY ("Id") REFERENCES "MediaSource" ("Id") ON DELETE CASCADE
);
CREATE TABLE "MediaChapter" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_MediaChapter" PRIMARY KEY AUTOINCREMENT,
    "MediaVersionId" INTEGER NOT NULL,
    "ChapterId" INTEGER NOT NULL,
    "StartTime" TEXT NOT NULL,
    "EndTime" TEXT NOT NULL,
    "Title" TEXT NULL,
    CONSTRAINT "FK_MediaChapter_MediaVersion_MediaVersionId" FOREIGN KEY ("MediaVersionId") REFERENCES "MediaVersion" ("Id") ON DELETE CASCADE
);
CREATE TABLE "MediaFile" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_MediaFile" PRIMARY KEY AUTOINCREMENT,
    "LibraryFolderId" INTEGER NULL,
    "MediaVersionId" INTEGER NOT NULL,
    "Path" TEXT NULL,
    "PathHash" TEXT NOT NULL,
    CONSTRAINT "FK_MediaFile_LibraryFolder_LibraryFolderId" FOREIGN KEY ("LibraryFolderId") REFERENCES "LibraryFolder" ("Id") ON DELETE RESTRICT,
    CONSTRAINT "FK_MediaFile_MediaVersion_MediaVersionId" FOREIGN KEY ("MediaVersionId") REFERENCES "MediaVersion" ("Id") ON DELETE CASCADE
);
CREATE TABLE "MediaItem" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_MediaItem" PRIMARY KEY AUTOINCREMENT,
    "LibraryPathId" INTEGER NOT NULL, "State" INTEGER NOT NULL DEFAULT 0,
    CONSTRAINT "FK_MediaItem_LibraryPath_LibraryPathId" FOREIGN KEY ("LibraryPathId") REFERENCES "LibraryPath" ("Id") ON DELETE CASCADE
);
CREATE TABLE "MediaSource" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_MediaSource" PRIMARY KEY AUTOINCREMENT
);
CREATE TABLE "MediaStream" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_MediaStream" PRIMARY KEY AUTOINCREMENT,
    "Index" INTEGER NOT NULL,
    "Codec" TEXT NULL,
    "Profile" TEXT NULL,
    "MediaStreamKind" INTEGER NOT NULL,
    "Language" TEXT NULL,
    "Channels" INTEGER NOT NULL,
    "Title" TEXT NULL,
    "Default" INTEGER NOT NULL,
    "Forced" INTEGER NOT NULL,
    "MediaVersionId" INTEGER NOT NULL, "BitsPerRawSample" INTEGER NOT NULL DEFAULT 0, "PixelFormat" TEXT NULL, "AttachedPic" INTEGER NOT NULL DEFAULT 0, "FileName" TEXT NULL, "MimeType" TEXT NULL, "ColorPrimaries" TEXT NULL, "ColorRange" TEXT NULL, "ColorSpace" TEXT NULL, "ColorTransfer" TEXT NULL,
    CONSTRAINT "FK_MediaStream_MediaVersion_MediaVersionId" FOREIGN KEY ("MediaVersionId") REFERENCES "MediaVersion" ("Id") ON DELETE CASCADE
);
CREATE TABLE "MediaVersion" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_MediaVersion" PRIMARY KEY AUTOINCREMENT,
    "DateAdded" TEXT NOT NULL,
    "DateUpdated" TEXT NOT NULL,
    "DisplayAspectRatio" TEXT NULL,
    "Duration" TEXT NOT NULL,
    "EpisodeId" INTEGER NULL,
    "Height" INTEGER NOT NULL,
    "ImageId" INTEGER NULL,
    "MovieId" INTEGER NULL,
    "MusicVideoId" INTEGER NULL,
    "Name" TEXT NULL,
    "OtherVideoId" INTEGER NULL,
    "RFrameRate" TEXT NULL,
    "RemoteStreamId" INTEGER NULL,
    "SampleAspectRatio" TEXT NULL,
    "SongId" INTEGER NULL,
    "VideoScanKind" INTEGER NOT NULL,
    "Width" INTEGER NOT NULL, "InterlacedRatio" REAL NULL,
    CONSTRAINT "FK_MediaVersion_Episode_EpisodeId" FOREIGN KEY ("EpisodeId") REFERENCES "Episode" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_MediaVersion_Image_ImageId" FOREIGN KEY ("ImageId") REFERENCES "Image" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_MediaVersion_Movie_MovieId" FOREIGN KEY ("MovieId") REFERENCES "Movie" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_MediaVersion_MusicVideo_MusicVideoId" FOREIGN KEY ("MusicVideoId") REFERENCES "MusicVideo" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_MediaVersion_OtherVideo_OtherVideoId" FOREIGN KEY ("OtherVideoId") REFERENCES "OtherVideo" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_MediaVersion_RemoteStream_RemoteStreamId" FOREIGN KEY ("RemoteStreamId") REFERENCES "RemoteStream" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_MediaVersion_Song_SongId" FOREIGN KEY ("SongId") REFERENCES "Song" ("Id") ON DELETE CASCADE
);
CREATE TABLE "MetadataGuid" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_MetadataGuid" PRIMARY KEY AUTOINCREMENT,
    "ArtistMetadataId" INTEGER NULL,
    "EpisodeMetadataId" INTEGER NULL,
    "Guid" varchar(128) COLLATE NOCASE NULL,
    "ImageMetadataId" INTEGER NULL,
    "MovieMetadataId" INTEGER NULL,
    "MusicVideoMetadataId" INTEGER NULL,
    "OtherVideoMetadataId" INTEGER NULL,
    "RemoteStreamMetadataId" INTEGER NULL,
    "SeasonMetadataId" INTEGER NULL,
    "ShowMetadataId" INTEGER NULL,
    "SongMetadataId" INTEGER NULL,
    CONSTRAINT "FK_MetadataGuid_ArtistMetadata_ArtistMetadataId" FOREIGN KEY ("ArtistMetadataId") REFERENCES "ArtistMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_MetadataGuid_EpisodeMetadata_EpisodeMetadataId" FOREIGN KEY ("EpisodeMetadataId") REFERENCES "EpisodeMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_MetadataGuid_ImageMetadata_ImageMetadataId" FOREIGN KEY ("ImageMetadataId") REFERENCES "ImageMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_MetadataGuid_MovieMetadata_MovieMetadataId" FOREIGN KEY ("MovieMetadataId") REFERENCES "MovieMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_MetadataGuid_MusicVideoMetadata_MusicVideoMetadataId" FOREIGN KEY ("MusicVideoMetadataId") REFERENCES "MusicVideoMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_MetadataGuid_OtherVideoMetadata_OtherVideoMetadataId" FOREIGN KEY ("OtherVideoMetadataId") REFERENCES "OtherVideoMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_MetadataGuid_RemoteStreamMetadata_RemoteStreamMetadataId" FOREIGN KEY ("RemoteStreamMetadataId") REFERENCES "RemoteStreamMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_MetadataGuid_SeasonMetadata_SeasonMetadataId" FOREIGN KEY ("SeasonMetadataId") REFERENCES "SeasonMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_MetadataGuid_ShowMetadata_ShowMetadataId" FOREIGN KEY ("ShowMetadataId") REFERENCES "ShowMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_MetadataGuid_SongMetadata_SongMetadataId" FOREIGN KEY ("SongMetadataId") REFERENCES "SongMetadata" ("Id") ON DELETE CASCADE
);
CREATE TABLE "Mood" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_Mood" PRIMARY KEY AUTOINCREMENT,
    "Name" TEXT NULL,
    "ArtistMetadataId" INTEGER NULL,
    CONSTRAINT "FK_Mood_ArtistMetadata_ArtistMetadataId" FOREIGN KEY ("ArtistMetadataId") REFERENCES "ArtistMetadata" ("Id") ON DELETE CASCADE
);
CREATE TABLE "Movie" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_Movie" PRIMARY KEY AUTOINCREMENT,
    CONSTRAINT "FK_Movie_MediaItem_Id" FOREIGN KEY ("Id") REFERENCES "MediaItem" ("Id") ON DELETE CASCADE
);
CREATE TABLE "MovieMetadata" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_MovieMetadata" PRIMARY KEY AUTOINCREMENT,
    "ContentRating" TEXT NULL,
    "DateAdded" TEXT NOT NULL,
    "DateUpdated" TEXT NOT NULL,
    "MetadataKind" INTEGER NOT NULL,
    "MovieId" INTEGER NOT NULL,
    "OriginalTitle" TEXT NULL,
    "Outline" TEXT NULL,
    "Plot" TEXT NULL,
    "ReleaseDate" TEXT NULL,
    "SortTitle" TEXT NULL,
    "Tagline" TEXT NULL,
    "Title" TEXT COLLATE NOCASE NULL,
    "Year" INTEGER NULL,
    CONSTRAINT "FK_MovieMetadata_Movie_MovieId" FOREIGN KEY ("MovieId") REFERENCES "Movie" ("Id") ON DELETE CASCADE
);
CREATE TABLE "MultiCollection" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_MultiCollection" PRIMARY KEY AUTOINCREMENT,
    "Name" varchar(50) COLLATE NOCASE NULL
);
CREATE TABLE "MultiCollectionItem" (
    "MultiCollectionId" INTEGER NOT NULL,
    "CollectionId" INTEGER NOT NULL,
    "ScheduleAsGroup" INTEGER NOT NULL,
    "PlaybackOrder" INTEGER NOT NULL,
    CONSTRAINT "PK_MultiCollectionItem" PRIMARY KEY ("MultiCollectionId", "CollectionId"),
    CONSTRAINT "FK_MultiCollectionItem_Collection_CollectionId" FOREIGN KEY ("CollectionId") REFERENCES "Collection" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_MultiCollectionItem_MultiCollection_MultiCollectionId" FOREIGN KEY ("MultiCollectionId") REFERENCES "MultiCollection" ("Id") ON DELETE CASCADE
);
CREATE TABLE "MultiCollectionSmartItem" (
    "MultiCollectionId" INTEGER NOT NULL,
    "SmartCollectionId" INTEGER NOT NULL,
    "ScheduleAsGroup" INTEGER NOT NULL,
    "PlaybackOrder" INTEGER NOT NULL,
    CONSTRAINT "PK_MultiCollectionSmartItem" PRIMARY KEY ("MultiCollectionId", "SmartCollectionId"),
    CONSTRAINT "FK_MultiCollectionSmartItem_MultiCollection_MultiCollectionId" FOREIGN KEY ("MultiCollectionId") REFERENCES "MultiCollection" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_MultiCollectionSmartItem_SmartCollection_SmartCollectionId" FOREIGN KEY ("SmartCollectionId") REFERENCES "SmartCollection" ("Id") ON DELETE CASCADE
);
CREATE TABLE "MusicVideo" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_MusicVideo" PRIMARY KEY AUTOINCREMENT,
    "ArtistId" INTEGER NOT NULL,
    CONSTRAINT "FK_MusicVideo_Artist_ArtistId" FOREIGN KEY ("ArtistId") REFERENCES "Artist" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_MusicVideo_MediaItem_Id" FOREIGN KEY ("Id") REFERENCES "MediaItem" ("Id") ON DELETE CASCADE
);
CREATE TABLE "MusicVideoArtist" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_MusicVideoArtist" PRIMARY KEY AUTOINCREMENT,
    "Name" TEXT NULL,
    "MusicVideoMetadataId" INTEGER NULL,
    CONSTRAINT "FK_MusicVideoArtist_MusicVideoMetadata_MusicVideoMetadataId" FOREIGN KEY ("MusicVideoMetadataId") REFERENCES "MusicVideoMetadata" ("Id") ON DELETE CASCADE
);
CREATE TABLE "MusicVideoMetadata" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_MusicVideoMetadata" PRIMARY KEY AUTOINCREMENT,
    "Album" TEXT NULL,
    "DateAdded" TEXT NOT NULL,
    "DateUpdated" TEXT NOT NULL,
    "MetadataKind" INTEGER NOT NULL,
    "MusicVideoId" INTEGER NOT NULL,
    "OriginalTitle" TEXT NULL,
    "Plot" TEXT NULL,
    "ReleaseDate" TEXT NULL,
    "SortTitle" TEXT NULL,
    "Title" TEXT NULL,
    "Year" INTEGER NULL, "Track" INTEGER NULL,
    CONSTRAINT "FK_MusicVideoMetadata_MusicVideo_MusicVideoId" FOREIGN KEY ("MusicVideoId") REFERENCES "MusicVideo" ("Id") ON DELETE CASCADE
);
CREATE TABLE "OtherVideo" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_OtherVideo" PRIMARY KEY AUTOINCREMENT,
    CONSTRAINT "FK_OtherVideo_MediaItem_Id" FOREIGN KEY ("Id") REFERENCES "MediaItem" ("Id") ON DELETE CASCADE
);
CREATE TABLE "OtherVideoMetadata" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_OtherVideoMetadata" PRIMARY KEY AUTOINCREMENT,
    "OtherVideoId" INTEGER NOT NULL,
    "MetadataKind" INTEGER NOT NULL,
    "Title" TEXT NULL,
    "OriginalTitle" TEXT NULL,
    "SortTitle" TEXT NULL,
    "Year" INTEGER NULL,
    "ReleaseDate" TEXT NULL,
    "DateAdded" TEXT NOT NULL,
    "DateUpdated" TEXT NOT NULL, "ContentRating" TEXT NULL, "Outline" TEXT NULL, "Plot" TEXT NULL, "Tagline" TEXT NULL,
    CONSTRAINT "FK_OtherVideoMetadata_OtherVideo_OtherVideoId" FOREIGN KEY ("OtherVideoId") REFERENCES "OtherVideo" ("Id") ON DELETE CASCADE
);
CREATE TABLE "Playlist" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_Playlist" PRIMARY KEY AUTOINCREMENT,
    "IsSystem" INTEGER NOT NULL,
    "Name" varchar(50) COLLATE NOCASE NULL,
    "PlaylistGroupId" INTEGER NOT NULL,
    CONSTRAINT "FK_Playlist_PlaylistGroup_PlaylistGroupId" FOREIGN KEY ("PlaylistGroupId") REFERENCES "PlaylistGroup" ("Id") ON DELETE CASCADE
);
CREATE TABLE "PlaylistGroup" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_PlaylistGroup" PRIMARY KEY AUTOINCREMENT,
    "Name" TEXT NULL
, "IsSystem" INTEGER NOT NULL DEFAULT 0);
CREATE TABLE "PlaylistItem" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_PlaylistItem" PRIMARY KEY AUTOINCREMENT,
    "Index" INTEGER NOT NULL,
    "PlaylistId" INTEGER NOT NULL,
    "CollectionType" INTEGER NOT NULL,
    "CollectionId" INTEGER NULL,
    "MediaItemId" INTEGER NULL,
    "MultiCollectionId" INTEGER NULL,
    "SmartCollectionId" INTEGER NULL, "IncludeInProgramGuide" INTEGER NOT NULL DEFAULT 0, "PlaybackOrder" INTEGER NOT NULL DEFAULT 0, "PlayAll" INTEGER NOT NULL DEFAULT 0, "Count" INTEGER NULL,
    CONSTRAINT "FK_PlaylistItem_Collection_CollectionId" FOREIGN KEY ("CollectionId") REFERENCES "Collection" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_PlaylistItem_MediaItem_MediaItemId" FOREIGN KEY ("MediaItemId") REFERENCES "MediaItem" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_PlaylistItem_MultiCollection_MultiCollectionId" FOREIGN KEY ("MultiCollectionId") REFERENCES "MultiCollection" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_PlaylistItem_Playlist_PlaylistId" FOREIGN KEY ("PlaylistId") REFERENCES "Playlist" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_PlaylistItem_SmartCollection_SmartCollectionId" FOREIGN KEY ("SmartCollectionId") REFERENCES "SmartCollection" ("Id") ON DELETE CASCADE
);
CREATE TABLE "Playout" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_Playout" PRIMARY KEY AUTOINCREMENT,
    "ChannelId" INTEGER NOT NULL,
    "DailyRebuildTime" TEXT NULL,
    "DecoId" INTEGER NULL,
    "OnDemandCheckpoint" TEXT NULL,
    "ProgramScheduleId" INTEGER NULL,
    "ScheduleFile" TEXT NULL,
    "ScheduleKind" INTEGER NOT NULL,
    "Seed" INTEGER NOT NULL,
    CONSTRAINT "FK_Playout_Channel_ChannelId" FOREIGN KEY ("ChannelId") REFERENCES "Channel" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Playout_Deco_DecoId" FOREIGN KEY ("DecoId") REFERENCES "Deco" ("Id") ON DELETE SET NULL,
    CONSTRAINT "FK_Playout_ProgramSchedule_ProgramScheduleId" FOREIGN KEY ("ProgramScheduleId") REFERENCES "ProgramSchedule" ("Id") ON DELETE CASCADE
);
CREATE TABLE "PlayoutAnchor" (
    "PlayoutId" INTEGER NOT NULL CONSTRAINT "PK_PlayoutAnchor" PRIMARY KEY,
    "DurationFinish" TEXT NULL,
    "InDurationFiller" INTEGER NOT NULL,
    "InFlood" INTEGER NOT NULL,
    "MultipleRemaining" INTEGER NULL,
    "NextGuideGroup" INTEGER NOT NULL,
    "NextStart" TEXT NOT NULL, "NextInstructionIndex" INTEGER NOT NULL DEFAULT 0, "Context" TEXT NULL,
    CONSTRAINT "FK_PlayoutAnchor_Playout_PlayoutId" FOREIGN KEY ("PlayoutId") REFERENCES "Playout" ("Id") ON DELETE CASCADE
);
CREATE TABLE "PlayoutBuildStatus" (
    "PlayoutId" INTEGER NOT NULL CONSTRAINT "PK_PlayoutBuildStatus" PRIMARY KEY,
    "LastBuild" TEXT NOT NULL,
    "Success" INTEGER NOT NULL,
    "Message" TEXT NULL,
    CONSTRAINT "FK_PlayoutBuildStatus_Playout_PlayoutId" FOREIGN KEY ("PlayoutId") REFERENCES "Playout" ("Id") ON DELETE CASCADE
);
CREATE TABLE "PlayoutGap" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_PlayoutGap" PRIMARY KEY AUTOINCREMENT,
    "PlayoutId" INTEGER NOT NULL,
    "Start" TEXT NOT NULL,
    "Finish" TEXT NOT NULL,
    CONSTRAINT "FK_PlayoutGap_Playout_PlayoutId" FOREIGN KEY ("PlayoutId") REFERENCES "Playout" ("Id") ON DELETE CASCADE
);
CREATE TABLE "PlayoutHistory" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_PlayoutHistory" PRIMARY KEY AUTOINCREMENT,
    "BlockId" INTEGER NULL,
    "Details" TEXT NULL,
    "Index" INTEGER NOT NULL,
    "Key" TEXT NULL,
    "PlaybackOrder" INTEGER NOT NULL,
    "PlayoutId" INTEGER NOT NULL,
    "When" TEXT NOT NULL, "Finish" TEXT NOT NULL DEFAULT '0001-01-01 00:00:00', "ChildKey" TEXT NULL, "IsCurrentChild" INTEGER NOT NULL DEFAULT 0,
    CONSTRAINT "FK_PlayoutHistory_Block_BlockId" FOREIGN KEY ("BlockId") REFERENCES "Block" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_PlayoutHistory_Playout_PlayoutId" FOREIGN KEY ("PlayoutId") REFERENCES "Playout" ("Id") ON DELETE CASCADE
);
CREATE TABLE "PlayoutItem" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_PlayoutItem" PRIMARY KEY AUTOINCREMENT,
    "BlockKey" TEXT NULL,
    "ChapterTitle" TEXT NULL,
    "CollectionEtag" TEXT NULL,
    "CollectionKey" TEXT NULL,
    "CustomTitle" TEXT NULL,
    "DisableWatermarks" INTEGER NOT NULL,
    "FillerKind" INTEGER NOT NULL,
    "Finish" TEXT NOT NULL,
    "GuideFinish" TEXT NULL,
    "GuideGroup" INTEGER NOT NULL,
    "GuideStart" TEXT NULL,
    "InPoint" TEXT NOT NULL,
    "MediaItemId" INTEGER NOT NULL,
    "OutPoint" TEXT NOT NULL,
    "PlayoutId" INTEGER NOT NULL,
    "PreferredAudioLanguageCode" TEXT NULL,
    "PreferredAudioTitle" TEXT NULL,
    "PreferredSubtitleLanguageCode" TEXT NULL,
    "Start" TEXT NOT NULL,
    "SubtitleMode" INTEGER NULL, "SchedulingContext" TEXT NULL,
    CONSTRAINT "FK_PlayoutItem_MediaItem_MediaItemId" FOREIGN KEY ("MediaItemId") REFERENCES "MediaItem" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_PlayoutItem_Playout_PlayoutId" FOREIGN KEY ("PlayoutId") REFERENCES "Playout" ("Id") ON DELETE CASCADE
);
CREATE TABLE "PlayoutItemGraphicsElement" (
    "PlayoutItemId" INTEGER NOT NULL,
    "GraphicsElementId" INTEGER NOT NULL, "Variables" TEXT NULL,
    CONSTRAINT "PK_PlayoutItemGraphicsElement" PRIMARY KEY ("PlayoutItemId", "GraphicsElementId"),
    CONSTRAINT "FK_PlayoutItemGraphicsElement_GraphicsElement_GraphicsElementId" FOREIGN KEY ("GraphicsElementId") REFERENCES "GraphicsElement" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_PlayoutItemGraphicsElement_PlayoutItem_PlayoutItemId" FOREIGN KEY ("PlayoutItemId") REFERENCES "PlayoutItem" ("Id") ON DELETE CASCADE
);
CREATE TABLE "PlayoutItemWatermark" (
    "PlayoutItemId" INTEGER NOT NULL,
    "WatermarkId" INTEGER NOT NULL,
    CONSTRAINT "PK_PlayoutItemWatermark" PRIMARY KEY ("PlayoutItemId", "WatermarkId"),
    CONSTRAINT "FK_PlayoutItemWatermark_ChannelWatermark_WatermarkId" FOREIGN KEY ("WatermarkId") REFERENCES "ChannelWatermark" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_PlayoutItemWatermark_PlayoutItem_PlayoutItemId" FOREIGN KEY ("PlayoutItemId") REFERENCES "PlayoutItem" ("Id") ON DELETE CASCADE
);
CREATE TABLE "PlayoutProgramScheduleAnchor" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_PlayoutProgramScheduleAnchor" PRIMARY KEY AUTOINCREMENT,
    "AnchorDate" TEXT NULL,
    "CollectionId" INTEGER NULL,
    "CollectionType" INTEGER NOT NULL,
    "FakeCollectionKey" TEXT NULL,
    "MediaItemId" INTEGER NULL,
    "MultiCollectionId" INTEGER NULL,
    "PlaylistId" INTEGER NULL,
    "PlayoutId" INTEGER NOT NULL,
    "RerunCollectionId" INTEGER NULL,
    "SmartCollectionId" INTEGER NULL, "SearchQuery" TEXT NULL,
    CONSTRAINT "FK_PlayoutProgramScheduleAnchor_Collection_CollectionId" FOREIGN KEY ("CollectionId") REFERENCES "Collection" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_PlayoutProgramScheduleAnchor_MediaItem_MediaItemId" FOREIGN KEY ("MediaItemId") REFERENCES "MediaItem" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_PlayoutProgramScheduleAnchor_MultiCollection_MultiCollectionId" FOREIGN KEY ("MultiCollectionId") REFERENCES "MultiCollection" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_PlayoutProgramScheduleAnchor_Playlist_PlaylistId" FOREIGN KEY ("PlaylistId") REFERENCES "Playlist" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_PlayoutProgramScheduleAnchor_Playout_PlayoutId" FOREIGN KEY ("PlayoutId") REFERENCES "Playout" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_PlayoutProgramScheduleAnchor_RerunCollection_RerunCollectionId" FOREIGN KEY ("RerunCollectionId") REFERENCES "RerunCollection" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_PlayoutProgramScheduleAnchor_SmartCollection_SmartCollectionId" FOREIGN KEY ("SmartCollectionId") REFERENCES "SmartCollection" ("Id") ON DELETE CASCADE
);
CREATE TABLE "PlayoutScheduleItemFillGroupIndex" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_PlayoutScheduleItemFillGroupIndex" PRIMARY KEY AUTOINCREMENT,
    "PlayoutId" INTEGER NOT NULL,
    "ProgramScheduleItemId" INTEGER NOT NULL,
    CONSTRAINT "FK_PlayoutScheduleItemFillGroupIndex_Playout_PlayoutId" FOREIGN KEY ("PlayoutId") REFERENCES "Playout" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_PlayoutScheduleItemFillGroupIndex_ProgramScheduleItem_ProgramScheduleItemId" FOREIGN KEY ("ProgramScheduleItemId") REFERENCES "ProgramScheduleItem" ("Id") ON DELETE CASCADE
);
CREATE TABLE "PlayoutTemplate" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_PlayoutTemplate" PRIMARY KEY AUTOINCREMENT,
    "DateUpdated" TEXT NOT NULL,
    "DaysOfMonth" TEXT NULL,
    "DaysOfWeek" TEXT NULL,
    "DecoTemplateId" INTEGER NULL,
    "EndDay" INTEGER NOT NULL,
    "EndMonth" INTEGER NOT NULL,
    "Index" INTEGER NOT NULL,
    "LimitToDateRange" INTEGER NOT NULL,
    "MonthsOfYear" TEXT NULL,
    "PlayoutId" INTEGER NOT NULL,
    "StartDay" INTEGER NOT NULL,
    "StartMonth" INTEGER NOT NULL,
    "TemplateId" INTEGER NOT NULL, "EndYear" INTEGER NULL, "StartYear" INTEGER NULL,
    CONSTRAINT "FK_PlayoutTemplate_DecoTemplate_DecoTemplateId" FOREIGN KEY ("DecoTemplateId") REFERENCES "DecoTemplate" ("Id") ON DELETE SET NULL,
    CONSTRAINT "FK_PlayoutTemplate_Playout_PlayoutId" FOREIGN KEY ("PlayoutId") REFERENCES "Playout" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_PlayoutTemplate_Template_TemplateId" FOREIGN KEY ("TemplateId") REFERENCES "Template" ("Id") ON DELETE CASCADE
);
CREATE TABLE "PlexCollection" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_PlexCollection" PRIMARY KEY AUTOINCREMENT,
    "Key" TEXT NULL,
    "Etag" TEXT NULL,
    "Name" TEXT NULL
);
CREATE TABLE "PlexConnection" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_PlexConnection" PRIMARY KEY AUTOINCREMENT,
    "IsActive" INTEGER NOT NULL,
    "Uri" TEXT NULL,
    "PlexMediaSourceId" INTEGER NOT NULL,
    CONSTRAINT "FK_PlexConnection_PlexMediaSource_PlexMediaSourceId" FOREIGN KEY ("PlexMediaSourceId") REFERENCES "PlexMediaSource" ("Id") ON DELETE CASCADE
);
CREATE TABLE "PlexEpisode" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_PlexEpisode" PRIMARY KEY AUTOINCREMENT,
    "Key" TEXT NULL, "Etag" TEXT NULL,
    CONSTRAINT "FK_PlexEpisode_Episode_Id" FOREIGN KEY ("Id") REFERENCES "Episode" ("Id") ON DELETE CASCADE
);
CREATE TABLE "PlexLibrary" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_PlexLibrary" PRIMARY KEY AUTOINCREMENT,
    "Key" TEXT NULL,
    "ShouldSyncItems" INTEGER NOT NULL, "LastNetworksScan" TEXT NULL,
    CONSTRAINT "FK_PlexLibrary_Library_Id" FOREIGN KEY ("Id") REFERENCES "Library" ("Id") ON DELETE CASCADE
);
CREATE TABLE "PlexMediaFile" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_PlexMediaFile" PRIMARY KEY AUTOINCREMENT,
    "PlexId" INTEGER NOT NULL,
    "Key" TEXT NULL,
    CONSTRAINT "FK_PlexMediaFile_MediaFile_Id" FOREIGN KEY ("Id") REFERENCES "MediaFile" ("Id") ON DELETE CASCADE
);
CREATE TABLE "PlexMediaSource" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_PlexMediaSource" PRIMARY KEY AUTOINCREMENT,
    "ClientIdentifier" TEXT NULL,
    "ProductVersion" TEXT NULL,
    "ServerName" TEXT NULL, "Platform" TEXT NULL, "PlatformVersion" TEXT NULL, "LastCollectionsScan" TEXT NULL,
    CONSTRAINT "FK_PlexMediaSource_MediaSource_Id" FOREIGN KEY ("Id") REFERENCES "MediaSource" ("Id") ON DELETE CASCADE
);
CREATE TABLE "PlexMovie" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_PlexMovie" PRIMARY KEY AUTOINCREMENT,
    "Key" TEXT NULL, "Etag" TEXT NULL,
    CONSTRAINT "FK_PlexMovie_Movie_Id" FOREIGN KEY ("Id") REFERENCES "Movie" ("Id") ON DELETE CASCADE
);
CREATE TABLE "PlexOtherVideo" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_PlexOtherVideo" PRIMARY KEY AUTOINCREMENT,
    "Key" TEXT NULL,
    "Etag" TEXT NULL,
    CONSTRAINT "FK_PlexOtherVideo_OtherVideo_Id" FOREIGN KEY ("Id") REFERENCES "OtherVideo" ("Id") ON DELETE CASCADE
);
CREATE TABLE "PlexPathReplacement" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_PlexPathReplacement" PRIMARY KEY AUTOINCREMENT,
    "PlexPath" TEXT NULL,
    "LocalPath" TEXT NULL,
    "PlexMediaSourceId" INTEGER NOT NULL,
    CONSTRAINT "FK_PlexPathReplacement_PlexMediaSource_PlexMediaSourceId" FOREIGN KEY ("PlexMediaSourceId") REFERENCES "PlexMediaSource" ("Id") ON DELETE CASCADE
);
CREATE TABLE "PlexSeason" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_PlexSeason" PRIMARY KEY AUTOINCREMENT,
    "Key" TEXT NULL, "Etag" TEXT NULL,
    CONSTRAINT "FK_PlexSeason_Season_Id" FOREIGN KEY ("Id") REFERENCES "Season" ("Id") ON DELETE CASCADE
);
CREATE TABLE "PlexShow" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_PlexShow" PRIMARY KEY AUTOINCREMENT,
    "Key" TEXT NULL, "Etag" TEXT NULL,
    CONSTRAINT "FK_PlexShow_Show_Id" FOREIGN KEY ("Id") REFERENCES "Show" ("Id") ON DELETE CASCADE
);
CREATE TABLE "ProgramSchedule" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_ProgramSchedule" PRIMARY KEY AUTOINCREMENT,
    "FixedStartTimeBehavior" INTEGER NOT NULL,
    "KeepMultiPartEpisodesTogether" INTEGER NOT NULL,
    "Name" varchar(50) COLLATE NOCASE NULL,
    "RandomStartPoint" INTEGER NOT NULL,
    "ShuffleScheduleItems" INTEGER NOT NULL,
    "TreatCollectionsAsShows" INTEGER NOT NULL
);
CREATE TABLE "ProgramScheduleAlternate" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_ProgramScheduleAlternate" PRIMARY KEY AUTOINCREMENT,
    "PlayoutId" INTEGER NOT NULL,
    "ProgramScheduleId" INTEGER NOT NULL,
    "Index" INTEGER NOT NULL,
    "DaysOfWeek" TEXT NULL,
    "DaysOfMonth" TEXT NULL,
    "MonthsOfYear" TEXT NULL, "EndDay" INTEGER NOT NULL DEFAULT 0, "EndMonth" INTEGER NOT NULL DEFAULT 0, "EndYear" INTEGER NULL, "LimitToDateRange" INTEGER NOT NULL DEFAULT 0, "StartDay" INTEGER NOT NULL DEFAULT 0, "StartMonth" INTEGER NOT NULL DEFAULT 0, "StartYear" INTEGER NULL,
    CONSTRAINT "FK_ProgramScheduleAlternate_Playout_PlayoutId" FOREIGN KEY ("PlayoutId") REFERENCES "Playout" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_ProgramScheduleAlternate_ProgramSchedule_ProgramScheduleId" FOREIGN KEY ("ProgramScheduleId") REFERENCES "ProgramSchedule" ("Id") ON DELETE CASCADE
);
CREATE TABLE "ProgramScheduleDurationItem" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_ProgramScheduleDurationItem" PRIMARY KEY AUTOINCREMENT,
    "PlayoutDuration" TEXT NOT NULL,
    "TailMode" INTEGER NOT NULL, "DiscardToFillAttempts" INTEGER NOT NULL DEFAULT 0,
    CONSTRAINT "FK_ProgramScheduleDurationItem_ProgramScheduleItem_Id" FOREIGN KEY ("Id") REFERENCES "ProgramScheduleItem" ("Id") ON DELETE CASCADE
);
CREATE TABLE "ProgramScheduleFloodItem" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_ProgramScheduleFloodItem" PRIMARY KEY AUTOINCREMENT,
    CONSTRAINT "FK_ProgramScheduleFloodItem_ProgramScheduleItem_Id" FOREIGN KEY ("Id") REFERENCES "ProgramScheduleItem" ("Id") ON DELETE CASCADE
);
CREATE TABLE "ProgramScheduleItem" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_ProgramScheduleItem" PRIMARY KEY AUTOINCREMENT,
    "CollectionId" INTEGER NULL,
    "CollectionType" INTEGER NOT NULL,
    "CustomTitle" TEXT NULL,
    "FakeCollectionKey" TEXT NULL,
    "FallbackFillerId" INTEGER NULL,
    "FillWithGroupMode" INTEGER NOT NULL,
    "FixedStartTimeBehavior" INTEGER NULL,
    "GuideMode" INTEGER NOT NULL,
    "Index" INTEGER NOT NULL,
    "MarathonBatchSize" INTEGER NULL,
    "MarathonGroupBy" INTEGER NOT NULL,
    "MarathonShuffleGroups" INTEGER NOT NULL,
    "MarathonShuffleItems" INTEGER NOT NULL,
    "MediaItemId" INTEGER NULL,
    "MidRollFillerId" INTEGER NULL,
    "MultiCollectionId" INTEGER NULL,
    "PlaybackOrder" INTEGER NOT NULL,
    "PlaylistId" INTEGER NULL,
    "PostRollFillerId" INTEGER NULL,
    "PreRollFillerId" INTEGER NULL,
    "PreferredAudioLanguageCode" TEXT NULL,
    "PreferredAudioTitle" TEXT NULL,
    "PreferredSubtitleLanguageCode" TEXT NULL,
    "ProgramScheduleId" INTEGER NOT NULL,
    "RerunCollectionId" INTEGER NULL,
    "SmartCollectionId" INTEGER NULL,
    "StartTime" TEXT NULL,
    "SubtitleMode" INTEGER NULL,
    "TailFillerId" INTEGER NULL, "SearchQuery" TEXT NULL, "SearchTitle" TEXT NULL,
    CONSTRAINT "FK_ProgramScheduleItem_Collection_CollectionId" FOREIGN KEY ("CollectionId") REFERENCES "Collection" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_ProgramScheduleItem_FillerPreset_FallbackFillerId" FOREIGN KEY ("FallbackFillerId") REFERENCES "FillerPreset" ("Id") ON DELETE SET NULL,
    CONSTRAINT "FK_ProgramScheduleItem_FillerPreset_MidRollFillerId" FOREIGN KEY ("MidRollFillerId") REFERENCES "FillerPreset" ("Id") ON DELETE SET NULL,
    CONSTRAINT "FK_ProgramScheduleItem_FillerPreset_PostRollFillerId" FOREIGN KEY ("PostRollFillerId") REFERENCES "FillerPreset" ("Id") ON DELETE SET NULL,
    CONSTRAINT "FK_ProgramScheduleItem_FillerPreset_PreRollFillerId" FOREIGN KEY ("PreRollFillerId") REFERENCES "FillerPreset" ("Id") ON DELETE SET NULL,
    CONSTRAINT "FK_ProgramScheduleItem_FillerPreset_TailFillerId" FOREIGN KEY ("TailFillerId") REFERENCES "FillerPreset" ("Id") ON DELETE SET NULL,
    CONSTRAINT "FK_ProgramScheduleItem_MediaItem_MediaItemId" FOREIGN KEY ("MediaItemId") REFERENCES "MediaItem" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_ProgramScheduleItem_MultiCollection_MultiCollectionId" FOREIGN KEY ("MultiCollectionId") REFERENCES "MultiCollection" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_ProgramScheduleItem_Playlist_PlaylistId" FOREIGN KEY ("PlaylistId") REFERENCES "Playlist" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_ProgramScheduleItem_ProgramSchedule_ProgramScheduleId" FOREIGN KEY ("ProgramScheduleId") REFERENCES "ProgramSchedule" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_ProgramScheduleItem_RerunCollection_RerunCollectionId" FOREIGN KEY ("RerunCollectionId") REFERENCES "RerunCollection" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_ProgramScheduleItem_SmartCollection_SmartCollectionId" FOREIGN KEY ("SmartCollectionId") REFERENCES "SmartCollection" ("Id") ON DELETE CASCADE
);
CREATE TABLE "ProgramScheduleItemGraphicsElement" (
    "ProgramScheduleItemId" INTEGER NOT NULL,
    "GraphicsElementId" INTEGER NOT NULL,
    CONSTRAINT "PK_ProgramScheduleItemGraphicsElement" PRIMARY KEY ("ProgramScheduleItemId", "GraphicsElementId"),
    CONSTRAINT "FK_ProgramScheduleItemGraphicsElement_GraphicsElement_GraphicsElementId" FOREIGN KEY ("GraphicsElementId") REFERENCES "GraphicsElement" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_ProgramScheduleItemGraphicsElement_ProgramScheduleItem_ProgramScheduleItemId" FOREIGN KEY ("ProgramScheduleItemId") REFERENCES "ProgramScheduleItem" ("Id") ON DELETE CASCADE
);
CREATE TABLE "ProgramScheduleItemWatermark" (
    "ProgramScheduleItemId" INTEGER NOT NULL,
    "WatermarkId" INTEGER NOT NULL,
    CONSTRAINT "PK_ProgramScheduleItemWatermark" PRIMARY KEY ("ProgramScheduleItemId", "WatermarkId"),
    CONSTRAINT "FK_ProgramScheduleItemWatermark_ChannelWatermark_WatermarkId" FOREIGN KEY ("WatermarkId") REFERENCES "ChannelWatermark" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_ProgramScheduleItemWatermark_ProgramScheduleItem_ProgramScheduleItemId" FOREIGN KEY ("ProgramScheduleItemId") REFERENCES "ProgramScheduleItem" ("Id") ON DELETE CASCADE
);
CREATE TABLE "ProgramScheduleMultipleItem" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_ProgramScheduleMultipleItem" PRIMARY KEY AUTOINCREMENT,
    "Count" TEXT NULL,
    "MultipleMode" INTEGER NOT NULL,
    CONSTRAINT "FK_ProgramScheduleMultipleItem_ProgramScheduleItem_Id" FOREIGN KEY ("Id") REFERENCES "ProgramScheduleItem" ("Id") ON DELETE CASCADE
);
CREATE TABLE "ProgramScheduleOneItem" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_ProgramScheduleOneItem" PRIMARY KEY AUTOINCREMENT,
    CONSTRAINT "FK_ProgramScheduleOneItem_ProgramScheduleItem_Id" FOREIGN KEY ("Id") REFERENCES "ProgramScheduleItem" ("Id") ON DELETE CASCADE
);
CREATE TABLE "RemoteStream" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_RemoteStream" PRIMARY KEY AUTOINCREMENT, "Duration" TEXT NULL, "FallbackQuery" TEXT NULL, "Script" TEXT NULL, "Url" TEXT NULL, "IsLive" INTEGER NOT NULL DEFAULT 0,
    CONSTRAINT "FK_RemoteStream_MediaItem_Id" FOREIGN KEY ("Id") REFERENCES "MediaItem" ("Id") ON DELETE CASCADE
);
CREATE TABLE "RemoteStreamMetadata" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_RemoteStreamMetadata" PRIMARY KEY AUTOINCREMENT,
    "RemoteStreamId" INTEGER NOT NULL,
    "MetadataKind" INTEGER NOT NULL,
    "Title" TEXT NULL,
    "OriginalTitle" TEXT NULL,
    "SortTitle" TEXT NULL,
    "Year" INTEGER NULL,
    "ReleaseDate" TEXT NULL,
    "DateAdded" TEXT NOT NULL,
    "DateUpdated" TEXT NOT NULL, "ContentRating" TEXT NULL, "Plot" TEXT NULL,
    CONSTRAINT "FK_RemoteStreamMetadata_RemoteStream_RemoteStreamId" FOREIGN KEY ("RemoteStreamId") REFERENCES "RemoteStream" ("Id") ON DELETE CASCADE
);
CREATE TABLE "RerunCollection" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_RerunCollection" PRIMARY KEY AUTOINCREMENT,
    "CollectionId" INTEGER NULL,
    "CollectionType" INTEGER NOT NULL,
    "FirstRunPlaybackOrder" INTEGER NOT NULL,
    "MediaItemId" INTEGER NULL,
    "MultiCollectionId" INTEGER NULL,
    "Name" varchar(50) COLLATE NOCASE NULL,
    "RerunPlaybackOrder" INTEGER NOT NULL,
    "SmartCollectionId" INTEGER NULL,
    CONSTRAINT "FK_RerunCollection_Collection_CollectionId" FOREIGN KEY ("CollectionId") REFERENCES "Collection" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_RerunCollection_MediaItem_MediaItemId" FOREIGN KEY ("MediaItemId") REFERENCES "MediaItem" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_RerunCollection_MultiCollection_MultiCollectionId" FOREIGN KEY ("MultiCollectionId") REFERENCES "MultiCollection" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_RerunCollection_SmartCollection_SmartCollectionId" FOREIGN KEY ("SmartCollectionId") REFERENCES "SmartCollection" ("Id") ON DELETE CASCADE
);
CREATE TABLE "RerunHistory" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_RerunHistory" PRIMARY KEY AUTOINCREMENT,
    "PlayoutId" INTEGER NOT NULL,
    "RerunCollectionId" INTEGER NOT NULL,
    "MediaItemId" INTEGER NOT NULL,
    "When" TEXT NOT NULL,
    CONSTRAINT "FK_RerunHistory_MediaItem_MediaItemId" FOREIGN KEY ("MediaItemId") REFERENCES "MediaItem" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_RerunHistory_Playout_PlayoutId" FOREIGN KEY ("PlayoutId") REFERENCES "Playout" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_RerunHistory_RerunCollection_RerunCollectionId" FOREIGN KEY ("RerunCollectionId") REFERENCES "RerunCollection" ("Id") ON DELETE CASCADE
);
CREATE TABLE "Resolution" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_Resolution" PRIMARY KEY AUTOINCREMENT,
    "Height" INTEGER NOT NULL,
    "Name" TEXT NULL,
    "Width" INTEGER NOT NULL
, "IsCustom" INTEGER NOT NULL DEFAULT 0);
CREATE TABLE "ScheduleItemsEnumeratorState" (
    "PlayoutAnchorPlayoutId" INTEGER NOT NULL CONSTRAINT "PK_ScheduleItemsEnumeratorState" PRIMARY KEY,
    "Seed" INTEGER NOT NULL,
    "Index" INTEGER NOT NULL,
    CONSTRAINT "FK_ScheduleItemsEnumeratorState_PlayoutAnchor_PlayoutAnchorPlayoutId" FOREIGN KEY ("PlayoutAnchorPlayoutId") REFERENCES "PlayoutAnchor" ("PlayoutId") ON DELETE CASCADE
);
CREATE TABLE "Season" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_Season" PRIMARY KEY AUTOINCREMENT,
    "ShowId" INTEGER NOT NULL, "SeasonNumber" INTEGER NOT NULL DEFAULT 0,
    CONSTRAINT "FK_Season_MediaItem_Id" FOREIGN KEY ("Id") REFERENCES "MediaItem" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Season_Show_ShowId" FOREIGN KEY ("ShowId") REFERENCES "Show" ("Id") ON DELETE CASCADE
);
CREATE TABLE "SeasonMetadata" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_SeasonMetadata" PRIMARY KEY AUTOINCREMENT,
    "Outline" TEXT NULL,
    "SeasonId" INTEGER NOT NULL,
    "MetadataKind" INTEGER NOT NULL,
    "Title" TEXT NULL,
    "OriginalTitle" TEXT NULL,
    "SortTitle" TEXT NULL,
    "ReleaseDate" TEXT NULL,
    "DateAdded" TEXT NOT NULL,
    "DateUpdated" TEXT NOT NULL, "Year" INTEGER NULL,
    CONSTRAINT "FK_SeasonMetadata_Season_SeasonId" FOREIGN KEY ("SeasonId") REFERENCES "Season" ("Id") ON DELETE CASCADE
);
CREATE TABLE "Show" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_Show" PRIMARY KEY AUTOINCREMENT,
    CONSTRAINT "FK_Show_MediaItem_Id" FOREIGN KEY ("Id") REFERENCES "MediaItem" ("Id") ON DELETE CASCADE
);
CREATE TABLE "ShowMetadata" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_ShowMetadata" PRIMARY KEY AUTOINCREMENT,
    "ContentRating" TEXT NULL,
    "DateAdded" TEXT NOT NULL,
    "DateUpdated" TEXT NOT NULL,
    "MetadataKind" INTEGER NOT NULL,
    "OriginalTitle" TEXT NULL,
    "Outline" TEXT NULL,
    "Plot" TEXT NULL,
    "ReleaseDate" TEXT NULL,
    "ShowId" INTEGER NOT NULL,
    "SortTitle" TEXT NULL,
    "Tagline" TEXT NULL,
    "Title" TEXT COLLATE NOCASE NULL,
    "Year" INTEGER NULL,
    CONSTRAINT "FK_ShowMetadata_Show_ShowId" FOREIGN KEY ("ShowId") REFERENCES "Show" ("Id") ON DELETE CASCADE
);
CREATE TABLE "SmartCollection" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_SmartCollection" PRIMARY KEY AUTOINCREMENT,
    "Name" varchar(50) COLLATE NOCASE NULL,
    "Query" TEXT NULL
);
CREATE TABLE "Song" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_Song" PRIMARY KEY AUTOINCREMENT,
    CONSTRAINT "FK_Song_MediaItem_Id" FOREIGN KEY ("Id") REFERENCES "MediaItem" ("Id") ON DELETE CASCADE
);
CREATE TABLE "SongMetadata" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_SongMetadata" PRIMARY KEY AUTOINCREMENT,
    "SongId" INTEGER NOT NULL,
    "MetadataKind" INTEGER NOT NULL,
    "Title" TEXT NULL,
    "OriginalTitle" TEXT NULL,
    "SortTitle" TEXT NULL,
    "Year" INTEGER NULL,
    "ReleaseDate" TEXT NULL,
    "DateAdded" TEXT NOT NULL,
    "DateUpdated" TEXT NOT NULL, "Album" TEXT NULL, "Artists" TEXT NULL, "Comment" TEXT NULL, "Track" TEXT NULL, "AlbumArtists" TEXT NULL,
    CONSTRAINT "FK_SongMetadata_Song_SongId" FOREIGN KEY ("SongId") REFERENCES "Song" ("Id") ON DELETE CASCADE
);
CREATE TABLE "Studio" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_Studio" PRIMARY KEY AUTOINCREMENT,
    "ArtistMetadataId" INTEGER NULL,
    "EpisodeMetadataId" INTEGER NULL,
    "ImageMetadataId" INTEGER NULL,
    "MovieMetadataId" INTEGER NULL,
    "MusicVideoMetadataId" INTEGER NULL,
    "Name" TEXT NULL,
    "OtherVideoMetadataId" INTEGER NULL,
    "RemoteStreamMetadataId" INTEGER NULL,
    "SeasonMetadataId" INTEGER NULL,
    "ShowMetadataId" INTEGER NULL,
    "SongMetadataId" INTEGER NULL,
    CONSTRAINT "FK_Studio_ArtistMetadata_ArtistMetadataId" FOREIGN KEY ("ArtistMetadataId") REFERENCES "ArtistMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Studio_EpisodeMetadata_EpisodeMetadataId" FOREIGN KEY ("EpisodeMetadataId") REFERENCES "EpisodeMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Studio_ImageMetadata_ImageMetadataId" FOREIGN KEY ("ImageMetadataId") REFERENCES "ImageMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Studio_MovieMetadata_MovieMetadataId" FOREIGN KEY ("MovieMetadataId") REFERENCES "MovieMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Studio_MusicVideoMetadata_MusicVideoMetadataId" FOREIGN KEY ("MusicVideoMetadataId") REFERENCES "MusicVideoMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Studio_OtherVideoMetadata_OtherVideoMetadataId" FOREIGN KEY ("OtherVideoMetadataId") REFERENCES "OtherVideoMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Studio_RemoteStreamMetadata_RemoteStreamMetadataId" FOREIGN KEY ("RemoteStreamMetadataId") REFERENCES "RemoteStreamMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Studio_SeasonMetadata_SeasonMetadataId" FOREIGN KEY ("SeasonMetadataId") REFERENCES "SeasonMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Studio_ShowMetadata_ShowMetadataId" FOREIGN KEY ("ShowMetadataId") REFERENCES "ShowMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Studio_SongMetadata_SongMetadataId" FOREIGN KEY ("SongMetadataId") REFERENCES "SongMetadata" ("Id") ON DELETE CASCADE
);
CREATE TABLE "Style" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_Style" PRIMARY KEY AUTOINCREMENT,
    "Name" TEXT NULL,
    "ArtistMetadataId" INTEGER NULL,
    CONSTRAINT "FK_Style_ArtistMetadata_ArtistMetadataId" FOREIGN KEY ("ArtistMetadataId") REFERENCES "ArtistMetadata" ("Id") ON DELETE CASCADE
);
CREATE TABLE "Subtitle" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_Subtitle" PRIMARY KEY AUTOINCREMENT,
    "ArtistMetadataId" INTEGER NULL,
    "Codec" TEXT NULL,
    "DateAdded" TEXT NOT NULL,
    "DateUpdated" TEXT NOT NULL,
    "Default" INTEGER NOT NULL,
    "EpisodeMetadataId" INTEGER NULL,
    "Forced" INTEGER NOT NULL,
    "ImageMetadataId" INTEGER NULL,
    "IsExtracted" INTEGER NOT NULL DEFAULT 0,
    "Language" TEXT NULL,
    "MovieMetadataId" INTEGER NULL,
    "MusicVideoMetadataId" INTEGER NULL,
    "OtherVideoMetadataId" INTEGER NULL,
    "Path" TEXT NULL,
    "RemoteStreamMetadataId" INTEGER NULL,
    "SDH" INTEGER NOT NULL DEFAULT 0,
    "SeasonMetadataId" INTEGER NULL,
    "ShowMetadataId" INTEGER NULL,
    "SongMetadataId" INTEGER NULL,
    "StreamIndex" INTEGER NOT NULL,
    "SubtitleKind" INTEGER NOT NULL,
    "Title" TEXT NULL,
    CONSTRAINT "FK_Subtitle_ArtistMetadata_ArtistMetadataId" FOREIGN KEY ("ArtistMetadataId") REFERENCES "ArtistMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Subtitle_EpisodeMetadata_EpisodeMetadataId" FOREIGN KEY ("EpisodeMetadataId") REFERENCES "EpisodeMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Subtitle_ImageMetadata_ImageMetadataId" FOREIGN KEY ("ImageMetadataId") REFERENCES "ImageMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Subtitle_MovieMetadata_MovieMetadataId" FOREIGN KEY ("MovieMetadataId") REFERENCES "MovieMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Subtitle_MusicVideoMetadata_MusicVideoMetadataId" FOREIGN KEY ("MusicVideoMetadataId") REFERENCES "MusicVideoMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Subtitle_OtherVideoMetadata_OtherVideoMetadataId" FOREIGN KEY ("OtherVideoMetadataId") REFERENCES "OtherVideoMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Subtitle_RemoteStreamMetadata_RemoteStreamMetadataId" FOREIGN KEY ("RemoteStreamMetadataId") REFERENCES "RemoteStreamMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Subtitle_SeasonMetadata_SeasonMetadataId" FOREIGN KEY ("SeasonMetadataId") REFERENCES "SeasonMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Subtitle_ShowMetadata_ShowMetadataId" FOREIGN KEY ("ShowMetadataId") REFERENCES "ShowMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Subtitle_SongMetadata_SongMetadataId" FOREIGN KEY ("SongMetadataId") REFERENCES "SongMetadata" ("Id") ON DELETE CASCADE
);
CREATE TABLE "Tag" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_Tag" PRIMARY KEY AUTOINCREMENT,
    "ArtistMetadataId" INTEGER NULL,
    "EpisodeMetadataId" INTEGER NULL,
    "ExternalCollectionId" TEXT NULL,
    "ExternalTypeId" TEXT NULL,
    "ImageMetadataId" INTEGER NULL,
    "MovieMetadataId" INTEGER NULL,
    "MusicVideoMetadataId" INTEGER NULL,
    "Name" TEXT NULL,
    "OtherVideoMetadataId" INTEGER NULL,
    "RemoteStreamMetadataId" INTEGER NULL,
    "SeasonMetadataId" INTEGER NULL,
    "ShowMetadataId" INTEGER NULL,
    "SongMetadataId" INTEGER NULL,
    CONSTRAINT "FK_Tag_ArtistMetadata_ArtistMetadataId" FOREIGN KEY ("ArtistMetadataId") REFERENCES "ArtistMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Tag_EpisodeMetadata_EpisodeMetadataId" FOREIGN KEY ("EpisodeMetadataId") REFERENCES "EpisodeMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Tag_ImageMetadata_ImageMetadataId" FOREIGN KEY ("ImageMetadataId") REFERENCES "ImageMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Tag_MovieMetadata_MovieMetadataId" FOREIGN KEY ("MovieMetadataId") REFERENCES "MovieMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Tag_MusicVideoMetadata_MusicVideoMetadataId" FOREIGN KEY ("MusicVideoMetadataId") REFERENCES "MusicVideoMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Tag_OtherVideoMetadata_OtherVideoMetadataId" FOREIGN KEY ("OtherVideoMetadataId") REFERENCES "OtherVideoMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Tag_RemoteStreamMetadata_RemoteStreamMetadataId" FOREIGN KEY ("RemoteStreamMetadataId") REFERENCES "RemoteStreamMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Tag_SeasonMetadata_SeasonMetadataId" FOREIGN KEY ("SeasonMetadataId") REFERENCES "SeasonMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Tag_ShowMetadata_ShowMetadataId" FOREIGN KEY ("ShowMetadataId") REFERENCES "ShowMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Tag_SongMetadata_SongMetadataId" FOREIGN KEY ("SongMetadataId") REFERENCES "SongMetadata" ("Id") ON DELETE CASCADE
);
CREATE TABLE "Template" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_Template" PRIMARY KEY AUTOINCREMENT,
    "DateUpdated" TEXT NOT NULL,
    "Name" varchar(50) COLLATE NOCASE NULL,
    "TemplateGroupId" INTEGER NOT NULL,
    CONSTRAINT "FK_Template_TemplateGroup_TemplateGroupId" FOREIGN KEY ("TemplateGroupId") REFERENCES "TemplateGroup" ("Id") ON DELETE CASCADE
);
CREATE TABLE "TemplateGroup" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_TemplateGroup" PRIMARY KEY AUTOINCREMENT,
    "Name" varchar(50) COLLATE NOCASE NULL
);
CREATE TABLE "TemplateItem" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_TemplateItem" PRIMARY KEY AUTOINCREMENT,
    "TemplateId" INTEGER NOT NULL,
    "BlockId" INTEGER NOT NULL,
    "StartTime" TEXT NOT NULL,
    CONSTRAINT "FK_TemplateItem_Block_BlockId" FOREIGN KEY ("BlockId") REFERENCES "Block" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_TemplateItem_Template_TemplateId" FOREIGN KEY ("TemplateId") REFERENCES "Template" ("Id") ON DELETE CASCADE
);
CREATE TABLE "TraktList" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_TraktList" PRIMARY KEY AUTOINCREMENT,
    "AutoRefresh" INTEGER NOT NULL,
    "Description" TEXT NULL,
    "GeneratePlaylist" INTEGER NOT NULL,
    "ItemCount" INTEGER NOT NULL,
    "LastMatch" TEXT NULL,
    "LastUpdate" TEXT NULL,
    "List" TEXT NULL,
    "Name" TEXT COLLATE NOCASE NULL,
    "PlaylistId" INTEGER NULL,
    "TraktId" INTEGER NOT NULL,
    "User" TEXT NULL,
    CONSTRAINT "FK_TraktList_Playlist_PlaylistId" FOREIGN KEY ("PlaylistId") REFERENCES "Playlist" ("Id") ON DELETE SET NULL
);
CREATE TABLE "TraktListItem" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_TraktListItem" PRIMARY KEY AUTOINCREMENT,
    "Episode" INTEGER NULL,
    "Kind" INTEGER NOT NULL,
    "MediaItemId" INTEGER NULL,
    "Rank" INTEGER NOT NULL,
    "Season" INTEGER NULL,
    "Title" TEXT NULL,
    "TraktId" INTEGER NOT NULL,
    "TraktListId" INTEGER NOT NULL,
    "Year" INTEGER NULL,
    CONSTRAINT "FK_TraktListItem_MediaItem_MediaItemId" FOREIGN KEY ("MediaItemId") REFERENCES "MediaItem" ("Id") ON DELETE SET NULL,
    CONSTRAINT "FK_TraktListItem_TraktList_TraktListId" FOREIGN KEY ("TraktListId") REFERENCES "TraktList" ("Id") ON DELETE CASCADE
);
CREATE TABLE "TraktListItemGuid" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_TraktListItemGuid" PRIMARY KEY AUTOINCREMENT,
    "Guid" TEXT NULL,
    "TraktListItemId" INTEGER NOT NULL,
    CONSTRAINT "FK_TraktListItemGuid_TraktListItem_TraktListItemId" FOREIGN KEY ("TraktListItemId") REFERENCES "TraktListItem" ("Id") ON DELETE CASCADE
);
CREATE TABLE "Writer" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_Writer" PRIMARY KEY AUTOINCREMENT,
    "EpisodeMetadataId" INTEGER NULL,
    "MovieMetadataId" INTEGER NULL,
    "Name" TEXT NULL,
    "OtherVideoMetadataId" INTEGER NULL,
    CONSTRAINT "FK_Writer_EpisodeMetadata_EpisodeMetadataId" FOREIGN KEY ("EpisodeMetadataId") REFERENCES "EpisodeMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Writer_MovieMetadata_MovieMetadataId" FOREIGN KEY ("MovieMetadataId") REFERENCES "MovieMetadata" ("Id") ON DELETE CASCADE,
    CONSTRAINT "FK_Writer_OtherVideoMetadata_OtherVideoMetadataId" FOREIGN KEY ("OtherVideoMetadataId") REFERENCES "OtherVideoMetadata" ("Id") ON DELETE CASCADE
);
CREATE TABLE "__EFMigrationsHistory" (
    "MigrationId" TEXT NOT NULL CONSTRAINT "PK___EFMigrationsHistory" PRIMARY KEY,
    "ProductVersion" TEXT NOT NULL
);
CREATE TABLE "__EFMigrationsLock" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK___EFMigrationsLock" PRIMARY KEY,
    "Timestamp" TEXT NOT NULL
);
CREATE INDEX "IX_Actor_ArtistMetadataId" ON "Actor" ("ArtistMetadataId");
CREATE UNIQUE INDEX "IX_Actor_ArtworkId" ON "Actor" ("ArtworkId");
CREATE INDEX "IX_Actor_EpisodeMetadataId" ON "Actor" ("EpisodeMetadataId");
CREATE INDEX "IX_Actor_ImageMetadataId" ON "Actor" ("ImageMetadataId");
CREATE INDEX "IX_Actor_MovieMetadataId" ON "Actor" ("MovieMetadataId");
CREATE INDEX "IX_Actor_MusicVideoMetadataId" ON "Actor" ("MusicVideoMetadataId");
CREATE INDEX "IX_Actor_OtherVideoMetadataId" ON "Actor" ("OtherVideoMetadataId");
CREATE INDEX "IX_Actor_RemoteStreamMetadataId" ON "Actor" ("RemoteStreamMetadataId");
CREATE INDEX "IX_Actor_SeasonMetadataId" ON "Actor" ("SeasonMetadataId");
CREATE INDEX "IX_Actor_ShowMetadataId" ON "Actor" ("ShowMetadataId");
CREATE INDEX "IX_Actor_SongMetadataId" ON "Actor" ("SongMetadataId");
CREATE INDEX "IX_ArtistMetadata_ArtistId" ON "ArtistMetadata" ("ArtistId");
CREATE INDEX "IX_ArtistMetadata_Title" ON "ArtistMetadata" ("Title");
CREATE INDEX "IX_Artwork_ArtistMetadataId" ON "Artwork" ("ArtistMetadataId");
CREATE INDEX "IX_Artwork_ChannelId" ON "Artwork" ("ChannelId");
CREATE INDEX "IX_Artwork_EpisodeMetadataId" ON "Artwork" ("EpisodeMetadataId");
CREATE INDEX "IX_Artwork_ImageMetadataId" ON "Artwork" ("ImageMetadataId");
CREATE INDEX "IX_Artwork_IsMetadataOrphan" ON "Artwork" ("IsMetadataOrphan");
CREATE INDEX "IX_Artwork_MovieMetadataId" ON "Artwork" ("MovieMetadataId");
CREATE INDEX "IX_Artwork_MusicVideoMetadataId" ON "Artwork" ("MusicVideoMetadataId");
CREATE INDEX "IX_Artwork_OtherVideoMetadataId" ON "Artwork" ("OtherVideoMetadataId");
CREATE INDEX "IX_Artwork_RemoteStreamMetadataId" ON "Artwork" ("RemoteStreamMetadataId");
CREATE INDEX "IX_Artwork_SeasonMetadataId" ON "Artwork" ("SeasonMetadataId");
CREATE INDEX "IX_Artwork_ShowMetadataId" ON "Artwork" ("ShowMetadataId");
CREATE INDEX "IX_Artwork_SongMetadataId" ON "Artwork" ("SongMetadataId");
CREATE UNIQUE INDEX "IX_BlockGroup_Name" ON "BlockGroup" ("Name");
CREATE INDEX "IX_BlockItemGraphicsElement_GraphicsElementId" ON "BlockItemGraphicsElement" ("GraphicsElementId");
CREATE INDEX "IX_BlockItemWatermark_WatermarkId" ON "BlockItemWatermark" ("WatermarkId");
CREATE INDEX "IX_BlockItem_BlockId" ON "BlockItem" ("BlockId");
CREATE INDEX "IX_BlockItem_CollectionId" ON "BlockItem" ("CollectionId");
CREATE INDEX "IX_BlockItem_MediaItemId" ON "BlockItem" ("MediaItemId");
CREATE INDEX "IX_BlockItem_MultiCollectionId" ON "BlockItem" ("MultiCollectionId");
CREATE INDEX "IX_BlockItem_SmartCollectionId" ON "BlockItem" ("SmartCollectionId");
CREATE UNIQUE INDEX "IX_Block_BlockGroupId_Name" ON "Block" ("BlockGroupId", "Name");
CREATE UNIQUE INDEX "IX_ChannelWatermark_Name" ON "ChannelWatermark" ("Name");
CREATE INDEX "IX_Channel_FFmpegProfileId" ON "Channel" ("FFmpegProfileId");
CREATE INDEX "IX_Channel_FallbackFillerId" ON "Channel" ("FallbackFillerId");
CREATE INDEX "IX_Channel_MirrorSourceChannelId" ON "Channel" ("MirrorSourceChannelId");
CREATE UNIQUE INDEX "IX_Channel_Number" ON "Channel" ("Number");
CREATE INDEX "IX_Channel_WatermarkId" ON "Channel" ("WatermarkId");
CREATE INDEX "IX_CollectionItem_MediaItemId" ON "CollectionItem" ("MediaItemId");
CREATE UNIQUE INDEX "IX_Collection_Name" ON "Collection" ("Name");
CREATE UNIQUE INDEX "IX_ConfigElement_Key" ON "ConfigElement" ("Key");
CREATE INDEX "IX_DecoBreakContent_CollectionId" ON "DecoBreakContent" ("CollectionId");
CREATE INDEX "IX_DecoBreakContent_DecoId" ON "DecoBreakContent" ("DecoId");
CREATE INDEX "IX_DecoBreakContent_MediaItemId" ON "DecoBreakContent" ("MediaItemId");
CREATE INDEX "IX_DecoBreakContent_MultiCollectionId" ON "DecoBreakContent" ("MultiCollectionId");
CREATE INDEX "IX_DecoBreakContent_PlaylistId" ON "DecoBreakContent" ("PlaylistId");
CREATE INDEX "IX_DecoBreakContent_SmartCollectionId" ON "DecoBreakContent" ("SmartCollectionId");
CREATE INDEX "IX_DecoGraphicsElement_GraphicsElementId" ON "DecoGraphicsElement" ("GraphicsElementId");
CREATE UNIQUE INDEX "IX_DecoGroup_Name" ON "DecoGroup" ("Name");
CREATE UNIQUE INDEX "IX_DecoTemplateGroup_Name" ON "DecoTemplateGroup" ("Name");
CREATE INDEX "IX_DecoTemplateItem_DecoId" ON "DecoTemplateItem" ("DecoId");
CREATE INDEX "IX_DecoTemplateItem_DecoTemplateId" ON "DecoTemplateItem" ("DecoTemplateId");
CREATE UNIQUE INDEX "IX_DecoTemplate_DecoTemplateGroupId_Name" ON "DecoTemplate" ("DecoTemplateGroupId", "Name");
CREATE INDEX "IX_DecoWatermark_WatermarkId" ON "DecoWatermark" ("WatermarkId");
CREATE INDEX "IX_Deco_DeadAirFallbackCollectionId" ON "Deco" ("DeadAirFallbackCollectionId");
CREATE INDEX "IX_Deco_DeadAirFallbackMediaItemId" ON "Deco" ("DeadAirFallbackMediaItemId");
CREATE INDEX "IX_Deco_DeadAirFallbackMultiCollectionId" ON "Deco" ("DeadAirFallbackMultiCollectionId");
CREATE INDEX "IX_Deco_DeadAirFallbackSmartCollectionId" ON "Deco" ("DeadAirFallbackSmartCollectionId");
CREATE UNIQUE INDEX "IX_Deco_DecoGroupId_Name" ON "Deco" ("DecoGroupId", "Name");
CREATE INDEX "IX_Deco_DefaultFillerCollectionId" ON "Deco" ("DefaultFillerCollectionId");
CREATE INDEX "IX_Deco_DefaultFillerMediaItemId" ON "Deco" ("DefaultFillerMediaItemId");
CREATE INDEX "IX_Deco_DefaultFillerMultiCollectionId" ON "Deco" ("DefaultFillerMultiCollectionId");
CREATE INDEX "IX_Deco_DefaultFillerSmartCollectionId" ON "Deco" ("DefaultFillerSmartCollectionId");
CREATE INDEX "IX_Director_EpisodeMetadataId" ON "Director" ("EpisodeMetadataId");
CREATE INDEX "IX_Director_MovieMetadataId" ON "Director" ("MovieMetadataId");
CREATE INDEX "IX_Director_MusicVideoMetadataId" ON "Director" ("MusicVideoMetadataId");
CREATE INDEX "IX_Director_OtherVideoMetadataId" ON "Director" ("OtherVideoMetadataId");
CREATE INDEX "IX_EmbyConnection_EmbyMediaSourceId" ON "EmbyConnection" ("EmbyMediaSourceId");
CREATE INDEX "IX_EmbyPathInfo_EmbyLibraryId" ON "EmbyPathInfo" ("EmbyLibraryId");
CREATE INDEX "IX_EmbyPathReplacement_EmbyMediaSourceId" ON "EmbyPathReplacement" ("EmbyMediaSourceId");
CREATE INDEX "IX_EpisodeMetadata_EpisodeId" ON "EpisodeMetadata" ("EpisodeId");
CREATE INDEX "IX_Episode_SeasonId" ON "Episode" ("SeasonId");
CREATE INDEX "IX_FFmpegProfile_ResolutionId" ON "FFmpegProfile" ("ResolutionId");
CREATE INDEX "IX_FillerPreset_CollectionId" ON "FillerPreset" ("CollectionId");
CREATE INDEX "IX_FillerPreset_MediaItemId" ON "FillerPreset" ("MediaItemId");
CREATE INDEX "IX_FillerPreset_MultiCollectionId" ON "FillerPreset" ("MultiCollectionId");
CREATE UNIQUE INDEX "IX_FillerPreset_Name" ON "FillerPreset" ("Name");
CREATE INDEX "IX_FillerPreset_PlaylistId" ON "FillerPreset" ("PlaylistId");
CREATE INDEX "IX_FillerPreset_SmartCollectionId" ON "FillerPreset" ("SmartCollectionId");
CREATE INDEX "IX_Genre_ArtistMetadataId" ON "Genre" ("ArtistMetadataId");
CREATE INDEX "IX_Genre_EpisodeMetadataId" ON "Genre" ("EpisodeMetadataId");
CREATE INDEX "IX_Genre_ImageMetadataId" ON "Genre" ("ImageMetadataId");
CREATE INDEX "IX_Genre_MovieMetadataId" ON "Genre" ("MovieMetadataId");
CREATE INDEX "IX_Genre_MusicVideoMetadataId" ON "Genre" ("MusicVideoMetadataId");
CREATE INDEX "IX_Genre_OtherVideoMetadataId" ON "Genre" ("OtherVideoMetadataId");
CREATE INDEX "IX_Genre_RemoteStreamMetadataId" ON "Genre" ("RemoteStreamMetadataId");
CREATE INDEX "IX_Genre_SeasonMetadataId" ON "Genre" ("SeasonMetadataId");
CREATE INDEX "IX_Genre_ShowMetadataId" ON "Genre" ("ShowMetadataId");
CREATE INDEX "IX_Genre_SongMetadataId" ON "Genre" ("SongMetadataId");
CREATE UNIQUE INDEX "IX_ImageFolderDuration_LibraryFolderId" ON "ImageFolderDuration" ("LibraryFolderId");
CREATE INDEX "IX_ImageMetadata_ImageId" ON "ImageMetadata" ("ImageId");
CREATE INDEX "IX_JellyfinCollection_ItemId" ON "JellyfinCollection" ("ItemId");
CREATE INDEX "IX_JellyfinConnection_JellyfinMediaSourceId" ON "JellyfinConnection" ("JellyfinMediaSourceId");
CREATE INDEX "IX_JellyfinEpisode_ItemId" ON "JellyfinEpisode" ("ItemId");
CREATE INDEX "IX_JellyfinLibrary_ItemId" ON "JellyfinLibrary" ("ItemId");
CREATE INDEX "IX_JellyfinMovie_ItemId" ON "JellyfinMovie" ("ItemId");
CREATE INDEX "IX_JellyfinPathInfo_JellyfinLibraryId" ON "JellyfinPathInfo" ("JellyfinLibraryId");
CREATE INDEX "IX_JellyfinPathReplacement_JellyfinMediaSourceId" ON "JellyfinPathReplacement" ("JellyfinMediaSourceId");
CREATE INDEX "IX_JellyfinSeason_ItemId" ON "JellyfinSeason" ("ItemId");
CREATE INDEX "IX_JellyfinShow_ItemId" ON "JellyfinShow" ("ItemId");
CREATE INDEX "IX_LibraryFolder_LibraryPathId" ON "LibraryFolder" ("LibraryPathId");
CREATE INDEX "IX_LibraryFolder_ParentId" ON "LibraryFolder" ("ParentId");
CREATE INDEX "IX_LibraryPath_LibraryId" ON "LibraryPath" ("LibraryId");
CREATE INDEX "IX_Library_MediaSourceId" ON "Library" ("MediaSourceId");
CREATE INDEX "IX_MediaChapter_MediaVersionId" ON "MediaChapter" ("MediaVersionId");
CREATE INDEX "IX_MediaFile_LibraryFolderId" ON "MediaFile" ("LibraryFolderId");
CREATE INDEX "IX_MediaFile_MediaVersionId" ON "MediaFile" ("MediaVersionId");
CREATE UNIQUE INDEX "IX_MediaFile_PathHash" ON "MediaFile" ("PathHash");
CREATE INDEX "IX_MediaItem_LibraryPathId" ON "MediaItem" ("LibraryPathId");
CREATE INDEX "IX_MediaStream_MediaVersionId" ON "MediaStream" ("MediaVersionId");
CREATE INDEX "IX_MediaVersion_EpisodeId" ON "MediaVersion" ("EpisodeId");
CREATE INDEX "IX_MediaVersion_ImageId" ON "MediaVersion" ("ImageId");
CREATE INDEX "IX_MediaVersion_MovieId" ON "MediaVersion" ("MovieId");
CREATE INDEX "IX_MediaVersion_MusicVideoId" ON "MediaVersion" ("MusicVideoId");
CREATE INDEX "IX_MediaVersion_OtherVideoId" ON "MediaVersion" ("OtherVideoId");
CREATE INDEX "IX_MediaVersion_RemoteStreamId" ON "MediaVersion" ("RemoteStreamId");
CREATE INDEX "IX_MediaVersion_SongId" ON "MediaVersion" ("SongId");
CREATE INDEX "IX_MetadataGuid_ArtistMetadataId" ON "MetadataGuid" ("ArtistMetadataId");
CREATE INDEX "IX_MetadataGuid_EpisodeMetadataId" ON "MetadataGuid" ("EpisodeMetadataId");
CREATE INDEX "IX_MetadataGuid_Guid" ON "MetadataGuid" ("Guid");
CREATE INDEX "IX_MetadataGuid_ImageMetadataId" ON "MetadataGuid" ("ImageMetadataId");
CREATE INDEX "IX_MetadataGuid_MovieMetadataId" ON "MetadataGuid" ("MovieMetadataId");
CREATE INDEX "IX_MetadataGuid_MusicVideoMetadataId" ON "MetadataGuid" ("MusicVideoMetadataId");
CREATE INDEX "IX_MetadataGuid_OtherVideoMetadataId" ON "MetadataGuid" ("OtherVideoMetadataId");
CREATE INDEX "IX_MetadataGuid_RemoteStreamMetadataId" ON "MetadataGuid" ("RemoteStreamMetadataId");
CREATE INDEX "IX_MetadataGuid_SeasonMetadataId" ON "MetadataGuid" ("SeasonMetadataId");
CREATE INDEX "IX_MetadataGuid_ShowMetadataId" ON "MetadataGuid" ("ShowMetadataId");
CREATE INDEX "IX_MetadataGuid_SongMetadataId" ON "MetadataGuid" ("SongMetadataId");
CREATE INDEX "IX_Mood_ArtistMetadataId" ON "Mood" ("ArtistMetadataId");
CREATE INDEX "IX_MovieMetadata_MovieId" ON "MovieMetadata" ("MovieId");
CREATE INDEX "IX_MovieMetadata_Title" ON "MovieMetadata" ("Title");
CREATE INDEX "IX_MultiCollectionItem_CollectionId" ON "MultiCollectionItem" ("CollectionId");
CREATE INDEX "IX_MultiCollectionSmartItem_SmartCollectionId" ON "MultiCollectionSmartItem" ("SmartCollectionId");
CREATE UNIQUE INDEX "IX_MultiCollection_Name" ON "MultiCollection" ("Name");
CREATE INDEX "IX_MusicVideoArtist_MusicVideoMetadataId" ON "MusicVideoArtist" ("MusicVideoMetadataId");
CREATE INDEX "IX_MusicVideoMetadata_MusicVideoId" ON "MusicVideoMetadata" ("MusicVideoId");
CREATE INDEX "IX_MusicVideo_ArtistId" ON "MusicVideo" ("ArtistId");
CREATE INDEX "IX_OtherVideoMetadata_OtherVideoId" ON "OtherVideoMetadata" ("OtherVideoId");
CREATE UNIQUE INDEX "IX_PlaylistGroup_Name" ON "PlaylistGroup" ("Name");
CREATE INDEX "IX_PlaylistItem_CollectionId" ON "PlaylistItem" ("CollectionId");
CREATE INDEX "IX_PlaylistItem_MediaItemId" ON "PlaylistItem" ("MediaItemId");
CREATE INDEX "IX_PlaylistItem_MultiCollectionId" ON "PlaylistItem" ("MultiCollectionId");
CREATE INDEX "IX_PlaylistItem_PlaylistId" ON "PlaylistItem" ("PlaylistId");
CREATE INDEX "IX_PlaylistItem_SmartCollectionId" ON "PlaylistItem" ("SmartCollectionId");
CREATE UNIQUE INDEX "IX_Playlist_PlaylistGroupId_Name" ON "Playlist" ("PlaylistGroupId", "Name");
CREATE INDEX "IX_PlayoutGap_PlayoutId" ON "PlayoutGap" ("PlayoutId");
CREATE INDEX "IX_PlayoutGap_Start_Finish" ON "PlayoutGap" ("Start", "Finish");
CREATE INDEX "IX_PlayoutHistory_BlockId" ON "PlayoutHistory" ("BlockId");
CREATE INDEX "IX_PlayoutHistory_PlayoutId" ON "PlayoutHistory" ("PlayoutId");
CREATE INDEX "IX_PlayoutItemGraphicsElement_GraphicsElementId" ON "PlayoutItemGraphicsElement" ("GraphicsElementId");
CREATE INDEX "IX_PlayoutItemWatermark_WatermarkId" ON "PlayoutItemWatermark" ("WatermarkId");
CREATE INDEX "IX_PlayoutItem_MediaItemId" ON "PlayoutItem" ("MediaItemId");
CREATE INDEX "IX_PlayoutItem_PlayoutId" ON "PlayoutItem" ("PlayoutId");
CREATE INDEX "IX_PlayoutItem_Start_Finish" ON "PlayoutItem" ("Start", "Finish");
CREATE INDEX "IX_PlayoutProgramScheduleAnchor_CollectionId" ON "PlayoutProgramScheduleAnchor" ("CollectionId");
CREATE INDEX "IX_PlayoutProgramScheduleAnchor_MediaItemId" ON "PlayoutProgramScheduleAnchor" ("MediaItemId");
CREATE INDEX "IX_PlayoutProgramScheduleAnchor_MultiCollectionId" ON "PlayoutProgramScheduleAnchor" ("MultiCollectionId");
CREATE INDEX "IX_PlayoutProgramScheduleAnchor_PlaylistId" ON "PlayoutProgramScheduleAnchor" ("PlaylistId");
CREATE INDEX "IX_PlayoutProgramScheduleAnchor_PlayoutId" ON "PlayoutProgramScheduleAnchor" ("PlayoutId");
CREATE INDEX "IX_PlayoutProgramScheduleAnchor_RerunCollectionId" ON "PlayoutProgramScheduleAnchor" ("RerunCollectionId");
CREATE INDEX "IX_PlayoutProgramScheduleAnchor_SmartCollectionId" ON "PlayoutProgramScheduleAnchor" ("SmartCollectionId");
CREATE INDEX "IX_PlayoutScheduleItemFillGroupIndex_PlayoutId" ON "PlayoutScheduleItemFillGroupIndex" ("PlayoutId");
CREATE INDEX "IX_PlayoutScheduleItemFillGroupIndex_ProgramScheduleItemId" ON "PlayoutScheduleItemFillGroupIndex" ("ProgramScheduleItemId");
CREATE INDEX "IX_PlayoutTemplate_DecoTemplateId" ON "PlayoutTemplate" ("DecoTemplateId");
CREATE INDEX "IX_PlayoutTemplate_PlayoutId" ON "PlayoutTemplate" ("PlayoutId");
CREATE INDEX "IX_PlayoutTemplate_TemplateId" ON "PlayoutTemplate" ("TemplateId");
CREATE INDEX "IX_Playout_ChannelId" ON "Playout" ("ChannelId");
CREATE INDEX "IX_Playout_DecoId" ON "Playout" ("DecoId");
CREATE INDEX "IX_Playout_ProgramScheduleId" ON "Playout" ("ProgramScheduleId");
CREATE INDEX "IX_PlexConnection_PlexMediaSourceId" ON "PlexConnection" ("PlexMediaSourceId");
CREATE INDEX "IX_PlexPathReplacement_PlexMediaSourceId" ON "PlexPathReplacement" ("PlexMediaSourceId");
CREATE INDEX "IX_ProgramScheduleAlternate_PlayoutId" ON "ProgramScheduleAlternate" ("PlayoutId");
CREATE INDEX "IX_ProgramScheduleAlternate_ProgramScheduleId" ON "ProgramScheduleAlternate" ("ProgramScheduleId");
CREATE INDEX "IX_ProgramScheduleItemGraphicsElement_GraphicsElementId" ON "ProgramScheduleItemGraphicsElement" ("GraphicsElementId");
CREATE INDEX "IX_ProgramScheduleItemWatermark_WatermarkId" ON "ProgramScheduleItemWatermark" ("WatermarkId");
CREATE INDEX "IX_ProgramScheduleItem_CollectionId" ON "ProgramScheduleItem" ("CollectionId");
CREATE INDEX "IX_ProgramScheduleItem_FallbackFillerId" ON "ProgramScheduleItem" ("FallbackFillerId");
CREATE INDEX "IX_ProgramScheduleItem_MediaItemId" ON "ProgramScheduleItem" ("MediaItemId");
CREATE INDEX "IX_ProgramScheduleItem_MidRollFillerId" ON "ProgramScheduleItem" ("MidRollFillerId");
CREATE INDEX "IX_ProgramScheduleItem_MultiCollectionId" ON "ProgramScheduleItem" ("MultiCollectionId");
CREATE INDEX "IX_ProgramScheduleItem_PlaylistId" ON "ProgramScheduleItem" ("PlaylistId");
CREATE INDEX "IX_ProgramScheduleItem_PostRollFillerId" ON "ProgramScheduleItem" ("PostRollFillerId");
CREATE INDEX "IX_ProgramScheduleItem_PreRollFillerId" ON "ProgramScheduleItem" ("PreRollFillerId");
CREATE INDEX "IX_ProgramScheduleItem_ProgramScheduleId" ON "ProgramScheduleItem" ("ProgramScheduleId");
CREATE INDEX "IX_ProgramScheduleItem_RerunCollectionId" ON "ProgramScheduleItem" ("RerunCollectionId");
CREATE INDEX "IX_ProgramScheduleItem_SmartCollectionId" ON "ProgramScheduleItem" ("SmartCollectionId");
CREATE INDEX "IX_ProgramScheduleItem_TailFillerId" ON "ProgramScheduleItem" ("TailFillerId");
CREATE UNIQUE INDEX "IX_ProgramSchedule_Name" ON "ProgramSchedule" ("Name");
CREATE INDEX "IX_RemoteStreamMetadata_RemoteStreamId" ON "RemoteStreamMetadata" ("RemoteStreamId");
CREATE INDEX "IX_RerunCollection_CollectionId" ON "RerunCollection" ("CollectionId");
CREATE INDEX "IX_RerunCollection_MediaItemId" ON "RerunCollection" ("MediaItemId");
CREATE INDEX "IX_RerunCollection_MultiCollectionId" ON "RerunCollection" ("MultiCollectionId");
CREATE UNIQUE INDEX "IX_RerunCollection_Name" ON "RerunCollection" ("Name");
CREATE INDEX "IX_RerunCollection_SmartCollectionId" ON "RerunCollection" ("SmartCollectionId");
CREATE INDEX "IX_RerunHistory_MediaItemId" ON "RerunHistory" ("MediaItemId");
CREATE INDEX "IX_RerunHistory_PlayoutId" ON "RerunHistory" ("PlayoutId");
CREATE INDEX "IX_RerunHistory_RerunCollectionId" ON "RerunHistory" ("RerunCollectionId");
CREATE INDEX "IX_SeasonMetadata_SeasonId" ON "SeasonMetadata" ("SeasonId");
CREATE INDEX "IX_Season_ShowId" ON "Season" ("ShowId");
CREATE INDEX "IX_ShowMetadata_ShowId" ON "ShowMetadata" ("ShowId");
CREATE INDEX "IX_ShowMetadata_Title" ON "ShowMetadata" ("Title");
CREATE UNIQUE INDEX "IX_SmartCollection_Name" ON "SmartCollection" ("Name");
CREATE INDEX "IX_SongMetadata_SongId" ON "SongMetadata" ("SongId");
CREATE INDEX "IX_Studio_ArtistMetadataId" ON "Studio" ("ArtistMetadataId");
CREATE INDEX "IX_Studio_EpisodeMetadataId" ON "Studio" ("EpisodeMetadataId");
CREATE INDEX "IX_Studio_ImageMetadataId" ON "Studio" ("ImageMetadataId");
CREATE INDEX "IX_Studio_MovieMetadataId" ON "Studio" ("MovieMetadataId");
CREATE INDEX "IX_Studio_MusicVideoMetadataId" ON "Studio" ("MusicVideoMetadataId");
CREATE INDEX "IX_Studio_OtherVideoMetadataId" ON "Studio" ("OtherVideoMetadataId");
CREATE INDEX "IX_Studio_RemoteStreamMetadataId" ON "Studio" ("RemoteStreamMetadataId");
CREATE INDEX "IX_Studio_SeasonMetadataId" ON "Studio" ("SeasonMetadataId");
CREATE INDEX "IX_Studio_ShowMetadataId" ON "Studio" ("ShowMetadataId");
CREATE INDEX "IX_Studio_SongMetadataId" ON "Studio" ("SongMetadataId");
CREATE INDEX "IX_Style_ArtistMetadataId" ON "Style" ("ArtistMetadataId");
CREATE INDEX "IX_Subtitle_ArtistMetadataId" ON "Subtitle" ("ArtistMetadataId");
CREATE INDEX "IX_Subtitle_EpisodeMetadataId" ON "Subtitle" ("EpisodeMetadataId");
CREATE INDEX "IX_Subtitle_ImageMetadataId" ON "Subtitle" ("ImageMetadataId");
CREATE INDEX "IX_Subtitle_MovieMetadataId" ON "Subtitle" ("MovieMetadataId");
CREATE INDEX "IX_Subtitle_MusicVideoMetadataId" ON "Subtitle" ("MusicVideoMetadataId");
CREATE INDEX "IX_Subtitle_OtherVideoMetadataId" ON "Subtitle" ("OtherVideoMetadataId");
CREATE INDEX "IX_Subtitle_RemoteStreamMetadataId" ON "Subtitle" ("RemoteStreamMetadataId");
CREATE INDEX "IX_Subtitle_SeasonMetadataId" ON "Subtitle" ("SeasonMetadataId");
CREATE INDEX "IX_Subtitle_ShowMetadataId" ON "Subtitle" ("ShowMetadataId");
CREATE INDEX "IX_Subtitle_SongMetadataId" ON "Subtitle" ("SongMetadataId");
CREATE INDEX "IX_Tag_ArtistMetadataId" ON "Tag" ("ArtistMetadataId");
CREATE INDEX "IX_Tag_EpisodeMetadataId" ON "Tag" ("EpisodeMetadataId");
CREATE INDEX "IX_Tag_ImageMetadataId" ON "Tag" ("ImageMetadataId");
CREATE INDEX "IX_Tag_MovieMetadataId" ON "Tag" ("MovieMetadataId");
CREATE INDEX "IX_Tag_MusicVideoMetadataId" ON "Tag" ("MusicVideoMetadataId");
CREATE INDEX "IX_Tag_OtherVideoMetadataId" ON "Tag" ("OtherVideoMetadataId");
CREATE INDEX "IX_Tag_RemoteStreamMetadataId" ON "Tag" ("RemoteStreamMetadataId");
CREATE INDEX "IX_Tag_SeasonMetadataId" ON "Tag" ("SeasonMetadataId");
CREATE INDEX "IX_Tag_ShowMetadataId" ON "Tag" ("ShowMetadataId");
CREATE INDEX "IX_Tag_SongMetadataId" ON "Tag" ("SongMetadataId");
CREATE UNIQUE INDEX "IX_TemplateGroup_Name" ON "TemplateGroup" ("Name");
CREATE INDEX "IX_TemplateItem_BlockId" ON "TemplateItem" ("BlockId");
CREATE INDEX "IX_TemplateItem_TemplateId" ON "TemplateItem" ("TemplateId");
CREATE UNIQUE INDEX "IX_Template_TemplateGroupId_Name" ON "Template" ("TemplateGroupId", "Name");
CREATE INDEX "IX_TraktListItemGuid_TraktListItemId" ON "TraktListItemGuid" ("TraktListItemId");
CREATE INDEX "IX_TraktListItem_MediaItemId" ON "TraktListItem" ("MediaItemId");
CREATE INDEX "IX_TraktListItem_TraktListId" ON "TraktListItem" ("TraktListId");
CREATE INDEX "IX_TraktList_Name" ON "TraktList" ("Name");
CREATE INDEX "IX_TraktList_PlaylistId" ON "TraktList" ("PlaylistId");
CREATE INDEX "IX_Writer_EpisodeMetadataId" ON "Writer" ("EpisodeMetadataId");
CREATE INDEX "IX_Writer_MovieMetadataId" ON "Writer" ("MovieMetadataId");
CREATE INDEX "IX_Writer_OtherVideoMetadataId" ON "Writer" ("OtherVideoMetadataId");
