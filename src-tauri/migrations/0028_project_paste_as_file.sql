-- Long pastes become a `.txt` attachment (#16, RT-25; `commands::paste_as_file`). Per project:
-- on/off, and the length (UTF-16 code units of the pasted chunk, JS `String.length`) above
-- which a paste is saved to a file instead of going into the composer. Both NOT NULL: every
-- existing project gets the defaults (on, 2000).
ALTER TABLE projects ADD COLUMN paste_as_file INTEGER NOT NULL DEFAULT 1;
ALTER TABLE projects ADD COLUMN paste_as_file_max_chars INTEGER NOT NULL DEFAULT 2000;
