-- Dependency resolution state for membership-changing scans.
-- State: 0 = resolving, 1 = resolved, 2 = failed.
CREATE TABLE `ScanRefreshResolution` (
    `LibraryId` INT NOT NULL,
    `Generation` BIGINT NOT NULL DEFAULT 1,
    `State` INT NOT NULL DEFAULT 0,
    `Reason` VARCHAR(64) NOT NULL,
    `Diagnostic` VARCHAR(2048) NULL,
    `StartedAt` VARCHAR(64) NOT NULL,
    `UpdatedAt` VARCHAR(64) NOT NULL,
    CONSTRAINT `PK_ScanRefreshResolution` PRIMARY KEY (`LibraryId`),
    CONSTRAINT `FK_ScanRefreshResolution_Library_LibraryId`
        FOREIGN KEY (`LibraryId`) REFERENCES `Library` (`Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE INDEX `IX_ScanRefreshResolution_StateUpdatedAt`
    ON `ScanRefreshResolution` (`State`, `UpdatedAt`);

-- Restart-safe Refresh intent retained until the scheduler successfully builds
-- the latest observed generation. State: 0 = pending, 1 = building, 2 = failed.
CREATE TABLE `ScanPlayoutRefreshIntent` (
    `PlayoutId` INT NOT NULL,
    `LibraryId` INT NULL,
    `Generation` BIGINT NOT NULL DEFAULT 1,
    `State` INT NOT NULL DEFAULT 0,
    `Reason` VARCHAR(64) NOT NULL,
    `Diagnostic` VARCHAR(2048) NULL,
    `CreatedAt` VARCHAR(64) NOT NULL,
    `UpdatedAt` VARCHAR(64) NOT NULL,
    CONSTRAINT `PK_ScanPlayoutRefreshIntent` PRIMARY KEY (`PlayoutId`),
    CONSTRAINT `FK_ScanPlayoutRefreshIntent_Playout_PlayoutId`
        FOREIGN KEY (`PlayoutId`) REFERENCES `Playout` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_ScanPlayoutRefreshIntent_Library_LibraryId`
        FOREIGN KEY (`LibraryId`) REFERENCES `Library` (`Id`) ON DELETE SET NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE INDEX `IX_ScanPlayoutRefreshIntent_StateUpdatedAt`
    ON `ScanPlayoutRefreshIntent` (`State`, `UpdatedAt`);
CREATE INDEX `IX_ScanPlayoutRefreshIntent_LibraryId`
    ON `ScanPlayoutRefreshIntent` (`LibraryId`);
