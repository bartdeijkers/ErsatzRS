-- ADR 0065: mark playlists whose lifecycle is owned by a Remote Stream
-- playlist manifest. ErsatzTV has no corresponding playlist-ingestion field.
ALTER TABLE `Playlist`
    ADD COLUMN `ManagedSource` VARCHAR(512) NULL;

CREATE UNIQUE INDEX `IX_Playlist_ManagedSource`
    ON `Playlist` (`ManagedSource`);
