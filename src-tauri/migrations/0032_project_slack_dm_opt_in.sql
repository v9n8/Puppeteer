-- Reading Slack DM threads becomes opt-in per project (`slack_threads::dm`): 1 = read DMs for this
-- project while the global `slack_dm_read` setting is on, 0 = never. 0029 made it "follow the
-- setting" (default 1); every project starts off again and new projects are off. SQLite cannot
-- change a column default, so the column is recreated under the same name.
ALTER TABLE projects DROP COLUMN slack_dm_read;
ALTER TABLE projects ADD COLUMN slack_dm_read INTEGER NOT NULL DEFAULT 0;
