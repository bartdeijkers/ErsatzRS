-- Host-owned administrative visibility has no ErsatzTV equivalent. It never
-- changes published content, generated playlists, refresh, or playback state.
ALTER TABLE "AddonMediaList" ADD COLUMN "AdminHidden" INTEGER NOT NULL DEFAULT 0;

CREATE INDEX "IX_AddonMediaList_AdminHidden_Name"
    ON "AddonMediaList" ("AdminHidden", "Name");
