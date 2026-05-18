DO $$
BEGIN
  IF EXISTS (
    SELECT 1
    FROM information_schema.columns
    WHERE table_schema = 'public'
      AND table_name = 'challenges'
      AND column_name = 'summary'
      AND data_type <> 'jsonb'
  ) THEN
    ALTER TABLE challenges
      ALTER COLUMN summary DROP DEFAULT;

    ALTER TABLE challenges
      ALTER COLUMN summary TYPE JSONB
      USING jsonb_build_object('en', summary, 'zh', summary);

    ALTER TABLE challenges
      ALTER COLUMN summary SET DEFAULT '{"en":"","zh":""}'::jsonb;
  END IF;
END $$;

UPDATE challenges
SET spec_json = spec_json - 'challenge_summary' ||
  jsonb_build_object(
    'summary',
    CASE
      WHEN jsonb_typeof(spec_json->'challenge_summary') = 'string' THEN
        jsonb_build_object(
          'en',
          spec_json->>'challenge_summary',
          'zh',
          spec_json->>'challenge_summary'
        )
      ELSE
        spec_json->'challenge_summary'
    END
  )
WHERE spec_json ? 'challenge_summary';
