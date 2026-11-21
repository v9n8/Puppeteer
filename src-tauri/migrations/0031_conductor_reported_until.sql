-- The newest worker `finished_at` an auto-resume nudge has already reported, per conductor
-- (CO-08). The nudge names the workers that finished after it. Kept only in memory before; after
-- an app restart the conductor's `last_activity_at` stood in for it, which skips a worker whose
-- nudge failed before the user sent the conductor a message. NULL = nothing reported since this
-- column exists; the old stand-in is used then.
ALTER TABLE conductor_sessions ADD COLUMN reported_until TEXT;
