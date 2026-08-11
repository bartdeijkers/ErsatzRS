-- ADR 0066: persist HTTP validators only for the signature-verified catalog
-- bytes. A 304 response may reuse CatalogJson without re-downloading its
-- detached signature because repository trust changes clear all three fields.

ALTER TABLE `AddonRepository`
    ADD COLUMN `CatalogEtag` VARCHAR(1024) NULL,
    ADD COLUMN `CatalogLastModified` VARCHAR(128) NULL,
    ADD COLUMN `StateRevision` BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN `CatalogRevision` BIGINT NOT NULL DEFAULT 0;

ALTER TABLE `AddonInstallation`
    ADD COLUMN `StateRevision` VARCHAR(36) CHARACTER SET ascii COLLATE ascii_bin
        NOT NULL DEFAULT 'initial',
    ADD COLUMN `DataNamespace` VARCHAR(255) CHARACTER SET ascii COLLATE ascii_bin
        NOT NULL DEFAULT 'legacy';

CREATE TABLE `AddonLifecycleLock` (
    `Id` TINYINT NOT NULL,
    `Revision` BIGINT NOT NULL DEFAULT 0,
    CONSTRAINT `PK_AddonLifecycleLock` PRIMARY KEY (`Id`),
    CONSTRAINT `CK_AddonLifecycleLock_Id` CHECK (`Id` = 1)
) ENGINE=InnoDB;

INSERT INTO `AddonLifecycleLock` (`Id`, `Revision`) VALUES (1, 0);
