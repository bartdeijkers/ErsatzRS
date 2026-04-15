CREATE TABLE language_codes (
    id INTEGER PRIMARY KEY,
    three_code_1 TEXT NOT NULL UNIQUE,
    three_code_2 TEXT,
    two_code TEXT,
    english_name TEXT NOT NULL,
    french_name TEXT NOT NULL
);

CREATE TABLE resolutions (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    is_custom INTEGER NOT NULL CHECK (is_custom IN (0, 1)),
    width INTEGER NOT NULL,
    height INTEGER NOT NULL
);

CREATE TABLE config_entries (
    id INTEGER PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    value TEXT NOT NULL
);

CREATE TABLE ffmpeg_profiles (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    thread_count INTEGER NOT NULL,
    normalize_audio INTEGER NOT NULL CHECK (normalize_audio IN (0, 1)),
    normalize_video INTEGER NOT NULL CHECK (normalize_video IN (0, 1)),
    hardware_acceleration TEXT NOT NULL,
    vaapi_display TEXT,
    vaapi_driver TEXT NOT NULL,
    vaapi_device TEXT,
    qsv_extra_hardware_frames INTEGER,
    resolution_id INTEGER NOT NULL,
    scaling_behavior TEXT NOT NULL,
    pad_mode TEXT NOT NULL,
    video_format TEXT NOT NULL,
    video_profile TEXT,
    video_preset TEXT,
    allow_b_frames INTEGER NOT NULL CHECK (allow_b_frames IN (0, 1)),
    bit_depth TEXT NOT NULL,
    video_bitrate INTEGER NOT NULL,
    video_buffer_size INTEGER NOT NULL,
    tonemap_algorithm TEXT NOT NULL,
    audio_format TEXT NOT NULL,
    audio_bitrate INTEGER NOT NULL,
    audio_buffer_size INTEGER NOT NULL,
    normalize_loudness_mode TEXT NOT NULL,
    target_loudness REAL,
    audio_channels INTEGER NOT NULL,
    audio_sample_rate INTEGER NOT NULL,
    normalize_framerate INTEGER NOT NULL CHECK (normalize_framerate IN (0, 1)),
    normalize_colors INTEGER NOT NULL CHECK (normalize_colors IN (0, 1)),
    deinterlace_video INTEGER,
    FOREIGN KEY (resolution_id) REFERENCES resolutions(id)
);

CREATE TABLE filler_presets (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    filler_mode TEXT NOT NULL,
    collection_type TEXT NOT NULL,
    collection_id INTEGER,
    media_item_id INTEGER,
    multi_collection_id INTEGER,
    smart_collection_id INTEGER,
    playlist_id INTEGER,
    duration_ms INTEGER,
    count INTEGER,
    pad_to_next_hour INTEGER NOT NULL CHECK (pad_to_next_hour IN (0, 1)),
    allow_watermarks INTEGER NOT NULL CHECK (allow_watermarks IN (0, 1)),
    playback_order TEXT NOT NULL
);

CREATE TABLE channel_watermarks (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    mode TEXT NOT NULL,
    image_source TEXT NOT NULL,
    image TEXT,
    original_content_type TEXT,
    location TEXT NOT NULL,
    size TEXT NOT NULL,
    width_percent REAL NOT NULL,
    horizontal_margin_percent REAL NOT NULL,
    vertical_margin_percent REAL NOT NULL,
    frequency_minutes INTEGER NOT NULL,
    duration_seconds INTEGER NOT NULL,
    opacity INTEGER NOT NULL,
    place_within_source_content INTEGER NOT NULL CHECK (place_within_source_content IN (0, 1)),
    opacity_expression TEXT,
    z_index INTEGER NOT NULL
);

CREATE TABLE graphics_elements (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    x INTEGER NOT NULL,
    y INTEGER NOT NULL,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    z_index INTEGER NOT NULL,
    path TEXT,
    text TEXT,
    script TEXT,
    variables TEXT
);

CREATE TABLE artwork (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL,
    kind TEXT NOT NULL,
    width INTEGER,
    height INTEGER,
    sort_order INTEGER NOT NULL
);

CREATE TABLE channels (
    id INTEGER PRIMARY KEY,
    unique_id TEXT NOT NULL UNIQUE,
    number TEXT NOT NULL UNIQUE,
    sort_number REAL NOT NULL,
    name TEXT NOT NULL,
    channel_group TEXT,
    categories TEXT,
    ffmpeg_profile_id INTEGER NOT NULL,
    slug_seconds REAL,
    watermark_id INTEGER,
    fallback_filler_id INTEGER,
    streaming_mode TEXT NOT NULL,
    stream_selector_mode TEXT NOT NULL,
    stream_selector TEXT,
    preferred_audio_language_code TEXT,
    preferred_audio_title TEXT,
    preferred_subtitle_language_code TEXT,
    subtitle_mode TEXT NOT NULL,
    music_video_credits_mode TEXT NOT NULL,
    music_video_credits_template TEXT,
    song_video_mode TEXT NOT NULL,
    playout_source TEXT NOT NULL,
    playout_mode TEXT NOT NULL,
    mirror_source_channel_id INTEGER,
    playout_offset_ms INTEGER,
    transcode_mode TEXT NOT NULL,
    idle_behavior TEXT NOT NULL,
    is_enabled INTEGER NOT NULL CHECK (is_enabled IN (0, 1)),
    show_in_epg INTEGER NOT NULL CHECK (show_in_epg IN (0, 1)),
    FOREIGN KEY (ffmpeg_profile_id) REFERENCES ffmpeg_profiles(id),
    FOREIGN KEY (watermark_id) REFERENCES channel_watermarks(id) ON DELETE SET NULL,
    FOREIGN KEY (fallback_filler_id) REFERENCES filler_presets(id) ON DELETE SET NULL,
    FOREIGN KEY (mirror_source_channel_id) REFERENCES channels(id) ON DELETE SET NULL
);

CREATE INDEX idx_channels_enabled ON channels(is_enabled);
CREATE INDEX idx_channels_sort ON channels(sort_number, number);

