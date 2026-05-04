PRAGMA foreign_keys = ON;

DELETE FROM "Playout";
DELETE FROM "Channel";
DELETE FROM "FFmpegProfile";
DELETE FROM "MediaSource";
DELETE FROM "ConfigElement"
WHERE "Key" IN ('ffmpeg.default_profile_id', 'ffmpeg.default_resolution_id');
DELETE FROM "Resolution";

DELETE FROM sqlite_sequence
WHERE name IN (
    'Channel',
    'FFmpegProfile',
    'Library',
    'LibraryPath',
    'LocalLibrary',
    'LocalMediaSource',
    'MediaItem',
    'MediaSource',
    'MediaVersion',
    'Movie',
    'MovieMetadata',
    'Playout',
    'Resolution'
);

INSERT INTO "Resolution" ("Id", "Name", "Width", "Height", "IsCustom")
VALUES (1, '1920x1080', 1920, 1080, 0);

INSERT INTO "FFmpegProfile" (
    "Id",
    "AudioBitrate",
    "AudioBufferSize",
    "AudioChannels",
    "AudioFormat",
    "AudioSampleRate",
    "BitDepth",
    "DeinterlaceVideo",
    "HardwareAcceleration",
    "Name",
    "NormalizeFramerate",
    "NormalizeLoudnessMode",
    "QsvExtraHardwareFrames",
    "ResolutionId",
    "ScalingBehavior",
    "ThreadCount",
    "VaapiDevice",
    "VaapiDriver",
    "VideoBitrate",
    "VideoBufferSize",
    "VideoFormat",
    "VideoProfile",
    "VideoPreset",
    "AllowBFrames",
    "VaapiDisplay",
    "TonemapAlgorithm",
    "TargetLoudness",
    "PadMode",
    "NormalizeAudio",
    "NormalizeColors",
    "NormalizeVideo"
) VALUES (
    1,
    192,
    384,
    2,
    1,
    48,
    0,
    1,
    0,
    'E2E Profile',
    0,
    0,
    64,
    1,
    0,
    0,
    NULL,
    0,
    2000,
    4000,
    1,
    'high',
    NULL,
    0,
    NULL,
    0,
    NULL,
    1,
    1,
    1,
    0
);

INSERT INTO "ConfigElement" ("Key", "Value")
VALUES
    ('ffmpeg.default_profile_id', '1'),
    ('ffmpeg.default_resolution_id', '1');

INSERT INTO "MediaSource" ("Id") VALUES (1);
INSERT INTO "LocalMediaSource" ("Id") VALUES (1);
INSERT INTO "Library" ("Id", "MediaSourceId", "MediaKind", "Name", "LastScan")
VALUES (1, 1, 1, 'E2E Movies', NULL);
INSERT INTO "LocalLibrary" ("Id") VALUES (1);
INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path", "LastScan")
VALUES (1, 1, 'tests/fixtures/media', NULL);

INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (1, 1, 0);
INSERT INTO "Movie" ("Id") VALUES (1);
INSERT INTO "MovieMetadata" (
    "Id",
    "MovieId",
    "Title",
    "OriginalTitle",
    "SortTitle",
    "Year",
    "ReleaseDate",
    "DateAdded",
    "DateUpdated",
    "MetadataKind"
) VALUES (
    1,
    1,
    'E2E Test Movie',
    'E2E Test Movie',
    'E2E Test Movie',
    2026,
    '2026-01-01',
    '2026-01-01',
    '2026-01-01',
    0
);
INSERT INTO "MediaVersion" (
    "Id",
    "MovieId",
    "DateAdded",
    "DateUpdated",
    "Duration",
    "Height",
    "Width",
    "VideoScanKind"
) VALUES (
    1,
    1,
    '2026-01-01',
    '2026-01-01',
    '00:05:00',
    1080,
    1920,
    0
);

INSERT INTO "Channel" (
    "Id",
    "UniqueId",
    "Number",
    "SortNumber",
    "Name",
    "Group",
    "FFmpegProfileId",
    "StreamingMode",
    "StreamSelectorMode",
    "SubtitleMode",
    "MusicVideoCreditsMode",
    "SongVideoMode",
    "PlayoutSource",
    "PlayoutMode",
    "TranscodeMode",
    "IdleBehavior",
    "IsEnabled",
    "ShowInEpg"
) VALUES (
    1,
    '00000000-0000-0000-0000-000000000001',
    '100',
    100.0,
    'E2E Channel',
    'ErsatzTV',
    1,
    5,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    1,
    1
);

INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed")
VALUES (1, 1, 0, 1);
