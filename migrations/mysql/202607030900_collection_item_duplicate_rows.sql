-- ErsatzRS extension; no direct ErsatzTV migration exists.
-- ADR 0048 follow-up: allow the same media item to appear in one collection
-- more than once when each row carries a different playback override (for
-- example one row per commentary or alternate audio track). The composite
-- primary key (`CollectionId`, `MediaItemId`) cannot represent that, so the
-- key moves to a surrogate `Id`, mirroring the `PlaylistItem` row shape.
-- Row-tuple uniqueness (no two rows with an identical override set) is
-- enforced by the add-to-collection flows.
ALTER TABLE `CollectionItem`
    DROP PRIMARY KEY,
    ADD COLUMN `Id` INT NOT NULL AUTO_INCREMENT FIRST,
    ADD CONSTRAINT `PK_CollectionItem` PRIMARY KEY (`Id`),
    ADD INDEX `IX_CollectionItem_CollectionId_MediaItemId` (`CollectionId`, `MediaItemId`);
