-- Reshape GraphicsElement to the full ErsatzTV-parity field set (Decision C).
--
-- The DB is the canonical store for graphics elements in ErsatzRS; an ErsatzTV-style
-- YAML file at `Path` is parsed to upsert into these columns. Shared fields become
-- typed columns; kind-specific extras live in the JSON `Config` column. This replaces
-- the half-built 0002 drawvg geometry columns (the matching domain struct drops the
-- x/y/width/height/enable_expression/text/script/variables fields).
--
-- `ZIndex` (added in 0002) is retained as a shared column. Booleans use `INT`, matching
-- the existing migrations (e.g. `UseGraphicsElementsDuringFiller`).

-- Drop the 0002 drawvg geometry stopgap columns.
ALTER TABLE `GraphicsElement` DROP COLUMN `X`;
ALTER TABLE `GraphicsElement` DROP COLUMN `Y`;
ALTER TABLE `GraphicsElement` DROP COLUMN `Width`;
ALTER TABLE `GraphicsElement` DROP COLUMN `Height`;
ALTER TABLE `GraphicsElement` DROP COLUMN `EnableExpression`;
ALTER TABLE `GraphicsElement` DROP COLUMN `Text`;

-- Shared, typed columns. NULL where the field is kind-specific or optional.
ALTER TABLE `GraphicsElement` ADD COLUMN `OpacityPercent` INT NULL;
ALTER TABLE `GraphicsElement` ADD COLUMN `OpacityExpression` LONGTEXT NULL;
ALTER TABLE `GraphicsElement` ADD COLUMN `Location` INT NOT NULL DEFAULT 0;
ALTER TABLE `GraphicsElement` ADD COLUMN `HorizontalMarginPercent` DOUBLE NULL;
ALTER TABLE `GraphicsElement` ADD COLUMN `VerticalMarginPercent` DOUBLE NULL;
ALTER TABLE `GraphicsElement` ADD COLUMN `LocationX` DOUBLE NULL;
ALTER TABLE `GraphicsElement` ADD COLUMN `LocationY` DOUBLE NULL;
ALTER TABLE `GraphicsElement` ADD COLUMN `Scale` INT NOT NULL DEFAULT 0;
ALTER TABLE `GraphicsElement` ADD COLUMN `ScaleWidthPercent` DOUBLE NULL;
ALTER TABLE `GraphicsElement` ADD COLUMN `PlaceWithinSourceContent` INT NOT NULL DEFAULT 0;
ALTER TABLE `GraphicsElement` ADD COLUMN `EpgEntries` INT NOT NULL DEFAULT 0;

-- Kind-specific configuration as JSON. NULL is treated as the kind default.
ALTER TABLE `GraphicsElement` ADD COLUMN `Config` LONGTEXT NULL;
