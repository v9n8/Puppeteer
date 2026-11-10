-- Structured quotes a prompt carried (`runs::quotes`), so the sent message shows
-- them as cards that lead back to what they quote.
--
-- The model still gets them as a markdown blockquote inside the prompt text; these
-- rows only describe it. A prompt is matched by `quote_set`, which its record in the
-- run log carries (`quote_set` on the `user` line), not by a byte offset: a log
-- rebuilt from the engine transcript moves offsets and simply loses the field, and
-- then the message shows the plain blockquote again.
--
-- kind 'entry': an assistant message of the run, by its message id
--   (`source_entry_id`, the engine's `message.id`), the index of the text block
--   within that message (`block_index`) and UTF-16 offsets into the text that
--   block renders as (`start_offset`..`end_offset`; equal = the whole message).
-- kind 'file':  a file window's selection (`path`, `line_start`..`line_end`).
-- kind 'text':  nothing to go back to (a question bubble, a message without an id).
CREATE TABLE run_quotes (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    quote_set TEXT NOT NULL,
    position INTEGER NOT NULL,
    kind TEXT NOT NULL,
    source_run_id TEXT,
    source_entry_id TEXT,
    block_index INTEGER,
    start_offset INTEGER,
    end_offset INTEGER,
    path TEXT,
    line_start INTEGER,
    line_end INTEGER,
    excerpt TEXT NOT NULL,
    note TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    UNIQUE (run_id, quote_set, position)
);

-- A trigger rather than a foreign key, like 0006 and 0008: it holds on a
-- connection without `foreign_keys` too, and also fires for runs a project's
-- cascade deletes.
CREATE TRIGGER trg_runs_quotes_delete AFTER DELETE ON runs BEGIN
    DELETE FROM run_quotes WHERE run_id = old.id;
END;
