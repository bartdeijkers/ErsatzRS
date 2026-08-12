-- ADR 0066: pin the production Ed25519 public key for the official add-on
-- repository. The 202608111200 seed intentionally shipped an empty key until
-- the private signing key was provisioned in the publisher's secret store, so
-- that row could never refresh. The key below is the raw 32-byte public key in
-- canonical standard Base64; it verifies the detached signature published at
-- https://bartdeijkers.github.io/ErsatzRS-addons/index-v1.json.sig
--
-- The empty-key guard leaves an operator-replaced trust root untouched. The
-- cached catalog, HTTP validators, and last sequence are cleared because no
-- catalog was ever verified against this key, and StateRevision is bumped
-- because the trust root changed.

UPDATE "AddonRepository"
SET "PublicKey" = 'D0o3JNx9IZAo+CzWvV0V2Dfitsk0Z8Fy1D7Mk+WIAZw=',
    "CatalogJson" = NULL,
    "CatalogEtag" = NULL,
    "CatalogLastModified" = NULL,
    "LastSequence" = NULL,
    "LastError" = NULL,
    "StateRevision" = "StateRevision" + 1
WHERE "Id" = 'org.ersatzrs.addons.official'
  AND "PublicKey" = '';
