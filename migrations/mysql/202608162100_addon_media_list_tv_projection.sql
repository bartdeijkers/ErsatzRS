-- ErsatzRS extension: project selected add-on lists into normal television
-- hierarchy while retaining their Remote Stream playback source.

ALTER TABLE `AddonMediaList` ADD COLUMN `PresentationKind` INT NOT NULL DEFAULT 0;
ALTER TABLE `AddonMediaList` ADD COLUMN `RequestedPresentationKind` INT NOT NULL DEFAULT 0;
ALTER TABLE `AddonMediaList` ADD COLUMN `PresentationError` LONGTEXT NULL;

ALTER TABLE `AddonMediaListItem` ADD COLUMN `ChapterInput` LONGTEXT NULL;

CREATE TABLE `AddonMediaListTelevisionShow` (
    `AddonMediaListId` INT NOT NULL,
    `ShowId` INT NOT NULL,
    CONSTRAINT `PK_AddonMediaListTelevisionShow` PRIMARY KEY (`AddonMediaListId`),
    CONSTRAINT `FK_AddonMediaListTelevisionShow_AddonMediaList_AddonMediaListId`
        FOREIGN KEY (`AddonMediaListId`) REFERENCES `AddonMediaList` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_AddonMediaListTelevisionShow_Show_ShowId`
        FOREIGN KEY (`ShowId`) REFERENCES `Show` (`Id`) ON DELETE CASCADE
);

CREATE UNIQUE INDEX `IX_AddonMediaListTelevisionShow_ShowId`
    ON `AddonMediaListTelevisionShow` (`ShowId`);

CREATE TABLE `AddonMediaListItemEpisode` (
    `AddonMediaListItemId` INT NOT NULL,
    `EpisodeId` INT NOT NULL,
    CONSTRAINT `PK_AddonMediaListItemEpisode` PRIMARY KEY (`AddonMediaListItemId`),
    CONSTRAINT `FK_AddonMediaListItemEpisode_AddonMediaListItem_AddonMediaListItemId`
        FOREIGN KEY (`AddonMediaListItemId`) REFERENCES `AddonMediaListItem` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_AddonMediaListItemEpisode_Episode_EpisodeId`
        FOREIGN KEY (`EpisodeId`) REFERENCES `Episode` (`Id`) ON DELETE CASCADE
);

CREATE UNIQUE INDEX `IX_AddonMediaListItemEpisode_EpisodeId`
    ON `AddonMediaListItemEpisode` (`EpisodeId`);

UPDATE `AddonMediaListItem` AS `item`
JOIN `AddonMediaList` AS `list` ON `list`.`Id` = `item`.`AddonMediaListId`
JOIN (
    SELECT `AddonMediaListId`, COUNT(*) AS `RootCount`
    FROM `AddonMediaListItem`
    WHERE `ParentProviderId` IS NULL
    GROUP BY `AddonMediaListId`
) AS `roots` ON `roots`.`AddonMediaListId` = `item`.`AddonMediaListId`
SET `item`.`ChapterInput` = `list`.`ChapterInput`
WHERE `item`.`ParentProviderId` IS NULL
  AND COALESCE(`list`.`ChapterInput`, '') <> ''
  AND `roots`.`RootCount` = 1;

UPDATE `AddonMediaList` AS `list`
SET `list`.`ChapterInput` = NULL
WHERE COALESCE(`list`.`ChapterInput`, '') <> ''
  AND 1 = (
      SELECT COUNT(*)
      FROM `AddonMediaListItem` AS `root`
      WHERE `root`.`AddonMediaListId` = `list`.`Id`
        AND `root`.`ParentProviderId` IS NULL
  );
