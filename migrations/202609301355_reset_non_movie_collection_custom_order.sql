-- Repair legacy settings using the upstream movie-only ordering contract.
-- Retain memberships, row identities, indices and audio/subtitle overrides.
-- Empty and movie-only collections are unchanged. Previous enabled flags for
-- non-movie collections cannot be reconstructed; this repair runs once.
UPDATE "Collection"
SET "UseCustomPlaybackOrder" = 0
WHERE "UseCustomPlaybackOrder" = 1
  AND "Id" IN (
      SELECT "ci"."CollectionId"
      FROM "CollectionItem" AS "ci"
      LEFT JOIN "Movie" AS "m" ON "m"."Id" = "ci"."MediaItemId"
      WHERE "m"."Id" IS NULL
  );
