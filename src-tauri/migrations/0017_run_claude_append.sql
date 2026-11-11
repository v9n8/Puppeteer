-- The text appended to Claude's system prompt on a session's first turn
-- (`runs::claude_prompt`, AC-06): git-account context, framing and response style.
-- Every resume of that session sends it again byte for byte, so the prompt (and
-- its cache prefix) is the same on every turn, whether or not the SDK records
-- the prompt itself (`systemPrompt.snapshot`).
--
-- '' = the session started with nothing to append. NULL = not recorded (a run
-- from before this column, or one never started on Claude): the next Claude
-- resume rebuilds it and records it then.
ALTER TABLE runs ADD COLUMN claude_append TEXT;