CREATE TABLE channel_artwork (
    channel_id INTEGER NOT NULL,
    artwork_id INTEGER NOT NULL,
    PRIMARY KEY (channel_id, artwork_id),
    FOREIGN KEY (channel_id) REFERENCES channels(id) ON DELETE CASCADE,
    FOREIGN KEY (artwork_id) REFERENCES artwork(id) ON DELETE CASCADE
);

CREATE TABLE media_sources (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL,
    last_scan TEXT
);

CREATE TABLE local_media_sources (
    id INTEGER PRIMARY KEY,
    FOREIGN KEY (id) REFERENCES media_sources(id) ON DELETE CASCADE
);

CREATE TABLE plex_media_sources (
    id INTEGER PRIMARY KEY,
    server_name TEXT,
    machine_identifier TEXT,
    FOREIGN KEY (id) REFERENCES media_sources(id) ON DELETE CASCADE
);

CREATE TABLE plex_connections (
    id INTEGER PRIMARY KEY,
    plex_media_source_id INTEGER NOT NULL,
    is_active INTEGER NOT NULL CHECK (is_active IN (0, 1)),
    uri TEXT NOT NULL,
    FOREIGN KEY (plex_media_source_id) REFERENCES plex_media_sources(id) ON DELETE CASCADE
);

CREATE TABLE plex_path_replacements (
    id INTEGER PRIMARY KEY,
    plex_media_source_id INTEGER NOT NULL,
    path TEXT NOT NULL,
    replacement_path TEXT NOT NULL,
    FOREIGN KEY (plex_media_source_id) REFERENCES plex_media_sources(id) ON DELETE CASCADE
);

CREATE TABLE jellyfin_media_sources (
    id INTEGER PRIMARY KEY,
    server_name TEXT,
    operating_system TEXT,
    last_collections_scan TEXT,
    FOREIGN KEY (id) REFERENCES media_sources(id) ON DELETE CASCADE
);

CREATE TABLE jellyfin_connections (
    id INTEGER PRIMARY KEY,
    jellyfin_media_source_id INTEGER NOT NULL,
    address TEXT NOT NULL,
    FOREIGN KEY (jellyfin_media_source_id) REFERENCES jellyfin_media_sources(id) ON DELETE CASCADE
);

CREATE TABLE jellyfin_path_replacements (
    id INTEGER PRIMARY KEY,
    jellyfin_media_source_id INTEGER NOT NULL,
    path TEXT NOT NULL,
    replacement_path TEXT NOT NULL,
    FOREIGN KEY (jellyfin_media_source_id) REFERENCES jellyfin_media_sources(id) ON DELETE CASCADE
);

CREATE TABLE emby_media_sources (
    id INTEGER PRIMARY KEY,
    server_name TEXT,
    operating_system TEXT,
    last_collections_scan TEXT,
    FOREIGN KEY (id) REFERENCES media_sources(id) ON DELETE CASCADE
);

CREATE TABLE emby_connections (
    id INTEGER PRIMARY KEY,
    emby_media_source_id INTEGER NOT NULL,
    address TEXT NOT NULL,
    FOREIGN KEY (emby_media_source_id) REFERENCES emby_media_sources(id) ON DELETE CASCADE
);

CREATE TABLE emby_path_replacements (
    id INTEGER PRIMARY KEY,
    emby_media_source_id INTEGER NOT NULL,
    path TEXT NOT NULL,
    replacement_path TEXT NOT NULL,
    FOREIGN KEY (emby_media_source_id) REFERENCES emby_media_sources(id) ON DELETE CASCADE
);

CREATE TABLE libraries (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    media_kind TEXT NOT NULL,
    media_source_id INTEGER NOT NULL,
    media_source_kind TEXT NOT NULL,
    FOREIGN KEY (media_source_id) REFERENCES media_sources(id) ON DELETE CASCADE
);

CREATE INDEX idx_libraries_media_source ON libraries(media_source_id);

CREATE TABLE local_libraries (
    id INTEGER PRIMARY KEY,
    FOREIGN KEY (id) REFERENCES libraries(id) ON DELETE CASCADE
);

CREATE TABLE plex_libraries (
    id INTEGER PRIMARY KEY,
    key TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES libraries(id) ON DELETE CASCADE
);

CREATE TABLE jellyfin_libraries (
    id INTEGER PRIMARY KEY,
    item_id TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES libraries(id) ON DELETE CASCADE
);

CREATE TABLE emby_libraries (
    id INTEGER PRIMARY KEY,
    item_id TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES libraries(id) ON DELETE CASCADE
);

CREATE TABLE library_paths (
    id INTEGER PRIMARY KEY,
    library_id INTEGER NOT NULL,
    path TEXT NOT NULL,
    network_path TEXT,
    FOREIGN KEY (library_id) REFERENCES libraries(id) ON DELETE CASCADE
);

CREATE INDEX idx_library_paths_library ON library_paths(library_id);

CREATE TABLE library_folders (
    id INTEGER PRIMARY KEY,
    library_id INTEGER NOT NULL,
    path TEXT NOT NULL,
    FOREIGN KEY (library_id) REFERENCES libraries(id) ON DELETE CASCADE
);

CREATE INDEX idx_library_folders_library ON library_folders(library_id);

CREATE TABLE image_folder_durations (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL,
    duration_ms INTEGER NOT NULL,
    metadata_source TEXT NOT NULL
);

CREATE TABLE media_items (
    id INTEGER PRIMARY KEY,
    library_path_id INTEGER NOT NULL,
    state TEXT NOT NULL,
    FOREIGN KEY (library_path_id) REFERENCES library_paths(id) ON DELETE CASCADE
);

CREATE INDEX idx_media_items_library_path ON media_items(library_path_id);

CREATE TABLE movies (
    id INTEGER PRIMARY KEY,
    metadata_id INTEGER,
    FOREIGN KEY (id) REFERENCES media_items(id) ON DELETE CASCADE
);

CREATE TABLE shows (
    id INTEGER PRIMARY KEY,
    metadata_id INTEGER,
    FOREIGN KEY (id) REFERENCES media_items(id) ON DELETE CASCADE
);

