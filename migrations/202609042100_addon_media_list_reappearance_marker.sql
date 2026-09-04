ALTER TABLE "AddonMediaListItemTombstone"
    ADD COLUMN "ReappearedAt" TEXT NULL;

ALTER TABLE "AddonMediaListItem"
    ADD COLUMN "LastReconcileGeneration" INTEGER NULL;
