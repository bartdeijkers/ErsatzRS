ALTER TABLE `RemoteStreamMetadata`
    ADD COLUMN `CollectionName` TEXT NULL,
    ADD COLUMN `OriginalBroadcasters` TEXT NULL,
    ADD COLUMN `Broadcasters` TEXT NULL;
