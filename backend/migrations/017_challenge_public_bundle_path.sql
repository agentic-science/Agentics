ALTER TABLE challenges
  ADD COLUMN IF NOT EXISTS public_bundle_path TEXT;

UPDATE challenges
SET public_bundle_path = bundle_path
WHERE public_bundle_path IS NULL
  AND bundle_path IS NOT NULL;
