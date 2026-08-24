-- ErsatzRS-owned complete Classic Refresh state. Dated program-schedule
-- anchors retain the ErsatzTV-compatible per-source state; this row proves
-- that the corresponding scheduler-wide checkpoint is complete before a
-- destructive Refresh can replace the materialized window.
CREATE TABLE `PlayoutClassicCheckpoint` (
    `PlayoutId` INT NOT NULL,
    `CheckpointDate` VARCHAR(64) NOT NULL,
    `Context` LONGTEXT NOT NULL,
    CONSTRAINT `PK_PlayoutClassicCheckpoint` PRIMARY KEY (`PlayoutId`, `CheckpointDate`),
    CONSTRAINT `FK_PlayoutClassicCheckpoint_Playout_PlayoutId`
        FOREIGN KEY (`PlayoutId`) REFERENCES `Playout` (`Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;
