-- Reshape GraphicsElement to the full ErsatzTV-parity field set (Decision C).
--
-- The DB is the canonical store for graphics elements in ErsatzRS; an ErsatzTV-style
-- YAML file at "Path" is parsed to upsert into these columns. Shared fields become
-- typed columns; kind-specific extras live in the JSON "Config" column. This replaces
-- the half-built 0002 drawvg geometry columns (the matching domain struct drops the
-- x/y/width/height/enable_expression/text/script/variables fields).
--
-- Requires SQLite >= 3.35 for ALTER TABLE ... DROP COLUMN (bundled with the build).
-- "ZIndex" (added in 0002) is retained as a shared column.

-- Drop the 0002 drawvg geometry stopgap columns.
ALTER TABLE "GraphicsElement" DROP COLUMN "X";
ALTER TABLE "GraphicsElement" DROP COLUMN "Y";
ALTER TABLE "GraphicsElement" DROP COLUMN "Width";
ALTER TABLE "GraphicsElement" DROP COLUMN "Height";
ALTER TABLE "GraphicsElement" DROP COLUMN "EnableExpression";
ALTER TABLE "GraphicsElement" DROP COLUMN "Text";

-- Shared, typed columns. NULL where the field is kind-specific or optional.
ALTER TABLE "GraphicsElement" ADD COLUMN "OpacityPercent" INTEGER NULL;
ALTER TABLE "GraphicsElement" ADD COLUMN "OpacityExpression" TEXT NULL;
ALTER TABLE "GraphicsElement" ADD COLUMN "Location" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "GraphicsElement" ADD COLUMN "HorizontalMarginPercent" REAL NULL;
ALTER TABLE "GraphicsElement" ADD COLUMN "VerticalMarginPercent" REAL NULL;
ALTER TABLE "GraphicsElement" ADD COLUMN "LocationX" REAL NULL;
ALTER TABLE "GraphicsElement" ADD COLUMN "LocationY" REAL NULL;
ALTER TABLE "GraphicsElement" ADD COLUMN "Scale" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "GraphicsElement" ADD COLUMN "ScaleWidthPercent" REAL NULL;
ALTER TABLE "GraphicsElement" ADD COLUMN "PlaceWithinSourceContent" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "GraphicsElement" ADD COLUMN "EpgEntries" INTEGER NOT NULL DEFAULT 0;

-- Kind-specific configuration as JSON. NULL is treated as the kind default.
ALTER TABLE "GraphicsElement" ADD COLUMN "Config" TEXT NULL;
