-- ErsatzRS extension (ADRs 0121/0127): retain the provider-selected source identity
-- separately from the internal integer MediaSource.Id. Existing files remain
-- unqualified; this declaration is not native acquisition or readiness proof.
ALTER TABLE "MediaFile" ADD COLUMN "ProviderMediaSourceId" TEXT NULL;

-- A changed selected source invalidates observations of the previous source.
-- No-op updates, including NULL to NULL, retain their existing observations.
CREATE TRIGGER "TR_MediaFile_InvalidateTeletextProviderSource"
BEFORE UPDATE OF "ProviderMediaSourceId" ON "MediaFile"
FOR EACH ROW
WHEN OLD."ProviderMediaSourceId" IS NOT NEW."ProviderMediaSourceId"
BEGIN
    DELETE FROM "SourceTeletextMetadata" WHERE "MediaFileId" = OLD."Id";
END;
