ALTER TABLE `AddonMediaList`
    ADD COLUMN `ReviewState` INT NOT NULL DEFAULT 0,
    ADD COLUMN `ProviderMetadataJson` LONGTEXT NULL,
    ADD COLUMN `MetadataOverrideJson` LONGTEXT NULL,
    ADD COLUMN `PublishedRevision` INT NOT NULL DEFAULT 0,
    ADD COLUMN `ReviewError` LONGTEXT NULL;

UPDATE `AddonMediaList`
SET `ReviewState` = 2,
    `ProviderMetadataJson` = '{}',
    `MetadataOverrideJson` = '{}',
    `PublishedRevision` = 1;

ALTER TABLE `AddonMediaList`
    MODIFY COLUMN `ProviderMetadataJson` LONGTEXT NOT NULL,
    MODIFY COLUMN `MetadataOverrideJson` LONGTEXT NOT NULL;

ALTER TABLE `AddonMediaListItem`
    ADD COLUMN `Included` BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN `Published` BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN `ProviderMetadataJson` LONGTEXT NULL,
    ADD COLUMN `MetadataOverrideJson` LONGTEXT NULL,
    ADD COLUMN `MetadataConflictJson` LONGTEXT NULL,
    ADD COLUMN `CollectionRole` INT NOT NULL DEFAULT 0;

UPDATE `AddonMediaListItem`
SET `Published` = TRUE,
    `ProviderMetadataJson` = '{}',
    `MetadataOverrideJson` = '{}';

ALTER TABLE `AddonMediaListItem`
    MODIFY COLUMN `ProviderMetadataJson` LONGTEXT NOT NULL,
    MODIFY COLUMN `MetadataOverrideJson` LONGTEXT NOT NULL;

CREATE INDEX `IX_AddonMediaList_ReviewState` ON `AddonMediaList` (`ReviewState`);
CREATE INDEX `IX_AddonMediaListItem_ListIncluded`
    ON `AddonMediaListItem` (`AddonMediaListId`, `Included`, `Rank`);
