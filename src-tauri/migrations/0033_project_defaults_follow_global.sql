-- A project default for new sessions (0013) equal to the global setting of the same meaning
-- becomes NULL, i.e. follows the global setting, so a later global change carries the project
-- along. Picking the starred option on the project page now stores NULL too; before, it stored
-- a copy that stayed on the old value. "Same meaning" is what the star marks: a blank or unknown
-- global engine is `claude`, a blank or unknown permission mode is `default`, an unknown effort
-- matches nothing, a blank style is `default` (no style); models compare exactly (an alias is not
-- the full id). Values that differ and unknown values stay. Trashed projects are converted too.
-- `enabled_models`, `claude_account_id` and the legacy `default_engine` are not touched.
--
-- The permission modes and effort levels are a snapshot of `runs::is_valid_permission_mode` and
-- `runs::EFFORT_LEVELS` (a test keeps them equal), so this file means the same thing later.
UPDATE projects SET default_engine_override = NULL
WHERE TRIM(default_engine_override) = COALESCE(
  (SELECT CASE WHEN TRIM(value) IN ('claude', 'codex') THEN TRIM(value) END FROM settings WHERE key = 'default_engine'),
  'claude');

UPDATE projects SET default_model_claude = NULL
WHERE TRIM(default_model_claude) = (SELECT TRIM(value) FROM settings WHERE key = 'claude_model');

UPDATE projects SET default_model_codex = NULL
WHERE TRIM(default_model_codex) = (SELECT TRIM(value) FROM settings WHERE key = 'codex_model');

UPDATE projects SET default_permission_mode = NULL
WHERE TRIM(default_permission_mode) = COALESCE(
  (SELECT CASE WHEN TRIM(value) IN ('default', 'acceptEdits', 'bypassPermissions', 'plan', 'auto', 'dontAsk')
               THEN TRIM(value) END
   FROM settings WHERE key = 'default_permission_mode'),
  'default');

UPDATE projects SET default_effort = NULL
WHERE TRIM(default_effort) = (SELECT TRIM(value) FROM settings
                              WHERE key = 'default_effort' AND TRIM(value) IN ('low', 'medium', 'high', 'xhigh', 'max'));

UPDATE projects SET default_style = NULL
WHERE TRIM(default_style) = COALESCE(NULLIF((SELECT TRIM(value) FROM settings WHERE key = 'default_style'), ''), 'default');