CREATE TABLE seasons (
    id INTEGER PRIMARY KEY,
    show_id INTEGER,
    metadata_id INTEGER,
    season_number INTEGER NOT NULL,
    FOREIGN KEY (id) REFERENCES media_items(id) ON DELETE CASCADE,
    FOREIGN KEY (show_id) REFERENCES shows(id) ON DELETE SET NULL
);

CREATE TABLE episodes (
    id INTEGER PRIMARY KEY,
    show_id INTEGER,
    season_id INTEGER,
    metadata_id INTEGER,
    season_number INTEGER NOT NULL,
    episode_number INTEGER NOT NULL,
    FOREIGN KEY (id) REFERENCES media_items(id) ON DELETE CASCADE,
    FOREIGN KEY (show_id) REFERENCES shows(id) ON DELETE SET NULL,
    FOREIGN KEY (season_id) REFERENCES seasons(id) ON DELETE SET NULL
);

CREATE TABLE music_videos (
    id INTEGER PRIMARY KEY,
    metadata_id INTEGER,
    FOREIGN KEY (id) REFERENCES media_items(id) ON DELETE CASCADE
);

CREATE TABLE other_videos (
    id INTEGER PRIMARY KEY,
    metadata_id INTEGER,
    FOREIGN KEY (id) REFERENCES media_items(id) ON DELETE CASCADE
);

CREATE TABLE artists (
    id INTEGER PRIMARY KEY,
    metadata_id INTEGER,
    FOREIGN KEY (id) REFERENCES media_items(id) ON DELETE CASCADE
);

