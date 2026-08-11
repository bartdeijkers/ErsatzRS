-- ADR 0066: repository-backed add-ons are an ErsatzRS extension with no
-- ErsatzTV schema counterpart. The legacy RemoteStream.Script column remains
-- inert so this migration stays additive.

CREATE TABLE `AddonRepository` (
    `Id` VARCHAR(255) NOT NULL,
    `Name` VARCHAR(255) NOT NULL,
    `IndexUrl` VARCHAR(767) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    `PublicKey` VARCHAR(64) NOT NULL,
    `IsOfficial` INT NOT NULL DEFAULT 0,
    `IsEnabled` INT NOT NULL DEFAULT 1,
    `LastSequence` BIGINT NULL,
    `LastRefresh` LONGTEXT NULL,
    `LastError` LONGTEXT NULL,
    `CatalogJson` LONGTEXT NULL,
    CONSTRAINT `PK_AddonRepository` PRIMARY KEY (`Id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE UNIQUE INDEX `IX_AddonRepository_IndexUrl`
    ON `AddonRepository` (`IndexUrl`);

CREATE TABLE `AddonInstallation` (
    `AddonId` VARCHAR(255) NOT NULL,
    `RepositoryId` VARCHAR(255) NULL,
    `Version` VARCHAR(255) NOT NULL,
    `Enabled` INT NOT NULL DEFAULT 0,
    `PackageSha256` VARCHAR(64) NOT NULL,
    `ContentSha256` VARCHAR(64) NOT NULL,
    `ManifestJson` LONGTEXT NOT NULL,
    `InstallPath` LONGTEXT NOT NULL,
    `InstalledAt` LONGTEXT NOT NULL,
    `UpdatedAt` LONGTEXT NOT NULL,
    `LastCheckCode` VARCHAR(255) NULL,
    `LastCheckMessage` LONGTEXT NULL,
    CONSTRAINT `PK_AddonInstallation` PRIMARY KEY (`AddonId`),
    CONSTRAINT `FK_AddonInstallation_AddonRepository_RepositoryId`
        FOREIGN KEY (`RepositoryId`) REFERENCES `AddonRepository` (`Id`) ON DELETE SET NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

CREATE INDEX `IX_AddonInstallation_RepositoryId`
    ON `AddonInstallation` (`RepositoryId`);

CREATE TABLE `AddonSetting` (
    `AddonId` VARCHAR(255) NOT NULL,
    `Key` VARCHAR(255) NOT NULL,
    `Value` LONGTEXT NULL,
    `ReferenceKind` VARCHAR(32) NULL,
    `Reference` LONGTEXT NULL,
    CONSTRAINT `PK_AddonSetting` PRIMARY KEY (`AddonId`, `Key`),
    CONSTRAINT `FK_AddonSetting_AddonInstallation_AddonId`
        FOREIGN KEY (`AddonId`) REFERENCES `AddonInstallation` (`AddonId`) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

ALTER TABLE `RemoteStream`
    ADD COLUMN `AddonId` VARCHAR(255) NULL,
    ADD CONSTRAINT `FK_RemoteStream_AddonInstallation_AddonId`
        FOREIGN KEY (`AddonId`) REFERENCES `AddonInstallation` (`AddonId`) ON DELETE RESTRICT;

CREATE INDEX `IX_RemoteStream_AddonId` ON `RemoteStream` (`AddonId`);

INSERT INTO `AddonRepository` (
    `Id`, `Name`, `IndexUrl`, `PublicKey`, `IsOfficial`, `IsEnabled`
) VALUES (
    'org.ersatzrs.addons.official',
    'Official ErsatzRS add-ons',
    'https://bartdeijkers.github.io/ErsatzRS-addons/index-v1.json',
    '',
    1,
    1
);
