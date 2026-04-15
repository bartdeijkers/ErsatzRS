CREATE TABLE config_entries (
    id INTEGER PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    value TEXT NOT NULL
);

CREATE TABLE ffmpeg_profiles (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    data TEXT NOT NULL
);

CREATE TABLE filler_presets (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    data TEXT NOT NULL
);

CREATE TABLE channel_watermarks (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    data TEXT NOT NULL
);

CREATE TABLE channels (
    id INTEGER PRIMARY KEY,
    unique_id TEXT NOT NULL UNIQUE,
    number TEXT NOT NULL UNIQUE,
    sort_number REAL NOT NULL,
    name TEXT NOT NULL,
    ffmpeg_profile_id INTEGER NOT NULL,
    watermark_id INTEGER,
    fallback_filler_id INTEGER,
    is_enabled INTEGER NOT NULL CHECK (is_enabled IN (0, 1)),
    data TEXT NOT NULL,
    FOREIGN KEY (ffmpeg_profile_id) REFERENCES ffmpeg_profiles(id),
    FOREIGN KEY (watermark_id) REFERENCES channel_watermarks(id) ON DELETE SET NULL,
    FOREIGN KEY (fallback_filler_id) REFERENCES filler_presets(id) ON DELETE SET NULL
);

CREATE INDEX idx_channels_enabled ON channels(is_enabled);

CREATE TABLE channel_artwork (
    channel_id INTEGER NOT NULL,
    artwork_id INTEGER NOT NULL,
    PRIMARY KEY (channel_id, artwork_id),
    FOREIGN KEY (channel_id) REFERENCES channels(id) ON DELETE CASCADE
);

CREATE TABLE graphics_elements (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('Image', 'Text', 'Subtitle', 'Motion', 'Script')),
    data TEXT NOT NULL
);

CREATE TABLE media_sources (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('Local', 'Plex', 'Jellyfin', 'Emby')),
    last_scan TEXT,
    data TEXT NOT NULL
);

CREATE TABLE plex_media_sources (
    id INTEGER PRIMARY KEY,
    data TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES media_sources(id) ON DELETE CASCADE
);

CREATE TABLE jellyfin_media_sources (
    id INTEGER PRIMARY KEY,
    data TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES media_sources(id) ON DELETE CASCADE
);

CREATE TABLE emby_media_sources (
    id INTEGER PRIMARY KEY,
    data TEXT NOT NULL,
    FOREIGN KEY (id) REFERENCES media_sources(id) ON DELETE CASCADE
);

CREATE TABLE libraries (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    media_kind TEXT NOT NULL,
    media_source_id INTEGER NOT NULL,
    media_source_kind TEXT NOT NULL,
    data TEXT NOT NULL,
    FOREIGN KEY (media_source_id) REFERENCES media_sources(id) ON DELETE CASCADE
);

CREATE INDEX idx_libraries_media_source ON libraries(media_source_id);

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

CREATE TABLE media_items (
    id INTEGER PRIMARY KEY,
    library_path_id INTEGER NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('Normal', 'FileNotFound', 'Unavailable', 'RemoteOnly')),
    data TEXT NOT NULL,
    FOREIGN KEY (library_path_id) REFERENCES library_paths(id) ON DELETE CASCADE
);

CREATE INDEX idx_media_items_library_path ON media_items(library_path_id);

CREATE TABLE media_versions (
    id INTEGER PRIMARY KEY,
    media_item_id INTEGER,
    data TEXT NOT NULL,
    FOREIGN KEY (media_item_id) REFERENCES media_items(id) ON DELETE CASCADE
);

CREATE TABLE media_files (
    id INTEGER PRIMARY KEY,
    media_version_id INTEGER NOT NULL,
    path TEXT NOT NULL UNIQUE,
    data TEXT NOT NULL,
    FOREIGN KEY (media_version_id) REFERENCES media_versions(id) ON DELETE CASCADE
);