CREATE TABLE songs (
    id INTEGER PRIMARY KEY,
    metadata_id INTEGER,
    artist_id INTEGER,
    FOREIGN KEY (id) REFERENCES media_items(id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists(id) ON DELETE SET NULL
);

CREATE TABLE images (
    id INTEGER PRIMARY KEY,
    metadata_id INTEGER,
    duration_id INTEGER,
    FOREIGN KEY (id) REFERENCES media_items(id) ON DELETE CASCADE,
    FOREIGN KEY (duration_id) REFERENCES image_folder_durations(id) ON DELETE SET NULL
);

CREATE TABLE remote_streams (
    id INTEGER PRIMARY KEY,
    metadata_id INTEGER,
    url TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES media_items(id) ON DELETE CASCADE
);

CREATE TABLE chapter_media_items (
    id INTEGER PRIMARY KEY,
    source_media_item_id INTEGER NOT NULL,
    chapter_id INTEGER NOT NULL,
    FOREIGN KEY (id) REFERENCES media_items(id) ON DELETE CASCADE,
    FOREIGN KEY (source_media_item_id) REFERENCES media_items(id) ON DELETE CASCADE
);

CREATE TABLE plex_movies (
    id INTEGER PRIMARY KEY,
    key TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES movies(id) ON DELETE CASCADE
);

CREATE TABLE plex_shows (
    id INTEGER PRIMARY KEY,
    key TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES shows(id) ON DELETE CASCADE
);

CREATE TABLE plex_seasons (
    id INTEGER PRIMARY KEY,
    key TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES seasons(id) ON DELETE CASCADE
);

CREATE TABLE plex_episodes (
    id INTEGER PRIMARY KEY,
    key TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES episodes(id) ON DELETE CASCADE
);

CREATE TABLE plex_other_videos (
    id INTEGER PRIMARY KEY,
    key TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES other_videos(id) ON DELETE CASCADE
);

CREATE TABLE jellyfin_movies (
    id INTEGER PRIMARY KEY,
    item_id TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES movies(id) ON DELETE CASCADE
);

CREATE TABLE jellyfin_shows (
    id INTEGER PRIMARY KEY,
    item_id TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES shows(id) ON DELETE CASCADE
);

CREATE TABLE jellyfin_seasons (
    id INTEGER PRIMARY KEY,
    item_id TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES seasons(id) ON DELETE CASCADE
);

CREATE TABLE jellyfin_episodes (
    id INTEGER PRIMARY KEY,
    item_id TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES episodes(id) ON DELETE CASCADE
);

CREATE TABLE emby_movies (
    id INTEGER PRIMARY KEY,
    item_id TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES movies(id) ON DELETE CASCADE
);

CREATE TABLE emby_shows (
    id INTEGER PRIMARY KEY,
    item_id TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES shows(id) ON DELETE CASCADE
);

CREATE TABLE emby_seasons (
    id INTEGER PRIMARY KEY,
    item_id TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES seasons(id) ON DELETE CASCADE
);

CREATE TABLE emby_episodes (
    id INTEGER PRIMARY KEY,
    item_id TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES episodes(id) ON DELETE CASCADE
);

CREATE TABLE media_versions (
    id INTEGER PRIMARY KEY,
    media_item_id INTEGER,
    name TEXT NOT NULL,
    duration_ms INTEGER NOT NULL,
    sample_aspect_ratio TEXT,
    display_aspect_ratio TEXT,
    r_frame_rate TEXT,
    video_scan_kind TEXT NOT NULL,
    interlaced_ratio REAL,
    date_added TEXT NOT NULL,
    date_updated TEXT NOT NULL,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    FOREIGN KEY (media_item_id) REFERENCES media_items(id) ON DELETE CASCADE
);

CREATE TABLE media_files (
    id INTEGER PRIMARY KEY,
    media_version_id INTEGER NOT NULL,
    path TEXT NOT NULL UNIQUE,
    part INTEGER NOT NULL,
    size INTEGER,
    date_updated TEXT,
    FOREIGN KEY (media_version_id) REFERENCES media_versions(id) ON DELETE CASCADE
);

CREATE INDEX idx_media_files_path ON media_files(path);

CREATE TABLE plex_media_files (
    id INTEGER PRIMARY KEY,
    key TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES media_files(id) ON DELETE CASCADE
);

CREATE TABLE media_streams (
    id INTEGER PRIMARY KEY,
    media_version_id INTEGER NOT NULL,
    stream_index INTEGER NOT NULL,
    kind TEXT NOT NULL,
    codec TEXT,
    language TEXT,
    title TEXT,
    is_default INTEGER NOT NULL CHECK (is_default IN (0, 1)),
    is_forced INTEGER NOT NULL CHECK (is_forced IN (0, 1)),
    channels INTEGER,
    width INTEGER,
    height INTEGER,
    FOREIGN KEY (media_version_id) REFERENCES media_versions(id) ON DELETE CASCADE
);

CREATE TABLE media_chapters (
    id INTEGER PRIMARY KEY,
    media_version_id INTEGER NOT NULL,
    title TEXT NOT NULL,
    start_time_ms INTEGER NOT NULL,
    end_time_ms INTEGER NOT NULL,
    FOREIGN KEY (media_version_id) REFERENCES media_versions(id) ON DELETE CASCADE
);

CREATE TABLE movie_metadata (
    id INTEGER PRIMARY KEY,
    item_id INTEGER,
    metadata_kind TEXT NOT NULL,
    title TEXT NOT NULL,
    original_title TEXT,
    sort_title TEXT,
    year INTEGER,
    release_date TEXT,
    date_added TEXT NOT NULL,
    date_updated TEXT NOT NULL,
    outline TEXT,
    plot TEXT,
    tagline TEXT,
    FOREIGN KEY (item_id) REFERENCES movies(id) ON DELETE CASCADE
);

CREATE TABLE show_metadata (
    id INTEGER PRIMARY KEY,
    item_id INTEGER,
    metadata_kind TEXT NOT NULL,
    title TEXT NOT NULL,
    original_title TEXT,
    sort_title TEXT,
    year INTEGER,
    release_date TEXT,
    date_added TEXT NOT NULL,
    date_updated TEXT NOT NULL,
    outline TEXT,
    plot TEXT,
    FOREIGN KEY (item_id) REFERENCES shows(id) ON DELETE CASCADE
);

CREATE TABLE season_metadata (
    id INTEGER PRIMARY KEY,
    item_id INTEGER,
    metadata_kind TEXT NOT NULL,
    title TEXT NOT NULL,
    original_title TEXT,
    sort_title TEXT,
    year INTEGER,
    release_date TEXT,
    date_added TEXT NOT NULL,
    date_updated TEXT NOT NULL,
    season_number INTEGER NOT NULL,
    FOREIGN KEY (item_id) REFERENCES seasons(id) ON DELETE CASCADE
);

CREATE TABLE episode_metadata (
    id INTEGER PRIMARY KEY,
    item_id INTEGER,
    metadata_kind TEXT NOT NULL,
    title TEXT NOT NULL,
    original_title TEXT,
    sort_title TEXT,
    year INTEGER,
    release_date TEXT,
    date_added TEXT NOT NULL,
    date_updated TEXT NOT NULL,
    season_number INTEGER NOT NULL,
    episode_number INTEGER NOT NULL,
    outline TEXT,
    plot TEXT,
    FOREIGN KEY (item_id) REFERENCES episodes(id) ON DELETE CASCADE
);

CREATE TABLE music_video_metadata (
    id INTEGER PRIMARY KEY,
    item_id INTEGER,
    metadata_kind TEXT NOT NULL,
    title TEXT NOT NULL,
    original_title TEXT,
    sort_title TEXT,
    year INTEGER,
    release_date TEXT,
    date_added TEXT NOT NULL,
    date_updated TEXT NOT NULL,
    album TEXT,
    track INTEGER,
    FOREIGN KEY (item_id) REFERENCES music_videos(id) ON DELETE CASCADE
);

CREATE TABLE song_metadata (
    id INTEGER PRIMARY KEY,
    item_id INTEGER,
    metadata_kind TEXT NOT NULL,
    title TEXT NOT NULL,
    original_title TEXT,
    sort_title TEXT,
    year INTEGER,
    release_date TEXT,
    date_added TEXT NOT NULL,
    date_updated TEXT NOT NULL,
    album TEXT,
    album_artist TEXT,
    track INTEGER,
    disc INTEGER,
    FOREIGN KEY (item_id) REFERENCES songs(id) ON DELETE CASCADE
);

CREATE TABLE artist_metadata (
    id INTEGER PRIMARY KEY,
    item_id INTEGER,
    metadata_kind TEXT NOT NULL,
    title TEXT NOT NULL,
    original_title TEXT,
    sort_title TEXT,
    year INTEGER,
    release_date TEXT,
    date_added TEXT NOT NULL,
    date_updated TEXT NOT NULL,
    biography TEXT,
    FOREIGN KEY (item_id) REFERENCES artists(id) ON DELETE CASCADE
);

CREATE TABLE image_metadata (
    id INTEGER PRIMARY KEY,
    item_id INTEGER,
    metadata_kind TEXT NOT NULL,
    title TEXT NOT NULL,
    original_title TEXT,
    sort_title TEXT,
    year INTEGER,
    release_date TEXT,
    date_added TEXT NOT NULL,
    date_updated TEXT NOT NULL,
    FOREIGN KEY (item_id) REFERENCES images(id) ON DELETE CASCADE
);

CREATE TABLE other_video_metadata (
    id INTEGER PRIMARY KEY,
    item_id INTEGER,
    metadata_kind TEXT NOT NULL,
    title TEXT NOT NULL,
    original_title TEXT,
    sort_title TEXT,
    year INTEGER,
    release_date TEXT,
    date_added TEXT NOT NULL,
    date_updated TEXT NOT NULL,
    FOREIGN KEY (item_id) REFERENCES other_videos(id) ON DELETE CASCADE
);

CREATE TABLE remote_stream_metadata (
    id INTEGER PRIMARY KEY,
    item_id INTEGER,
    metadata_kind TEXT NOT NULL,
    title TEXT NOT NULL,
    original_title TEXT,
    sort_title TEXT,
    year INTEGER,
    release_date TEXT,
    date_added TEXT NOT NULL,
    date_updated TEXT NOT NULL,
    FOREIGN KEY (item_id) REFERENCES remote_streams(id) ON DELETE CASCADE
);

CREATE TABLE metadata_artwork (
    metadata_kind TEXT NOT NULL,
    metadata_id INTEGER NOT NULL,
    artwork_id INTEGER NOT NULL,
    PRIMARY KEY (metadata_kind, metadata_id, artwork_id),
    FOREIGN KEY (artwork_id) REFERENCES artwork(id) ON DELETE CASCADE
);

CREATE TABLE metadata_guids (
    id INTEGER PRIMARY KEY,
    metadata_kind TEXT NOT NULL,
    metadata_id INTEGER NOT NULL,
    source TEXT NOT NULL,
    guid TEXT NOT NULL
);

CREATE TABLE genres (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE metadata_genres (
    metadata_kind TEXT NOT NULL,
    metadata_id INTEGER NOT NULL,
    genre_id INTEGER NOT NULL,
    PRIMARY KEY (metadata_kind, metadata_id, genre_id),
    FOREIGN KEY (genre_id) REFERENCES genres(id) ON DELETE CASCADE
);

CREATE TABLE tags (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE metadata_tags (
    metadata_kind TEXT NOT NULL,
    metadata_id INTEGER NOT NULL,
    tag_id INTEGER NOT NULL,
    PRIMARY KEY (metadata_kind, metadata_id, tag_id),
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
);

CREATE TABLE studios (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE metadata_studios (
    metadata_kind TEXT NOT NULL,
    metadata_id INTEGER NOT NULL,
    studio_id INTEGER NOT NULL,
    PRIMARY KEY (metadata_kind, metadata_id, studio_id),
    FOREIGN KEY (studio_id) REFERENCES studios(id) ON DELETE CASCADE
);

CREATE TABLE actors (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    role TEXT,
    actor_order INTEGER NOT NULL
);

CREATE TABLE metadata_actors (
    metadata_kind TEXT NOT NULL,
    metadata_id INTEGER NOT NULL,
    actor_id INTEGER NOT NULL,
    PRIMARY KEY (metadata_kind, metadata_id, actor_id),
    FOREIGN KEY (actor_id) REFERENCES actors(id) ON DELETE CASCADE
);

CREATE TABLE directors (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE writers (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE moods (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE song_metadata_moods (
    song_metadata_id INTEGER NOT NULL,
    mood_id INTEGER NOT NULL,
    PRIMARY KEY (song_metadata_id, mood_id),
    FOREIGN KEY (song_metadata_id) REFERENCES song_metadata(id) ON DELETE CASCADE,
    FOREIGN KEY (mood_id) REFERENCES moods(id) ON DELETE CASCADE
);

CREATE TABLE styles (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE song_metadata_styles (
    song_metadata_id INTEGER NOT NULL,
    style_id INTEGER NOT NULL,
    PRIMARY KEY (song_metadata_id, style_id),
    FOREIGN KEY (song_metadata_id) REFERENCES song_metadata(id) ON DELETE CASCADE,
    FOREIGN KEY (style_id) REFERENCES styles(id) ON DELETE CASCADE
);

CREATE TABLE music_video_metadata_artists (
    music_video_metadata_id INTEGER NOT NULL,
    artist_id INTEGER NOT NULL,
    PRIMARY KEY (music_video_metadata_id, artist_id),
    FOREIGN KEY (music_video_metadata_id) REFERENCES music_video_metadata(id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists(id) ON DELETE CASCADE
);

CREATE TABLE song_metadata_artists (
    song_metadata_id INTEGER NOT NULL,
    artist_id INTEGER NOT NULL,
    PRIMARY KEY (song_metadata_id, artist_id),
    FOREIGN KEY (song_metadata_id) REFERENCES song_metadata(id) ON DELETE CASCADE,
    FOREIGN KEY (artist_id) REFERENCES artists(id) ON DELETE CASCADE
);

CREATE TABLE subtitles (
    id INTEGER PRIMARY KEY,
    metadata_kind TEXT NOT NULL,
    metadata_id INTEGER NOT NULL,
    path TEXT,
    language TEXT,
    title TEXT,
    kind TEXT NOT NULL,
    is_default INTEGER NOT NULL CHECK (is_default IN (0, 1)),
    is_forced INTEGER NOT NULL CHECK (is_forced IN (0, 1))
);

CREATE TABLE collections (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    collection_type TEXT NOT NULL
);

CREATE TABLE plex_collections (
    id INTEGER PRIMARY KEY,
    key TEXT NOT NULL,
    plex_media_source_id INTEGER NOT NULL,
    FOREIGN KEY (id) REFERENCES collections(id) ON DELETE CASCADE,
    FOREIGN KEY (plex_media_source_id) REFERENCES plex_media_sources(id) ON DELETE CASCADE
);

CREATE TABLE jellyfin_collections (
    id INTEGER PRIMARY KEY,
    item_id TEXT NOT NULL,
    jellyfin_media_source_id INTEGER NOT NULL,
    FOREIGN KEY (id) REFERENCES collections(id) ON DELETE CASCADE,
    FOREIGN KEY (jellyfin_media_source_id) REFERENCES jellyfin_media_sources(id) ON DELETE CASCADE
);

CREATE TABLE emby_collections (
    id INTEGER PRIMARY KEY,
    item_id TEXT NOT NULL,
    emby_media_source_id INTEGER NOT NULL,
    FOREIGN KEY (id) REFERENCES collections(id) ON DELETE CASCADE,
    FOREIGN KEY (emby_media_source_id) REFERENCES emby_media_sources(id) ON DELETE CASCADE
);

CREATE TABLE collection_items (
    id INTEGER PRIMARY KEY,
    collection_id INTEGER NOT NULL,
    media_item_id INTEGER NOT NULL,
    sort_order INTEGER NOT NULL,
    FOREIGN KEY (collection_id) REFERENCES collections(id) ON DELETE CASCADE,
    FOREIGN KEY (media_item_id) REFERENCES media_items(id) ON DELETE CASCADE
);

CREATE INDEX idx_collection_items_media_item ON collection_items(media_item_id);

CREATE TABLE multi_collections (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL
);

CREATE TABLE multi_collection_items (
    id INTEGER PRIMARY KEY,
    multi_collection_id INTEGER NOT NULL,
    collection_type TEXT NOT NULL,
    collection_id INTEGER,
    media_item_id INTEGER,
    playlist_id INTEGER,
    playback_order TEXT NOT NULL,
    sort_order INTEGER NOT NULL,
    FOREIGN KEY (multi_collection_id) REFERENCES multi_collections(id) ON DELETE CASCADE
);

CREATE TABLE multi_collection_smart_items (
    id INTEGER PRIMARY KEY,
    multi_collection_id INTEGER NOT NULL,
    smart_collection_id INTEGER NOT NULL,
    sort_order INTEGER NOT NULL,
    FOREIGN KEY (multi_collection_id) REFERENCES multi_collections(id) ON DELETE CASCADE
);

CREATE TABLE smart_collections (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    query TEXT NOT NULL
);

CREATE TABLE rerun_collections (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    collection_type TEXT NOT NULL,
    collection_id INTEGER,
    media_item_id INTEGER,
    multi_collection_id INTEGER,
    smart_collection_id INTEGER
);

CREATE TABLE playlists (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL
);

CREATE TABLE playlist_groups (
    id INTEGER PRIMARY KEY,
    playlist_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    sort_order INTEGER NOT NULL,
    FOREIGN KEY (playlist_id) REFERENCES playlists(id) ON DELETE CASCADE
);

CREATE TABLE playlist_items (
    id INTEGER PRIMARY KEY,
    playlist_id INTEGER NOT NULL,
    playlist_group_id INTEGER,
    collection_type TEXT NOT NULL,
    collection_id INTEGER,
    media_item_id INTEGER,
    multi_collection_id INTEGER,
    smart_collection_id INTEGER,
    playback_order TEXT NOT NULL,
    count INTEGER,
    play_all INTEGER NOT NULL DEFAULT 0 CHECK (play_all IN (0, 1)),
    sort_order INTEGER NOT NULL,
    FOREIGN KEY (playlist_id) REFERENCES playlists(id) ON DELETE CASCADE,
    FOREIGN KEY (playlist_group_id) REFERENCES playlist_groups(id) ON DELETE SET NULL
);

CREATE TABLE trakt_lists (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    user_name TEXT NOT NULL,
    list_name TEXT NOT NULL,
    updated_at TEXT
);

CREATE TABLE trakt_list_items (
    id INTEGER PRIMARY KEY,
    trakt_list_id INTEGER NOT NULL,
    kind TEXT NOT NULL,
    media_item_id INTEGER,
    season INTEGER,
    episode INTEGER,
    enumerator_seed INTEGER NOT NULL,
    enumerator_index INTEGER NOT NULL,
    FOREIGN KEY (trakt_list_id) REFERENCES trakt_lists(id) ON DELETE CASCADE,
    FOREIGN KEY (media_item_id) REFERENCES media_items(id) ON DELETE SET NULL
);

CREATE TABLE trakt_list_item_guids (
    id INTEGER PRIMARY KEY,
    trakt_list_item_id INTEGER NOT NULL,
    source TEXT NOT NULL,
    value TEXT NOT NULL,
    FOREIGN KEY (trakt_list_item_id) REFERENCES trakt_list_items(id) ON DELETE CASCADE
);

CREATE TABLE program_schedules (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    keep_multi_part_episodes_together INTEGER NOT NULL CHECK (keep_multi_part_episodes_together IN (0, 1)),
    treat_collections_as_shows INTEGER NOT NULL CHECK (treat_collections_as_shows IN (0, 1)),
    shuffle_schedule_items INTEGER NOT NULL CHECK (shuffle_schedule_items IN (0, 1)),
    random_start_point INTEGER NOT NULL CHECK (random_start_point IN (0, 1)),
    fixed_start_time_behavior TEXT NOT NULL
);

CREATE TABLE program_schedule_items (
    id INTEGER PRIMARY KEY,
    program_schedule_id INTEGER NOT NULL,
    collection_type TEXT NOT NULL,
    collection_id INTEGER,
    media_item_id INTEGER,
    multi_collection_id INTEGER,
    smart_collection_id INTEGER,
    rerun_collection_id INTEGER,
    playlist_id INTEGER,
    search_query TEXT,
    playout_mode TEXT NOT NULL,
    playback_order TEXT NOT NULL,
    multiple_mode TEXT NOT NULL,
    multiple_count TEXT,
    duration_ms INTEGER,
    start_type TEXT NOT NULL,
    start_time_ms INTEGER,
    fixed_start_time_behavior TEXT,
    guide_mode TEXT NOT NULL,
    tail_mode TEXT NOT NULL,
    fill_with_group_mode TEXT NOT NULL,
    marathon_group_by TEXT NOT NULL,
    marathon_shuffle_items INTEGER NOT NULL CHECK (marathon_shuffle_items IN (0, 1)),
    custom_title TEXT,
    preferred_audio_language_code TEXT,
    preferred_audio_title TEXT,
    preferred_subtitle_language_code TEXT,
    subtitle_mode TEXT,
    sort_order INTEGER NOT NULL,
    FOREIGN KEY (program_schedule_id) REFERENCES program_schedules(id) ON DELETE CASCADE
);

CREATE INDEX idx_program_schedule_items_schedule ON program_schedule_items(program_schedule_id);

CREATE TABLE program_schedule_alternates (
    id INTEGER PRIMARY KEY,
    program_schedule_id INTEGER NOT NULL,
    playout_id INTEGER,
    schedule_item_id INTEGER NOT NULL,
    collection_type TEXT NOT NULL,
    collection_id INTEGER,
    media_item_id INTEGER,
    multi_collection_id INTEGER,
    smart_collection_id INTEGER,
    playlist_id INTEGER,
    fake_collection_key TEXT,
    FOREIGN KEY (program_schedule_id) REFERENCES program_schedules(id) ON DELETE CASCADE
);

CREATE TABLE block_groups (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL
);

CREATE TABLE blocks (
    id INTEGER PRIMARY KEY,
    block_group_id INTEGER,
    name TEXT NOT NULL,
    duration_ms INTEGER NOT NULL,
    stop_scheduling TEXT NOT NULL,
    FOREIGN KEY (block_group_id) REFERENCES block_groups(id) ON DELETE SET NULL
);

CREATE INDEX idx_blocks_group ON blocks(block_group_id);

CREATE TABLE block_items (
    id INTEGER PRIMARY KEY,
    block_id INTEGER NOT NULL,
    collection_type TEXT NOT NULL,
    collection_id INTEGER,
    media_item_id INTEGER,
    multi_collection_id INTEGER,
    smart_collection_id INTEGER,
    playlist_id INTEGER,
    search_query TEXT,
    playback_order TEXT NOT NULL,
    sort_order INTEGER NOT NULL,
    FOREIGN KEY (block_id) REFERENCES blocks(id) ON DELETE CASCADE
);

CREATE TABLE template_groups (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL
);

CREATE TABLE templates (
    id INTEGER PRIMARY KEY,
    template_group_id INTEGER,
    name TEXT NOT NULL,
    FOREIGN KEY (template_group_id) REFERENCES template_groups(id) ON DELETE SET NULL
);

CREATE INDEX idx_templates_group ON templates(template_group_id);

CREATE TABLE template_items (
    id INTEGER PRIMARY KEY,
    template_id INTEGER NOT NULL,
    block_id INTEGER NOT NULL,
    sort_order INTEGER NOT NULL,
    FOREIGN KEY (template_id) REFERENCES templates(id) ON DELETE CASCADE,
    FOREIGN KEY (block_id) REFERENCES blocks(id) ON DELETE CASCADE
);

CREATE TABLE deco_groups (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL
);

CREATE TABLE decos (
    id INTEGER PRIMARY KEY,
    deco_group_id INTEGER,
    name TEXT NOT NULL,
    mode TEXT NOT NULL,
    break_content_id INTEGER,
    FOREIGN KEY (deco_group_id) REFERENCES deco_groups(id) ON DELETE SET NULL
);

CREATE INDEX idx_decos_group ON decos(deco_group_id);

CREATE TABLE deco_break_contents (
    id INTEGER PRIMARY KEY,
    deco_id INTEGER NOT NULL UNIQUE,
    placement TEXT NOT NULL,
    collection_type TEXT NOT NULL,
    collection_id INTEGER,
    media_item_id INTEGER,
    playlist_id INTEGER,
    FOREIGN KEY (deco_id) REFERENCES decos(id) ON DELETE CASCADE
);

CREATE TABLE deco_template_groups (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL
);

CREATE TABLE deco_templates (
    id INTEGER PRIMARY KEY,
    deco_template_group_id INTEGER,
    name TEXT NOT NULL,
    FOREIGN KEY (deco_template_group_id) REFERENCES deco_template_groups(id) ON DELETE SET NULL
);

CREATE TABLE deco_template_items (
    id INTEGER PRIMARY KEY,
    deco_template_id INTEGER NOT NULL,
    deco_id INTEGER NOT NULL,
    sort_order INTEGER NOT NULL,
    FOREIGN KEY (deco_template_id) REFERENCES deco_templates(id) ON DELETE CASCADE,
    FOREIGN KEY (deco_id) REFERENCES decos(id) ON DELETE CASCADE
);

CREATE TABLE playouts (
    id INTEGER PRIMARY KEY,
    channel_id INTEGER NOT NULL,
    program_schedule_id INTEGER,
    schedule_file TEXT,
    schedule_kind TEXT NOT NULL,
    seed INTEGER NOT NULL,
    daily_rebuild_time_ms INTEGER,
    deco_id INTEGER,
    on_demand_checkpoint TEXT,
    FOREIGN KEY (channel_id) REFERENCES channels(id) ON DELETE CASCADE,
    FOREIGN KEY (program_schedule_id) REFERENCES program_schedules(id) ON DELETE SET NULL,
    FOREIGN KEY (deco_id) REFERENCES decos(id) ON DELETE SET NULL
);

CREATE INDEX idx_playouts_channel ON playouts(channel_id);
CREATE INDEX idx_playouts_program_schedule ON playouts(program_schedule_id);

CREATE TABLE playout_items (
    id INTEGER PRIMARY KEY,
    playout_id INTEGER NOT NULL,
    media_item_id INTEGER NOT NULL,
    start TEXT NOT NULL,
    finish TEXT NOT NULL,
    guide_start TEXT,
    guide_finish TEXT,
    custom_title TEXT,
    guide_group INTEGER NOT NULL,
    filler_kind TEXT NOT NULL,
    in_point_ms INTEGER NOT NULL,
    out_point_ms INTEGER NOT NULL,
    chapter_title TEXT,
    disable_watermarks INTEGER NOT NULL CHECK (disable_watermarks IN (0, 1)),
    preferred_audio_language_code TEXT,
    preferred_audio_title TEXT,
    preferred_subtitle_language_code TEXT,
    subtitle_mode TEXT,
    block_key TEXT,
    collection_key TEXT,
    collection_etag TEXT,
    scheduling_context TEXT,
    FOREIGN KEY (playout_id) REFERENCES playouts(id) ON DELETE CASCADE,
    FOREIGN KEY (media_item_id) REFERENCES media_items(id) ON DELETE CASCADE
);

CREATE INDEX idx_playout_items_range ON playout_items(playout_id, start, finish);

CREATE TABLE playout_gaps (
    id INTEGER PRIMARY KEY,
    playout_id INTEGER NOT NULL,
    start TEXT NOT NULL,
    finish TEXT NOT NULL,
    FOREIGN KEY (playout_id) REFERENCES playouts(id) ON DELETE CASCADE
);

CREATE TABLE playout_anchors (
    playout_id INTEGER PRIMARY KEY,
    schedule_items_seed INTEGER NOT NULL,
    schedule_items_index INTEGER NOT NULL,
    next_start TEXT NOT NULL,
    multiple_remaining INTEGER,
    duration_finish TEXT,
    in_flood INTEGER NOT NULL CHECK (in_flood IN (0, 1)),
    in_duration_filler INTEGER NOT NULL CHECK (in_duration_filler IN (0, 1)),
    next_guide_group INTEGER NOT NULL,
    next_instruction_index INTEGER NOT NULL,
    context TEXT,
    FOREIGN KEY (playout_id) REFERENCES playouts(id) ON DELETE CASCADE
);

CREATE TABLE playout_program_schedule_anchors (
    id INTEGER PRIMARY KEY,
    playout_id INTEGER NOT NULL,
    anchor_date TEXT,
    collection_type TEXT NOT NULL,
    collection_id INTEGER,
    multi_collection_id INTEGER,
    smart_collection_id INTEGER,
    rerun_collection_id INTEGER,
    media_item_id INTEGER,
    playlist_id INTEGER,
    search_query TEXT,
    fake_collection_key TEXT,
    enumerator_seed INTEGER NOT NULL,
    enumerator_index INTEGER NOT NULL,
    FOREIGN KEY (playout_id) REFERENCES playouts(id) ON DELETE CASCADE
);

CREATE TABLE playout_schedule_item_fill_group_indices (
    id INTEGER PRIMARY KEY,
    playout_id INTEGER NOT NULL,
    program_schedule_item_id INTEGER NOT NULL,
    enumerator_seed INTEGER NOT NULL,
    enumerator_index INTEGER NOT NULL,
    FOREIGN KEY (playout_id) REFERENCES playouts(id) ON DELETE CASCADE,
    FOREIGN KEY (program_schedule_item_id) REFERENCES program_schedule_items(id) ON DELETE CASCADE
);

CREATE TABLE playout_templates (
    playout_id INTEGER NOT NULL,
    template_id INTEGER NOT NULL,
    PRIMARY KEY (playout_id, template_id),
    FOREIGN KEY (playout_id) REFERENCES playouts(id) ON DELETE CASCADE,
    FOREIGN KEY (template_id) REFERENCES templates(id) ON DELETE CASCADE
);

CREATE TABLE playout_history (
    id INTEGER PRIMARY KEY,
    playout_id INTEGER NOT NULL,
    history_key TEXT NOT NULL,
    child_key TEXT,
    history_when TEXT NOT NULL,
    finish TEXT,
    history_index INTEGER NOT NULL DEFAULT 0,
    details TEXT NOT NULL DEFAULT '',
    is_current_child INTEGER NOT NULL CHECK (is_current_child IN (0, 1)),
    FOREIGN KEY (playout_id) REFERENCES playouts(id) ON DELETE CASCADE
);

CREATE TABLE playout_build_statuses (
    playout_id INTEGER PRIMARY KEY,
    last_build TEXT NOT NULL,
    success INTEGER NOT NULL CHECK (success IN (0, 1)),
    message TEXT,
    FOREIGN KEY (playout_id) REFERENCES playouts(id) ON DELETE CASCADE
);

CREATE TABLE rerun_history (
    id INTEGER PRIMARY KEY,
    rerun_collection_id INTEGER NOT NULL,
    media_item_id INTEGER NOT NULL,
    date TEXT NOT NULL,
    FOREIGN KEY (rerun_collection_id) REFERENCES rerun_collections(id) ON DELETE CASCADE,
    FOREIGN KEY (media_item_id) REFERENCES media_items(id) ON DELETE CASCADE
);

CREATE TABLE program_schedule_item_watermarks (
    program_schedule_item_id INTEGER NOT NULL,
    watermark_id INTEGER NOT NULL,
    PRIMARY KEY (program_schedule_item_id, watermark_id),
    FOREIGN KEY (program_schedule_item_id) REFERENCES program_schedule_items(id) ON DELETE CASCADE,
    FOREIGN KEY (watermark_id) REFERENCES channel_watermarks(id) ON DELETE CASCADE
);

CREATE TABLE block_item_watermarks (
    block_item_id INTEGER NOT NULL,
    watermark_id INTEGER NOT NULL,
    PRIMARY KEY (block_item_id, watermark_id),
    FOREIGN KEY (block_item_id) REFERENCES block_items(id) ON DELETE CASCADE,
    FOREIGN KEY (watermark_id) REFERENCES channel_watermarks(id) ON DELETE CASCADE
);

CREATE TABLE deco_watermarks (
    deco_id INTEGER NOT NULL,
    watermark_id INTEGER NOT NULL,
    PRIMARY KEY (deco_id, watermark_id),
    FOREIGN KEY (deco_id) REFERENCES decos(id) ON DELETE CASCADE,
    FOREIGN KEY (watermark_id) REFERENCES channel_watermarks(id) ON DELETE CASCADE
);

CREATE TABLE playout_item_watermarks (
    playout_item_id INTEGER NOT NULL,
    watermark_id INTEGER NOT NULL,
    PRIMARY KEY (playout_item_id, watermark_id),
    FOREIGN KEY (playout_item_id) REFERENCES playout_items(id) ON DELETE CASCADE,
    FOREIGN KEY (watermark_id) REFERENCES channel_watermarks(id) ON DELETE CASCADE
);

CREATE TABLE program_schedule_item_graphics_elements (
    program_schedule_item_id INTEGER NOT NULL,
    graphics_element_id INTEGER NOT NULL,
    PRIMARY KEY (program_schedule_item_id, graphics_element_id),
    FOREIGN KEY (program_schedule_item_id) REFERENCES program_schedule_items(id) ON DELETE CASCADE,
    FOREIGN KEY (graphics_element_id) REFERENCES graphics_elements(id) ON DELETE CASCADE
);

CREATE TABLE block_item_graphics_elements (
    block_item_id INTEGER NOT NULL,
    graphics_element_id INTEGER NOT NULL,
    PRIMARY KEY (block_item_id, graphics_element_id),
    FOREIGN KEY (block_item_id) REFERENCES block_items(id) ON DELETE CASCADE,
    FOREIGN KEY (graphics_element_id) REFERENCES graphics_elements(id) ON DELETE CASCADE
);

CREATE TABLE deco_graphics_elements (
    deco_id INTEGER NOT NULL,
    graphics_element_id INTEGER NOT NULL,
    PRIMARY KEY (deco_id, graphics_element_id),
    FOREIGN KEY (deco_id) REFERENCES decos(id) ON DELETE CASCADE,
    FOREIGN KEY (graphics_element_id) REFERENCES graphics_elements(id) ON DELETE CASCADE
);

CREATE TABLE playout_item_graphics_elements (
    playout_item_id INTEGER NOT NULL,
    graphics_element_id INTEGER,
    variables TEXT,
    PRIMARY KEY (playout_item_id, graphics_element_id),
    FOREIGN KEY (playout_item_id) REFERENCES playout_items(id) ON DELETE CASCADE,
    FOREIGN KEY (graphics_element_id) REFERENCES graphics_elements(id) ON DELETE CASCADE
);

INSERT INTO resolutions (id, name, is_custom, width, height)
VALUES (0, 'Source', 0, 0, 0);
