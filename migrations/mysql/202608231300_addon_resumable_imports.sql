-- AMM-G host-owned overview discovery and deferred detail state.
-- DetailImportPolicy: 0 = automatic, 1 = manual_only.
ALTER TABLE `AddonMediaList`
    ADD COLUMN `DetailImportPolicy` INT NOT NULL DEFAULT 0,
    ADD COLUMN `DetailImportAuthorizedAt` VARCHAR(64) NULL,
    ADD COLUMN `DetailProjectionPendingAt` VARCHAR(64) NULL;

CREATE TABLE `AddonMediaListImportJob` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `AddonMediaListId` INT NOT NULL,
    `Generation` INT NOT NULL,
    `State` INT NOT NULL,
    `RecordCapability` VARCHAR(64) NOT NULL,
    `RequestedFolder` VARCHAR(767) NULL,
    `Cursor` VARCHAR(4096) NULL,
    `RevisionToken` VARCHAR(384) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NULL,
    `VerificationPass` INT NOT NULL DEFAULT 0,
    `ExpectedIdentityHash` VARCHAR(64) NULL,
    `ListRecordJson` LONGTEXT NULL,
    `TotalHint` INT NULL,
    `DiscoveredCount` INT NOT NULL DEFAULT 0,
    `OverviewComplete` BOOLEAN NOT NULL DEFAULT FALSE,
    `AttemptCount` INT NOT NULL DEFAULT 0,
    `Diagnostic` VARCHAR(2048) NULL,
    `NextEligibleAt` VARCHAR(64) NULL,
    `CancellationRequested` BOOLEAN NOT NULL DEFAULT FALSE,
    `LeaseOwner` VARCHAR(64) NULL,
    `LeaseExpiresAt` VARCHAR(64) NULL,
    `HeartbeatAt` VARCHAR(64) NULL,
    `LastProgressAt` VARCHAR(64) NULL,
    `CreatedAt` VARCHAR(64) NOT NULL,
    `UpdatedAt` VARCHAR(64) NOT NULL,
    `CompletedAt` VARCHAR(64) NULL,
    CONSTRAINT `PK_AddonMediaListImportJob` PRIMARY KEY (`Id`),
    CONSTRAINT `FK_AddonImportJob_AddonMediaList_ListId`
        FOREIGN KEY (`AddonMediaListId`) REFERENCES `AddonMediaList` (`Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE UNIQUE INDEX `IX_AddonMediaListImportJob_ListGeneration`
    ON `AddonMediaListImportJob` (`AddonMediaListId`, `Generation`);
CREATE INDEX `IX_AddonMediaListImportJob_StateLease`
    ON `AddonMediaListImportJob` (`State`, `NextEligibleAt`, `LeaseExpiresAt`, `UpdatedAt`);

CREATE TABLE `AddonMediaListImportItem` (
    `Id` INT NOT NULL AUTO_INCREMENT,
    `AddonMediaListImportJobId` INT NOT NULL,
    `VerificationPass` INT NOT NULL DEFAULT 0,
    `ProviderId` VARCHAR(384) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    `Rank` INT NOT NULL,
    `RecordJson` LONGTEXT NOT NULL,
    `OverviewFingerprint` VARCHAR(64) NOT NULL,
    CONSTRAINT `PK_AddonMediaListImportItem` PRIMARY KEY (`Id`),
    CONSTRAINT `FK_AddonImportItem_AddonImportJob_JobId`
        FOREIGN KEY (`AddonMediaListImportJobId`) REFERENCES `AddonMediaListImportJob` (`Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE UNIQUE INDEX `IX_AddonMediaListImportItem_JobProviderId`
    ON `AddonMediaListImportItem` (`AddonMediaListImportJobId`, `VerificationPass`, `ProviderId`);
CREATE UNIQUE INDEX `IX_AddonMediaListImportItem_JobRank`
    ON `AddonMediaListImportItem` (`AddonMediaListImportJobId`, `VerificationPass`, `Rank`);

CREATE TABLE `AddonMediaListImportCursor` (
    `AddonMediaListImportJobId` INT NOT NULL,
    `VerificationPass` INT NOT NULL,
    `CursorHash` VARCHAR(64) NOT NULL,
    CONSTRAINT `PK_AddonMediaListImportCursor`
        PRIMARY KEY (`AddonMediaListImportJobId`, `VerificationPass`, `CursorHash`),
    CONSTRAINT `FK_AddonImportCursor_AddonImportJob_JobId`
        FOREIGN KEY (`AddonMediaListImportJobId`) REFERENCES `AddonMediaListImportJob` (`Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE TABLE `AddonMediaListDetailJob` (
    `AddonMediaListItemId` INT NOT NULL,
    `AddonMediaListId` INT NOT NULL,
    `ProviderHost` VARCHAR(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    `State` INT NOT NULL DEFAULT 0,
    `ManualPriority` BOOLEAN NOT NULL DEFAULT FALSE,
    `OverviewFingerprint` VARCHAR(64) NOT NULL,
    `OverviewRecordJson` LONGTEXT NOT NULL,
    `AttemptCount` INT NOT NULL DEFAULT 0,
    `Outcome` VARCHAR(128) NULL,
    `ResultRecordJson` LONGTEXT NULL,
    `Diagnostic` VARCHAR(2048) NULL,
    `NextEligibleAt` VARCHAR(64) NULL,
    `LeaseOwner` VARCHAR(64) NULL,
    `LeaseExpiresAt` VARCHAR(64) NULL,
    `RequestedAt` VARCHAR(64) NULL,
    `StartedAt` VARCHAR(64) NULL,
    `CompletedAt` VARCHAR(64) NULL,
    `UpdatedAt` VARCHAR(64) NOT NULL,
    CONSTRAINT `PK_AddonMediaListDetailJob` PRIMARY KEY (`AddonMediaListItemId`),
    CONSTRAINT `FK_AddonDetailJob_AddonMediaListItem_ItemId`
        FOREIGN KEY (`AddonMediaListItemId`) REFERENCES `AddonMediaListItem` (`Id`) ON DELETE CASCADE,
    CONSTRAINT `FK_AddonDetailJob_AddonMediaList_ListId`
        FOREIGN KEY (`AddonMediaListId`) REFERENCES `AddonMediaList` (`Id`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE INDEX `IX_AddonMediaListDetailJob_Claim`
    ON `AddonMediaListDetailJob` (`State`, `ManualPriority`, `NextEligibleAt`, `UpdatedAt`);
CREATE INDEX `IX_AddonMediaListDetailJob_ListState`
    ON `AddonMediaListDetailJob` (`AddonMediaListId`, `State`);
CREATE INDEX `IX_AddonMediaListDetailJob_HostState`
    ON `AddonMediaListDetailJob` (`ProviderHost`, `State`);

CREATE TABLE `AddonProviderHostCooldown` (
    `ProviderHost` VARCHAR(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    `NextEligibleAt` VARCHAR(64) NOT NULL,
    `SampledDelaySeconds` INT NOT NULL,
    `LastAttemptAt` VARCHAR(64) NOT NULL,
    `LeaseOwner` VARCHAR(64) NULL,
    `LeaseExpiresAt` VARCHAR(64) NULL,
    `UpdatedAt` VARCHAR(64) NOT NULL,
    CONSTRAINT `PK_AddonProviderHostCooldown` PRIMARY KEY (`ProviderHost`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE INDEX `IX_AddonProviderHostCooldown_NextEligibleAt`
    ON `AddonProviderHostCooldown` (`NextEligibleAt`);
