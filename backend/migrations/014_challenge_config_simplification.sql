WITH rewritten_targets AS (
  SELECT
    c.name,
    jsonb_agg(
      CASE
        WHEN target.target_json ->> 'accelerator' = 'cpu' THEN
          jsonb_set(renamed.target_json, '{accelerator}', 'null'::jsonb, true)
        ELSE renamed.target_json
      END
      ORDER BY target.ordinality
    ) AS targets_json
  FROM challenges c
  CROSS JOIN LATERAL jsonb_array_elements(c.spec_json -> 'targets')
    WITH ORDINALITY AS target(target_json, ordinality)
  CROSS JOIN LATERAL (
    SELECT
      CASE
        WHEN target.target_json #> '{resource_profile,hardware}' IS NOT NULL THEN
          jsonb_set(
            target.target_json #- '{resource_profile,hardware}',
            '{resource_profile,hardware_metadata}',
            target.target_json #> '{resource_profile,hardware}',
            true
          )
        ELSE target.target_json
      END AS target_json
  ) renamed
  WHERE c.spec_json IS NOT NULL
    AND c.spec_json ? 'targets'
  GROUP BY c.name
)
UPDATE challenges c
SET spec_json = jsonb_set(c.spec_json, '{targets}', rewritten_targets.targets_json, false)
FROM rewritten_targets
WHERE c.name = rewritten_targets.name;

UPDATE challenges
SET spec_json = spec_json
  #- '{execution,validation_prepare,external_data}'
  #- '{execution,validation_prepare,cache_key_hint}'
  #- '{execution,official_prepare,external_data}'
  #- '{execution,official_prepare,cache_key_hint}'
WHERE spec_json IS NOT NULL;

UPDATE challenges
SET
  starts_at = COALESCE(starts_at, created_at),
  spec_json = jsonb_set(
    spec_json,
    '{starts_at}',
    to_jsonb(to_char(COALESCE(starts_at, created_at) AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')),
    true
  )
WHERE status = 'active'
  AND spec_json IS NOT NULL
  AND (
    starts_at IS NULL
    OR NOT spec_json ? 'starts_at'
    OR spec_json -> 'starts_at' = 'null'::jsonb
  );

ALTER TABLE challenges
  ADD CONSTRAINT challenges_active_starts_at_check
  CHECK (status <> 'active' OR spec_json IS NULL OR starts_at IS NOT NULL);
