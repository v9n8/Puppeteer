-- The permission mode a conductor's sidecar last launched with (CO-05). The auto-wake resumes a
-- conductor in exactly that mode and refuses to guess one (a looser guess could let it spawn
-- looser workers). Kept only in memory before, so after an app restart a worker finishing could
-- not wake its conductor until the user resumed it by hand. NULL = not launched since this column
-- exists; the wake then still refuses, as before.
ALTER TABLE conductor_sessions ADD COLUMN permission_mode TEXT;
