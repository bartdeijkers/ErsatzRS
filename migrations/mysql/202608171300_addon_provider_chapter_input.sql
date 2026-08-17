-- Provider chapter text is retained separately so operator corrections win
-- without discarding the latest add-on value.
ALTER TABLE `AddonMediaListItem` ADD COLUMN `ProviderChapterInput` LONGTEXT NULL;
