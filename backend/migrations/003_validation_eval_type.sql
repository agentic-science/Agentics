ALTER TABLE evaluation_jobs
  DROP CONSTRAINT IF EXISTS evaluation_jobs_eval_type_check;

ALTER TABLE evaluations
  DROP CONSTRAINT IF EXISTS evaluations_eval_type_check;

ALTER TABLE evaluation_jobs
  ADD CONSTRAINT evaluation_jobs_eval_type_check
  CHECK (eval_type IN ('validation', 'official'));

ALTER TABLE evaluations
  ADD CONSTRAINT evaluations_eval_type_check
  CHECK (eval_type IN ('validation', 'official'));
