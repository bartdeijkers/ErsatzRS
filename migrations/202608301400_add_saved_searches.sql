-- ErsatzRS-only: reusable advanced-search drafts are intentionally separate
-- from SmartCollection so recalling or editing a query has no playout effects.
CREATE TABLE "SavedSearch" (
    "Id" INTEGER NOT NULL CONSTRAINT "PK_SavedSearch" PRIMARY KEY AUTOINCREMENT,
    "Name" TEXT NOT NULL COLLATE NOCASE,
    "Query" TEXT NOT NULL
);

CREATE UNIQUE INDEX "IX_SavedSearch_Name" ON "SavedSearch" ("Name");