CREATE INDEX idx_media_files_path ON media_files(path);

CREATE TABLE collections (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    collection_type TEXT NOT NULL,
    data TEXT NOT NULL
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

CREATE TABLE smart_collections (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    query TEXT NOT NULL,
    data TEXT NOT NULL
);

CREATE TABLE playlists (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    data TEXT NOT NULL
);

CREATE TABLE playlist_items (
    id INTEGER PRIMARY KEY,
    playlist_id INTEGER NOT NULL,
    sort_order INTEGER NOT NULL,
    data TEXT NOT NULL,
    FOREIGN KEY (playlist_id) REFERENCES playlists(id) ON DELETE CASCADE
);

CREATE TABLE program_schedules (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    data TEXT NOT NULL
);

CREATE TABLE program_schedule_items (
    id INTEGER PRIMARY KEY,
    program_schedule_id INTEGER NOT NULL,
    sort_order INTEGER NOT NULL,
    data TEXT NOT NULL,
    FOREIGN KEY (program_schedule_id) REFERENCES program_schedules(id) ON DELETE CASCADE
);

CREATE INDEX idx_program_schedule_items_schedule ON program_schedule_items(program_schedule_id);

CREATE TABLE program_schedule_alternates (
    id INTEGER PRIMARY KEY,
    program_schedule_id INTEGER NOT NULL,
    playout_id INTEGER,
    data TEXT NOT NULL,
    FOREIGN KEY (program_schedule_id) REFERENCES program_schedules(id) ON DELETE CASCADE
);

CREATE TABLE block_groups (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    data TEXT
);

CREATE TABLE blocks (
    id INTEGER PRIMARY KEY,
    block_group_id INTEGER,
    name TEXT NOT NULL,
    duration_ms INTEGER NOT NULL,
    data TEXT NOT NULL,
    FOREIGN KEY (block_group_id) REFERENCES block_groups(id) ON DELETE SET NULL
);

CREATE INDEX idx_blocks_group ON blocks(block_group_id);

CREATE TABLE block_items (
    id INTEGER PRIMARY KEY,
    block_id INTEGER NOT NULL,
    sort_order INTEGER NOT NULL,
    data TEXT NOT NULL,
    FOREIGN KEY (block_id) REFERENCES blocks(id) ON DELETE CASCADE
);

CREATE TABLE deco_groups (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    data TEXT
);

CREATE TABLE decos (
    id INTEGER PRIMARY KEY,
    deco_group_id INTEGER,
    name TEXT NOT NULL,
    data TEXT NOT NULL,
    FOREIGN KEY (deco_group_id) REFERENCES deco_groups(id) ON DELETE SET NULL
);

CREATE INDEX idx_decos_group ON decos(deco_group_id);

CREATE TABLE templates (
    id INTEGER PRIMARY KEY,
    template_group_id INTEGER,
    name TEXT NOT NULL,
    data TEXT NOT NULL
);

CREATE INDEX idx_templates_group ON templates(template_group_id);

CREATE TABLE template_items (
    id INTEGER PRIMARY KEY,
    template_id INTEGER NOT NULL,
    block_id INTEGER NOT NULL,
    sort_order INTEGER NOT NULL,
    data TEXT NOT NULL,
    FOREIGN KEY (template_id) REFERENCES templates(id) ON DELETE CASCADE
);

CREATE TABLE playouts (
    id INTEGER PRIMARY KEY,
    channel_id INTEGER NOT NULL,
    program_schedule_id INTEGER,
    deco_id INTEGER,
    schedule_kind TEXT NOT NULL,
    data TEXT NOT NULL,
    FOREIGN KEY (channel_id) REFERENCES channels(id) ON DELETE CASCADE,
    FOREIGN KEY (program_schedule_id) REFERENCES program_schedules(id) ON DELETE SET NULL
);

CREATE INDEX idx_playouts_channel ON playouts(channel_id);
CREATE INDEX idx_playouts_program_schedule ON playouts(program_schedule_id);

CREATE TABLE playout_items (
    id INTEGER PRIMARY KEY,
    playout_id INTEGER NOT NULL,
    media_item_id INTEGER NOT NULL,
    start TEXT NOT NULL,
    finish TEXT NOT NULL,
    data TEXT NOT NULL,
    FOREIGN KEY (playout_id) REFERENCES playouts(id) ON DELETE CASCADE,
    FOREIGN KEY (media_item_id) REFERENCES media_items(id) ON DELETE CASCADE
);

CREATE INDEX idx_playout_items_range ON playout_items(playout_id, start, finish);

CREATE TABLE playout_build_statuses (
    playout_id INTEGER PRIMARY KEY,
    last_build TEXT NOT NULL,
    success INTEGER NOT NULL CHECK (success IN (0, 1)),
    message TEXT,
    data TEXT NOT NULL,
    FOREIGN KEY (playout_id) REFERENCES playouts(id) ON DELETE CASCADE
);

CREATE TABLE playout_gaps (
    id INTEGER PRIMARY KEY,
    playout_id INTEGER NOT NULL,
    start TEXT NOT NULL,
    finish TEXT NOT NULL,
    data TEXT NOT NULL,
    FOREIGN KEY (playout_id) REFERENCES playouts(id) ON DELETE CASCADE
);

CREATE TABLE movie_metadata (
    id INTEGER PRIMARY KEY,
    item_id INTEGER,
    title TEXT NOT NULL,
    data TEXT NOT NULL,
    FOREIGN KEY (item_id) REFERENCES media_items(id) ON DELETE CASCADE
);

CREATE TABLE episode_metadata (
    id INTEGER PRIMARY KEY,
    item_id INTEGER,
    title TEXT NOT NULL,
    data TEXT NOT NULL,
    FOREIGN KEY (item_id) REFERENCES media_items(id) ON DELETE CASCADE
);

CREATE TABLE music_video_metadata (
    id INTEGER PRIMARY KEY,
    item_id INTEGER,
    title TEXT NOT NULL,
    data TEXT NOT NULL,
    FOREIGN KEY (item_id) REFERENCES media_items(id) ON DELETE CASCADE
);

CREATE TABLE program_schedule_item_watermarks (
    program_schedule_item_id INTEGER NOT NULL,
    watermark_id INTEGER NOT NULL,
    PRIMARY KEY (program_schedule_item_id, watermark_id)
);

CREATE TABLE block_item_watermarks (
    block_item_id INTEGER NOT NULL,
    watermark_id INTEGER NOT NULL,
    PRIMARY KEY (block_item_id, watermark_id)
);

CREATE TABLE deco_watermarks (
    deco_id INTEGER NOT NULL,
    watermark_id INTEGER NOT NULL,
    PRIMARY KEY (deco_id, watermark_id)
);

CREATE TABLE playout_item_watermarks (
    playout_item_id INTEGER NOT NULL,
    watermark_id INTEGER NOT NULL,
    PRIMARY KEY (playout_item_id, watermark_id)
);

CREATE TABLE program_schedule_item_graphics_elements (
    program_schedule_item_id INTEGER NOT NULL,
    graphics_element_id INTEGER NOT NULL,
    PRIMARY KEY (program_schedule_item_id, graphics_element_id)
);

CREATE TABLE block_item_graphics_elements (
    block_item_id INTEGER NOT NULL,
    graphics_element_id INTEGER NOT NULL,
    PRIMARY KEY (block_item_id, graphics_element_id)
);

CREATE TABLE deco_graphics_elements (
    deco_id INTEGER NOT NULL,
    graphics_element_id INTEGER NOT NULL,
    PRIMARY KEY (deco_id, graphics_element_id)
);

CREATE TABLE playout_item_graphics_elements (
    playout_item_id INTEGER NOT NULL,
    graphics_element_id INTEGER,
    variables TEXT
);
